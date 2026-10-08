use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use ed25519_dalek::VerifyingKey;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::update_engine::{
    integrity::{self, FileStatus, FileVerification},
    manifest::{
        normalize_manifest_path, parse_manifest, resolve_manifest_path, validate_source_hosts,
        validate_target, ExpectedTarget, FileKind, FileRole, Manifest,
    },
    staging,
};

// Configuración actual de producción; al añadir reinos pasará a ser configuración por reino.
const MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/warcrafted-server/launcher/main/docs/contenido/manifest.json";
const MANIFEST_PUBLIC_KEY_HEX: &str =
    "c52f0e55186486655520f8a03042fd95f194020a19489944fac2b603fe2fdc6b";
const MANIFEST_KEY_ID: &str = "warcrafted-manifest-2026";
const ALLOWED_SOURCE_HOSTS: &[&str] = &["raw.githubusercontent.com", "github.com"];
const STAGING_DIRECTORY: &str = ".warcrafted-staging";

static NEXT_STAGING_DIRECTORY: AtomicU64 = AtomicU64::new(0);
static NEXT_BACKUP_FILE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClientFileStatus {
    pub path: String,
    pub role: FileRole,
    pub status: ClientFileState,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ClientFileState {
    Ok,
    Missing,
    Corrupt,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateProgress {
    path: String,
    index: usize,
    total: usize,
    status: &'static str,
    message: Option<String>,
}

struct ClientSnapshot {
    manifest: Manifest,
    report: Vec<FileVerification>,
}

struct StagingDirectory(PathBuf);

impl Drop for StagingDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[tauri::command]
pub(crate) async fn check_client_status(
    install_dir: String,
) -> Result<Vec<ClientFileStatus>, String> {
    let snapshot = load_client_snapshot(&install_dir).await?;
    Ok(snapshot
        .manifest
        .files
        .iter()
        .zip(&snapshot.report)
        .map(|(file, verification)| to_client_file_status(file.role, verification))
        .collect())
}

#[tauri::command]
pub(crate) async fn update_client(app: AppHandle, install_dir: String) -> Result<(), String> {
    let snapshot = load_client_snapshot(&install_dir).await?;
    let pending: Vec<_> = snapshot
        .manifest
        .files
        .iter()
        .zip(&snapshot.report)
        .filter(|(file, verification)| {
            file.role == FileRole::Required && verification.status != FileStatus::Valid
        })
        .map(|(file, _)| file.clone())
        .collect();

    if pending.is_empty() {
        return Ok(());
    }

    let install_root = prepare_install_root(Path::new(&install_dir))?;
    let staging_directory = create_staging_directory(&install_root)?;
    let client = reqwest::Client::builder()
        .build()
        .map_err(|error| format!("no se pudo preparar el cliente HTTP: {error}"))?;
    let total = pending.len();
    let mut failures = Vec::new();

    for (index, file) in pending.iter().enumerate() {
        let result =
            install_manifest_file(&client, file, &install_root, &staging_directory.0).await;
        let (status, message) = match result {
            Ok(()) => ("ok", None),
            Err(message) => {
                failures.push(format!("{}: {message}", file.path));
                ("error", Some(message))
            }
        };

        let _ = app.emit(
            "update-progress",
            UpdateProgress {
                path: file.path.clone(),
                index: index + 1,
                total,
                status,
                message,
            },
        );
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Fallaron {} de {total} archivos obligatorios: {}",
            failures.len(),
            failures.join("; ")
        ))
    }
}

#[tauri::command]
pub(crate) async fn launch_game(
    install_dir: String,
    executable_name: String,
) -> Result<(), String> {
    let snapshot = load_client_snapshot(&install_dir).await?;
    match decide_launch(&snapshot.manifest, &snapshot.report, &executable_name) {
        LaunchDecision::Blocked(paths) => {
            return Err(format!(
                "No se puede iniciar el juego; archivos obligatorios no válidos: {}",
                paths.join(", ")
            ));
        }
        LaunchDecision::Allowed => {}
    }

    let normalized_executable = normalize_manifest_path(&executable_name)
        .map_err(|error| format!("ruta de ejecutable inválida: {error}"))?;
    let install_root = fs::canonicalize(&install_dir)
        .map_err(|error| format!("no se pudo abrir el directorio del cliente: {error}"))?;
    let executable_path = resolve_manifest_path(&install_root, &normalized_executable)
        .map_err(|error| error.to_string())?;
    validate_install_path(&install_root, &executable_path, false)?;
    let metadata = fs::symlink_metadata(&executable_path)
        .map_err(|error| format!("no se pudo acceder al ejecutable: {error}"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("el ejecutable no es un archivo normal del cliente".into());
    }

    Command::new(&executable_path)
        .current_dir(&install_root)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("no se pudo iniciar el juego: {error}"))
}

#[tauri::command]
pub(crate) fn greet(name: &str) -> String {
    format!("¡Hola, {}! Te saludamos desde Rust.", name)
}

async fn load_client_snapshot(install_dir: &str) -> Result<ClientSnapshot, String> {
    let manifest = fetch_manifest().await?;
    let install_root = PathBuf::from(install_dir);
    let manifest_for_verification = manifest.clone();
    let report = tokio::task::spawn_blocking(move || {
        integrity::verify_manifest_files(&manifest_for_verification, &install_root)
    })
    .await
    .map_err(|error| format!("falló la tarea de verificación de integridad: {error}"))?
    .map_err(|error| error.to_string())?;

    Ok(ClientSnapshot { manifest, report })
}

async fn fetch_manifest() -> Result<Manifest, String> {
    let response = reqwest::get(MANIFEST_URL)
        .await
        .map_err(|error| format!("no se pudo descargar el manifest: {error}"))?
        .error_for_status()
        .map_err(|error| format!("respuesta HTTP inválida para el manifest: {error}"))?;
    let document = response
        .bytes()
        .await
        .map_err(|error| format!("no se pudo leer el manifest: {error}"))?;

    let public_key_bytes = decode_public_key(MANIFEST_PUBLIC_KEY_HEX)?;
    let verifying_key = VerifyingKey::from_bytes(&public_key_bytes)
        .map_err(|error| format!("clave pública de manifest inválida: {error}"))?;
    let trusted_keys = BTreeMap::from([(MANIFEST_KEY_ID.to_owned(), verifying_key)]);
    let manifest = parse_manifest(&document, &trusted_keys)
        .map_err(|error| format!("no se pudo validar el manifest: {error}"))?;
    validate_target(
        &manifest,
        &ExpectedTarget {
            realm: "icetracks",
            channel: "production",
            client_build: 12340,
        },
    )
    .map_err(|error| format!("el manifest no corresponde al cliente esperado: {error}"))?;
    validate_source_hosts(&manifest, ALLOWED_SOURCE_HOSTS)
        .map_err(|error| format!("el manifest contiene un origen no permitido: {error}"))?;

    Ok(manifest)
}

fn decode_public_key(value: &str) -> Result<[u8; 32], String> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("la clave pública debe contener 32 bytes en hexadecimal".into());
    }

    let mut decoded = [0; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        decoded[index] = (hex_digit(pair[0])? << 4) | hex_digit(pair[1])?;
    }
    Ok(decoded)
}

fn hex_digit(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err("la clave pública contiene un carácter hexadecimal inválido".into()),
    }
}

fn to_client_file_status(role: FileRole, verification: &FileVerification) -> ClientFileStatus {
    let (status, message) = match &verification.status {
        FileStatus::Valid => (ClientFileState::Ok, None),
        FileStatus::Missing => (ClientFileState::Missing, None),
        FileStatus::Corrupt { expected, actual } => (
            ClientFileState::Corrupt,
            Some(format!("SHA-256 esperado {expected}, recibido {actual}")),
        ),
        FileStatus::Unreadable { message } => (ClientFileState::Error, Some(message.clone())),
    };

    ClientFileStatus {
        path: verification.path.clone(),
        role,
        status,
        message,
    }
}

async fn install_manifest_file(
    client: &reqwest::Client,
    file: &crate::update_engine::manifest::ManifestFile,
    install_root: &Path,
    staging_root: &Path,
) -> Result<(), String> {
    let staged_path = staging::stage_manifest_file(client, file, staging_root)
        .await
        .map_err(|error| error.to_string())?;
    let destination =
        resolve_manifest_path(install_root, &file.path).map_err(|error| error.to_string())?;

    if file.kind == FileKind::Archive {
        let extraction_destination = destination.parent().ok_or_else(|| {
            "el archivo comprimido no puede reemplazar la raíz del cliente".to_owned()
        })?;
        if extraction_destination == install_root {
            return Err("el archivo comprimido no puede reemplazar la raíz del cliente".into());
        }

        let archive_path = staged_path.clone();
        let extraction_destination = extraction_destination.to_path_buf();
        tokio::task::spawn_blocking(move || {
            staging::extract_archive(&archive_path, &extraction_destination)
                .map_err(|error| error.to_string())
        })
        .await
        .map_err(|error| format!("falló la tarea de extracción del archivo: {error}"))??;
    }

    let install_root = install_root.to_path_buf();
    tokio::task::spawn_blocking(move || {
        promote_staged_file(&install_root, &staged_path, &destination)
    })
    .await
    .map_err(|error| format!("falló la tarea de instalación del archivo: {error}"))??;
    Ok(())
}

fn prepare_install_root(path: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(path)
        .map_err(|error| format!("no se pudo preparar el directorio del cliente: {error}"))?;
    fs::canonicalize(path)
        .map_err(|error| format!("no se pudo resolver el directorio del cliente: {error}"))
}

fn create_staging_directory(install_root: &Path) -> Result<StagingDirectory, String> {
    let parent = install_root.join(STAGING_DIRECTORY);
    match fs::symlink_metadata(&parent) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err("el directorio de staging no es seguro".into());
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&parent)
                .map_err(|error| format!("no se pudo crear el directorio de staging: {error}"))?;
        }
        Err(error) => {
            return Err(format!(
                "no se pudo acceder al directorio de staging: {error}"
            ));
        }
    }
    let parent = fs::canonicalize(parent)
        .map_err(|error| format!("no se pudo resolver el directorio de staging: {error}"))?;
    if !parent.starts_with(install_root) {
        return Err("el directorio de staging escapa del cliente".into());
    }

    for _ in 0..32 {
        let sequence = NEXT_STAGING_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!("run-{}-{sequence}", std::process::id()));
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(StagingDirectory(candidate)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "no se pudo crear el staging de actualización: {error}"
                ));
            }
        }
    }
    Err("no se pudo reservar un directorio de staging".into())
}

fn promote_staged_file(
    install_root: &Path,
    staged_path: &Path,
    destination: &Path,
) -> Result<(), String> {
    validate_install_path(install_root, destination, true)?;
    let staged_metadata = fs::symlink_metadata(staged_path)
        .map_err(|error| format!("no se pudo acceder al archivo validado: {error}"))?;
    if !staged_metadata.is_file() || staged_metadata.file_type().is_symlink() {
        return Err("el archivo validado de staging no es seguro".into());
    }

    let backup = match fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            let backup = unique_backup_path(destination);
            fs::rename(destination, &backup)
                .map_err(|error| format!("no se pudo apartar el archivo anterior: {error}"))?;
            Some(backup)
        }
        Ok(_) => return Err("el destino del archivo no es un archivo normal".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(format!(
                "no se pudo acceder al destino del archivo: {error}"
            ))
        }
    };

    if let Err(error) = fs::rename(staged_path, destination) {
        if let Some(backup) = &backup {
            let _ = fs::rename(backup, destination);
        }
        return Err(format!("no se pudo instalar el archivo validado: {error}"));
    }
    if let Some(backup) = backup {
        let _ = fs::remove_file(backup);
    }
    Ok(())
}

fn unique_backup_path(destination: &Path) -> PathBuf {
    let file_name = destination.file_name().unwrap_or_default();
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    loop {
        let sequence = NEXT_BACKUP_FILE.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{}.warcrafted-backup-{}-{sequence}",
            file_name.to_string_lossy(),
            std::process::id()
        ));
        if !candidate.exists() {
            return candidate;
        }
    }
}

fn validate_install_path(root: &Path, path: &Path, create_parents: bool) -> Result<(), String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "la ruta escapa del directorio del cliente".to_owned())?;
    let components: Vec<_> = relative.components().collect();
    if components.is_empty()
        || components
            .iter()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("la ruta del archivo no es segura".into());
    }

    let mut current = root.to_path_buf();
    for (index, component) in components.iter().enumerate() {
        current.push(component.as_os_str());
        let is_leaf = index + 1 == components.len();
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "la ruta contiene un enlace simbólico: {}",
                    current.display()
                ));
            }
            Ok(metadata) if !is_leaf && !metadata.is_dir() => {
                return Err(format!(
                    "un componente de la ruta no es directorio: {}",
                    current.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && !is_leaf => {
                if create_parents {
                    fs::create_dir(&current).map_err(|error| {
                        format!("no se pudo crear un directorio del cliente: {error}")
                    })?;
                } else {
                    return Err(format!(
                        "falta un directorio del cliente: {}",
                        current.display()
                    ));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("no se pudo verificar la ruta del cliente: {error}")),
        }
    }
    Ok(())
}

enum LaunchDecision {
    Allowed,
    Blocked(Vec<String>),
}

fn decide_launch(
    manifest: &Manifest,
    report: &[FileVerification],
    executable_name: &str,
) -> LaunchDecision {
    let mut blockers = Vec::new();
    for file in manifest
        .files
        .iter()
        .filter(|file| file.role == FileRole::Required)
    {
        match report.iter().find(|entry| entry.path == file.path) {
            Some(entry) if entry.status == FileStatus::Valid => {}
            Some(_) | None => blockers.push(file.path.clone()),
        }
    }

    let executable_path = normalize_manifest_path(executable_name).ok();
    match executable_path {
        Some(path)
            if manifest
                .files
                .iter()
                .any(|file| file.path == path && file.role == FileRole::Required) => {}
        _ => blockers.push(executable_name.to_owned()),
    }

    if blockers.is_empty() {
        LaunchDecision::Allowed
    } else {
        blockers.sort();
        blockers.dedup();
        LaunchDecision::Blocked(blockers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update_engine::manifest::{
        Compression, FileSource, ManifestFile, ManifestSignature,
    };

    fn manifest() -> Manifest {
        Manifest {
            schema_version: 1,
            realm: "icetracks".into(),
            channel: "production".into(),
            client_build: 12340,
            manifest_version: 1,
            published_at: "2026-10-08T00:00:00Z".into(),
            min_launcher_version: "0.1.0".into(),
            files: ["Wow.exe", "Data/required.MPQ"]
                .into_iter()
                .map(|path| ManifestFile {
                    path: path.into(),
                    role: FileRole::Required,
                    kind: FileKind::ClientBase,
                    addon_group: None,
                    size_bytes: 0,
                    sha256: "0".repeat(64),
                    source: Some(FileSource {
                        url: "https://github.com/example/file".into(),
                        compressed_size_bytes: 0,
                        compression: Compression::None,
                    }),
                    assembly: None,
                })
                .collect(),
            signature: ManifestSignature {
                key_id: MANIFEST_KEY_ID.into(),
                algorithm: "ed25519".into(),
                value: String::new(),
            },
        }
    }

    #[test]
    fn launch_is_blocked_when_a_required_file_is_not_valid() {
        let manifest = manifest();
        let report = vec![
            FileVerification {
                path: "Wow.exe".into(),
                status: FileStatus::Valid,
            },
            FileVerification {
                path: "Data/required.MPQ".into(),
                status: FileStatus::Corrupt {
                    expected: "0".repeat(64),
                    actual: "1".repeat(64),
                },
            },
        ];

        match decide_launch(&manifest, &report, "Wow.exe") {
            LaunchDecision::Blocked(paths) => assert_eq!(paths, ["Data/required.MPQ"]),
            LaunchDecision::Allowed => panic!("no se debe permitir lanzar un cliente inválido"),
        }
    }

    #[test]
    fn launch_is_allowed_only_for_a_valid_required_executable() {
        let manifest = manifest();
        let report = manifest
            .files
            .iter()
            .map(|file| FileVerification {
                path: file.path.clone(),
                status: FileStatus::Valid,
            })
            .collect::<Vec<_>>();

        assert!(matches!(
            decide_launch(&manifest, &report, "Wow.exe"),
            LaunchDecision::Allowed
        ));
        assert!(matches!(
            decide_launch(&manifest, &report, "../outside.exe"),
            LaunchDecision::Blocked(_)
        ));
    }
}

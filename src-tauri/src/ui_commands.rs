use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Component, Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::{Duration, Instant},
};

use ed25519_dalek::VerifyingKey;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::{disk_space, settings};
use crate::update_engine::{
    integrity::{self, CachedFileState, FileStatus, FileVerification, VerifyMode},
    manifest::{
        normalize_manifest_path, parse_manifest, resolve_manifest_path, validate_source_hosts,
        validate_target, ExpectedTarget, FileKind, FileRole, Manifest,
    },
    staging,
};
use crate::verify_cache;

// Configuración actual de producción; al añadir reinos pasará a ser configuración por reino.
const MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/warcrafted-server/launcher/main/docs/contenido/manifest.json";
const MANIFEST_PUBLIC_KEY_HEX: &str =
    "c52f0e55186486655520f8a03042fd95f194020a19489944fac2b603fe2fdc6b";
const MANIFEST_KEY_ID: &str = "warcrafted-manifest-2026";
const ALLOWED_SOURCE_HOSTS: &[&str] = &["raw.githubusercontent.com", "github.com"];
const STAGING_DIRECTORY: &str = ".warcrafted-staging";
const GAME_EXECUTABLE: &str = "Wow.exe";

static NEXT_STAGING_DIRECTORY: AtomicU64 = AtomicU64::new(0);
static NEXT_BACKUP_FILE: AtomicU64 = AtomicU64::new(0);
static CANCEL_OPERATION: AtomicBool = AtomicBool::new(false);
static GAME_RUNNING: AtomicBool = AtomicBool::new(false);

struct GameReservation {
    active: bool,
}

impl GameReservation {
    fn new() -> Self {
        Self { active: true }
    }

    fn release(&mut self) {
        if self.active {
            release_game(&GAME_RUNNING);
            self.active = false;
        }
    }
}

impl Drop for GameReservation {
    fn drop(&mut self) {
        self.release();
    }
}

fn try_reserve_game(game_running: &AtomicBool) -> bool {
    game_running
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
}

fn release_game(game_running: &AtomicBool) {
    game_running.store(false, Ordering::Release);
}

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct VerifyProgressPayload {
    index: usize,
    total: usize,
    path: String,
    file_bytes_done: u64,
    file_bytes_total: u64,
    bytes_done: u64,
    bytes_total: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SettingsResponse {
    client_dir: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClientStatusResponse {
    files: Vec<ClientFileStatus>,
    installed: bool,
    pending_bytes: u64,
    free_bytes: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiskSpaceResponse {
    free_bytes: u64,
}

struct ClientSnapshot {
    manifest: Manifest,
    report: Vec<FileVerification>,
    cache: HashMap<String, CachedFileState>,
    cache_directory: PathBuf,
}

struct StagingDirectory(PathBuf);

impl Drop for StagingDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[tauri::command]
pub(crate) fn get_settings(app: AppHandle) -> Result<SettingsResponse, String> {
    let config_dir = settings_directory(&app)?;
    let settings = settings::load_settings(&config_dir)?;
    let client_dir = settings
        .client_dir
        .map(|path| settings::validate_client_dir(&path))
        .transpose()?
        .map(|path| display_path(&path));
    Ok(SettingsResponse { client_dir })
}

#[tauri::command]
pub(crate) fn set_client_dir(app: AppHandle, path: String) -> Result<String, String> {
    let client_dir = settings::validate_client_dir(Path::new(&path))?;
    let config_dir = settings_directory(&app)?;
    let mut launcher_settings = settings::load_settings(&config_dir)?;
    launcher_settings.client_dir = Some(client_dir.clone());
    settings::save_settings(&config_dir, &launcher_settings)?;
    Ok(display_path(&client_dir))
}

#[tauri::command]
pub(crate) fn create_install_dir(app: AppHandle, parent: String) -> Result<String, String> {
    let client_dir = create_install_dir_for_parent(Path::new(&parent))?;
    let config_dir = settings_directory(&app)?;
    let mut launcher_settings = settings::load_settings(&config_dir)?;
    launcher_settings.client_dir = Some(client_dir.clone());
    settings::save_settings(&config_dir, &launcher_settings)?;
    Ok(display_path(&client_dir))
}

fn create_install_dir_for_parent(parent: &Path) -> Result<PathBuf, String> {
    let validated_parent = settings::validate_client_dir(parent)?;
    let parent_metadata = fs::symlink_metadata(&validated_parent)
        .map_err(|error| format!("La ubicación elegida debe ser una carpeta existente: {error}"))?;
    if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
        return Err("La ubicación elegida debe ser una carpeta existente.".into());
    }

    let install_dir = validated_parent.join("WarCrafted WotLK");
    match fs::create_dir(&install_dir) {
        Ok(()) => Ok(install_dir),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let metadata = fs::symlink_metadata(&install_dir).map_err(|error| {
                format!("No se pudo validar la carpeta WarCrafted WotLK: {error}")
            })?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(
                    "La ruta «WarCrafted WotLK» existe, pero no es una carpeta segura.".into(),
                );
            }
            let mut entries = fs::read_dir(&install_dir)
                .map_err(|error| format!("No se pudo leer la carpeta WarCrafted WotLK: {error}"))?;
            if entries.next().is_some() {
                return Err("Ya existe una carpeta «WarCrafted WotLK» con contenido en esa ubicación; elígela con «Elegir carpeta…» o usa otra ubicación.".into());
            }
            Ok(install_dir)
        }
        Err(error) => Err(format!(
            "No se pudo crear la carpeta WarCrafted WotLK: {error}"
        )),
    }
}

// En Windows canonicalize devuelve rutas «\\?\C:\...»; para mostrarlas se quita el prefijo
// salvo en rutas UNC, donde es necesario.
fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => rest.to_owned(),
        _ => text.into_owned(),
    }
}

#[tauri::command]
pub(crate) fn clear_cache(app: AppHandle) -> Result<(), String> {
    let client_dir = configured_client_dir(&app)?;
    clear_cache_for_client(&client_dir)
}

#[tauri::command]
pub(crate) async fn check_client_status(
    app: AppHandle,
    full: Option<bool>,
) -> Result<ClientStatusResponse, String> {
    CANCEL_OPERATION.store(false, Ordering::Relaxed);
    let client_dir = configured_client_dir(&app)?;
    let mode = if full.unwrap_or(false) {
        VerifyMode::Full
    } else {
        VerifyMode::Quick
    };
    let snapshot = load_client_snapshot(&app, &client_dir, mode).await?;
    let pending_bytes = pending_required_bytes(&snapshot.manifest, &snapshot.report);
    let files = snapshot
        .manifest
        .files
        .iter()
        .zip(&snapshot.report)
        .map(|(file, verification)| to_client_file_status(file.role, verification))
        .collect();
    let installed =
        fs::metadata(client_dir.join(GAME_EXECUTABLE)).is_ok_and(|metadata| metadata.is_file());
    if CANCEL_OPERATION.load(Ordering::Relaxed) {
        return Err("Operación cancelada.".into());
    }
    Ok(ClientStatusResponse {
        files,
        installed,
        pending_bytes,
        free_bytes: disk_space::free_bytes(&client_dir).ok(),
    })
}

#[tauri::command]
pub(crate) async fn update_client(app: AppHandle) -> Result<(), String> {
    if GAME_RUNNING.load(Ordering::Acquire) {
        return Err("Cierra el juego antes de actualizar: sus archivos están en uso.".into());
    }
    CANCEL_OPERATION.store(false, Ordering::Relaxed);
    let client_dir = configured_client_dir(&app)?;
    let mut snapshot = load_client_snapshot(&app, &client_dir, VerifyMode::Quick).await?;
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

    let pending_bytes = pending.iter().fold(0u64, |sum, file| sum.saturating_add(file.size_bytes));
    disk_space::ensure_enough(pending_bytes, disk_space::free_bytes(&client_dir)?)?;

    let install_root = prepare_install_root(&client_dir)?;
    let staging_directory = create_staging_directory(&install_root)?;
    let client = reqwest::Client::builder()
        .build()
        .map_err(|error| format!("no se pudo preparar el cliente HTTP: {error}"))?;
    let total = pending.len();
    let mut failures = Vec::new();

    for (index, file) in pending.iter().enumerate() {
        if CANCEL_OPERATION.load(Ordering::Relaxed) {
            return Err("Operación cancelada.".into());
        }
        let result =
            install_manifest_file(&client, file, &install_root, &staging_directory.0).await;
        let (status, message) = match result {
            Ok(()) => {
                match installed_file_cache_state(&install_root, file) {
                    Some(state) => {
                        snapshot.cache.insert(file.path.clone(), state);
                    }
                    None => {
                        snapshot.cache.remove(&file.path);
                    }
                }
                ("ok", None)
            }
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

    if CANCEL_OPERATION.load(Ordering::Relaxed) {
        return Err("Operación cancelada.".into());
    }

    verify_cache::save_cache(&snapshot.cache_directory, &client_dir, &snapshot.cache)?;

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
pub(crate) fn get_disk_space(app: AppHandle) -> Result<DiskSpaceResponse, String> {
    let client_dir = configured_client_dir(&app)?;
    Ok(DiskSpaceResponse {
        free_bytes: disk_space::free_bytes(&client_dir)?,
    })
}

fn pending_required_bytes(manifest: &Manifest, report: &[FileVerification]) -> u64 {
    manifest
        .files
        .iter()
        .zip(report)
        .filter(|(file, verification)| {
            file.role == FileRole::Required && verification.status != FileStatus::Valid
        })
        .fold(0u64, |sum, (file, _)| sum.saturating_add(file.size_bytes))
}

#[tauri::command]
pub(crate) async fn launch_game(app: AppHandle) -> Result<(), String> {
    if !try_reserve_game(&GAME_RUNNING) {
        return Err("El juego ya está en ejecución.".into());
    }
    let mut reservation = GameReservation::new();
    CANCEL_OPERATION.store(false, Ordering::Relaxed);
    let client_dir = configured_client_dir(&app)?;
    let snapshot = load_client_snapshot(&app, &client_dir, VerifyMode::Quick).await?;
    if CANCEL_OPERATION.load(Ordering::Relaxed) {
        return Err("Operación cancelada.".into());
    }
    match decide_launch(&snapshot.manifest, &snapshot.report, GAME_EXECUTABLE) {
        LaunchDecision::Blocked(paths) => {
            return Err(format!(
                "No se puede iniciar el juego; archivos obligatorios no válidos: {}",
                paths.join(", ")
            ));
        }
        LaunchDecision::Allowed => {}
    }

    let normalized_executable = normalize_manifest_path(GAME_EXECUTABLE)
        .map_err(|error| format!("ruta de ejecutable inválida: {error}"))?;
    let install_root = fs::canonicalize(&client_dir)
        .map_err(|error| format!("no se pudo abrir el directorio del cliente: {error}"))?;
    let executable_path = resolve_manifest_path(&install_root, &normalized_executable)
        .map_err(|error| error.to_string())?;
    validate_install_path(&install_root, &executable_path, false)?;
    let metadata = fs::symlink_metadata(&executable_path)
        .map_err(|error| format!("no se pudo acceder al ejecutable: {error}"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("el ejecutable no es un archivo normal del cliente".into());
    }

    if CANCEL_OPERATION.load(Ordering::Relaxed) {
        return Err("Operación cancelada.".into());
    }
    let child = Command::new(&executable_path)
        .current_dir(&install_root)
        .spawn()
        .map_err(|error| format!("no se pudo iniciar el juego: {error}"))?;

    let app = app.clone();
    std::thread::spawn(move || {
        let mut child = child;
        let _ = child.wait();
        reservation.release();
        let _ = app.emit("game-exited", ());
    });
    Ok(())
}

#[tauri::command]
pub(crate) fn get_game_running() -> bool {
    GAME_RUNNING.load(Ordering::Acquire)
}

#[tauri::command]
pub(crate) fn cancel_operation() {
    CANCEL_OPERATION.store(true, Ordering::Relaxed);
}

fn settings_directory(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map_err(|error| format!("No se pudo localizar la carpeta de ajustes: {error}"))
}

fn configured_client_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let config_dir = settings_directory(app)?;
    let launcher_settings = settings::load_settings(&config_dir)?;
    let path = launcher_settings
        .client_dir
        .ok_or_else(|| "Elige primero la carpeta del cliente.".to_owned())?;
    settings::validate_client_dir(&path)
}

async fn load_client_snapshot(
    app: &AppHandle,
    install_dir: &Path,
    mode: VerifyMode,
) -> Result<ClientSnapshot, String> {
    let manifest = fetch_manifest().await?;
    let install_root = install_dir.to_path_buf();
    let manifest_for_verification = manifest.clone();
    let cache_directory = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("No se pudo localizar la caché de verificación: {error}"))?;
    let mut cache = verify_cache::load_cache(&cache_directory, install_dir)?;
    let app = app.clone();
    let (report, cache) = tokio::task::spawn_blocking(move || {
        let mut last_emitted_at = None;
        let mut last_index = 0;
        let mut last_emitted_progress = None;
        let report = integrity::verify_manifest_files_with_progress(
            &manifest_for_verification,
            &install_root,
            &CANCEL_OPERATION,
            mode,
            &mut cache,
            |progress| {
                let first_for_file = progress.index != last_index;
                let last_for_file = progress.file_bytes_done >= progress.file_bytes_total;
                let interval_elapsed = last_emitted_at
                    .map(|time: Instant| time.elapsed() >= Duration::from_millis(100))
                    .unwrap_or(true);
                let current_progress = (
                    progress.index,
                    progress.file_bytes_done,
                    progress.bytes_done,
                );
                if (first_for_file || last_for_file || interval_elapsed)
                    && last_emitted_progress != Some(current_progress)
                {
                    let _ = app.emit(
                        "verify-progress",
                        VerifyProgressPayload {
                            index: progress.index,
                            total: progress.total,
                            path: progress.path.clone(),
                            file_bytes_done: progress.file_bytes_done,
                            file_bytes_total: progress.file_bytes_total,
                            bytes_done: progress.bytes_done,
                            bytes_total: progress.bytes_total,
                        },
                    );
                    last_emitted_at = Some(Instant::now());
                    last_emitted_progress = Some(current_progress);
                }
                last_index = progress.index;
            },
        )?;
        Ok::<_, integrity::IntegrityError>((report, cache))
    })
    .await
    .map_err(|error| format!("falló la tarea de verificación de integridad: {error}"))?
    .map_err(|error| error.to_string())?;

    if CANCEL_OPERATION.load(Ordering::Relaxed) {
        return Err("Operación cancelada.".into());
    }
    verify_cache::save_cache(&cache_directory, install_dir, &cache)?;

    Ok(ClientSnapshot {
        manifest,
        report,
        cache,
        cache_directory,
    })
}

fn installed_file_cache_state(
    install_root: &Path,
    file: &crate::update_engine::manifest::ManifestFile,
) -> Option<CachedFileState> {
    let path = resolve_manifest_path(install_root, &file.path).ok()?;
    let metadata = fs::metadata(path).ok()?;
    let modified_unix_nanos = metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(CachedFileState {
        size: metadata.len(),
        modified_unix_nanos,
        sha256: file.sha256.clone(),
    })
}

fn clear_cache_for_client(client_dir: &Path) -> Result<(), String> {
    let client_metadata = match fs::symlink_metadata(client_dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "No se pudo acceder a la carpeta del cliente: {error}"
            ))
        }
    };
    if client_metadata.file_type().is_symlink() {
        return Err("La carpeta del cliente no puede ser un enlace simbólico.".into());
    }
    if !client_metadata.is_dir() {
        return Err("La ruta del cliente no es una carpeta.".into());
    }

    let canonical_client = fs::canonicalize(client_dir)
        .map_err(|error| format!("No se pudo validar la carpeta del cliente: {error}"))?;
    let cache_dir = canonical_client.join("Cache");
    let cache_metadata = match fs::symlink_metadata(&cache_dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "No se pudo acceder a la caché del cliente: {error}"
            ))
        }
    };
    if cache_metadata.file_type().is_symlink() {
        return Err("La caché del cliente no puede ser un enlace simbólico.".into());
    }
    if !cache_metadata.is_dir() {
        return Err("La ruta Cache existe, pero no es una carpeta.".into());
    }

    let canonical_cache = fs::canonicalize(&cache_dir)
        .map_err(|error| format!("No se pudo validar la caché del cliente: {error}"))?;
    if canonical_cache == canonical_client || !canonical_cache.starts_with(&canonical_client) {
        return Err("La ruta de caché escapa de la carpeta del cliente.".into());
    }

    fs::remove_dir_all(canonical_cache)
        .map_err(|error| format!("No se pudo borrar la caché del cliente: {error}"))
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
        FileStatus::Corrupt { actual, .. } => (
            ClientFileState::Corrupt,
            Some(if actual.starts_with("tamaño") {
                format!("Versión distinta de la requerida ({actual}).")
            } else {
                "Contenido modificado o dañado.".to_owned()
            }),
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
    use std::sync::atomic::AtomicU64;

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn game_reservation_rejects_a_second_launch_until_released() {
        let game_running = AtomicBool::new(false);

        assert!(try_reserve_game(&game_running));
        assert!(!try_reserve_game(&game_running));

        release_game(&game_running);

        assert!(try_reserve_game(&game_running));
    }

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "warcrafted-ui-commands-test-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("crear directorio de prueba");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

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

    #[test]
    fn display_path_strips_only_local_verbatim_prefix() {
        assert_eq!(display_path(Path::new(r"\\?\C:\Wow")), r"C:\Wow");
        assert_eq!(
            display_path(Path::new(r"\\?\UNC\server\share")),
            r"\\?\UNC\server\share"
        );
        assert_eq!(display_path(Path::new("/home/wow")), "/home/wow");
    }

    #[test]
    fn create_install_dir_creates_the_named_child() {
        let directory = TestDirectory::new();

        let install_dir = create_install_dir_for_parent(&directory.0).expect("crear instalación");

        assert_eq!(
            install_dir.canonicalize().unwrap(),
            directory.0.join("WarCrafted WotLK").canonicalize().unwrap()
        );
        assert!(install_dir.is_dir());
    }

    #[test]
    fn create_install_dir_reuses_an_existing_empty_child() {
        let directory = TestDirectory::new();
        let install_dir = directory.0.join("WarCrafted WotLK");
        fs::create_dir(&install_dir).expect("crear carpeta vacía");

        assert_eq!(
            create_install_dir_for_parent(&directory.0)
                .expect("reutilizar carpeta vacía")
                .canonicalize()
                .unwrap(),
            install_dir.canonicalize().unwrap()
        );
    }

    #[test]
    fn create_install_dir_rejects_an_existing_nonempty_child() {
        let directory = TestDirectory::new();
        let install_dir = directory.0.join("WarCrafted WotLK");
        fs::create_dir(&install_dir).expect("crear carpeta");
        fs::write(install_dir.join("existing.file"), b"data").expect("crear contenido");

        let error = create_install_dir_for_parent(&directory.0).unwrap_err();

        assert_eq!(error, "Ya existe una carpeta «WarCrafted WotLK» con contenido en esa ubicación; elígela con «Elegir carpeta…» o usa otra ubicación.");
    }

    #[test]
    fn create_install_dir_rejects_relative_or_missing_parents() {
        assert!(create_install_dir_for_parent(Path::new("relative/parent")).is_err());

        let directory = TestDirectory::new();
        assert!(create_install_dir_for_parent(&directory.0.join("missing-parent")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn create_install_dir_rejects_a_file_or_symbolic_link_at_the_child_path() {
        use std::os::unix::fs::symlink;

        let directory = TestDirectory::new();
        let child = directory.0.join("WarCrafted WotLK");
        fs::write(&child, b"file").expect("crear archivo");
        assert!(create_install_dir_for_parent(&directory.0).is_err());
        fs::remove_file(&child).expect("borrar archivo");

        let target = directory.0.join("target");
        fs::create_dir(&target).expect("crear destino");
        symlink(&target, &child).expect("crear enlace simbólico");
        assert!(create_install_dir_for_parent(&directory.0).is_err());
    }

    #[test]
    fn clear_cache_removes_only_cache_and_keeps_other_client_directories() {
        let directory = TestDirectory::new();
        let client = directory.0.join("client");
        fs::create_dir_all(client.join("Cache/WDB")).expect("crear caché");
        fs::create_dir_all(client.join("WTF")).expect("crear WTF");
        fs::create_dir_all(client.join("Data")).expect("crear Data");
        fs::write(client.join("Cache/WDB/file"), b"cache").expect("crear archivo de caché");
        fs::write(client.join("WTF/config.wtf"), b"config").expect("crear ajustes del juego");
        fs::write(client.join("Data/patch.MPQ"), b"patch").expect("crear datos del cliente");

        clear_cache_for_client(&client).expect("borrar caché");

        assert!(!client.join("Cache").exists());
        assert!(client.join("WTF/config.wtf").is_file());
        assert!(client.join("Data/patch.MPQ").is_file());
    }

    #[test]
    fn clear_cache_succeeds_when_cache_or_client_is_missing() {
        let directory = TestDirectory::new();
        let client = directory.0.join("client");
        fs::create_dir(&client).expect("crear cliente");

        clear_cache_for_client(&client).expect("caché ausente");
        clear_cache_for_client(&directory.0.join("missing-client")).expect("cliente ausente");
    }

    #[cfg(unix)]
    #[test]
    fn clear_cache_rejects_a_symbolic_link_without_removing_its_target() {
        use std::os::unix::fs::symlink;

        let directory = TestDirectory::new();
        let client = directory.0.join("client");
        let cache_target = directory.0.join("cache-target");
        fs::create_dir(&client).expect("crear cliente");
        fs::create_dir(&cache_target).expect("crear destino de caché");
        fs::write(cache_target.join("keep"), b"keep").expect("crear archivo destino");
        symlink(&cache_target, client.join("Cache")).expect("crear enlace de caché");

        assert!(clear_cache_for_client(&client).is_err());
        assert_eq!(
            fs::read(cache_target.join("keep")).expect("leer destino"),
            b"keep"
        );
    }
}

//! Descarga y verifica archivos dentro del directorio temporal de staging.

use std::{
    error::Error,
    ffi::OsString,
    fmt, fs as std_fs,
    io::{self, Seek, SeekFrom},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use reqwest::{header, Client, StatusCode};
use sha2::{Digest, Sha256};
use tokio::{
    fs::{self, File, OpenOptions},
    io::{AsyncReadExt, AsyncWriteExt},
    time,
};

use super::manifest::{resolve_manifest_path, FileSource, ManifestError, ManifestFile};

const HASH_BUFFER_SIZE: usize = 1024 * 1024;
const MAX_DOWNLOAD_ATTEMPTS: usize = 4;
static NEXT_ARCHIVE_DIRECTORY: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub enum StagingError {
    Manifest(ManifestError),
    Network(reqwest::Error),
    HttpStatus(u16),
    InvalidContentRange,
    IncompleteDownload {
        path: String,
        expected: u64,
        actual: u64,
    },
    SizeMismatch {
        path: String,
        expected: u64,
        actual: u64,
    },
    HashMismatch {
        path: String,
        expected: String,
        actual: String,
    },
    InvalidSourceSize {
        path: String,
        file_size: u64,
        source_size: u64,
    },
    UnsafePath(PathBuf),
    UnsafeArchiveEntry(PathBuf),
    Filesystem(io::Error),
}

impl fmt::Display for StagingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Manifest(error) => write!(formatter, "ruta de manifest inválida: {error}"),
            Self::Network(error) => write!(formatter, "falló la descarga: {error}"),
            Self::HttpStatus(status) => write!(formatter, "el servidor respondió HTTP {status}"),
            Self::InvalidContentRange => {
                write!(formatter, "el servidor respondió un Content-Range inválido")
            }
            Self::IncompleteDownload {
                path,
                expected,
                actual,
            } => write!(
                formatter,
                "descarga incompleta de {path}: se esperaban {expected} bytes y llegaron {actual}"
            ),
            Self::SizeMismatch {
                path,
                expected,
                actual,
            } => write!(
                formatter,
                "tamaño incorrecto de {path}: se esperaban {expected} bytes y llegaron {actual}"
            ),
            Self::HashMismatch {
                path,
                expected,
                actual,
            } => write!(
                formatter,
                "hash SHA-256 incorrecto de {path}: esperado {expected}, recibido {actual}"
            ),
            Self::InvalidSourceSize {
                path,
                file_size,
                source_size,
            } => write!(
                formatter,
                "tamaños incompatibles en {path}: manifest {file_size}, origen {source_size}"
            ),
            Self::UnsafePath(path) => write!(
                formatter,
                "la ruta de staging contiene un enlace simbólico o escapa del directorio: {}",
                path.display()
            ),
            Self::UnsafeArchiveEntry(path) => write!(
                formatter,
                "el archivo TAR contiene una entrada no permitida: {}",
                path.display()
            ),
            Self::Filesystem(error) => write!(formatter, "falló el filesystem de staging: {error}"),
        }
    }
}

impl Error for StagingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Manifest(error) => Some(error),
            Self::Network(error) => Some(error),
            Self::Filesystem(error) => Some(error),
            _ => None,
        }
    }
}

/// Descarga y verifica un archivo dentro de `staging_root`; nunca escribe en la instalación.
pub async fn stage_manifest_file(
    client: &Client,
    manifest_file: &ManifestFile,
    staging_root: &Path,
) -> Result<PathBuf, StagingError> {
    stage_manifest_file_with_progress(client, manifest_file, staging_root, &mut |_: u64| {}).await
}

/// Igual que [`stage_manifest_file`], informando de los bytes de cada bloque descargado.
///
/// `on_bytes` recibe la longitud de cada bloque escrito, también al descargar los fragmentos
/// de un archivo ensamblado. Las funciones existentes siguen usando [`stage_manifest_file`].
pub async fn stage_manifest_file_with_progress(
    client: &Client,
    manifest_file: &ManifestFile,
    staging_root: &Path,
    on_bytes: &mut (dyn FnMut(u64) + Send),
) -> Result<PathBuf, StagingError> {
    fs::create_dir_all(staging_root)
        .await
        .map_err(StagingError::Filesystem)?;
    let staging_root = fs::canonicalize(staging_root)
        .await
        .map_err(StagingError::Filesystem)?;
    let target = resolve_manifest_path(&staging_root, &manifest_file.path)
        .map_err(StagingError::Manifest)?;
    ensure_safe_file_path(&staging_root, &target)
        .await
        .map_err(map_path_error)?;

    match (&manifest_file.source, &manifest_file.assembly) {
        (Some(source), None) => {
            ensure_source_size(&manifest_file.path, manifest_file.size_bytes, source)?;
            download_verified(
                client,
                &staging_root,
                source,
                &target,
                &manifest_file.path,
                manifest_file.size_bytes,
                &manifest_file.sha256,
                on_bytes,
            )
            .await?;
        }
        (None, Some(assembly)) => {
            let mut parts = Vec::with_capacity(assembly.parts.len());
            for (index, part) in assembly.parts.iter().enumerate() {
                ensure_source_size(
                    &format!("{} (fragmento {})", manifest_file.path, index + 1),
                    part.size_bytes,
                    &part.source,
                )?;
                let part_path = sidecar_path(&target, &format!("part-{index:06}"));
                ensure_safe_file_path(&staging_root, &part_path)
                    .await
                    .map_err(map_path_error)?;
                download_verified(
                    client,
                    &staging_root,
                    &part.source,
                    &part_path,
                    &format!("{} (fragmento {})", manifest_file.path, index + 1),
                    part.size_bytes,
                    &part.sha256,
                    on_bytes,
                )
                .await?;
                parts.push(part_path);
            }

            let assembling_path = sidecar_path(&target, "assembling");
            ensure_safe_file_path(&staging_root, &assembling_path)
                .await
                .map_err(map_path_error)?;
            concatenate_parts(&parts, &assembling_path).await?;
            let actual = hash_file(&assembling_path).await?;
            if !actual.eq_ignore_ascii_case(&manifest_file.sha256) {
                let _ = fs::remove_file(&assembling_path).await;
                return Err(StagingError::HashMismatch {
                    path: manifest_file.path.clone(),
                    expected: manifest_file.sha256.clone(),
                    actual,
                });
            }
            let actual_size = fs::metadata(&assembling_path)
                .await
                .map_err(StagingError::Filesystem)?
                .len();
            if actual_size != manifest_file.size_bytes {
                let _ = fs::remove_file(&assembling_path).await;
                return Err(StagingError::SizeMismatch {
                    path: manifest_file.path.clone(),
                    expected: manifest_file.size_bytes,
                    actual: actual_size,
                });
            }
            promote_file(&assembling_path, &target).await?;
            for part in parts {
                let _ = fs::remove_file(part).await;
            }
        }
        (Some(_), Some(_)) | (None, None) => {
            return Err(StagingError::Manifest(ManifestError::InvalidField(
                "el archivo debe declarar exactamente uno de source/assembly".into(),
            )))
        }
    }

    Ok(target)
}

/// Extrae un TAR verificado y sustituye sus directorios de primer nivel.
pub fn extract_archive(archive_path: &Path, destination_root: &Path) -> Result<(), StagingError> {
    let metadata = std_fs::symlink_metadata(archive_path).map_err(StagingError::Filesystem)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(StagingError::UnsafePath(archive_path.to_path_buf()));
    }

    let mut archive =
        tar::Archive::new(std_fs::File::open(archive_path).map_err(StagingError::Filesystem)?);
    let directories = validate_archive_entries(&mut archive)?;
    let mut archive_file = archive.into_inner();
    archive_file
        .seek(SeekFrom::Start(0))
        .map_err(StagingError::Filesystem)?;
    let mut archive = tar::Archive::new(archive_file);

    let destination = prepare_archive_destination(destination_root)?;
    let extraction_root = create_archive_staging_directory(&destination)?;

    let result = extract_archive_entries(&mut archive, &extraction_root)
        .and_then(|()| promote_archive_directories(&extraction_root, &destination, &directories));
    let cleanup = std_fs::remove_dir_all(&extraction_root).map_err(StagingError::Filesystem);

    match (result, cleanup) {
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

fn validate_archive_entries(
    archive: &mut tar::Archive<std_fs::File>,
) -> Result<Vec<OsString>, StagingError> {
    let archive_root = Path::new("/");
    let mut directories = Vec::new();
    for entry in archive.entries().map_err(StagingError::Filesystem)? {
        let entry = entry.map_err(StagingError::Filesystem)?;
        let path = entry.path().map_err(StagingError::Filesystem)?;
        let entry_type = entry.header().entry_type();
        let Some(resolved) = resolve_archive_entry_path(archive_root, &path, entry_type)? else {
            continue;
        };
        let relative = resolved
            .strip_prefix(archive_root)
            .map_err(|_| StagingError::UnsafeArchiveEntry(path.to_path_buf()))?;
        let mut components = relative.components();
        let Some(Component::Normal(name)) = components.next() else {
            return Err(StagingError::UnsafeArchiveEntry(path.to_path_buf()));
        };
        if components.next().is_none() && !entry_type.is_dir() {
            return Err(StagingError::UnsafeArchiveEntry(path.to_path_buf()));
        }
        let name = name.to_os_string();
        if !directories.contains(&name) {
            directories.push(name);
        }
    }
    if directories.is_empty() {
        return Err(StagingError::UnsafeArchiveEntry(PathBuf::from(".")));
    }
    Ok(directories)
}

fn extract_archive_entries(
    archive: &mut tar::Archive<std_fs::File>,
    extraction_root: &Path,
) -> Result<(), StagingError> {
    for entry in archive.entries().map_err(StagingError::Filesystem)? {
        let mut entry = entry.map_err(StagingError::Filesystem)?;
        let path = entry.path().map_err(StagingError::Filesystem)?;
        let entry_type = entry.header().entry_type();
        let Some(target) = resolve_archive_entry_path(extraction_root, &path, entry_type)? else {
            continue;
        };

        if entry_type.is_dir() {
            std_fs::create_dir_all(&target).map_err(StagingError::Filesystem)?;
            continue;
        }

        let parent = target
            .parent()
            .ok_or_else(|| StagingError::UnsafeArchiveEntry(path.to_path_buf()))?;
        std_fs::create_dir_all(parent).map_err(StagingError::Filesystem)?;
        let mut output = std_fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(StagingError::Filesystem)?;
        io::copy(&mut entry, &mut output).map_err(StagingError::Filesystem)?;
        output.sync_all().map_err(StagingError::Filesystem)?;
    }
    Ok(())
}

fn resolve_archive_entry_path(
    root: &Path,
    path: &Path,
    entry_type: tar::EntryType,
) -> Result<Option<PathBuf>, StagingError> {
    if entry_type.is_symlink() || entry_type.is_hard_link() {
        return Err(StagingError::UnsafeArchiveEntry(path.to_path_buf()));
    }
    if !entry_type.is_file() && !entry_type.is_dir() {
        return Err(StagingError::UnsafeArchiveEntry(path.to_path_buf()));
    }

    let raw_path = path
        .to_str()
        .ok_or_else(|| StagingError::UnsafeArchiveEntry(path.to_path_buf()))?;
    if matches!(raw_path, "." | "./") {
        return if entry_type.is_dir() {
            Ok(None)
        } else {
            Err(StagingError::UnsafeArchiveEntry(path.to_path_buf()))
        };
    }

    resolve_manifest_path(root, raw_path)
        .map(Some)
        .map_err(|_| StagingError::UnsafeArchiveEntry(path.to_path_buf()))
}

fn prepare_archive_destination(destination_root: &Path) -> Result<PathBuf, StagingError> {
    let file_name = destination_root
        .file_name()
        .ok_or_else(|| StagingError::UnsafePath(destination_root.to_path_buf()))?;
    let requested_parent = destination_root
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std_fs::create_dir_all(requested_parent).map_err(StagingError::Filesystem)?;
    let parent = std_fs::canonicalize(requested_parent).map_err(StagingError::Filesystem)?;
    let destination = parent.join(file_name);

    match std_fs::symlink_metadata(&destination) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(StagingError::UnsafePath(destination));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(StagingError::Filesystem(error)),
    }

    Ok(destination)
}

fn create_archive_staging_directory(destination: &Path) -> Result<PathBuf, StagingError> {
    for _ in 0..32 {
        let candidate = archive_sidecar_path(destination, "staging");
        match std_fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(StagingError::Filesystem(error)),
        }
    }
    Err(StagingError::Filesystem(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "no se pudo reservar un directorio temporal para extraer el TAR",
    )))
}

fn archive_sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut name = OsString::from(".");
    name.push(path.file_name().unwrap_or_default());
    name.push(format!(
        ".warcrafted-{suffix}-{}-{}",
        std::process::id(),
        NEXT_ARCHIVE_DIRECTORY.fetch_add(1, Ordering::Relaxed)
    ));
    path.with_file_name(name)
}

fn promote_archive_directories(
    extraction_root: &Path,
    destination: &Path,
    directories: &[OsString],
) -> Result<(), StagingError> {
    let destination_exists = match std_fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(StagingError::UnsafePath(destination.to_path_buf()))
        }
        Ok(_) => true,
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => return Err(StagingError::Filesystem(error)),
    };

    // Comprueba todos los destinos antes de mover ninguno para rechazar archivos y enlaces sin
    // alterar una instalación parcialmente.
    if destination_exists {
        for name in directories {
            let target = destination.join(name);
            match std_fs::symlink_metadata(&target) {
                Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                    return Err(StagingError::UnsafePath(target));
                }
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(StagingError::Filesystem(error)),
            }
        }
    } else {
        std_fs::create_dir(destination).map_err(StagingError::Filesystem)?;
    }

    let mut promoted = Vec::new();
    for name in directories {
        let staged = extraction_root.join(name);
        let target = destination.join(name);
        let backup = match std_fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                let rollback_error = rollback_archive_directories(&mut promoted);
                remove_created_archive_destination(destination, destination_exists);
                if let Some(error) = rollback_error {
                    return Err(StagingError::Filesystem(error));
                }
                return Err(StagingError::UnsafePath(target));
            }
            Ok(_) => {
                let backup = archive_sidecar_path(&target, "backup");
                if let Err(error) = std_fs::rename(&target, &backup) {
                    let rollback_error = rollback_archive_directories(&mut promoted);
                    remove_created_archive_destination(destination, destination_exists);
                    if let Some(error) = rollback_error {
                        return Err(StagingError::Filesystem(error));
                    }
                    return Err(StagingError::Filesystem(error));
                }
                Some(backup)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => {
                let rollback_error = rollback_archive_directories(&mut promoted);
                remove_created_archive_destination(destination, destination_exists);
                if let Some(error) = rollback_error {
                    return Err(StagingError::Filesystem(error));
                }
                return Err(StagingError::Filesystem(error));
            }
        };

        if let Err(error) = std_fs::rename(&staged, &target) {
            let current_rollback_error = backup
                .as_ref()
                .and_then(|backup| std_fs::rename(backup, &target).err());
            let rollback_error =
                rollback_archive_directories(&mut promoted).or(current_rollback_error);
            remove_created_archive_destination(destination, destination_exists);
            if let Some(error) = rollback_error {
                return Err(StagingError::Filesystem(error));
            }
            return Err(StagingError::Filesystem(error));
        }
        promoted.push(PromotedArchiveDirectory {
            staged,
            target,
            backup,
        });
    }

    let mut cleanup_error = None;
    for directory in promoted {
        if let Some(backup) = directory.backup {
            if let Err(error) = std_fs::remove_dir_all(backup) {
                if cleanup_error.is_none() {
                    cleanup_error = Some(error);
                }
            }
        }
    }
    if let Some(error) = cleanup_error {
        return Err(StagingError::Filesystem(error));
    }
    Ok(())
}

struct PromotedArchiveDirectory {
    staged: PathBuf,
    target: PathBuf,
    backup: Option<PathBuf>,
}

fn rollback_archive_directories(promoted: &mut Vec<PromotedArchiveDirectory>) -> Option<io::Error> {
    let mut rollback_error = None;
    for directory in promoted.drain(..).rev() {
        if let Err(error) = std_fs::rename(&directory.target, &directory.staged) {
            if rollback_error.is_none() {
                rollback_error = Some(error);
            }
        }
        if let Some(backup) = directory.backup {
            if let Err(error) = std_fs::rename(backup, directory.target) {
                if rollback_error.is_none() {
                    rollback_error = Some(error);
                }
            }
        }
    }
    rollback_error
}

fn remove_created_archive_destination(destination: &Path, existed: bool) {
    if !existed {
        let _ = std_fs::remove_dir(destination);
    }
}

fn ensure_source_size(path: &str, file_size: u64, source: &FileSource) -> Result<(), StagingError> {
    if source.compressed_size_bytes != file_size {
        return Err(StagingError::InvalidSourceSize {
            path: path.to_owned(),
            file_size,
            source_size: source.compressed_size_bytes,
        });
    }
    Ok(())
}

async fn download_verified(
    client: &Client,
    staging_root: &Path,
    source: &FileSource,
    destination: &Path,
    display_path: &str,
    expected_size: u64,
    expected_hash: &str,
    on_bytes: &mut (dyn FnMut(u64) + Send),
) -> Result<(), StagingError> {
    let partial_path = sidecar_path(destination, "download");
    ensure_safe_file_path(staging_root, &partial_path)
        .await
        .map_err(map_path_error)?;

    for attempt in 0..MAX_DOWNLOAD_ATTEMPTS {
        match download_attempt(
            client,
            source,
            &partial_path,
            display_path,
            expected_size,
            on_bytes,
        )
        .await
        {
            Ok(()) => {
                let actual = hash_file(&partial_path).await?;
                if !actual.eq_ignore_ascii_case(expected_hash) {
                    let _ = fs::remove_file(&partial_path).await;
                    return Err(StagingError::HashMismatch {
                        path: display_path.to_owned(),
                        expected: expected_hash.to_owned(),
                        actual,
                    });
                }
                promote_file(&partial_path, destination).await?;
                return Ok(());
            }
            Err(error) if error.is_retryable() && attempt + 1 < MAX_DOWNLOAD_ATTEMPTS => {
                time::sleep(Duration::from_secs(1 << attempt)).await;
            }
            Err(error) => return Err(error),
        }
    }

    unreachable!("el bucle siempre devuelve éxito o un error")
}

async fn download_attempt(
    client: &Client,
    source: &FileSource,
    partial_path: &Path,
    display_path: &str,
    expected_size: u64,
    on_bytes: &mut (dyn FnMut(u64) + Send),
) -> Result<(), StagingError> {
    let mut offset = match fs::metadata(partial_path).await {
        Ok(metadata) if metadata.is_file() => metadata.len(),
        Ok(_) => return Err(StagingError::UnsafePath(partial_path.to_path_buf())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
        Err(error) => return Err(StagingError::Filesystem(error)),
    };
    if offset > expected_size {
        truncate_file(partial_path).await?;
        offset = 0;
    }

    let mut request = client.get(&source.url);
    if offset > 0 {
        request = request.header(header::RANGE, format!("bytes={offset}-"));
    }
    let mut response = request.send().await.map_err(StagingError::Network)?;
    let status = response.status();
    if status == StatusCode::PARTIAL_CONTENT {
        if offset == 0 || !content_range_starts_at(&response, offset) {
            truncate_file(partial_path).await?;
            return Err(StagingError::InvalidContentRange);
        }
    } else if offset > 0 && status == StatusCode::OK {
        truncate_file(partial_path).await?;
        offset = 0;
    } else if status != StatusCode::OK {
        return Err(StagingError::HttpStatus(status.as_u16()));
    }

    let mut output = OpenOptions::new()
        .create(true)
        .write(true)
        .append(offset > 0)
        .truncate(offset == 0)
        .open(partial_path)
        .await
        .map_err(StagingError::Filesystem)?;
    let mut total = offset;
    loop {
        let chunk = match response.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(error) => {
                output.flush().await.map_err(StagingError::Filesystem)?;
                return Err(StagingError::Network(error));
            }
        };
        let next_total = total.saturating_add(chunk.len() as u64);
        if next_total > expected_size {
            output.flush().await.map_err(StagingError::Filesystem)?;
            drop(output);
            truncate_file(partial_path).await?;
            return Err(StagingError::SizeMismatch {
                path: display_path.to_owned(),
                expected: expected_size,
                actual: next_total,
            });
        }
        let chunk_len = chunk.len() as u64;
        output
            .write_all(&chunk)
            .await
            .map_err(StagingError::Filesystem)?;
        total = next_total;
        on_bytes(chunk_len);
    }
    output.flush().await.map_err(StagingError::Filesystem)?;
    output.sync_all().await.map_err(StagingError::Filesystem)?;
    if total != expected_size {
        return Err(StagingError::IncompleteDownload {
            path: display_path.to_owned(),
            expected: expected_size,
            actual: total,
        });
    }
    Ok(())
}

fn content_range_starts_at(response: &reqwest::Response, offset: u64) -> bool {
    response
        .headers()
        .get(header::CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("bytes "))
        .and_then(|value| value.split_once('-'))
        .and_then(|(start, _)| start.parse::<u64>().ok())
        == Some(offset)
}

async fn concatenate_parts(parts: &[PathBuf], destination: &Path) -> Result<(), StagingError> {
    let mut output = File::create(destination)
        .await
        .map_err(StagingError::Filesystem)?;
    let mut buffer = vec![0; HASH_BUFFER_SIZE];
    for part in parts {
        let mut input = File::open(part).await.map_err(StagingError::Filesystem)?;
        loop {
            let bytes_read = input
                .read(&mut buffer)
                .await
                .map_err(StagingError::Filesystem)?;
            if bytes_read == 0 {
                break;
            }
            output
                .write_all(&buffer[..bytes_read])
                .await
                .map_err(StagingError::Filesystem)?;
        }
    }
    output.flush().await.map_err(StagingError::Filesystem)?;
    output.sync_all().await.map_err(StagingError::Filesystem)?;
    Ok(())
}

async fn hash_file(path: &Path) -> Result<String, StagingError> {
    let mut file = File::open(path).await.map_err(StagingError::Filesystem)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; HASH_BUFFER_SIZE];
    loop {
        let bytes_read = file
            .read(&mut buffer)
            .await
            .map_err(StagingError::Filesystem)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    Ok(encode_hex(hasher.finalize().iter().copied()))
}

async fn truncate_file(path: &Path) -> Result<(), StagingError> {
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .await
        .map_err(StagingError::Filesystem)?
        .sync_all()
        .await
        .map_err(StagingError::Filesystem)
}

async fn promote_file(source: &Path, destination: &Path) -> Result<(), StagingError> {
    match fs::symlink_metadata(destination).await {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(StagingError::UnsafePath(destination.to_path_buf()))
        }
        Ok(_) => fs::remove_file(destination)
            .await
            .map_err(StagingError::Filesystem)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(StagingError::Filesystem(error)),
    }
    fs::rename(source, destination)
        .await
        .map_err(StagingError::Filesystem)
}

async fn ensure_safe_file_path(root: &Path, path: &Path) -> Result<(), PathSafetyError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| PathSafetyError::Unsafe(path.to_path_buf()))?;
    let components: Vec<_> = relative.components().collect();
    if components.is_empty()
        || components
            .iter()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(PathSafetyError::Unsafe(path.to_path_buf()));
    }

    let mut current = root.to_path_buf();
    for (index, component) in components.iter().enumerate() {
        current.push(component.as_os_str());
        let is_leaf = index + 1 == components.len();
        match fs::symlink_metadata(&current).await {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(PathSafetyError::Unsafe(current))
            }
            Ok(metadata) if !is_leaf && !metadata.is_dir() => {
                return Err(PathSafetyError::Unsafe(current))
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound && !is_leaf => {
                fs::create_dir(&current)
                    .await
                    .map_err(PathSafetyError::Filesystem)?;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(PathSafetyError::Filesystem(error)),
        }
    }
    Ok(())
}

enum PathSafetyError {
    Unsafe(PathBuf),
    Filesystem(io::Error),
}

fn map_path_error(error: PathSafetyError) -> StagingError {
    match error {
        PathSafetyError::Unsafe(path) => StagingError::UnsafePath(path),
        PathSafetyError::Filesystem(error) => StagingError::Filesystem(error),
    }
}

fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let name = path.file_name().unwrap_or_default();
    let mut sidecar_name = OsString::from(".");
    sidecar_name.push(name);
    sidecar_name.push(".");
    sidecar_name.push(suffix);
    path.with_file_name(sidecar_name)
}

impl StagingError {
    fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Network(_)
                | Self::HttpStatus(_)
                | Self::InvalidContentRange
                | Self::IncompleteDownload { .. }
        )
    }
}

fn encode_hex(bytes: impl IntoIterator<Item = u8>) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::new();
    for byte in bytes {
        encoded.push(DIGITS[(byte >> 4) as usize] as char);
        encoded.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    encoded
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use sha2::{Digest, Sha256};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
        task::JoinHandle,
    };

    use super::*;
    use crate::update_engine::manifest::{
        Assembly, AssemblyPart, Compression, FileKind, FileRole, FileSource,
    };

    static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let unique = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "warcrafted-staging-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn sha256(contents: &[u8]) -> String {
        encode_hex(Sha256::digest(contents).iter().copied())
    }

    fn source(url: String, size: usize) -> FileSource {
        FileSource {
            url,
            compressed_size_bytes: size as u64,
            compression: Compression::None,
        }
    }

    fn manifest_file(path: &str, contents: &[u8], source: FileSource) -> ManifestFile {
        ManifestFile {
            path: path.to_owned(),
            role: FileRole::Required,
            kind: FileKind::ClientBase,
            addon_group: None,
            size_bytes: contents.len() as u64,
            sha256: sha256(contents),
            source: Some(source),
            assembly: None,
        }
    }

    fn assembled_file(path: &str, parts: &[(&str, &[u8])], url: &str) -> ManifestFile {
        let contents: Vec<_> = parts
            .iter()
            .flat_map(|(_, bytes)| bytes.iter().copied())
            .collect();
        ManifestFile {
            path: path.to_owned(),
            role: FileRole::Required,
            kind: FileKind::ClientBase,
            addon_group: None,
            size_bytes: contents.len() as u64,
            sha256: sha256(&contents),
            source: None,
            assembly: Some(Assembly {
                part_size_bytes: parts
                    .iter()
                    .map(|(_, bytes)| bytes.len())
                    .max()
                    .unwrap_or(0) as u64,
                parts: parts
                    .iter()
                    .enumerate()
                    .map(|(index, (_, bytes))| AssemblyPart {
                        sha256: sha256(bytes),
                        size_bytes: bytes.len() as u64,
                        source: source(format!("{url}/part-{index}"), bytes.len()),
                    })
                    .collect(),
            }),
        }
    }

    async fn start_server(
        routes: HashMap<String, Vec<u8>>,
        request_count: usize,
    ) -> (String, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            for _ in 0..request_count {
                let (stream, _) = listener.accept().await.unwrap();
                serve_request(stream, &routes).await;
            }
        });
        (format!("http://{address}"), task)
    }

    async fn serve_request(mut stream: TcpStream, routes: &HashMap<String, Vec<u8>>) {
        let mut request = Vec::new();
        let mut buffer = [0; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let bytes_read = stream.read(&mut buffer).await.unwrap();
            assert_ne!(
                bytes_read, 0,
                "el cliente cerró antes de completar la petición"
            );
            request.extend_from_slice(&buffer[..bytes_read]);
        }
        let request_line = String::from_utf8_lossy(&request);
        let path = request_line
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap();
        let body = routes.get(path).expect("ruta de prueba registrada");
        let headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(headers.as_bytes()).await.unwrap();
        stream.write_all(body).await.unwrap();
    }

    fn client() -> Client {
        Client::builder().no_proxy().build().unwrap()
    }

    fn append_tar_file(builder: &mut tar::Builder<fs::File>, path: &str, contents: &[u8]) {
        let mut header = tar::Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(0o644);
        builder.append_data(&mut header, path, contents).unwrap();
    }

    fn create_tar(path: &Path, files: &[(&str, &[u8])]) {
        let file = fs::File::create(path).unwrap();
        let mut builder = tar::Builder::new(file);
        for (entry_path, contents) in files {
            append_tar_file(&mut builder, entry_path, contents);
        }
        builder.finish().unwrap();
    }

    fn create_tar_with_traversal(path: &Path) {
        create_tar(
            path,
            &[
                ("Safe/new-file.txt", b"new"),
                ("Safe/escape.txt", b"escape"),
            ],
        );
        let mut contents = fs::read(path).unwrap();
        let header = &mut contents[1024..1536];
        header[..100].fill(0);
        header[.."../escape.txt".len()].copy_from_slice(b"../escape.txt");
        header[148..156].fill(b' ');
        let checksum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
        let checksum = format!("{checksum:06o}\0 ");
        header[148..156].copy_from_slice(checksum.as_bytes());
        fs::write(path, contents).unwrap();
    }

    fn assert_no_archive_sidecars(root: &Path) {
        for entry in fs::read_dir(root).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().into_owned();
            assert!(
                !name.contains(".warcrafted-staging-"),
                "resto de staging: {name}"
            );
            assert!(
                !name.contains(".warcrafted-backup-"),
                "resto de backup: {name}"
            );
            if entry.file_type().unwrap().is_dir() {
                assert_no_archive_sidecars(&entry.path());
            }
        }
    }

    #[tokio::test]
    async fn downloads_file_and_verifies_its_sha256() {
        let body = b"small client file";
        let mut routes = HashMap::new();
        routes.insert("/file".to_owned(), body.to_vec());
        let (base_url, server) = start_server(routes, 1).await;
        let file = manifest_file(
            "Data/client.bin",
            body,
            source(format!("{base_url}/file"), body.len()),
        );
        let staging = TempDir::new();

        let result = stage_manifest_file(&client(), &file, &staging.0)
            .await
            .unwrap();

        assert_eq!(fs::read(result).unwrap(), body);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn rejects_a_download_with_the_wrong_sha256() {
        let body = b"unexpected file";
        let mut routes = HashMap::new();
        routes.insert("/file".to_owned(), body.to_vec());
        let (base_url, server) = start_server(routes, 1).await;
        let mut file = manifest_file(
            "Data/client.bin",
            body,
            source(format!("{base_url}/file"), body.len()),
        );
        file.sha256 = sha256(b"expected file");
        let staging = TempDir::new();

        let result = stage_manifest_file(&client(), &file, &staging.0).await;

        assert!(matches!(result, Err(StagingError::HashMismatch { .. })));
        assert!(!staging.0.join("Data/client.bin").exists());
        server.await.unwrap();
    }

    #[tokio::test]
    async fn downloads_and_assembles_verified_parts_in_manifest_order() {
        let pieces: [(&str, &[u8]); 3] = [
            ("/part-0", b"first-"),
            ("/part-1", b"second-"),
            ("/part-2", b"third"),
        ];
        let routes = pieces
            .iter()
            .map(|(path, contents)| ((*path).to_owned(), contents.to_vec()))
            .collect();
        let (base_url, server) = start_server(routes, pieces.len()).await;
        let file = assembled_file("Data/large.bin", &pieces, &base_url);
        let staging = TempDir::new();

        let result = stage_manifest_file(&client(), &file, &staging.0)
            .await
            .unwrap();

        assert_eq!(fs::read(result).unwrap(), b"first-second-third");
        server.await.unwrap();
    }

    #[test]
    fn extracts_archive_files_from_nested_directories() {
        let temp_dir = TempDir::new();
        let archive_path = temp_dir.0.join("addon.tar");
        create_tar(
            &archive_path,
            &[
                ("RuneEngraver/RuneEngraver.toc", b"toc"),
                ("RuneEngraver/Core/Init.lua", b"init"),
                ("RuneEngraver/Locales/enUS.lua", b"locale"),
            ],
        );
        let destination = temp_dir.0.join("installed/RuneEngraver");
        fs::create_dir_all(&destination).unwrap();
        fs::write(destination.join("old-file.txt"), b"old").unwrap();

        extract_archive(&archive_path, &destination).unwrap();

        assert_eq!(
            fs::read(destination.join("RuneEngraver/RuneEngraver.toc")).unwrap(),
            b"toc"
        );
        assert_eq!(
            fs::read(destination.join("RuneEngraver/Core/Init.lua")).unwrap(),
            b"init"
        );
        assert_eq!(
            fs::read(destination.join("RuneEngraver/Locales/enUS.lua")).unwrap(),
            b"locale"
        );
        assert_eq!(fs::read(destination.join("old-file.txt")).unwrap(), b"old");
        assert_no_archive_sidecars(&temp_dir.0);
    }

    #[test]
    fn replaces_only_archive_directories_and_preserves_other_addons() {
        let temp_dir = TempDir::new();
        let archive_path = temp_dir.0.join("addon.tar");
        create_tar(&archive_path, &[("RuneEngraver/nuevo.lua", b"nuevo")]);
        let destination = temp_dir.0.join("Interface/AddOns");
        fs::create_dir_all(destination.join("OtroAddon")).unwrap();
        fs::write(destination.join("OtroAddon/archivo.lua"), b"otro intacto").unwrap();
        fs::create_dir_all(destination.join("RuneEngraver")).unwrap();
        fs::write(destination.join("RuneEngraver/viejo.lua"), b"viejo").unwrap();

        extract_archive(&archive_path, &destination).unwrap();

        assert_eq!(
            fs::read(destination.join("OtroAddon/archivo.lua")).unwrap(),
            b"otro intacto"
        );
        assert_eq!(
            fs::read(destination.join("RuneEngraver/nuevo.lua")).unwrap(),
            b"nuevo"
        );
        assert!(!destination.join("RuneEngraver/viejo.lua").exists());
        assert_no_archive_sidecars(&temp_dir.0);
    }

    #[test]
    fn rejects_root_level_archive_files_without_changing_destination() {
        let temp_dir = TempDir::new();
        let archive_path = temp_dir.0.join("unsafe.tar");
        create_tar(&archive_path, &[("suelto.lua", b"nuevo")]);
        let destination = temp_dir.0.join("Interface/AddOns");
        fs::create_dir_all(destination.join("OtroAddon")).unwrap();
        fs::write(destination.join("OtroAddon/archivo.lua"), b"intacto").unwrap();

        let result = extract_archive(&archive_path, &destination);

        assert!(matches!(result, Err(StagingError::UnsafeArchiveEntry(_))));
        assert_eq!(
            fs::read(destination.join("OtroAddon/archivo.lua")).unwrap(),
            b"intacto"
        );
        assert!(!destination.join("suelto.lua").exists());
        assert_no_archive_sidecars(&temp_dir.0);
    }

    #[test]
    fn rejects_archive_when_matching_destination_is_not_a_directory() {
        let temp_dir = TempDir::new();
        let archive_path = temp_dir.0.join("addon.tar");
        create_tar(&archive_path, &[("RuneEngraver/nuevo.lua", b"nuevo")]);
        let destination = temp_dir.0.join("Interface/AddOns");
        fs::create_dir_all(&destination).unwrap();
        fs::write(destination.join("RuneEngraver"), b"archivo existente").unwrap();
        fs::write(destination.join("otro.txt"), b"intacto").unwrap();

        let result = extract_archive(&archive_path, &destination);

        assert!(matches!(result, Err(StagingError::UnsafePath(_))));
        assert_eq!(
            fs::read(destination.join("RuneEngraver")).unwrap(),
            b"archivo existente"
        );
        assert_eq!(fs::read(destination.join("otro.txt")).unwrap(), b"intacto");
        assert_no_archive_sidecars(&temp_dir.0);
    }

    #[test]
    fn replaces_multiple_archive_directories_and_preserves_unlisted_directory() {
        let temp_dir = TempDir::new();
        let archive_path = temp_dir.0.join("addons.tar");
        create_tar(
            &archive_path,
            &[("A/nuevo.lua", b"A"), ("B/nuevo.lua", b"B")],
        );
        let destination = temp_dir.0.join("Interface/AddOns");
        for name in ["A", "B", "C"] {
            fs::create_dir_all(destination.join(name)).unwrap();
            fs::write(destination.join(name).join("viejo.lua"), name.as_bytes()).unwrap();
        }

        extract_archive(&archive_path, &destination).unwrap();

        for name in ["A", "B"] {
            assert_eq!(
                fs::read(destination.join(name).join("nuevo.lua")).unwrap(),
                name.as_bytes()
            );
            assert!(!destination.join(name).join("viejo.lua").exists());
        }
        assert_eq!(fs::read(destination.join("C/viejo.lua")).unwrap(), b"C");
        assert_no_archive_sidecars(&temp_dir.0);
    }

    #[test]
    fn rejects_archive_traversal_without_changing_destination() {
        let temp_dir = TempDir::new();
        let archive_path = temp_dir.0.join("unsafe.tar");
        create_tar_with_traversal(&archive_path);
        let destination = temp_dir.0.join("installed/addon");
        fs::create_dir_all(&destination).unwrap();
        fs::write(destination.join("old-file.txt"), b"old").unwrap();

        let result = extract_archive(&archive_path, &destination);

        assert!(matches!(result, Err(StagingError::UnsafeArchiveEntry(_))));
        assert_eq!(fs::read(destination.join("old-file.txt")).unwrap(), b"old");
        assert!(!destination.join("Safe/new-file.txt").exists());
        assert!(!temp_dir.0.join("escape.txt").exists());
        assert_no_archive_sidecars(&temp_dir.0);
    }

    #[test]
    fn rejects_archive_symbolic_links_without_changing_destination() {
        let temp_dir = TempDir::new();
        let archive_path = temp_dir.0.join("symlink.tar");
        let mut builder = tar::Builder::new(fs::File::create(&archive_path).unwrap());
        append_tar_file(&mut builder, "Safe/new-file.txt", b"new");
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        builder
            .append_link(&mut header, "Safe/link", "../../outside")
            .unwrap();
        builder.finish().unwrap();
        let destination = temp_dir.0.join("installed/addon");
        fs::create_dir_all(&destination).unwrap();
        fs::write(destination.join("old-file.txt"), b"old").unwrap();

        let result = extract_archive(&archive_path, &destination);

        assert!(matches!(result, Err(StagingError::UnsafeArchiveEntry(_))));
        assert_eq!(fs::read(destination.join("old-file.txt")).unwrap(), b"old");
        assert!(!destination.join("Safe/new-file.txt").exists());
        assert_no_archive_sidecars(&temp_dir.0);
    }
}

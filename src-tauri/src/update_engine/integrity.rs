use std::{
    collections::HashMap,
    error::Error,
    fmt,
    fs::{self, File},
    io::{self, Read},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::UNIX_EPOCH,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::manifest::{resolve_manifest_path, Manifest, ManifestError};

const HASH_BUFFER_SIZE: usize = 1024 * 1024;
const PROGRESS_INTERVAL_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedFileState {
    pub size: u64,
    pub modified_unix_nanos: u128,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyMode {
    Quick,
    Full,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileVerification {
    pub path: String,
    pub status: FileStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyProgress {
    pub index: usize,
    pub total: usize,
    pub path: String,
    pub file_bytes_done: u64,
    pub file_bytes_total: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileStatus {
    Missing,
    Valid,
    Corrupt { expected: String, actual: String },
    Unreadable { message: String },
}

#[derive(Debug)]
pub enum IntegrityError {
    Cancelled,
    InvalidPath { path: String, source: ManifestError },
}

impl fmt::Display for IntegrityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("Operación cancelada."),
            Self::InvalidPath { path, source } => {
                write!(formatter, "ruta inválida {path}: {source}")
            }
        }
    }
}

impl Error for IntegrityError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Cancelled => None,
            Self::InvalidPath { source, .. } => Some(source),
        }
    }
}

/// Verifica los archivos finales descritos por el manifest sin modificar el disco.
pub fn verify_manifest_files(
    manifest: &Manifest,
    install_root: &Path,
) -> Result<Vec<FileVerification>, IntegrityError> {
    let cancel = AtomicBool::new(false);
    verify_manifest_files_with_progress(
        manifest,
        install_root,
        &cancel,
        VerifyMode::Full,
        &mut HashMap::new(),
        |_| {},
    )
}

/// Verifica los archivos del manifest e informa del avance por archivo y bytes leídos.
pub fn verify_manifest_files_with_progress(
    manifest: &Manifest,
    install_root: &Path,
    cancel: &AtomicBool,
    mode: VerifyMode,
    cache: &mut HashMap<String, CachedFileState>,
    mut on_progress: impl FnMut(&VerifyProgress),
) -> Result<Vec<FileVerification>, IntegrityError> {
    let total = manifest.files.len();
    let bytes_total = manifest
        .files
        .iter()
        .fold(0u64, |sum, file| sum.saturating_add(file.size_bytes));
    let mut bytes_done = 0u64;
    let mut report = Vec::with_capacity(total);

    for (file_index, file) in manifest.files.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(IntegrityError::Cancelled);
        }
        let index = file_index + 1;
        let bytes_done_before_file = bytes_done;
        let mut file_bytes_done = 0;
        report_progress(
            &mut on_progress,
            index,
            total,
            &file.path,
            file_bytes_done,
            file.size_bytes,
            bytes_done_before_file,
            bytes_total,
        );

        let file_path = resolve_manifest_path(install_root, &file.path).map_err(|source| {
            IntegrityError::InvalidPath {
                path: file.path.clone(),
                source,
            }
        })?;
        let (status, cached_state) = verify_file_with_progress(
            &file_path,
            &file.sha256,
            file.size_bytes,
            mode,
            cache.get(&file.path),
            cancel,
            |bytes_read| {
                file_bytes_done = bytes_read;
                report_progress(
                    &mut on_progress,
                    index,
                    total,
                    &file.path,
                    file_bytes_done,
                    file.size_bytes,
                    bytes_done_before_file,
                    bytes_total,
                );
            },
        )?;

        if status == FileStatus::Valid {
            if let Some(cached_state) = cached_state {
                cache.insert(file.path.clone(), cached_state);
            } else {
                cache.remove(&file.path);
            }
        } else {
            cache.remove(&file.path);
        }

        // Los archivos ausentes, descartados por tamaño o ilegibles también completan
        // su parte del trabajo del manifest para que el progreso alcance el total.
        file_bytes_done = file.size_bytes;
        bytes_done = bytes_done.saturating_add(file.size_bytes);
        report_progress(
            &mut on_progress,
            index,
            total,
            &file.path,
            file_bytes_done,
            file.size_bytes,
            bytes_done_before_file,
            bytes_total,
        );
        report.push(FileVerification {
            path: file.path.clone(),
            status,
        });
    }

    Ok(report)
}

fn report_progress(
    on_progress: &mut dyn FnMut(&VerifyProgress),
    index: usize,
    total: usize,
    path: &str,
    file_bytes_done: u64,
    file_bytes_total: u64,
    bytes_done_before_file: u64,
    bytes_total: u64,
) {
    on_progress(&VerifyProgress {
        index,
        total,
        path: path.to_owned(),
        file_bytes_done,
        file_bytes_total,
        bytes_done: bytes_done_before_file.saturating_add(file_bytes_done),
        bytes_total,
    });
}

fn verify_file_with_progress(
    path: &Path,
    expected: &str,
    expected_size: u64,
    mode: VerifyMode,
    cached_state: Option<&CachedFileState>,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(u64),
) -> Result<(FileStatus, Option<CachedFileState>), IntegrityError> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok((FileStatus::Missing, None))
        }
        Err(error) => {
            return Ok((
                FileStatus::Unreadable {
                    message: error.to_string(),
                },
                None,
            ))
        }
    };
    if metadata.len() != expected_size {
        return Ok((
            FileStatus::Corrupt {
                expected: expected.to_owned(),
                actual: format!(
                    "tamaño {} bytes, se esperaban {expected_size}",
                    metadata.len()
                ),
            },
            None,
        ));
    }

    let modified_unix_nanos = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos());
    if mode == VerifyMode::Quick
        && modified_unix_nanos.is_some_and(|modified_unix_nanos| {
            cached_state.is_some_and(|cached| {
                cached.size == metadata.len()
                    && cached.modified_unix_nanos == modified_unix_nanos
                    && cached.sha256.eq_ignore_ascii_case(expected)
            })
        })
    {
        return Ok((
            FileStatus::Valid,
            Some(CachedFileState {
                size: metadata.len(),
                modified_unix_nanos: modified_unix_nanos.expect("se comprobó antes"),
                sha256: expected.to_owned(),
            }),
        ));
    }

    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok((FileStatus::Missing, None))
        }
        Err(error) => {
            return Ok((
                FileStatus::Unreadable {
                    message: error.to_string(),
                },
                None,
            ))
        }
    };

    let mut hasher = Sha256::new();
    let mut buffer = [0; HASH_BUFFER_SIZE];
    let mut file_bytes_done = 0u64;
    let mut next_progress = PROGRESS_INTERVAL_BYTES;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(IntegrityError::Cancelled);
        }
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(bytes_read) => {
                hasher.update(&buffer[..bytes_read]);
                file_bytes_done = file_bytes_done.saturating_add(bytes_read as u64);
                if file_bytes_done >= next_progress {
                    on_progress(file_bytes_done);
                    next_progress = file_bytes_done.saturating_add(PROGRESS_INTERVAL_BYTES);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => {
                return Ok((
                    FileStatus::Unreadable {
                        message: error.to_string(),
                    },
                    None,
                ))
            }
        }
    }

    let actual = encode_hex(hasher.finalize().iter().copied());
    if actual.eq_ignore_ascii_case(expected) {
        let cached_state = modified_unix_nanos.map(|modified_unix_nanos| CachedFileState {
            size: metadata.len(),
            modified_unix_nanos,
            sha256: expected.to_owned(),
        });
        Ok((FileStatus::Valid, cached_state))
    } else {
        Ok((
            FileStatus::Corrupt {
                expected: expected.to_owned(),
                actual,
            },
            None,
        ))
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
        fs::{self, FileTimes},
        path::PathBuf,
        sync::atomic::{AtomicBool, AtomicU64, Ordering},
        time::{Duration, SystemTime},
    };

    use super::*;
    use crate::update_engine::manifest::{
        Assembly, AssemblyPart, Compression, FileKind, FileRole, FileSource, ManifestFile,
        ManifestSignature,
    };

    static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let unique = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "warcrafted-integrity-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn write(&self, relative_path: &str, contents: &[u8]) {
            let path = self.0.join(relative_path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
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

    fn manifest_file(path: &str, expected_hash: String, size_bytes: u64) -> ManifestFile {
        ManifestFile {
            path: path.to_owned(),
            role: FileRole::Required,
            kind: FileKind::ClientBase,
            addon_group: None,
            size_bytes,
            sha256: expected_hash,
            source: Some(FileSource {
                url: "https://example.invalid/file".into(),
                compressed_size_bytes: 0,
                compression: Compression::None,
            }),
            assembly: None,
        }
    }

    fn manifest(files: Vec<ManifestFile>) -> Manifest {
        Manifest {
            schema_version: 1,
            realm: "test".into(),
            channel: "stable".into(),
            client_build: 12340,
            manifest_version: 1,
            published_at: "2026-10-07T00:00:00Z".into(),
            min_launcher_version: "0.1.0".into(),
            files,
            signature: ManifestSignature {
                key_id: "test".into(),
                algorithm: "ed25519".into(),
                value: String::new(),
            },
        }
    }

    fn cached_state(path: &Path, size: u64, sha256: String) -> CachedFileState {
        let metadata = fs::metadata(path).expect("metadatos del archivo");
        CachedFileState {
            size,
            modified_unix_nanos: metadata
                .modified()
                .expect("fecha de modificación")
                .duration_since(UNIX_EPOCH)
                .expect("fecha posterior a UNIX_EPOCH")
                .as_nanos(),
            sha256,
        }
    }

    #[test]
    fn reports_valid_file_when_hash_matches() {
        let temp_dir = TempDir::new();
        let contents = b"client file contents";
        temp_dir.write("Data/file.MPQ", contents);
        let manifest = manifest(vec![manifest_file(
            "Data/file.MPQ",
            sha256(contents),
            contents.len() as u64,
        )]);

        let report = verify_manifest_files(&manifest, &temp_dir.0).unwrap();

        assert_eq!(report[0].status, FileStatus::Valid);
    }

    #[test]
    fn reports_missing_file() {
        let temp_dir = TempDir::new();
        let manifest = manifest(vec![manifest_file(
            "Data/missing.MPQ",
            sha256(b"expected"),
            8,
        )]);

        let report = verify_manifest_files(&manifest, &temp_dir.0).unwrap();

        assert_eq!(report[0].status, FileStatus::Missing);
    }

    #[test]
    fn reports_corrupt_file_with_expected_and_actual_hashes() {
        let temp_dir = TempDir::new();
        let expected = sha256(b"expected contents");
        let actual = sha256(b"different contents");
        temp_dir.write("Data/file.MPQ", b"different contents");
        let manifest = manifest(vec![manifest_file(
            "Data/file.MPQ",
            expected.clone(),
            b"different contents".len() as u64,
        )]);

        let report = verify_manifest_files(&manifest, &temp_dir.0).unwrap();

        assert_eq!(report[0].status, FileStatus::Corrupt { expected, actual });
    }

    #[test]
    fn rejects_a_matching_hash_when_the_file_size_differs() {
        let temp_dir = TempDir::new();
        let contents = b"contents with a matching hash";
        temp_dir.write("Data/file.MPQ", contents);
        let expected = sha256(contents);
        let manifest = manifest(vec![manifest_file(
            "Data/file.MPQ",
            expected.clone(),
            contents.len() as u64 + 1,
        )]);

        let report = verify_manifest_files(&manifest, &temp_dir.0).unwrap();

        assert_eq!(
            report[0].status,
            FileStatus::Corrupt {
                expected,
                actual: format!(
                    "tamaño {} bytes, se esperaban {}",
                    contents.len(),
                    contents.len() + 1
                ),
            }
        );
    }

    #[test]
    fn progress_counts_missing_files_and_finishes_at_the_manifest_total() {
        let temp_dir = TempDir::new();
        let contents = b"valid";
        temp_dir.write("Data/file.MPQ", contents);
        let manifest = manifest(vec![
            manifest_file("Data/file.MPQ", sha256(contents), contents.len() as u64),
            manifest_file("Data/missing.MPQ", sha256(b"absent"), 12),
        ]);
        let mut progress = Vec::new();
        let cancel = AtomicBool::new(false);
        let mut cache = HashMap::new();

        let report = verify_manifest_files_with_progress(
            &manifest,
            &temp_dir.0,
            &cancel,
            VerifyMode::Full,
            &mut cache,
            |value| progress.push(value.clone()),
        )
        .unwrap();

        assert_eq!(report[0].status, FileStatus::Valid);
        assert_eq!(report[1].status, FileStatus::Missing);
        assert!(!progress.is_empty());
        assert!(progress
            .windows(2)
            .all(|pair| pair[0].index <= pair[1].index));
        assert_eq!(
            progress
                .iter()
                .map(|value| value.index)
                .collect::<std::collections::BTreeSet<_>>(),
            [1, 2].into_iter().collect()
        );
        assert_eq!(
            progress.last().unwrap().bytes_done,
            progress.last().unwrap().bytes_total
        );
        assert_eq!(progress.last().unwrap().bytes_total, 17);
        assert_eq!(progress.last().unwrap().file_bytes_done, 12);
    }

    #[test]
    fn verifies_assembled_file_at_its_final_path() {
        let temp_dir = TempDir::new();
        let contents = b"assembled final contents";
        temp_dir.write("Data/large.MPQ", contents);
        let mut file = manifest_file("Data/large.MPQ", sha256(contents), contents.len() as u64);
        file.source = None;
        file.assembly = Some(Assembly {
            part_size_bytes: 4,
            parts: vec![AssemblyPart {
                sha256: sha256(b"part"),
                size_bytes: 4,
                source: FileSource {
                    url: "https://example.invalid/part".into(),
                    compressed_size_bytes: 4,
                    compression: Compression::None,
                },
            }],
        });
        let manifest = manifest(vec![file]);

        let report = verify_manifest_files(&manifest, &temp_dir.0).unwrap();

        assert_eq!(report[0].status, FileStatus::Valid);
    }

    #[test]
    fn cancellation_before_verification_returns_without_progress() {
        let temp_dir = TempDir::new();
        let contents = b"client file contents";
        temp_dir.write("Data/file.MPQ", contents);
        let manifest = manifest(vec![manifest_file(
            "Data/file.MPQ",
            sha256(contents),
            contents.len() as u64,
        )]);
        let cancel = AtomicBool::new(true);
        let mut progress = Vec::new();
        let mut cache = HashMap::new();

        let error = verify_manifest_files_with_progress(
            &manifest,
            &temp_dir.0,
            &cancel,
            VerifyMode::Full,
            &mut cache,
            |value| progress.push(value.clone()),
        )
        .unwrap_err();

        assert!(matches!(error, IntegrityError::Cancelled));
        assert_eq!(error.to_string(), "Operación cancelada.");
        assert!(progress.is_empty());
    }

    #[test]
    fn quick_verification_trusts_a_matching_cache_entry_without_hashing() {
        let temp_dir = TempDir::new();
        let contents = b"actual";
        let expected = sha256(b"forged");
        temp_dir.write("Data/file.MPQ", contents);
        let path = temp_dir.0.join("Data/file.MPQ");
        let mut cache = HashMap::from([(
            "Data/file.MPQ".into(),
            cached_state(&path, contents.len() as u64, expected.clone()),
        )]);
        let manifest = manifest(vec![manifest_file(
            "Data/file.MPQ",
            expected,
            contents.len() as u64,
        )]);

        let report = verify_manifest_files_with_progress(
            &manifest,
            &temp_dir.0,
            &AtomicBool::new(false),
            VerifyMode::Quick,
            &mut cache,
            |_| {},
        )
        .unwrap();

        assert_eq!(report[0].status, FileStatus::Valid);
        assert_eq!(cache["Data/file.MPQ"].sha256, sha256(b"forged"));
    }

    #[test]
    fn quick_verification_hashes_when_the_modification_time_changes() {
        let temp_dir = TempDir::new();
        let contents = b"actual";
        let expected = sha256(b"forged");
        temp_dir.write("Data/file.MPQ", contents);
        let path = temp_dir.0.join("Data/file.MPQ");
        let cached = cached_state(&path, contents.len() as u64, expected.clone());
        // Windows exige abrir en escritura para cambiar la fecha de modificación.
        fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(FileTimes::new().set_modified(SystemTime::now() + Duration::from_secs(3)))
            .expect("cambiar fecha de modificación");
        let mut cache = HashMap::from([("Data/file.MPQ".into(), cached.clone())]);
        let manifest = manifest(vec![manifest_file(
            "Data/file.MPQ",
            expected.clone(),
            contents.len() as u64,
        )]);

        let report = verify_manifest_files_with_progress(
            &manifest,
            &temp_dir.0,
            &AtomicBool::new(false),
            VerifyMode::Quick,
            &mut cache,
            |_| {},
        )
        .unwrap();

        assert_eq!(
            report[0].status,
            FileStatus::Corrupt {
                expected,
                actual: sha256(contents),
            }
        );
        assert!(!cache.contains_key("Data/file.MPQ"));
    }

    #[test]
    fn full_verification_hashes_even_when_the_cache_matches_the_manifest() {
        let temp_dir = TempDir::new();
        let contents = b"actual";
        let expected = sha256(b"forged");
        temp_dir.write("Data/file.MPQ", contents);
        let path = temp_dir.0.join("Data/file.MPQ");
        let mut cache = HashMap::from([(
            "Data/file.MPQ".into(),
            cached_state(&path, contents.len() as u64, expected.clone()),
        )]);
        let manifest = manifest(vec![manifest_file(
            "Data/file.MPQ",
            expected.clone(),
            contents.len() as u64,
        )]);

        let report = verify_manifest_files_with_progress(
            &manifest,
            &temp_dir.0,
            &AtomicBool::new(false),
            VerifyMode::Full,
            &mut cache,
            |_| {},
        )
        .unwrap();

        assert_eq!(
            report[0].status,
            FileStatus::Corrupt {
                expected,
                actual: sha256(contents),
            }
        );
        assert!(!cache.contains_key("Data/file.MPQ"));
    }

    #[test]
    fn hashed_valid_files_are_cached_and_invalid_files_are_removed() {
        let temp_dir = TempDir::new();
        let valid_contents = b"valid";
        let corrupt_contents = b"wrong";
        temp_dir.write("Data/valid.MPQ", valid_contents);
        temp_dir.write("Data/corrupt.MPQ", corrupt_contents);
        let valid_path = temp_dir.0.join("Data/valid.MPQ");
        let corrupt_path = temp_dir.0.join("Data/corrupt.MPQ");
        let missing_path = "Data/missing.MPQ";
        let mut cache = HashMap::from([
            (
                "Data/valid.MPQ".into(),
                cached_state(&valid_path, valid_contents.len() as u64, "stale".into()),
            ),
            (
                "Data/corrupt.MPQ".into(),
                cached_state(&corrupt_path, corrupt_contents.len() as u64, "stale".into()),
            ),
            (
                missing_path.into(),
                CachedFileState {
                    size: 7,
                    modified_unix_nanos: 0,
                    sha256: "stale".into(),
                },
            ),
        ]);
        let manifest = manifest(vec![
            manifest_file(
                "Data/valid.MPQ",
                sha256(valid_contents),
                valid_contents.len() as u64,
            ),
            manifest_file(
                "Data/corrupt.MPQ",
                sha256(b"other"),
                corrupt_contents.len() as u64,
            ),
            manifest_file(missing_path, sha256(b"missing"), 7),
        ]);

        let report = verify_manifest_files_with_progress(
            &manifest,
            &temp_dir.0,
            &AtomicBool::new(false),
            VerifyMode::Full,
            &mut cache,
            |_| {},
        )
        .unwrap();

        assert_eq!(report[0].status, FileStatus::Valid);
        assert_eq!(cache["Data/valid.MPQ"].sha256, sha256(valid_contents));
        assert!(cache.contains_key("Data/valid.MPQ"));
        assert_eq!(
            report[1].status,
            FileStatus::Corrupt {
                expected: sha256(b"other"),
                actual: sha256(corrupt_contents),
            }
        );
        assert_eq!(report[2].status, FileStatus::Missing);
        assert!(!cache.contains_key("Data/corrupt.MPQ"));
        assert!(!cache.contains_key(missing_path));
    }
}

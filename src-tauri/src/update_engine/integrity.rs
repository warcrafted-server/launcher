use std::{
    error::Error,
    fmt,
    fs::File,
    io::{self, Read},
    path::Path,
};

use sha2::{Digest, Sha256};

use super::manifest::{resolve_manifest_path, Manifest, ManifestError};

const HASH_BUFFER_SIZE: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileVerification {
    pub path: String,
    pub status: FileStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileStatus {
    Missing,
    Valid,
    Corrupt { expected: String, actual: String },
    Unreadable { message: String },
}

#[derive(Debug)]
pub struct IntegrityError {
    pub path: String,
    source: ManifestError,
}

impl fmt::Display for IntegrityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ruta inválida {}: {}", self.path, self.source)
    }
}

impl Error for IntegrityError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

/// Verifica los archivos finales descritos por el manifest sin modificar el disco.
pub fn verify_manifest_files(
    manifest: &Manifest,
    install_root: &Path,
) -> Result<Vec<FileVerification>, IntegrityError> {
    manifest
        .files
        .iter()
        .map(|file| {
            let file_path = resolve_manifest_path(install_root, &file.path).map_err(|source| {
                IntegrityError {
                    path: file.path.clone(),
                    source,
                }
            })?;

            Ok(FileVerification {
                path: file.path.clone(),
                status: verify_file(&file_path, &file.sha256),
            })
        })
        .collect()
}

fn verify_file(path: &Path, expected: &str) -> FileStatus {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return FileStatus::Missing,
        Err(error) => {
            return FileStatus::Unreadable {
                message: error.to_string(),
            }
        }
    };

    let mut hasher = Sha256::new();
    let mut buffer = [0; HASH_BUFFER_SIZE];
    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(bytes_read) => hasher.update(&buffer[..bytes_read]),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => {
                return FileStatus::Unreadable {
                    message: error.to_string(),
                }
            }
        }
    }

    let actual = encode_hex(hasher.finalize().iter().copied());
    if actual.eq_ignore_ascii_case(expected) {
        FileStatus::Valid
    } else {
        FileStatus::Corrupt {
            expected: expected.to_owned(),
            actual,
        }
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
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
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

    fn manifest_file(path: &str, expected_hash: String) -> ManifestFile {
        ManifestFile {
            path: path.to_owned(),
            role: FileRole::Required,
            kind: FileKind::ClientBase,
            addon_group: None,
            size_bytes: 0,
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

    #[test]
    fn reports_valid_file_when_hash_matches() {
        let temp_dir = TempDir::new();
        let contents = b"client file contents";
        temp_dir.write("Data/file.MPQ", contents);
        let manifest = manifest(vec![manifest_file("Data/file.MPQ", sha256(contents))]);

        let report = verify_manifest_files(&manifest, &temp_dir.0).unwrap();

        assert_eq!(report[0].status, FileStatus::Valid);
    }

    #[test]
    fn reports_missing_file() {
        let temp_dir = TempDir::new();
        let manifest = manifest(vec![manifest_file("Data/missing.MPQ", sha256(b"expected"))]);

        let report = verify_manifest_files(&manifest, &temp_dir.0).unwrap();

        assert_eq!(report[0].status, FileStatus::Missing);
    }

    #[test]
    fn reports_corrupt_file_with_expected_and_actual_hashes() {
        let temp_dir = TempDir::new();
        let expected = sha256(b"expected contents");
        let actual = sha256(b"different contents");
        temp_dir.write("Data/file.MPQ", b"different contents");
        let manifest = manifest(vec![manifest_file("Data/file.MPQ", expected.clone())]);

        let report = verify_manifest_files(&manifest, &temp_dir.0).unwrap();

        assert_eq!(report[0].status, FileStatus::Corrupt { expected, actual });
    }

    #[test]
    fn verifies_assembled_file_at_its_final_path() {
        let temp_dir = TempDir::new();
        let contents = b"assembled final contents";
        temp_dir.write("Data/large.MPQ", contents);
        let mut file = manifest_file("Data/large.MPQ", sha256(contents));
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
}

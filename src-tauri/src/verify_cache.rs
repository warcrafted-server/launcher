use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

use crate::update_engine::integrity::CachedFileState;

static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct VerifyCache {
    client_dir: String,
    files: HashMap<String, CachedFileState>,
}

pub(crate) fn load_cache(
    directory: &Path,
    client_dir: &Path,
) -> Result<HashMap<String, CachedFileState>, String> {
    let path = directory.join("verify-cache.json");
    let contents = match fs::read(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(HashMap::new());
        }
        Err(error) => return Err(format!("No se pudo leer la caché de verificación: {error}")),
    };

    let cache: VerifyCache = match serde_json::from_slice(&contents) {
        Ok(cache) => cache,
        Err(_) => return Ok(HashMap::new()),
    };
    if cache.client_dir != client_dir_string(client_dir) {
        return Ok(HashMap::new());
    }
    Ok(cache.files)
}

pub(crate) fn save_cache(
    directory: &Path,
    client_dir: &Path,
    files: &HashMap<String, CachedFileState>,
) -> Result<(), String> {
    fs::create_dir_all(directory)
        .map_err(|error| format!("No se pudo preparar la caché de verificación: {error}"))?;
    let destination = directory.join("verify-cache.json");
    let contents = serde_json::to_vec_pretty(&VerifyCache {
        client_dir: client_dir_string(client_dir),
        files: files.clone(),
    })
    .map_err(|error| format!("No se pudo preparar la caché de verificación: {error}"))?;

    let (temporary_path, mut temporary_file) = create_temporary_file(directory)?;
    let write_result = (|| {
        temporary_file
            .write_all(&contents)
            .map_err(|error| format!("No se pudo escribir la caché de verificación: {error}"))?;
        temporary_file
            .sync_all()
            .map_err(|error| format!("No se pudo guardar la caché de verificación: {error}"))?;
        drop(temporary_file);
        fs::rename(&temporary_path, &destination)
            .map_err(|error| format!("No se pudo aplicar la caché de verificación: {error}"))?;
        sync_directory(directory)?;
        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    write_result
}

fn client_dir_string(client_dir: &Path) -> String {
    client_dir.to_string_lossy().into_owned()
}

fn create_temporary_file(directory: &Path) -> Result<(PathBuf, File), String> {
    for _ in 0..32 {
        let sequence = NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!(
            ".verify-cache-{}-{sequence}.tmp",
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "No se pudo crear el archivo temporal de la caché de verificación: {error}"
                ));
            }
        }
    }
    Err("No se pudo reservar un archivo temporal para la caché de verificación.".into())
}

#[cfg(unix)]
fn sync_directory(directory: &Path) -> Result<(), String> {
    File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|error| {
            format!("No se pudo confirmar el guardado de la caché de verificación: {error}")
        })
}

#[cfg(not(unix))]
fn sync_directory(_directory: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "warcrafted-verify-cache-test-{}-{sequence}",
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

    fn sample_state() -> CachedFileState {
        CachedFileState {
            size: 17,
            modified_unix_nanos: 1_791_490_000_000_000_000,
            sha256: "0123456789abcdef".into(),
        }
    }

    #[test]
    fn cache_round_trip() {
        let directory = TestDirectory::new();
        let client_dir = directory.0.join("client");
        let files = HashMap::from([("Data/file.MPQ".into(), sample_state())]);

        save_cache(&directory.0, &client_dir, &files).expect("guardar caché");

        assert_eq!(
            load_cache(&directory.0, &client_dir).expect("leer caché"),
            files
        );
        let document: serde_json::Value = serde_json::from_slice(
            &fs::read(directory.0.join("verify-cache.json")).expect("leer JSON"),
        )
        .expect("JSON válido");
        assert!(document.get("clientDir").is_some());
        assert!(document["files"]["Data/file.MPQ"]["modifiedUnixNanos"].is_number());
    }

    #[test]
    fn invalid_json_starts_with_an_empty_cache() {
        let directory = TestDirectory::new();
        let client_dir = directory.0.join("client");
        fs::write(directory.0.join("verify-cache.json"), b"{ invalid json")
            .expect("escribir JSON inválido");

        assert!(load_cache(&directory.0, &client_dir)
            .expect("descartar JSON inválido")
            .is_empty());
    }

    #[test]
    fn a_different_client_directory_starts_with_an_empty_cache() {
        let directory = TestDirectory::new();
        let original_client = directory.0.join("client-one");
        let other_client = directory.0.join("client-two");
        let files = HashMap::from([("Data/file.MPQ".into(), sample_state())]);
        save_cache(&directory.0, &original_client, &files).expect("guardar caché");

        assert!(load_cache(&directory.0, &other_client)
            .expect("descartar caché de otra carpeta")
            .is_empty());
    }
}

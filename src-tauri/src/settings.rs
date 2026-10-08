use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct LauncherSettings {
    pub(crate) client_dir: Option<PathBuf>,
}

pub(crate) fn load_settings(directory: &Path) -> Result<LauncherSettings, String> {
    let path = directory.join("settings.json");
    let contents = match fs::read(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LauncherSettings::default());
        }
        Err(error) => {
            return Err(format!(
                "No se pudieron leer los ajustes del launcher: {error}"
            ));
        }
    };

    serde_json::from_slice(&contents).map_err(|error| {
        format!(
            "El archivo de ajustes del launcher no es válido ({}): {error}",
            path.display()
        )
    })
}

pub(crate) fn save_settings(directory: &Path, settings: &LauncherSettings) -> Result<(), String> {
    fs::create_dir_all(directory)
        .map_err(|error| format!("No se pudo preparar la carpeta de ajustes: {error}"))?;
    let destination = directory.join("settings.json");
    let contents = serde_json::to_vec_pretty(settings)
        .map_err(|error| format!("No se pudieron preparar los ajustes: {error}"))?;

    let (temporary_path, mut temporary_file) = create_temporary_file(directory)?;
    let write_result = (|| {
        temporary_file
            .write_all(&contents)
            .map_err(|error| format!("No se pudieron escribir los ajustes: {error}"))?;
        temporary_file
            .sync_all()
            .map_err(|error| format!("No se pudieron guardar los ajustes: {error}"))?;
        drop(temporary_file);
        fs::rename(&temporary_path, &destination)
            .map_err(|error| format!("No se pudieron aplicar los ajustes: {error}"))?;
        sync_directory(directory)?;
        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    write_result
}

fn create_temporary_file(directory: &Path) -> Result<(PathBuf, File), String> {
    for _ in 0..32 {
        let sequence = NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!(".settings-{}-{sequence}.tmp", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "No se pudo crear el archivo temporal de ajustes: {error}"
                ));
            }
        }
    }
    Err("No se pudo reservar un archivo temporal para los ajustes.".into())
}

#[cfg(unix)]
fn sync_directory(directory: &Path) -> Result<(), String> {
    File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("No se pudo confirmar el guardado de los ajustes: {error}"))
}

#[cfg(not(unix))]
fn sync_directory(_directory: &Path) -> Result<(), String> {
    Ok(())
}

pub(crate) fn validate_client_dir(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("La carpeta del cliente debe ser una ruta absoluta.".into());
    }

    let normalized = normalize_absolute_path(path)?;
    if is_filesystem_root(&normalized) {
        return Err("La raíz del sistema no puede usarse como carpeta del cliente.".into());
    }

    match fs::symlink_metadata(&normalized) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err("La carpeta del cliente no puede ser un enlace simbólico.".into())
        }
        Ok(metadata) if !metadata.is_dir() => {
            Err("La ruta elegida existe, pero no es una carpeta.".into())
        }
        Ok(_) => fs::canonicalize(&normalized)
            .map_err(|error| format!("No se pudo validar la carpeta del cliente: {error}")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = normalized.parent().ok_or_else(|| {
                "La carpeta del cliente no tiene una carpeta superior válida.".to_owned()
            })?;
            let parent_metadata = fs::metadata(parent).map_err(|error| {
                format!("La carpeta superior del cliente no está disponible: {error}")
            })?;
            if !parent_metadata.is_dir() {
                return Err("La carpeta superior del cliente no es una carpeta.".into());
            }
            Ok(normalized)
        }
        Err(error) => Err(format!(
            "No se pudo validar la carpeta del cliente: {error}"
        )),
    }
}

fn normalize_absolute_path(path: &Path) -> Result<PathBuf, String> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(
                    normalized.components().next_back(),
                    Some(Component::Normal(_))
                ) {
                    normalized.pop();
                }
            }
            Component::Normal(name) => normalized.push(name),
        }
    }
    if !normalized.is_absolute() {
        return Err("La carpeta del cliente debe ser una ruta absoluta.".into());
    }
    Ok(normalized)
}

fn is_filesystem_root(path: &Path) -> bool {
    !path
        .components()
        .any(|component| matches!(component, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "warcrafted-settings-test-{}-{sequence}",
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

    #[test]
    fn settings_round_trip_and_ignore_unknown_fields() {
        let directory = TestDirectory::new();
        let settings = LauncherSettings {
            client_dir: Some(directory.0.join("client")),
        };

        save_settings(&directory.0, &settings).expect("guardar ajustes");
        assert_eq!(load_settings(&directory.0).expect("leer ajustes"), settings);

        fs::write(
            directory.0.join("settings.json"),
            r#"{"clientDir":null,"futureOption":true}"#,
        )
        .expect("escribir campo desconocido");
        assert_eq!(
            load_settings(&directory.0).expect("ignorar campo desconocido"),
            LauncherSettings::default()
        );
    }

    #[test]
    fn absent_settings_file_returns_defaults() {
        let directory = TestDirectory::new();
        assert_eq!(
            load_settings(&directory.0).expect("ajustes por defecto"),
            LauncherSettings::default()
        );
    }

    #[test]
    fn corrupt_settings_return_an_error_without_changing_the_file() {
        let directory = TestDirectory::new();
        let path = directory.0.join("settings.json");
        let corrupt_contents = b"{ invalid json";
        fs::write(&path, corrupt_contents).expect("escribir JSON corrupto");

        assert!(load_settings(&directory.0).is_err());
        assert_eq!(fs::read(path).expect("releer archivo"), corrupt_contents);
    }

    #[test]
    fn client_directory_validation_checks_absolute_root_and_entry_types() {
        let directory = TestDirectory::new();
        let client = directory.0.join("client");
        fs::create_dir(&client).expect("crear cliente");
        let file = directory.0.join("file");
        fs::write(&file, b"x").expect("crear archivo");

        assert!(validate_client_dir(Path::new("relative/client")).is_err());
        #[cfg(unix)]
        let filesystem_root = PathBuf::from("/");
        #[cfg(windows)]
        let filesystem_root = PathBuf::from(r"C:\");
        assert!(validate_client_dir(&filesystem_root).is_err());
        assert!(validate_client_dir(&file).is_err());
        assert_eq!(
            validate_client_dir(&client).expect("carpeta existente"),
            fs::canonicalize(&client).expect("canonicalizar cliente")
        );

        let new_client = directory.0.join("new-client");
        assert_eq!(
            validate_client_dir(&new_client).expect("carpeta nueva"),
            new_client
        );
    }

    #[cfg(unix)]
    #[test]
    fn client_directory_validation_rejects_symbolic_links() {
        use std::os::unix::fs::symlink;

        let directory = TestDirectory::new();
        let target = directory.0.join("target");
        let link = directory.0.join("link");
        fs::create_dir(&target).expect("crear destino");
        symlink(&target, &link).expect("crear enlace");

        assert!(validate_client_dir(&link).is_err());
    }
}

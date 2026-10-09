//! Configuración del cliente de World of Warcraft que el launcher ajusta por el jugador.
//!
//! Al primer arranque el cliente 3.3.5a muestra los textos legales (EULA, términos de uso,
//! aviso de terminación sin previo aviso y de análisis) y no deja jugar hasta aceptarlos.
//! Marcarlos por adelantado en `<cliente>/WTF/Config.wtf` evita ese trámite.
//!
//! `WTF/Config.wtf` lo crea el propio juego y el jugador lo modifica, por eso no puede formar
//! parte del manifest con hash: aquí solo se añaden o corrigen claves concretas sin tocar el
//! resto del archivo.

use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

/// Carpeta del cliente donde vive la configuración del jugador.
const WTF_DIRECTORY: &str = "WTF";
/// Nombre canónico del archivo; si el cliente ya creó uno con otra capitalización, se reutiliza.
const CONFIG_FILE_NAME: &str = "Config.wtf";
/// Claves de «textos legales ya aceptados» que el cliente lee al arrancar.
const LEGAL_KEYS: [&str; 4] = [
    "readEULA",
    "readTOS",
    "readTerminationWithoutNotice",
    "readScanning",
];

static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(0);

/// Asegura que `<cliente>/WTF/Config.wtf` marca como aceptados los textos legales del cliente.
///
/// Crea la carpeta `WTF` y el archivo si faltan, reutiliza un `config.wtf` existente (comparación
/// sin distinguir mayúsculas) y conserva todas las demás líneas y el estilo de saltos de línea.
/// Escritura atómica (temporal + `rename`) y sin seguir enlaces simbólicos.
pub(crate) fn ensure_legal_prompts_accepted(client_dir: &Path) -> Result<(), String> {
    let client_metadata = fs::symlink_metadata(client_dir)
        .map_err(|error| format!("no se pudo acceder a la carpeta del cliente: {error}"))?;
    if !client_metadata.is_dir() {
        return Err("la carpeta del cliente no es un directorio".into());
    }

    let wtf_directory = client_dir.join(WTF_DIRECTORY);
    match fs::symlink_metadata(&wtf_directory) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err("la carpeta WTF del cliente no puede ser un enlace simbólico".into());
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err("la ruta WTF del cliente existe, pero no es una carpeta".into());
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&wtf_directory)
                .map_err(|error| format!("no se pudo crear la carpeta WTF: {error}"))?;
        }
        Err(error) => return Err(format!("no se pudo acceder a la carpeta WTF: {error}")),
    }

    let config_path = existing_config_path(&wtf_directory)?
        .unwrap_or_else(|| wtf_directory.join(CONFIG_FILE_NAME));

    let contents = match fs::read(&config_path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(format!("no se pudo leer Config.wtf: {error}")),
    };
    let updated = with_legal_prompts_accepted(&contents);
    if updated == contents {
        return Ok(());
    }
    write_atomically(&config_path, &updated)
}

/// Localiza un `Config.wtf` ya existente cuyo nombre coincida sin distinguir mayúsculas.
///
/// Rechaza enlaces simbólicos y carpetas con ese nombre antes de que se puedan seguir o reemplazar.
fn existing_config_path(wtf_directory: &Path) -> Result<Option<PathBuf>, String> {
    let entries = fs::read_dir(wtf_directory)
        .map_err(|error| format!("no se pudo leer la carpeta WTF: {error}"))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("no se pudo leer la carpeta WTF: {error}"))?;
        if !entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case(CONFIG_FILE_NAME)
        {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|error| format!("no se pudo inspeccionar Config.wtf: {error}"))?;
        if file_type.is_symlink() {
            return Err("Config.wtf no puede ser un enlace simbólico".into());
        }
        if file_type.is_dir() {
            return Err("Config.wtf existe, pero es una carpeta".into());
        }
        return Ok(Some(entry.path()));
    }
    Ok(None)
}

/// Transformación pura y testeable: devuelve `contents` con las claves legales a "1".
///
/// Trabaja sobre bytes para no corromper archivos con contenido que no sea UTF-8. Conserva todas
/// las líneas y su orden, mantiene el estilo de salto de línea detectado (CRLF o LF; CRLF por
/// defecto para un archivo nuevo), reemplaza en su sitio las claves ya presentes (comparación de
/// la clave sin distinguir mayúsculas) y añade al final las que falten.
fn with_legal_prompts_accepted(contents: &[u8]) -> Vec<u8> {
    let line_ending = detect_line_ending(contents);
    let had_trailing_newline = contents.last() == Some(&b'\n');
    let mut found = [false; LEGAL_KEYS.len()];
    let mut lines: Vec<Vec<u8>> = Vec::new();

    for line in split_content_lines(contents) {
        let mut matched = None;
        for (index, key) in LEGAL_KEYS.iter().enumerate() {
            if line_matches_key(&line, key) {
                matched = Some(index);
                break;
            }
        }
        match matched {
            Some(index) => {
                found[index] = true;
                lines.push(canonical_line(LEGAL_KEYS[index]));
            }
            None => lines.push(line),
        }
    }

    for (index, key) in LEGAL_KEYS.iter().enumerate() {
        if !found[index] {
            lines.push(canonical_line(key));
        }
    }

    let mut result = Vec::new();
    let last_index = lines.len().saturating_sub(1);
    for (index, line) in lines.iter().enumerate() {
        result.extend_from_slice(line);
        if index != last_index || had_trailing_newline {
            result.extend_from_slice(line_ending);
        }
    }
    result
}

/// Estilo de salto de línea del archivo: CRLF si aparece alguno, LF si solo hay LF y CRLF por
/// defecto (archivo nuevo o sin saltos de línea).
fn detect_line_ending(contents: &[u8]) -> &'static [u8] {
    if contents.windows(2).any(|window| window == b"\r\n") {
        b"\r\n"
    } else if contents.contains(&b'\n') {
        b"\n"
    } else {
        b"\r\n"
    }
}

/// Divide el contenido en líneas sin su terminador, sin exigir UTF-8 válido.
fn split_content_lines(contents: &[u8]) -> Vec<Vec<u8>> {
    let mut lines = Vec::new();
    let mut current = Vec::new();
    for &byte in contents {
        if byte == b'\n' {
            if current.last() == Some(&b'\r') {
                current.pop();
            }
            lines.push(std::mem::take(&mut current));
        } else {
            current.push(byte);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

/// Comprueba si la línea es un `SET <clave> ...` para `key`, sin distinguir mayúsculas.
fn line_matches_key(line: &[u8], key: &str) -> bool {
    let mut tokens = line
        .split(|byte| byte.is_ascii_whitespace())
        .filter(|token| !token.is_empty());
    match tokens.next() {
        Some(command) if command.eq_ignore_ascii_case(b"set") => {}
        _ => return false,
    }
    match tokens.next() {
        Some(found) => found.eq_ignore_ascii_case(key.as_bytes()),
        None => false,
    }
}

fn canonical_line(key: &str) -> Vec<u8> {
    format!("SET {key} \"1\"").into_bytes()
}

/// Escribe `contents` de forma atómica: temporal en la misma carpeta, `sync_all` y `rename`.
fn write_atomically(path: &Path, contents: &[u8]) -> Result<(), String> {
    let directory = path
        .parent()
        .ok_or_else(|| "Config.wtf no tiene una carpeta superior válida".to_owned())?;
    let (temporary_path, mut temporary_file) = create_temporary_file(directory)?;
    let write_result = (|| {
        temporary_file
            .write_all(contents)
            .map_err(|error| format!("no se pudo escribir Config.wtf: {error}"))?;
        temporary_file
            .sync_all()
            .map_err(|error| format!("no se pudo confirmar Config.wtf: {error}"))?;
        drop(temporary_file);
        fs::rename(&temporary_path, path)
            .map_err(|error| format!("no se pudo aplicar Config.wtf: {error}"))?;
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
        let path = directory.join(format!(".config-wtf-{}-{sequence}.tmp", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "no se pudo crear el archivo temporal de Config.wtf: {error}"
                ));
            }
        }
    }
    Err("no se pudo reservar un archivo temporal para Config.wtf.".into())
}

#[cfg(unix)]
fn sync_directory(directory: &Path) -> Result<(), String> {
    File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|error| format!("no se pudo confirmar el guardado de Config.wtf: {error}"))
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
                "warcrafted-game-config-test-{}-{sequence}",
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

    fn expected_line(key: &str) -> String {
        format!("SET {key} \"1\"")
    }

    #[test]
    fn creates_all_keys_for_a_new_file_with_crlf() {
        let result = with_legal_prompts_accepted(b"");

        assert_eq!(
            result,
            b"SET readEULA \"1\"\r\nSET readTOS \"1\"\r\n\
              SET readTerminationWithoutNotice \"1\"\r\nSET readScanning \"1\""
                .to_vec()
        );
    }

    #[test]
    fn keeps_other_options_and_their_order() {
        let input = b"SET locale \"esES\"\r\nSET gxWindow \"1\"\r\nSET readEULA \"0\"\r\n";

        let result = with_legal_prompts_accepted(input);
        let text = String::from_utf8(result).expect("UTF-8");

        assert!(text.contains("SET locale \"esES\"\r\nSET gxWindow \"1\"\r\n"));
        assert!(text.contains("SET readEULA \"1\""));
        let locale = text.find("SET locale").expect("locale");
        let eula = text.find("SET readEULA").expect("readEULA");
        assert!(locale < eula, "readEULA debe conservar su posición: {text}");
    }

    #[test]
    fn replaces_a_present_key_with_value_zero_in_place() {
        let input = b"SET readTOS \"0\"\nSET gxWindow \"1\"\n";

        let result = with_legal_prompts_accepted(input);
        let text = String::from_utf8(result).expect("UTF-8");

        assert_eq!(text.matches("readTOS").count(), 1, "{text}");
        assert!(text.contains("SET readTOS \"1\""));
        assert!(text.starts_with("SET readTOS \"1\"\n"), "{text}");
    }

    #[test]
    fn recognises_keys_regardless_of_case() {
        let input = b"set readeula \"0\"\r\n";

        let result = with_legal_prompts_accepted(input);
        let text = String::from_utf8(result).expect("UTF-8");

        assert_eq!(text.matches("readEULA").count(), 1, "{text}");
        assert!(text.contains("SET readEULA \"1\""));
    }

    #[test]
    fn preserves_lf_line_endings() {
        let input = b"SET gxWindow \"0\"\nSET readEULA \"1\"\n";

        let result = with_legal_prompts_accepted(input);

        assert!(!result.contains(&b'\r'), "no debe introducir CR");
        let text = String::from_utf8(result).expect("UTF-8");
        assert!(text.contains("SET gxWindow \"0\"\nSET readEULA \"1\"\n"));
        assert!(text.contains("SET readTOS \"1\"\n"));
    }

    #[test]
    fn preserves_crlf_line_endings() {
        let input = b"SET gxWindow \"0\"\r\nSET readEULA \"1\"\r\n";

        let result = with_legal_prompts_accepted(input);
        let text = String::from_utf8(result).expect("UTF-8");

        assert!(text.contains("SET gxWindow \"0\"\r\nSET readEULA \"1\"\r\n"));
        assert!(text.contains("SET readTOS \"1\"\r\n"));
    }

    #[test]
    fn is_idempotent() {
        let input = b"SET locale \"esES\"\r\nSET readEULA \"0\"\r\n";

        let once = with_legal_prompts_accepted(input);
        let twice = with_legal_prompts_accepted(&once);

        assert_eq!(once, twice);
    }

    #[test]
    fn does_not_corrupt_non_utf8_content() {
        let mut input = b"SET note \"".to_vec();
        input.extend_from_slice(&[0xff, 0xfe]);
        input.extend_from_slice(b"\"\r\n");

        let result = with_legal_prompts_accepted(&input);

        assert!(
            result.windows(2).any(|window| window == [0xff, 0xfe]),
            "los bytes ajenos deben conservarse"
        );
        let text = String::from_utf8_lossy(&result);
        assert!(text.contains("SET readEULA \"1\""));
    }

    #[test]
    fn creates_the_wtf_directory_and_config_file() {
        let directory = TestDirectory::new();
        let client = directory.0.join("client");
        fs::create_dir(&client).expect("crear cliente");

        ensure_legal_prompts_accepted(&client).expect("marcar textos legales");

        let contents = fs::read_to_string(client.join("WTF/Config.wtf")).expect("leer Config.wtf");
        for key in LEGAL_KEYS {
            assert!(contents.contains(&expected_line(key)), "{contents}");
        }
    }

    #[test]
    fn reuses_an_existing_lowercase_config_file() {
        let directory = TestDirectory::new();
        let client = directory.0.join("client");
        let wtf = client.join("WTF");
        fs::create_dir_all(&wtf).expect("crear WTF");
        fs::write(wtf.join("config.wtf"), b"SET locale \"esES\"\r\n").expect("escribir config");

        ensure_legal_prompts_accepted(&client).expect("marcar textos legales");

        let names: Vec<_> = fs::read_dir(&wtf)
            .expect("leer WTF")
            .map(|entry| entry.expect("entrada").file_name())
            .collect();
        assert_eq!(names.len(), 1, "no debe crear un segundo archivo: {names:?}");
        let contents = fs::read_to_string(wtf.join("config.wtf")).expect("leer config");
        assert!(contents.contains("SET locale \"esES\""));
        assert!(contents.contains("SET readEULA \"1\""));
        assert!(!wtf.join("Config.wtf").exists());
    }

    #[test]
    fn keeps_existing_options_on_disk() {
        let directory = TestDirectory::new();
        let client = directory.0.join("client");
        let wtf = client.join("WTF");
        fs::create_dir_all(&wtf).expect("crear WTF");
        fs::write(
            wtf.join("Config.wtf"),
            b"SET locale \"esES\"\nSET gxWindow \"1\"\n",
        )
        .expect("escribir config");

        ensure_legal_prompts_accepted(&client).expect("marcar textos legales");

        let contents = fs::read_to_string(wtf.join("Config.wtf")).expect("leer config");
        assert!(contents.starts_with("SET locale \"esES\"\nSET gxWindow \"1\"\n"));
    }

    #[test]
    fn is_idempotent_on_disk() {
        let directory = TestDirectory::new();
        let client = directory.0.join("client");
        fs::create_dir(&client).expect("crear cliente");

        ensure_legal_prompts_accepted(&client).expect("primera vez");
        let first = fs::read(client.join("WTF/Config.wtf")).expect("leer config");
        ensure_legal_prompts_accepted(&client).expect("segunda vez");
        let second = fs::read(client.join("WTF/Config.wtf")).expect("releer config");

        assert_eq!(first, second);
    }

    #[test]
    fn rejects_a_missing_client_directory_without_creating_it() {
        let directory = TestDirectory::new();
        let client = directory.0.join("missing-client");

        assert!(ensure_legal_prompts_accepted(&client).is_err());
        assert!(!client.exists(), "no debe inventarse la carpeta del cliente");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_symbolic_link_wtf_directory() {
        use std::os::unix::fs::symlink;

        let directory = TestDirectory::new();
        let client = directory.0.join("client");
        let outside = directory.0.join("outside");
        fs::create_dir(&client).expect("crear cliente");
        fs::create_dir(&outside).expect("crear carpeta externa");
        symlink(&outside, client.join("WTF")).expect("crear enlace WTF");

        assert!(ensure_legal_prompts_accepted(&client).is_err());
        assert!(
            !outside.join("Config.wtf").exists(),
            "no debe escribirse fuera del cliente"
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_symbolic_link_config_file() {
        use std::os::unix::fs::symlink;

        let directory = TestDirectory::new();
        let client = directory.0.join("client");
        let wtf = client.join("WTF");
        fs::create_dir_all(&wtf).expect("crear WTF");
        let target = directory.0.join("target.wtf");
        fs::write(&target, b"SET locale \"esES\"\r\n").expect("escribir destino");
        symlink(&target, wtf.join("Config.wtf")).expect("crear enlace Config.wtf");

        assert!(ensure_legal_prompts_accepted(&client).is_err());
        assert_eq!(
            fs::read(&target).expect("leer destino"),
            b"SET locale \"esES\"\r\n",
            "el destino del enlace no debe modificarse"
        );
    }
}

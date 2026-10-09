//! Detección de clientes de World of Warcraft instalados y validación de su versión.
//!
//! La versión se lee del recurso `VS_FIXEDFILEINFO` incrustado en `Wow.exe`: se busca su firma
//! (0xFEEF04BD, en little-endian `BD 04 EF FE`) y se leen `dwFileVersionMS`/`dwFileVersionLS`.
//! La búsqueda de instalaciones se limita en profundidad y en tiempo para no recorrer discos
//! enteros, es cancelable y nunca sigue enlaces simbólicos.

use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use serde::Serialize;

/// Ejecutable del cliente de World of Warcraft.
pub const GAME_EXECUTABLE: &str = "Wow.exe";
/// Versión exacta que espera WarCrafted: cliente 3.3.5a, build 12340.
pub const EXPECTED_VERSION: (u16, u16, u16, u16) = (3, 3, 5, 12340);
/// Firma de la estructura `VS_FIXEDFILEINFO` en el archivo (0xFEEF04BD little-endian).
const VS_FIXEDFILEINFO_SIGNATURE: [u8; 4] = [0xBD, 0x04, 0xEF, 0xFE];
/// Bytes que ocupan la firma y los campos de versión (`dwFileVersionMS`/`LS` incluidos).
const VERSION_FIELDS_LEN: usize = 16;
/// Niveles de carpetas que se exploran como máximo por debajo de cada raíz.
const MAX_DEPTH: usize = 2;
/// Tiempo máximo que puede durar una búsqueda completa de instalaciones.
pub const SEARCH_TIME_LIMIT: Duration = Duration::from_secs(10);
/// Tamaño de los bloques con los que se recorre el ejecutable (no se carga entero en memoria).
const SCAN_BLOCK_BYTES: usize = 256 * 1024;
/// Bytes que se conservan entre bloques para no perder una firma partida por el límite.
const SCAN_OVERLAP_BYTES: usize = VERSION_FIELDS_LEN;

/// Cliente de WoW encontrado en el disco.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedClient {
    pub path: String,
    pub version: Option<String>,
    pub valid: bool,
}

/// Resultado de inspeccionar una carpeta concreta como posible cliente.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientFolderCheck {
    pub has_wow_exe: bool,
    pub version: Option<String>,
    pub valid: bool,
}

/// Comprueba una carpeta concreta sin lanzar errores: solo interesa si hay `Wow.exe` y su versión.
pub fn inspect_client_folder(directory: &Path) -> ClientFolderCheck {
    let executable = directory.join(GAME_EXECUTABLE);
    if !executable.is_file() {
        return ClientFolderCheck {
            has_wow_exe: false,
            version: None,
            valid: false,
        };
    }

    // Si el ejecutable no se puede leer, la versión queda como desconocida (cliente no válido).
    let version = wow_exe_version(&executable).ok().flatten();
    ClientFolderCheck {
        has_wow_exe: true,
        version: version.map(format_version),
        valid: version.map(is_expected_build).unwrap_or(false),
    }
}

/// Lee la versión de `Wow.exe` desde una ruta.
pub fn wow_exe_version(path: &Path) -> Result<Option<(u16, u16, u16, u16)>, String> {
    let file = fs::File::open(path)
        .map_err(|error| format!("no se pudo abrir {}: {error}", path.display()))?;
    scan_for_version(file).map_err(|error| format!("no se pudo leer {}: {error}", path.display()))
}

/// Comprueba si una versión es exactamente la esperada (3.3.5.12340).
pub fn is_expected_build(version: (u16, u16, u16, u16)) -> bool {
    version == EXPECTED_VERSION
}

/// Da formato legible a una versión, por ejemplo `3.3.5.12340`.
pub fn format_version(version: (u16, u16, u16, u16)) -> String {
    let (major_ms, minor_ms, major_ls, minor_ls) = version;
    format!("{major_ms}.{minor_ms}.{major_ls}.{minor_ls}")
}

/// Extrae la versión de un `VS_FIXEDFILEINFO` presente en `bytes` (función pura).
pub fn version_from_bytes(bytes: &[u8]) -> Option<(u16, u16, u16, u16)> {
    let mut start = 0;
    while let Some(offset) = find_signature(&bytes[start..]) {
        let index = start + offset;
        start = index + 1;
        // La firma está en `index`; `dwStrucVersion` ocupa 4 bytes y después vienen MS y LS.
        let Some(fields) = bytes.get(index..index + VERSION_FIELDS_LEN) else {
            continue;
        };
        let file_version_ms = u32::from_le_bytes(fields[8..12].try_into().ok()?);
        let file_version_ls = u32::from_le_bytes(fields[12..16].try_into().ok()?);
        return Some((
            (file_version_ms >> 16) as u16,
            (file_version_ms & 0xFFFF) as u16,
            (file_version_ls >> 16) as u16,
            (file_version_ls & 0xFFFF) as u16,
        ));
    }
    None
}

fn find_signature(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(VS_FIXEDFILEINFO_SIGNATURE.len())
        .position(|window| window == VS_FIXEDFILEINFO_SIGNATURE)
}

/// Recorre el ejecutable por bloques sin cargarlo entero en memoria.
fn scan_for_version<R: Read>(mut reader: R) -> std::io::Result<Option<(u16, u16, u16, u16)>> {
    let mut carry: Vec<u8> = Vec::new();
    let mut block = vec![0u8; SCAN_BLOCK_BYTES];
    loop {
        let read = reader.read(&mut block)?;
        if read == 0 {
            return Ok(None);
        }
        let mut combined = Vec::with_capacity(carry.len() + read);
        combined.extend_from_slice(&carry);
        combined.extend_from_slice(&block[..read]);
        if let Some(version) = version_from_bytes(&combined) {
            return Ok(Some(version));
        }
        let keep = combined.len().min(SCAN_OVERLAP_BYTES);
        carry.clear();
        carry.extend_from_slice(&combined[combined.len() - keep..]);
    }
}

/// Busca clientes de WoW instalados en las ubicaciones habituales del sistema.
pub fn detect_clients(cancel: &AtomicBool) -> Vec<DetectedClient> {
    detect_clients_in(&candidate_roots(), cancel, Instant::now() + SEARCH_TIME_LIMIT)
}

/// Busca clientes partiendo de raíces concretas; útil para pruebas y para acotar la búsqueda.
pub fn detect_clients_in(
    roots: &[PathBuf],
    cancel: &AtomicBool,
    deadline: Instant,
) -> Vec<DetectedClient> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    for root in roots {
        if is_cancelled(cancel, deadline) {
            break;
        }
        scan_root(root, cancel, deadline, &mut found, &mut seen);
    }
    // Los clientes compatibles primero; después, orden estable por ruta.
    found.sort_by(|left, right| {
        right
            .valid
            .cmp(&left.valid)
            .then_with(|| left.path.cmp(&right.path))
    });
    found
}

/// Raíces donde suelen instalarse los clientes: unidades, Program Files, juegos y carpetas del usuario.
fn candidate_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    // Las unidades (C:, D:…) solo existen en Windows; en otros sistemas no se enumeran.
    #[cfg(windows)]
    for letter in b'A'..=b'Z' {
        let root = PathBuf::from(format!("{}:\\", letter as char));
        if root.is_dir() {
            roots.push(root);
        }
    }

    for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(value) = std::env::var_os(variable) {
            roots.push(PathBuf::from(value));
        }
    }

    #[cfg(windows)]
    roots.push(PathBuf::from(r"C:\Games"));

    if let Some(home) = user_home_directory() {
        for folder in [
            "Desktop",
            "Escritorio",
            "Documents",
            "Documentos",
            "Games",
            "Juegos",
        ] {
            roots.push(home.join(folder));
        }
    }

    roots
}

fn user_home_directory() -> Option<PathBuf> {
    ["USERPROFILE", "HOME"]
        .into_iter()
        .find_map(|variable| std::env::var_os(variable).map(PathBuf::from))
}

fn is_cancelled(cancel: &AtomicBool, deadline: Instant) -> bool {
    cancel.load(Ordering::Relaxed) || Instant::now() >= deadline
}

fn scan_root(
    root: &Path,
    cancel: &AtomicBool,
    deadline: Instant,
    found: &mut Vec<DetectedClient>,
    seen: &mut HashSet<PathBuf>,
) {
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((directory, depth)) = stack.pop() {
        if is_cancelled(cancel, deadline) {
            return;
        }
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            if is_cancelled(cancel, deadline) {
                return;
            }
            // `file_type()` no sigue enlaces simbólicos: se descartan para no salir del árbol.
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() || !file_type.is_dir() {
                continue;
            }
            let path = entry.path();
            let executable = path.join(GAME_EXECUTABLE);
            if executable.is_file() {
                record_client(path.clone(), &executable, found, seen);
            }
            // Los hijos de esta carpeta están a `depth + 1`; solo se exploran hasta `MAX_DEPTH`.
            if depth + 1 < MAX_DEPTH {
                stack.push((path, depth + 1));
            }
        }
    }
}

fn record_client(
    path: PathBuf,
    executable: &Path,
    found: &mut Vec<DetectedClient>,
    seen: &mut HashSet<PathBuf>,
) {
    let key = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    if !seen.insert(key) {
        return;
    }
    let version = wow_exe_version(executable).ok().flatten();
    found.push(DetectedClient {
        path: path.to_string_lossy().into_owned(),
        version: version.map(format_version),
        valid: version.map(is_expected_build).unwrap_or(false),
    });
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
                "warcrafted-client-detect-test-{}-{sequence}",
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

    /// Escribe un `VS_FIXEDFILEINFO` mínimo (firma + campos de versión) en `buffer`.
    fn write_fixed_file_info(buffer: &mut [u8], index: usize, version: (u16, u16, u16, u16)) {
        let (major_ms, minor_ms, major_ls, minor_ls) = version;
        let file_version_ms = (u32::from(major_ms) << 16) | u32::from(minor_ms);
        let file_version_ls = (u32::from(major_ls) << 16) | u32::from(minor_ls);
        buffer[index..index + 4].copy_from_slice(&VS_FIXEDFILEINFO_SIGNATURE);
        buffer[index + 4..index + 8].copy_from_slice(&0x0001_0000u32.to_le_bytes());
        buffer[index + 8..index + 12].copy_from_slice(&file_version_ms.to_le_bytes());
        buffer[index + 12..index + 16].copy_from_slice(&file_version_ls.to_le_bytes());
    }

    /// Construye un ejecutable sintético con la versión indicada.
    fn executable_with_version(version: (u16, u16, u16, u16)) -> Vec<u8> {
        let mut bytes = vec![0u8; 64];
        write_fixed_file_info(&mut bytes, 8, version);
        bytes
    }

    /// Crea la carpeta de un cliente falso con su `Wow.exe` (o uno ilegible si `None`).
    fn write_client(directory: &Path, version: Option<(u16, u16, u16, u16)>) {
        fs::create_dir_all(directory).expect("crear carpeta de cliente");
        let bytes = match version {
            Some(version) => executable_with_version(version),
            None => b"no es un ejecutable con version".to_vec(),
        };
        fs::write(directory.join(GAME_EXECUTABLE), bytes).expect("escribir Wow.exe");
    }

    fn far_future() -> Instant {
        Instant::now() + Duration::from_secs(30)
    }

    #[test]
    fn reads_the_version_from_synthetic_version_fields() {
        let bytes = executable_with_version((3, 3, 5, 12340));
        assert_eq!(version_from_bytes(&bytes), Some((3, 3, 5, 12340)));
        assert!(is_expected_build((3, 3, 5, 12340)));
        assert!(!is_expected_build((1, 12, 1, 5875)));
        assert_eq!(format_version((3, 3, 5, 12340)), "3.3.5.12340");
        assert_eq!(format_version((1, 12, 1, 5875)), "1.12.1.5875");
    }

    #[test]
    fn a_file_without_the_signature_has_no_version() {
        assert_eq!(version_from_bytes(&[]), None);
        assert_eq!(version_from_bytes(&[0u8; 128]), None);
        // Firma presente, pero sin espacio para los campos de versión.
        assert_eq!(version_from_bytes(&VS_FIXEDFILEINFO_SIGNATURE), None);
    }

    #[test]
    fn wow_exe_version_reads_the_path_and_reports_unknown_versions() {
        let directory = TestDirectory::new();
        let valid = directory.0.join("valid.exe");
        fs::write(&valid, executable_with_version((3, 3, 5, 12340))).expect("escribir válido");
        let other = directory.0.join("other.exe");
        fs::write(&other, executable_with_version((1, 12, 1, 5875))).expect("escribir otro");
        let none = directory.0.join("none.exe");
        fs::write(&none, b"sin firma").expect("escribir sin firma");

        assert_eq!(
            wow_exe_version(&valid).expect("leer válido"),
            Some((3, 3, 5, 12340))
        );
        assert_eq!(
            wow_exe_version(&other).expect("leer otro"),
            Some((1, 12, 1, 5875))
        );
        assert_eq!(wow_exe_version(&none).expect("leer sin firma"), None);
        assert!(wow_exe_version(&directory.0.join("inexistente.exe")).is_err());
    }

    #[test]
    fn a_signature_split_between_blocks_is_still_found() {
        // La firma empieza dos bytes antes del límite del bloque: los campos vienen en el siguiente.
        let mut bytes = vec![0u8; SCAN_BLOCK_BYTES + 32];
        let index = SCAN_BLOCK_BYTES - 2;
        write_fixed_file_info(&mut bytes, index, (3, 3, 5, 12340));

        let version = scan_for_version(std::io::Cursor::new(bytes)).expect("recorrer bloques");
        assert_eq!(version, Some((3, 3, 5, 12340)));
    }

    #[test]
    fn inspect_client_folder_reports_missing_and_present_clients() {
        let directory = TestDirectory::new();
        let empty = directory.0.join("empty");
        fs::create_dir_all(&empty).expect("crear carpeta vacía");
        assert_eq!(
            inspect_client_folder(&empty),
            ClientFolderCheck {
                has_wow_exe: false,
                version: None,
                valid: false,
            }
        );

        let valid = directory.0.join("valid");
        write_client(&valid, Some((3, 3, 5, 12340)));
        assert_eq!(
            inspect_client_folder(&valid),
            ClientFolderCheck {
                has_wow_exe: true,
                version: Some("3.3.5.12340".into()),
                valid: true,
            }
        );

        let wrong = directory.0.join("wrong");
        write_client(&wrong, Some((1, 12, 1, 5875)));
        assert_eq!(
            inspect_client_folder(&wrong),
            ClientFolderCheck {
                has_wow_exe: true,
                version: Some("1.12.1.5875".into()),
                valid: false,
            }
        );
    }

    #[test]
    fn detects_clients_and_sorts_valid_ones_first() {
        let directory = TestDirectory::new();
        write_client(&directory.0.join("OtroCliente"), Some((1, 12, 1, 5875)));
        write_client(&directory.0.join("WarCrafted"), Some((3, 3, 5, 12340)));
        write_client(&directory.0.join("Desconocido"), None);

        let found = detect_clients_in(&[directory.0.clone()], &AtomicBool::new(false), far_future());

        assert_eq!(found.len(), 3);
        assert!(found[0].valid, "el cliente compatible debe ir primero");
        assert_eq!(
            found[0].path,
            directory
                .0
                .join("WarCrafted")
                .to_string_lossy()
                .into_owned()
        );
        assert_eq!(found[0].version.as_deref(), Some("3.3.5.12340"));
        assert!(found.iter().skip(1).all(|client| !client.valid));
    }

    #[test]
    fn respects_the_maximum_search_depth() {
        let directory = TestDirectory::new();
        let nivel1 = directory.0.join("nivel1");
        let nivel2 = nivel1.join("nivel2");
        let nivel3 = nivel2.join("nivel3");
        write_client(&nivel1, Some((3, 3, 5, 12340)));
        write_client(&nivel2, Some((3, 3, 5, 12340)));
        write_client(&nivel3, Some((3, 3, 5, 12340)));

        let found = detect_clients_in(&[directory.0.clone()], &AtomicBool::new(false), far_future());
        let paths: Vec<String> = found.into_iter().map(|client| client.path).collect();

        assert!(paths.contains(&nivel1.to_string_lossy().into_owned()));
        assert!(paths.contains(&nivel2.to_string_lossy().into_owned()));
        assert!(
            !paths.contains(&nivel3.to_string_lossy().into_owned()),
            "no se debe bajar más allá del límite de profundidad"
        );
    }

    #[test]
    fn a_cancelled_search_returns_no_clients() {
        let directory = TestDirectory::new();
        write_client(&directory.0.join("WarCrafted"), Some((3, 3, 5, 12340)));

        let cancel = AtomicBool::new(true);
        assert!(detect_clients_in(&[directory.0.clone()], &cancel, far_future()).is_empty());
    }

    #[test]
    fn duplicated_roots_do_not_produce_duplicate_clients() {
        let directory = TestDirectory::new();
        write_client(&directory.0.join("WarCrafted"), Some((3, 3, 5, 12340)));

        let root = directory.0.clone();
        let found = detect_clients_in(&[root.clone(), root], &AtomicBool::new(false), far_future());

        assert_eq!(found.len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_symbolic_links() {
        use std::os::unix::fs::symlink;

        let directory = TestDirectory::new();
        let real = directory.0.join("real");
        write_client(&real.join("WarCrafted"), Some((3, 3, 5, 12340)));
        symlink(&real, directory.0.join("enlace")).expect("crear enlace simbólico");

        let found = detect_clients_in(&[directory.0.clone()], &AtomicBool::new(false), far_future());

        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].path,
            real.join("WarCrafted").to_string_lossy().into_owned()
        );
    }
}

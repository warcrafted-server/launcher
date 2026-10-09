//! Addons opcionales (decisión 0005): catálogo firmado, estado e instalación.
//!
//! Este módulo no sabe nada del cliente obligatorio ni de la UI: recibe el catálogo ya descargado,
//! lo valida contra la clave de confianza y ofrece las operaciones de instalación/desinstalación
//! sobre `Interface/AddOns`. Los addons opcionales nunca se mezclan con los obligatorios: ni en el
//! modelo de datos (aquí no se toca el manifest) ni en el disco (se rechaza cualquier carpeta que
//! coincida con una de un addon obligatorio o que pertenezca a Blizzard).

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    error::Error,
    fmt,
    path::{Component, Path, PathBuf},
};

use ed25519_dalek::VerifyingKey;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::update_engine::manifest::{
    self, validate_source_host, verify_signed_document, Compression, FileRole, FileSource, Manifest,
    ManifestError, ManifestFile,
};
use crate::update_engine::staging::{self, StagingError};

/// Única versión de esquema de catálogo admitida hoy.
pub const CATALOG_SCHEMA_VERSION: u32 = 1;

/// Mismo criterio de orígenes que el manifest: HTTPS y estos hosts.
pub const ALLOWED_ADDON_HOSTS: &[&str] = &["raw.githubusercontent.com", "github.com"];

/// Prefijo reservado por Blizzard: ningún addon opcional puede usar una carpeta así.
const BLIZZARD_FOLDER_PREFIX: &str = "blizzard_";

/// Subcarpeta del cliente donde viven los addons.
const ADDONS_SUBDIRECTORY: &str = "Interface/AddOns";

/// Documento firmado con los addons opcionales disponibles.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AddonCatalog {
    pub schema_version: u32,
    /// Contador monótono: nunca puede bajar respecto al último catálogo visto.
    pub catalog_version: u64,
    pub published_at: String,
    pub addons: Vec<OptionalAddon>,
    #[serde(default)]
    pub signature: Option<manifest::ManifestSignature>,
}

/// Addon opcional declarado en el catálogo.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OptionalAddon {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub license: String,
    pub homepage: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub source: AddonSource,
    /// Carpetas de primer nivel que el TAR instala dentro de `Interface/AddOns`.
    pub folders: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AddonSource {
    pub url: String,
}

/// Registro persistido de un addon opcional instalado por el launcher.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstalledAddon {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub folders: Vec<String>,
}

impl InstalledAddon {
    pub(crate) fn new(addon: &OptionalAddon) -> Self {
        Self {
            id: addon.id.clone(),
            version: addon.version.clone(),
            folders: addon.folders.clone(),
        }
    }

    fn owns_folder(&self, folder: &str) -> bool {
        self.folders
            .iter()
            .any(|owned| owned.eq_ignore_ascii_case(folder))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogError {
    Signature(ManifestError),
    UnsupportedSchema(u32),
    InvalidField(String),
    DuplicateAddonId(String),
    DuplicateFolder(String),
    RequiredAddonConflict(String),
    CatalogRollback { candidate: u64, applied: u64 },
}

impl fmt::Display for CatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Signature(error) => write!(formatter, "catálogo de addons inválido: {error}"),
            Self::UnsupportedSchema(version) => {
                write!(formatter, "schemaVersion del catálogo no compatible: {version}")
            }
            Self::InvalidField(field) => write!(formatter, "campo del catálogo inválido: {field}"),
            Self::DuplicateAddonId(id) => write!(formatter, "id de addon duplicado: {id}"),
            Self::DuplicateFolder(folder) => write!(formatter, "carpeta de addon duplicada: {folder}"),
            Self::RequiredAddonConflict(folder) => write!(
                formatter,
                "la carpeta {folder} pertenece a un addon obligatorio del cliente"
            ),
            Self::CatalogRollback { candidate, applied } => write!(
                formatter,
                "catalogVersion {candidate} es anterior a la versión aplicada {applied}"
            ),
        }
    }
}

impl Error for CatalogError {}

impl From<ManifestError> for CatalogError {
    fn from(error: ManifestError) -> Self {
        Self::Signature(error)
    }
}

#[derive(Debug)]
pub enum InstallError {
    Staging(StagingError),
    Filesystem(std::io::Error),
    InvalidFolder(String),
    UnsafeFolder(PathBuf),
    ExistingFolder(String),
    MissingFolder(String),
    TaskJoin(String),
}

impl fmt::Display for InstallError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Staging(error) => write!(formatter, "falló el tratamiento del archivo: {error}"),
            Self::Filesystem(error) => write!(formatter, "error de disco: {error}"),
            Self::InvalidFolder(folder) => write!(formatter, "carpeta de addon inválida: {folder}"),
            Self::UnsafeFolder(path) => write!(
                formatter,
                "la ruta {} no es una carpeta normal del cliente",
                path.display()
            ),
            Self::ExistingFolder(folder) => write!(
                formatter,
                "la carpeta {folder} ya existe y no la gestiona el launcher; hace falta confirmación"
            ),
            Self::MissingFolder(folder) => {
                write!(formatter, "el paquete no contenía la carpeta {folder}")
            }
            Self::TaskJoin(message) => {
                write!(formatter, "no se pudo completar la instalación: {message}")
            }
        }
    }
}

impl Error for InstallError {}

impl From<StagingError> for InstallError {
    fn from(error: StagingError) -> Self {
        Self::Staging(error)
    }
}


/// Estado derivado de un addon opcional, calculado en el backend para que la UI no decida.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AddonInstallState {
    NotInstalled,
    Installed,
    UpdateAvailable,
}

/// Verifica la firma del catálogo y todas las reglas de la decisión 0005.
///
/// `previous_catalog_version` es la última `catalogVersion` aplicada por este launcher (si la hay):
/// un catálogo anterior se rechaza (anti-rollback, igual que el manifest).
pub fn parse_catalog(
    document: &[u8],
    trusted_keys: &BTreeMap<String, VerifyingKey>,
    previous_catalog_version: Option<u64>,
) -> Result<AddonCatalog, CatalogError> {
    verify_signed_document(document, trusted_keys)?;
    let catalog: AddonCatalog = serde_json::from_slice(document)
        .map_err(|error| CatalogError::InvalidField(format!("JSON del catálogo: {error}")))?;
    validate_catalog(&catalog, previous_catalog_version)?;
    Ok(catalog)
}

fn validate_catalog(
    catalog: &AddonCatalog,
    previous_catalog_version: Option<u64>,
) -> Result<(), CatalogError> {
    if catalog.schema_version != CATALOG_SCHEMA_VERSION {
        return Err(CatalogError::UnsupportedSchema(catalog.schema_version));
    }
    if !manifest::is_rfc3339_utc(&catalog.published_at) {
        return Err(CatalogError::InvalidField("publishedAt".into()));
    }
    if let Some(applied) = previous_catalog_version {
        if catalog.catalog_version < applied {
            return Err(CatalogError::CatalogRollback {
                candidate: catalog.catalog_version,
                applied,
            });
        }
    }

    let mut seen_ids = HashSet::new();
    let mut seen_folders = HashSet::new();
    for addon in &catalog.addons {
        validate_addon(addon)?;
        if !seen_ids.insert(addon.id.as_str()) {
            return Err(CatalogError::DuplicateAddonId(addon.id.clone()));
        }
        for folder in &addon.folders {
            if !seen_folders.insert(folder.to_ascii_lowercase()) {
                return Err(CatalogError::DuplicateFolder(folder.clone()));
            }
        }
    }
    Ok(())
}

fn validate_addon(addon: &OptionalAddon) -> Result<(), CatalogError> {
    let invalid = |field: &str| CatalogError::InvalidField(format!("{field} de {}", addon.id));
    if !is_slug(&addon.id) {
        return Err(invalid("id"));
    }
    if addon.name.trim().is_empty() {
        return Err(invalid("name"));
    }
    if addon.description.trim().is_empty()
        || addon.author.trim().is_empty()
        || addon.license.trim().is_empty()
    {
        return Err(invalid("ficha"));
    }
    if !addon.homepage.starts_with("https://") {
        return Err(invalid("homepage"));
    }
    manifest::validate_semver(&addon.version).map_err(|_| invalid("version"))?;
    if !is_lowercase_sha256(&addon.sha256) {
        return Err(invalid("sha256"));
    }
    if addon.size_bytes == 0 {
        return Err(invalid("sizeBytes"));
    }
    validate_source_host(&addon.source.url, ALLOWED_ADDON_HOSTS, &addon.id)
        .map_err(|_| invalid("source.url"))?;
    if addon.folders.is_empty() {
        return Err(invalid("folders"));
    }
    for folder in &addon.folders {
        validate_folder_name(folder).map_err(CatalogError::InvalidField)?;
        if folder
            .to_ascii_lowercase()
            .starts_with(BLIZZARD_FOLDER_PREFIX)
        {
            return Err(invalid("folders"));
        }
    }
    Ok(())
}

fn is_lowercase_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 48
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// Valida que `folder` sea una única componente normal (sin separadores, `..` ni ruta absoluta).
pub fn validate_folder_name(folder: &str) -> Result<(), String> {
    if folder.is_empty() || folder.len() > 100 {
        return Err(format!("nombre de carpeta no válido: {folder:?}"));
    }
    let mut components = Path::new(folder).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(part)), None) if part == std::ffi::OsStr::new(folder) => Ok(()),
        _ => Err(format!("nombre de carpeta no válido: {folder:?}")),
    }
}

/// Carpetas de primer nivel de los addons obligatorios declarados en el manifest.
pub fn required_addon_folders(manifest: &Manifest) -> BTreeSet<String> {
    let mut folders = BTreeSet::new();
    for file in &manifest.files {
        if file.role != FileRole::Required {
            continue;
        }
        if let Ok(relative) = Path::new(&file.path).strip_prefix(ADDONS_SUBDIRECTORY) {
            if let Some(Component::Normal(folder)) = relative.components().next() {
                folders.insert(folder.to_string_lossy().into_owned());
            }
        }
    }
    folders
}

/// Rechaza que un addon opcional ocupe la carpeta de un addon obligatorio del manifest.
pub fn validate_required_addon_conflicts(
    folders: &[String],
    required_folders: &BTreeSet<String>,
) -> Result<(), CatalogError> {
    let reserved: HashSet<String> = required_folders
        .iter()
        .map(|folder| folder.to_ascii_lowercase())
        .collect();
    for folder in folders {
        if reserved.contains(&folder.to_ascii_lowercase()) {
            return Err(CatalogError::RequiredAddonConflict(folder.clone()));
        }
    }
    Ok(())
}


/// Decide el estado de un addon comparando la versión del catálogo con el registro persistido.
///
/// Función pura: `folders_present` lo calcula el llamante a partir del disco (véase
/// `addon_folders_present`). Un registro cuyo contenido ya no está en `Interface/AddOns` se trata
/// como no instalado, para que la UI pueda reinstalarlo sin inventar estados intermedios.
pub fn decide_addon_state(
    catalog_version: &str,
    installed: Option<&InstalledAddon>,
    folders_present: bool,
) -> AddonInstallState {
    match installed {
        None => AddonInstallState::NotInstalled,
        Some(_) if !folders_present => AddonInstallState::NotInstalled,
        Some(record) if record.version == catalog_version => AddonInstallState::Installed,
        Some(_) => AddonInstallState::UpdateAvailable,
    }
}

/// Comprueba que TODAS las carpetas declaradas existan como carpetas normales.
pub fn addon_folders_present(addons_root: &Path, folders: &[String]) -> bool {
    !folders.is_empty()
        && folders.iter().all(|folder| {
            validate_folder_name(folder).is_ok()
                && std::fs::symlink_metadata(addons_root.join(folder))
                    .map(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
                    .unwrap_or(false)
        })
}

/// Raíz de addons del cliente.
pub fn addons_root(client_dir: &Path) -> PathBuf {
    client_dir.join(ADDONS_SUBDIRECTORY)
}

/// Busca el registro de un addon por identificador.
pub fn find_installed<'a>(installed: &'a [InstalledAddon], id: &str) -> Option<&'a InstalledAddon> {
    installed.iter().find(|record| record.id == id)
}

fn check_target_folders(
    addons_root: &Path,
    folders: &[String],
    registered: Option<&InstalledAddon>,
    replace_existing: bool,
) -> Result<(), InstallError> {
    for folder in folders {
        validate_folder_name(folder).map_err(InstallError::InvalidFolder)?;
        let target = addons_root.join(folder);
        match std::fs::symlink_metadata(&target) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(InstallError::Filesystem(error)),
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(InstallError::UnsafeFolder(target));
                }
            }
        }
        if registered.is_some_and(|record| record.owns_folder(folder)) {
            continue;
        }
        if !replace_existing {
            return Err(InstallError::ExistingFolder(folder.clone()));
        }
    }
    Ok(())
}



/// Descarga el TAR del addon, verifica su hash, lo extrae en `Interface/AddOns` y devuelve el
/// registro que debe guardarse en `settings.json`.
///
/// `registered` es el registro anterior del mismo addon (si lo hay) y `replace_existing` autoriza
/// sustituir carpetas que no gestiona el launcher (la UI debe haber avisado antes al jugador). El
/// TAR se verifica contra `sha256` antes de extraerse y `staging::extract_archive` sustituye los
/// directorios de primer nivel de forma atómica, así que un paquete inválido no deja nada a medias.
pub async fn install_addon(
    client: &Client,
    addon: &OptionalAddon,
    client_dir: &Path,
    staging_root: &Path,
    downloads_root: &Path,
    registered: Option<&InstalledAddon>,
    replace_existing: bool,
    on_bytes: &mut (dyn FnMut(u64) + Send),
) -> Result<InstalledAddon, InstallError> {
    let addons_root = addons_root(client_dir);
    std::fs::create_dir_all(&addons_root).map_err(InstallError::Filesystem)?;
    check_target_folders(&addons_root, &addon.folders, registered, replace_existing)?;

    let archive = ManifestFile {
        path: format!("optional-addons/{}.tar", addon.id),
        role: FileRole::Optional,
        kind: manifest::FileKind::Archive,
        addon_group: None,
        size_bytes: addon.size_bytes,
        sha256: addon.sha256.clone(),
        source: Some(FileSource {
            url: addon.source.url.clone(),
            compressed_size_bytes: addon.size_bytes,
            compression: Compression::None,
        }),
        assembly: None,
    };
    let archive_path = staging::stage_manifest_file_with_downloads(
        client,
        &archive,
        staging_root,
        downloads_root,
        on_bytes,
    )
    .await?;

    let destination = addons_root.clone();
    tokio::task::spawn_blocking(move || staging::extract_archive(&archive_path, &destination))
        .await
        .map_err(|error| InstallError::TaskJoin(error.to_string()))??;

    for folder in &addon.folders {
        let target = addons_root.join(folder);
        let metadata = std::fs::symlink_metadata(&target)
            .map_err(|_| InstallError::MissingFolder(folder.clone()))?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(InstallError::UnsafeFolder(target));
        }
    }
    Ok(InstalledAddon::new(addon))
}

/// Borra del disco las carpetas registradas de un addon opcional.
///
/// Nunca sigue enlaces ni borra nada fuera de `Interface/AddOns`: cada nombre se valida como una
/// única componente normal y cada destino debe ser una carpeta real (no un enlace simbólico).
pub fn uninstall_addon(client_dir: &Path, installed: &InstalledAddon) -> Result<(), InstallError> {
    let addons_root = addons_root(client_dir);
    for folder in &installed.folders {
        validate_folder_name(folder).map_err(InstallError::InvalidFolder)?;
        let target = addons_root.join(folder);
        match std::fs::symlink_metadata(&target) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(InstallError::Filesystem(error)),
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(InstallError::UnsafeFolder(target))
            }
            Ok(_) => {}
        }
        std::fs::remove_dir_all(&target).map_err(InstallError::Filesystem)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    use ed25519_dalek::{Signer, SigningKey};

    use crate::update_engine::manifest::{FileKind, ManifestSignature};

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(0);
    const TEST_SHA256: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const TEST_URL: &str =
        "https://raw.githubusercontent.com/warcrafted-server/launcher/main/addons/questhelper.tar";

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "warcrafted-optional-addons-test-{}-{sequence}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).expect("crear directorio de prueba");
            Self(path)
        }

        fn addons_root(&self) -> PathBuf {
            let root = addons_root(&self.0);
            std::fs::create_dir_all(&root).expect("crear raíz de addons");
            root
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn addon(id: &str, folders: &[&str]) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "name": "Addon de prueba",
            "description": "Addon opcional de prueba.",
            "author": "WarCrafted",
            "version": "1.2.0",
            "license": "GPL-3.0-or-later",
            "homepage": "https://github.com/warcrafted-server/launcher",
            "sha256": TEST_SHA256,
            "sizeBytes": 1024,
            "source": { "url": TEST_URL },
            "folders": folders,
        })
    }

    fn signed_catalog_with(
        schema_version: u32,
        catalog_version: u64,
        addons: Vec<serde_json::Value>,
    ) -> (Vec<u8>, BTreeMap<String, VerifyingKey>) {
        let signing_key = SigningKey::from_bytes(&[9; 32]);
        let mut document = serde_json::json!({
            "schemaVersion": schema_version,
            "catalogVersion": catalog_version,
            "publishedAt": "2026-10-09T12:00:00Z",
            "addons": addons,
        });
        let canonical = manifest::canonical_json(&document);
        let signature = signing_key.sign(&canonical);
        document["signature"] = serde_json::json!({
            "keyId": "test-key",
            "algorithm": "ed25519",
            "value": signature.to_bytes().iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
        });
        let keys = BTreeMap::from([("test-key".to_string(), signing_key.verifying_key())]);
        (serde_json::to_vec(&document).unwrap(), keys)
    }

    fn signed_catalog(
        catalog_version: u64,
        addons: Vec<serde_json::Value>,
    ) -> (Vec<u8>, BTreeMap<String, VerifyingKey>) {
        signed_catalog_with(CATALOG_SCHEMA_VERSION, catalog_version, addons)
    }

    fn manifest_with_files(entries: &[(&str, FileRole)]) -> Manifest {
        Manifest {
            schema_version: 1,
            realm: "icetracks".into(),
            channel: "production".into(),
            client_build: 12340,
            manifest_version: 1,
            published_at: "2026-10-09T00:00:00Z".into(),
            min_launcher_version: "0.1.0".into(),
            files: entries
                .iter()
                .map(|(path, role)| ManifestFile {
                    path: (*path).into(),
                    role: *role,
                    kind: FileKind::ClientBase,
                    addon_group: None,
                    size_bytes: 1,
                    sha256: TEST_SHA256.into(),
                    source: Some(FileSource {
                        url: "https://raw.githubusercontent.com/warcrafted-server/launcher/x".into(),
                        compressed_size_bytes: 1,
                        compression: Compression::None,
                    }),
                    assembly: None,
                })
                .collect(),
            signature: ManifestSignature {
                key_id: "test-key".into(),
                algorithm: "ed25519".into(),
                value: String::new(),
            },
        }
    }

    #[test]
    fn parses_and_validates_a_signed_catalog() {
        let (document, keys) = signed_catalog(3, vec![addon("questhelper", &["QuestHelper"])]);
        let catalog = parse_catalog(&document, &keys, None).expect("catálogo válido");
        assert_eq!(catalog.catalog_version, 3);
        assert_eq!(catalog.addons.len(), 1);
        assert_eq!(catalog.addons[0].id, "questhelper");
        assert_eq!(catalog.addons[0].folders, vec!["QuestHelper".to_string()]);
    }

    #[test]
    fn rejects_a_catalog_signed_by_an_unknown_key() {
        let (document, _) = signed_catalog(3, vec![addon("questhelper", &["QuestHelper"])]);
        let other = SigningKey::from_bytes(&[11; 32]).verifying_key();
        let keys = BTreeMap::from([("test-key".to_string(), other)]);
        assert!(matches!(
            parse_catalog(&document, &keys, None),
            Err(CatalogError::Signature(_))
        ));
    }

    #[test]
    fn rejects_a_catalog_older_than_the_applied_one() {
        let (document, keys) = signed_catalog(4, vec![addon("questhelper", &["QuestHelper"])]);
        assert_eq!(
            parse_catalog(&document, &keys, Some(5)),
            Err(CatalogError::CatalogRollback {
                candidate: 4,
                applied: 5
            })
        );
        assert!(parse_catalog(&document, &keys, Some(4)).is_ok());
    }

    #[test]
    fn rejects_an_unsupported_schema_version() {
        let (document, keys) =
            signed_catalog_with(2, 3, vec![addon("questhelper", &["QuestHelper"])]);
        assert_eq!(
            parse_catalog(&document, &keys, None),
            Err(CatalogError::UnsupportedSchema(2))
        );
    }

    #[test]
    fn rejects_duplicate_addon_ids_and_folders() {
        let (document, keys) = signed_catalog(
            1,
            vec![
                addon("questhelper", &["QuestHelper"]),
                addon("questhelper", &["OtraCarpeta"]),
            ],
        );
        assert_eq!(
            parse_catalog(&document, &keys, None),
            Err(CatalogError::DuplicateAddonId("questhelper".into()))
        );

        let (document, keys) = signed_catalog(
            1,
            vec![
                addon("questhelper", &["Compartido"]),
                addon("otro-addon", &["CompArtido"]),
            ],
        );
        assert_eq!(
            parse_catalog(&document, &keys, None),
            Err(CatalogError::DuplicateFolder("CompArtido".into()))
        );
    }

    #[test]
    fn rejects_folders_that_escape_the_client() {
        for folder in ["..", "/etc/passwd", "sub/dir", "Blizzard_Auction"] {
            let (document, keys) = signed_catalog(1, vec![addon("questhelper", &[folder])]);
            assert!(
                matches!(
                    parse_catalog(&document, &keys, None),
                    Err(CatalogError::InvalidField(_))
                ),
                "la carpeta {folder:?} debería rechazarse"
            );
        }
    }

    #[test]
    fn rejects_an_addon_downloaded_from_a_foreign_host() {
        let mut entry = addon("questhelper", &["QuestHelper"]);
        entry["source"]["url"] = serde_json::json!("https://example.com/questhelper.tar");
        let (document, keys) = signed_catalog(1, vec![entry]);
        assert!(matches!(
            parse_catalog(&document, &keys, None),
            Err(CatalogError::InvalidField(_))
        ));
    }

    #[test]
    fn rejects_invalid_addon_fields() {
        let cases: [(&str, serde_json::Value); 6] = [
            ("id", serde_json::json!("Quest Helper")),
            ("name", serde_json::json!("  ")),
            ("version", serde_json::json!("1.2")),
            ("sha256", serde_json::json!("A".repeat(64))),
            ("sizeBytes", serde_json::json!(0)),
            ("homepage", serde_json::json!("http://github.com/warcrafted-server/launcher")),
        ];
        for (field, value) in cases {
            let mut entry = addon("questhelper", &["QuestHelper"]);
            entry[field] = value;
            let (document, keys) = signed_catalog(1, vec![entry]);
            assert!(
                matches!(
                    parse_catalog(&document, &keys, None),
                    Err(CatalogError::InvalidField(_))
                ),
                "el campo {field} debería rechazarse"
            );
        }
    }

    #[test]
    fn accepts_only_single_normal_folder_names() {
        for folder in ["QuestHelper", "DBM-Core", "addon_1"] {
            assert!(
                validate_folder_name(folder).is_ok(),
                "{folder:?} debería aceptarse"
            );
        }
        let too_long = "a".repeat(101);
        for folder in ["", "..", ".", "sub/dir", "/etc/passwd", too_long.as_str()] {
            assert!(
                validate_folder_name(folder).is_err(),
                "{folder:?} debería rechazarse"
            );
        }
    }

    #[test]
    fn extracts_required_addon_folders_from_the_manifest() {
        let manifest = manifest_with_files(&[
            (
                "Interface/AddOns/RuneEngraver/RuneEngraver.toc",
                FileRole::Required,
            ),
            ("Interface/AddOns/RuneEngraver/extra.lua", FileRole::Required),
            (
                "Interface/AddOns/OpcionalDePrueba/opcional.toc",
                FileRole::Optional,
            ),
            ("Data/patch-W.MPQ", FileRole::Required),
            ("Interface/AddOns/AutoShoot/AutoShoot.toc", FileRole::Required),
        ]);

        let folders = required_addon_folders(&manifest)
            .into_iter()
            .collect::<Vec<_>>();

        assert_eq!(
            folders,
            vec!["AutoShoot".to_string(), "RuneEngraver".to_string()]
        );
    }

    #[test]
    fn rejects_an_optional_addon_that_uses_a_required_folder() {
        let required = BTreeSet::from(["RuneEngraver".to_string()]);
        assert_eq!(
            validate_required_addon_conflicts(&["runeengraver".to_string()], &required),
            Err(CatalogError::RequiredAddonConflict("runeengraver".into()))
        );
        assert!(
            validate_required_addon_conflicts(&["QuestHelper".to_string()], &required).is_ok()
        );
    }

    #[test]
    fn decides_the_state_of_an_optional_addon() {
        let record = InstalledAddon {
            id: "questhelper".into(),
            version: "1.0.0".into(),
            folders: vec!["QuestHelper".into()],
        };

        assert_eq!(
            decide_addon_state("1.0.0", None, false),
            AddonInstallState::NotInstalled
        );
        assert_eq!(
            decide_addon_state("1.0.0", Some(&record), false),
            AddonInstallState::NotInstalled
        );
        assert_eq!(
            decide_addon_state("1.0.0", Some(&record), true),
            AddonInstallState::Installed
        );
        assert_eq!(
            decide_addon_state("2.0.0", Some(&record), true),
            AddonInstallState::UpdateAvailable
        );
    }

    #[test]
    fn finds_installed_addons_by_id() {
        let installed = vec![InstalledAddon {
            id: "questhelper".into(),
            version: "1.0.0".into(),
            folders: vec![],
        }];

        assert!(find_installed(&installed, "questhelper").is_some());
        assert!(find_installed(&installed, "otro-addon").is_none());
    }

    #[test]
    fn addon_folders_present_requires_every_normal_folder() {
        let directory = TestDirectory::new();
        let root = directory.addons_root();
        std::fs::create_dir_all(root.join("QuestHelper")).expect("crear QuestHelper");
        std::fs::create_dir_all(root.join("QuestHelper_Lib")).expect("crear librería");
        std::fs::write(root.join("Fichero"), b"x").expect("crear fichero");

        assert!(addon_folders_present(&root, &["QuestHelper".into()]));
        assert!(addon_folders_present(
            &root,
            &["QuestHelper".into(), "QuestHelper_Lib".into()]
        ));
        assert!(!addon_folders_present(
            &root,
            &["QuestHelper".into(), "Falta".into()]
        ));
        assert!(!addon_folders_present(&root, &[]));
        assert!(!addon_folders_present(&root, &["../fuera".into()]));
        assert!(!addon_folders_present(&root, &["Fichero".into()]));
    }

    #[cfg(unix)]
    #[test]
    fn addon_folders_present_rejects_symlinked_folders() {
        let directory = TestDirectory::new();
        let root = directory.addons_root();
        let outside = directory.0.join("fuera");
        std::fs::create_dir_all(&outside).expect("crear carpeta externa");
        std::os::unix::fs::symlink(&outside, root.join("QuestHelper")).expect("crear enlace");

        assert!(!addon_folders_present(&root, &["QuestHelper".into()]));
    }

    #[test]
    fn uninstalls_only_the_registered_folders() {
        let directory = TestDirectory::new();
        let root = directory.addons_root();
        std::fs::create_dir_all(root.join("QuestHelper/libs")).expect("crear addon");
        std::fs::write(root.join("QuestHelper/libs/lib.lua"), b"return {}").expect("crear librería");
        std::fs::create_dir_all(root.join("Ajeno")).expect("crear carpeta ajena");

        let installed = InstalledAddon {
            id: "questhelper".into(),
            version: "1.0.0".into(),
            folders: vec!["QuestHelper".into(), "YaNoEsta".into()],
        };

        uninstall_addon(&directory.0, &installed).expect("desinstalar");

        assert!(!root.join("QuestHelper").exists());
        assert!(root.join("Ajeno").exists());
    }

    #[test]
    fn uninstall_rejects_invalid_and_unsafe_folders() {
        let directory = TestDirectory::new();
        let root = directory.addons_root();
        let escaping = InstalledAddon {
            id: "questhelper".into(),
            version: "1.0.0".into(),
            folders: vec!["../fuera".into()],
        };
        assert!(matches!(
            uninstall_addon(&directory.0, &escaping),
            Err(InstallError::InvalidFolder(_))
        ));

        std::fs::write(root.join("Fichero"), b"x").expect("crear fichero");
        let file = InstalledAddon {
            id: "questhelper".into(),
            version: "1.0.0".into(),
            folders: vec!["Fichero".into()],
        };
        assert!(matches!(
            uninstall_addon(&directory.0, &file),
            Err(InstallError::UnsafeFolder(_))
        ));
        assert!(root.join("Fichero").exists());
    }

    #[test]
    fn resolves_the_addons_root_inside_the_client() {
        assert_eq!(
            addons_root(Path::new("/cliente")),
            Path::new("/cliente/Interface/AddOns")
        );
    }

    fn sha256_hex(contents: &[u8]) -> String {
        use sha2::{Digest, Sha256};

        Sha256::digest(contents)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn build_quest_helper_tar(core: &[u8]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (path, contents) in [
            ("QuestHelper/QuestHelper.toc", &b"## Interface: 30300"[..]),
            ("QuestHelper/core.lua", core),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(0o644);
            builder.append_data(&mut header, path, contents).unwrap();
        }
        builder.into_inner().unwrap()
    }

    fn quest_helper_addon(url: &str, archive: &[u8]) -> OptionalAddon {
        OptionalAddon {
            id: "questhelper".into(),
            name: "QuestHelper".into(),
            description: "Guía de misiones.".into(),
            author: "WarCrafted".into(),
            version: "1.0.0".into(),
            license: "GPL-3.0-or-later".into(),
            homepage: "https://github.com/warcrafted-server/launcher".into(),
            sha256: sha256_hex(archive),
            size_bytes: archive.len() as u64,
            source: AddonSource { url: url.into() },
            folders: vec!["QuestHelper".into()],
        }
    }

    /// Servidor HTTP de prueba de una sola petición; responde al `Range` que pida el cliente.
    async fn serve_once(body: Vec<u8>) -> (String, tokio::task::JoinHandle<()>) {
        use tokio::{
            io::{AsyncReadExt, AsyncWriteExt},
            net::TcpListener,
        };

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let bytes_read = stream.read(&mut buffer).await.unwrap();
                assert_ne!(bytes_read, 0, "el cliente cerró antes de completar la petición");
                request.extend_from_slice(&buffer[..bytes_read]);
            }
            let request_text = String::from_utf8_lossy(&request).to_ascii_lowercase();
            let start = request_text.lines().find_map(|line| {
                let value = line.trim().strip_prefix("range: bytes=")?;
                value.split('-').next()?.parse::<u64>().ok()
            });

            match start {
                Some(offset) => {
                    let from = offset as usize;
                    let tail = &body[from..];
                    let headers = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {from}-{last}/{total}\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n",
                        last = body.len().saturating_sub(1),
                        total = body.len(),
                        length = tail.len(),
                    );
                    stream.write_all(headers.as_bytes()).await.unwrap();
                    stream.write_all(tail).await.unwrap();
                }
                None => {
                    let headers = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    stream.write_all(headers.as_bytes()).await.unwrap();
                    stream.write_all(&body).await.unwrap();
                }
            }
        });
        (format!("http://{address}/questhelper.tar"), task)
    }

    #[tokio::test]
    async fn installs_updates_and_uninstalls_an_addon_from_a_local_server() {
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let directory = TestDirectory::new();
        let client_dir = directory.0.join("cliente");
        let addons_root = addons_root(&client_dir);
        std::fs::create_dir_all(addons_root.join("Ajeno")).expect("crear carpeta ajena");
        let staging_root = directory.0.join("staging");
        let downloads_root = directory.0.join("downloads");
        std::fs::create_dir_all(&staging_root).expect("crear staging");
        std::fs::create_dir_all(&downloads_root).expect("crear descargas");

        let first = build_quest_helper_tar(b"return 1");
        let (url, server) = serve_once(first.clone()).await;
        let addon = quest_helper_addon(&url, &first);
        let record = install_addon(
            &client,
            &addon,
            &client_dir,
            &staging_root,
            &downloads_root,
            None,
            false,
            &mut |_| {},
        )
        .await
        .expect("instalar addon");
        server.await.unwrap();

        assert_eq!(record.version, "1.0.0");
        assert_eq!(
            std::fs::read(addons_root.join("QuestHelper/core.lua")).unwrap(),
            b"return 1"
        );
        assert!(addons_root.join("Ajeno").is_dir());

        let second = build_quest_helper_tar(b"return 2");
        let (url, server) = serve_once(second.clone()).await;
        let mut updated = quest_helper_addon(&url, &second);
        updated.version = "1.1.0".into();
        let record = install_addon(
            &client,
            &updated,
            &client_dir,
            &staging_root,
            &downloads_root,
            Some(&record),
            false,
            &mut |_| {},
        )
        .await
        .expect("actualizar addon");
        server.await.unwrap();

        assert_eq!(record.version, "1.1.0");
        assert_eq!(
            std::fs::read(addons_root.join("QuestHelper/core.lua")).unwrap(),
            b"return 2"
        );

        uninstall_addon(&client_dir, &record).expect("desinstalar");

        assert!(!addons_root.join("QuestHelper").exists());
        assert!(addons_root.join("Ajeno").is_dir());
    }

    #[tokio::test]
    async fn install_refuses_a_folder_the_launcher_does_not_manage() {
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let directory = TestDirectory::new();
        let client_dir = directory.0.join("cliente");
        let addons_root = addons_root(&client_dir);
        std::fs::create_dir_all(addons_root.join("QuestHelper")).expect("crear carpeta manual");
        std::fs::write(addons_root.join("QuestHelper/manual.lua"), b"return 0")
            .expect("crear addon manual");

        let archive = build_quest_helper_tar(b"return 1");
        let (url, _server) = serve_once(archive.clone()).await;
        let addon = quest_helper_addon(&url, &archive);

        let result = install_addon(
            &client,
            &addon,
            &client_dir,
            &directory.0,
            &directory.0,
            None,
            false,
            &mut |_| {},
        )
        .await;

        assert!(matches!(result, Err(InstallError::ExistingFolder(_))));
        assert!(addons_root.join("QuestHelper/manual.lua").is_file());
    }
}


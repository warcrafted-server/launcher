use std::{
    cmp::Ordering,
    collections::BTreeMap,
    error::Error,
    fmt,
    path::{Component, Path, PathBuf},
};

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub schema_version: u32,
    pub realm: String,
    pub channel: String,
    pub client_build: u32,
    pub manifest_version: u64,
    pub published_at: String,
    pub min_launcher_version: String,
    pub files: Vec<ManifestFile>,
    pub signature: ManifestSignature,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManifestFile {
    pub path: String,
    pub role: FileRole,
    pub kind: FileKind,
    #[serde(default)]
    pub addon_group: Option<String>,
    pub size_bytes: u64,
    pub sha256: String,
    /// Exactamente uno de `source`/`assembly` debe estar presente (decisión 0003):
    /// un archivo se descarga de una pieza o se ensambla a partir de fragmentos.
    #[serde(default)]
    pub source: Option<FileSource>,
    #[serde(default)]
    pub assembly: Option<Assembly>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Assembly {
    pub part_size_bytes: u64,
    pub parts: Vec<AssemblyPart>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AssemblyPart {
    pub sha256: String,
    pub size_bytes: u64,
    pub source: FileSource,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FileRole {
    Required,
    Optional,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    ClientBase,
    ClientPatch,
    Addon,
    Config,
    Archive,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileSource {
    pub url: String,
    pub compressed_size_bytes: u64,
    pub compression: Compression,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Compression {
    None,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManifestSignature {
    pub key_id: String,
    pub algorithm: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedTarget<'a> {
    pub realm: &'a str,
    pub channel: &'a str,
    pub client_build: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    InvalidJson(String),
    MissingSignature,
    InvalidSignature(String),
    UnknownSigningKey(String),
    UnsupportedSchema(u32),
    InvalidField(String),
    TargetMismatch(&'static str),
    ManifestRollback { candidate: u64, applied: u64 },
    LauncherTooOld { minimum: String, current: String },
    InvalidVersion(String),
    InvalidPath(String),
}

impl fmt::Display for ManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson(message) => write!(formatter, "JSON de manifest inválido: {message}"),
            Self::MissingSignature => write!(formatter, "falta la firma del manifest"),
            Self::InvalidSignature(message) => {
                write!(formatter, "firma de manifest inválida: {message}")
            }
            Self::UnknownSigningKey(key_id) => {
                write!(formatter, "clave de firma desconocida: {key_id}")
            }
            Self::UnsupportedSchema(version) => {
                write!(formatter, "schemaVersion no compatible: {version}")
            }
            Self::InvalidField(field) => write!(formatter, "campo de manifest inválido: {field}"),
            Self::TargetMismatch(field) => write!(formatter, "el manifest no coincide con {field}"),
            Self::ManifestRollback { candidate, applied } => write!(
                formatter,
                "manifestVersion {candidate} no es mayor que la versión aplicada {applied}"
            ),
            Self::LauncherTooOld { minimum, current } => write!(
                formatter,
                "el manifest requiere launcher {minimum} y la versión actual es {current}"
            ),
            Self::InvalidVersion(version) => {
                write!(formatter, "versión semántica inválida: {version}")
            }
            Self::InvalidPath(path) => write!(formatter, "ruta de manifest inválida: {path}"),
        }
    }
}

impl Error for ManifestError {}

/// Verifica la firma Ed25519 de un documento firmado y devuelve su JSON sin `signature`.
///
/// La firma se calcula sobre el JSON canónico del documento sin el campo `signature`, igual que
/// genera `sign_and_verify` en `docs/contenido/generar_manifest.py`. Es reutilizable por cualquier
/// documento firmado con el mismo mecanismo (por ejemplo el catálogo de addons opcionales).
pub fn verify_signed_document(
    document: &[u8],
    trusted_keys: &BTreeMap<String, VerifyingKey>,
) -> Result<Value, ManifestError> {
    let mut value: Value = serde_json::from_slice(document)
        .map_err(|error| ManifestError::InvalidJson(error.to_string()))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| ManifestError::InvalidField("raíz JSON".into()))?;
    let signature_value = object
        .get("signature")
        .cloned()
        .ok_or(ManifestError::MissingSignature)?;
    let signature: ManifestSignature = serde_json::from_value(signature_value)
        .map_err(|error| ManifestError::InvalidSignature(error.to_string()))?;
    if signature.algorithm != "ed25519" {
        return Err(ManifestError::InvalidSignature(
            "algorithm debe ser ed25519".into(),
        ));
    }
    let key = trusted_keys
        .get(&signature.key_id)
        .ok_or_else(|| ManifestError::UnknownSigningKey(signature.key_id.clone()))?;

    object.remove("signature");
    let canonical = canonical_json(&value);
    let signature_bytes = decode_signature(&signature.value)?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|error| ManifestError::InvalidSignature(error.to_string()))?;
    key.verify(&canonical, &signature)
        .map_err(|error| ManifestError::InvalidSignature(error.to_string()))?;

    Ok(value)
}

/// Verifica la firma antes de convertir el resto de los campos al modelo tipado.
pub fn parse_manifest(
    document: &[u8],
    trusted_keys: &BTreeMap<String, VerifyingKey>,
) -> Result<Manifest, ManifestError> {
    verify_signed_document(document, trusted_keys)?;

    let mut manifest: Manifest = serde_json::from_slice(document)
        .map_err(|error| ManifestError::InvalidJson(error.to_string()))?;
    validate_manifest(&mut manifest)?;
    Ok(manifest)
}

fn validate_manifest(manifest: &mut Manifest) -> Result<(), ManifestError> {
    if manifest.schema_version != 1 {
        return Err(ManifestError::UnsupportedSchema(manifest.schema_version));
    }
    if manifest.realm.trim().is_empty() || manifest.channel.trim().is_empty() {
        return Err(ManifestError::InvalidField("realm/channel".into()));
    }
    if !is_rfc3339_utc(&manifest.published_at) {
        return Err(ManifestError::InvalidField("publishedAt".into()));
    }
    parse_semver(&manifest.min_launcher_version)?;

    for file in &mut manifest.files {
        file.path = normalize_manifest_path(&file.path)?;
        if file.sha256.len() != 64 || !file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(ManifestError::InvalidField(format!(
                "sha256 de {}",
                file.path
            )));
        }
        match (&file.source, &file.assembly) {
            (Some(source), None) => {
                if !source.url.starts_with("https://") {
                    return Err(ManifestError::InvalidField(format!(
                        "source.url de {}",
                        file.path
                    )));
                }
            }
            (None, Some(assembly)) => {
                if assembly.parts.is_empty() {
                    return Err(ManifestError::InvalidField(format!(
                        "assembly.parts de {} no puede estar vacío",
                        file.path
                    )));
                }
                for part in &assembly.parts {
                    if part.sha256.len() != 64
                        || !part.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
                    {
                        return Err(ManifestError::InvalidField(format!(
                            "sha256 de fragmento de {}",
                            file.path
                        )));
                    }
                    if !part.source.url.starts_with("https://") {
                        return Err(ManifestError::InvalidField(format!(
                            "source.url de fragmento de {}",
                            file.path
                        )));
                    }
                }
            }
            (Some(_), Some(_)) | (None, None) => {
                return Err(ManifestError::InvalidField(format!(
                    "{} debe declarar exactamente uno de source/assembly",
                    file.path
                )));
            }
        }
        match (file.kind, file.addon_group.as_deref()) {
            (FileKind::Addon, Some(group)) if !group.trim().is_empty() => {}
            (FileKind::Addon, _) => {
                return Err(ManifestError::InvalidField(format!(
                    "addonGroup de {}",
                    file.path
                )))
            }
            (_, None) => {}
            (_, Some(_)) => {
                return Err(ManifestError::InvalidField(format!(
                    "addonGroup de {}",
                    file.path
                )))
            }
        }
    }
    Ok(())
}

pub fn validate_target(
    manifest: &Manifest,
    expected: &ExpectedTarget<'_>,
) -> Result<(), ManifestError> {
    if manifest.realm != expected.realm {
        return Err(ManifestError::TargetMismatch("realm"));
    }
    if manifest.channel != expected.channel {
        return Err(ManifestError::TargetMismatch("channel"));
    }
    if manifest.client_build != expected.client_build {
        return Err(ManifestError::TargetMismatch("clientBuild"));
    }
    Ok(())
}

pub fn validate_source_hosts(
    manifest: &Manifest,
    allowed_hosts: &[&str],
) -> Result<(), ManifestError> {
    for file in &manifest.files {
        match (&file.source, &file.assembly) {
            (Some(source), _) => validate_source_host(&source.url, allowed_hosts, &file.path)?,
            (None, Some(assembly)) => {
                for part in &assembly.parts {
                    validate_source_host(&part.source.url, allowed_hosts, &file.path)?;
                }
            }
            (None, None) => {}
        }
    }
    Ok(())
}

/// Comprueba que una URL de descarga sea HTTPS y de uno de los hosts permitidos.
///
/// Reutilizable por cualquier documento que declare orígenes de descarga (por ejemplo el catálogo
/// de addons opcionales); `path` solo se usa para identificar el origen en el mensaje de error.
pub fn validate_source_host(
    url: &str,
    allowed_hosts: &[&str],
    path: &str,
) -> Result<(), ManifestError> {
    let authority = url
        .strip_prefix("https://")
        .and_then(|url| url.split(['/', '?', '#']).next())
        .filter(|authority| !authority.is_empty() && !authority.contains('@'));
    let host = authority.filter(|authority| {
        authority
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
    });
    if !host.is_some_and(|host| {
        allowed_hosts
            .iter()
            .any(|allowed| host.eq_ignore_ascii_case(allowed))
    }) {
        return Err(ManifestError::InvalidField(format!(
            "host de source.url no permitido para {path}"
        )));
    }
    Ok(())
}

/// Valida que `version` sea una versión semántica admitida (mismas reglas que el manifest).
pub fn validate_semver(version: &str) -> Result<(), ManifestError> {
    parse_semver(version).map(|_| ())
}

pub fn validate_versions(
    manifest: &Manifest,
    applied_manifest_version: u64,
    current_launcher_version: &str,
) -> Result<(), ManifestError> {
    if manifest.manifest_version <= applied_manifest_version {
        return Err(ManifestError::ManifestRollback {
            candidate: manifest.manifest_version,
            applied: applied_manifest_version,
        });
    }
    let minimum = parse_semver(&manifest.min_launcher_version)?;
    let current = parse_semver(current_launcher_version)?;
    if compare_semver(&minimum, &current) == Ordering::Greater {
        return Err(ManifestError::LauncherTooOld {
            minimum: manifest.min_launcher_version.clone(),
            current: current_launcher_version.to_owned(),
        });
    }
    Ok(())
}

/// Normaliza separadores redundantes y `.`; rechaza rutas que puedan escapar de la raíz.
pub fn normalize_manifest_path(path: &str) -> Result<String, ManifestError> {
    let invalid = || ManifestError::InvalidPath(path.to_owned());
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path.contains(':')
        || path.starts_with("//")
    {
        return Err(invalid());
    }

    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => continue,
            ".." => return Err(invalid()),
            _ => parts.push(part),
        }
    }
    if parts.is_empty() {
        return Err(invalid());
    }
    Ok(parts.join("/"))
}

/// Resuelve una ruta validada bajo `root` sin consultar el filesystem.
pub fn resolve_manifest_path(root: &Path, path: &str) -> Result<PathBuf, ManifestError> {
    let normalized = normalize_manifest_path(path)?;
    let mut resolved = root.to_path_buf();
    for component in Path::new(&normalized).components() {
        match component {
            Component::Normal(part) => resolved.push(part),
            _ => return Err(ManifestError::InvalidPath(path.to_owned())),
        }
    }
    if !resolved.starts_with(root) {
        return Err(ManifestError::InvalidPath(path.to_owned()));
    }
    Ok(resolved)
}

pub(crate) fn canonical_json(value: &Value) -> Vec<u8> {
    fn write(value: &Value, output: &mut String) {
        match value {
            Value::Object(object) => {
                output.push('{');
                let mut entries: Vec<_> = object.iter().collect();
                entries.sort_unstable_by(|left, right| left.0.cmp(right.0));
                for (index, (key, value)) in entries.into_iter().enumerate() {
                    if index > 0 {
                        output.push(',');
                    }
                    output.push_str(&serde_json::to_string(key).expect("serializar clave JSON"));
                    output.push(':');
                    write(value, output);
                }
                output.push('}');
            }
            Value::Array(values) => {
                output.push('[');
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        output.push(',');
                    }
                    write(value, output);
                }
                output.push(']');
            }
            _ => output.push_str(&serde_json::to_string(value).expect("serializar valor JSON")),
        }
    }

    let mut output = String::new();
    write(value, &mut output);
    output.into_bytes()
}

fn decode_signature(value: &str) -> Result<Vec<u8>, ManifestError> {
    let decoded = if value.len() == 128 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        decode_hex(value)
    } else {
        decode_base64(value)
    };
    match decoded {
        Some(bytes) if bytes.len() == 64 => Ok(bytes),
        _ => Err(ManifestError::InvalidSignature(
            "value debe codificar 64 bytes en hexadecimal o Base64".into(),
        )),
    }
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| Some((hex_digit(pair[0])? << 4) | hex_digit(pair[1])?))
        .collect()
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn decode_base64(value: &str) -> Option<Vec<u8>> {
    let bytes = value.as_bytes();
    if bytes.len() % 4 != 0 {
        return None;
    }
    let mut output = Vec::with_capacity(bytes.len() / 4 * 3);
    for (chunk_index, chunk) in bytes.chunks_exact(4).enumerate() {
        let last = chunk_index + 1 == bytes.len() / 4;
        let padding = if chunk[3] == b'=' {
            if !last {
                return None;
            }
            if chunk[2] == b'=' {
                2
            } else {
                1
            }
        } else {
            0
        };
        if (padding == 2 && chunk[2] != b'=') || (padding > 0 && !last) {
            return None;
        }
        let a = base64_digit(chunk[0])? as u32;
        let b = base64_digit(chunk[1])? as u32;
        let c = if padding == 2 {
            0
        } else {
            base64_digit(chunk[2])? as u32
        };
        let d = if padding > 0 {
            0
        } else {
            base64_digit(chunk[3])? as u32
        };
        if (padding == 2 && b & 0x0f != 0) || (padding == 1 && c & 0x03 != 0) {
            return None;
        }
        let block = (a << 18) | (b << 12) | (c << 6) | d;
        output.push((block >> 16) as u8);
        if padding < 2 {
            output.push((block >> 8) as u8);
        }
        if padding == 0 {
            output.push(block as u8);
        }
    }
    Some(output)
}

fn base64_digit(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Semver<'a> {
    core: [u64; 3],
    prerelease: Option<&'a str>,
}

fn parse_semver(version: &str) -> Result<Semver<'_>, ManifestError> {
    let without_build = version.split_once('+').map_or(version, |(core, _)| core);
    let (core, prerelease) = without_build
        .split_once('-')
        .map_or((without_build, None), |(core, prerelease)| {
            (core, Some(prerelease))
        });
    let components: Vec<_> = core.split('.').collect();
    if components.len() != 3 {
        return Err(ManifestError::InvalidVersion(version.to_owned()));
    }
    let mut numbers = [0; 3];
    for (index, component) in components.iter().enumerate() {
        if component.is_empty()
            || (component.len() > 1 && component.starts_with('0'))
            || !component.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(ManifestError::InvalidVersion(version.to_owned()));
        }
        numbers[index] = component
            .parse()
            .map_err(|_| ManifestError::InvalidVersion(version.to_owned()))?;
    }
    if prerelease.is_some_and(|identifiers| {
        identifiers.is_empty()
            || identifiers.split('.').any(|identifier| {
                identifier.is_empty()
                    || !identifier
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                    || (identifier.len() > 1
                        && identifier.starts_with('0')
                        && identifier.bytes().all(|byte| byte.is_ascii_digit()))
            })
    }) {
        return Err(ManifestError::InvalidVersion(version.to_owned()));
    }
    Ok(Semver {
        core: numbers,
        prerelease,
    })
}

fn compare_semver(left: &Semver<'_>, right: &Semver<'_>) -> Ordering {
    left.core
        .cmp(&right.core)
        .then_with(|| match (&left.prerelease, &right.prerelease) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(left), Some(right)) => compare_prerelease(left, right),
        })
}

fn compare_prerelease(left: &str, right: &str) -> Ordering {
    let left_parts: Vec<_> = left.split('.').collect();
    let right_parts: Vec<_> = right.split('.').collect();
    for (left_part, right_part) in left_parts.iter().zip(&right_parts) {
        let left_number = left_part.parse::<u64>();
        let right_number = right_part.parse::<u64>();
        let order = match (left_number, right_number) {
            (Ok(left), Ok(right)) => left.cmp(&right),
            (Ok(_), Err(_)) => Ordering::Less,
            (Err(_), Ok(_)) => Ordering::Greater,
            (Err(_), Err(_)) => left_part.cmp(right_part),
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    left_parts.len().cmp(&right_parts.len())
}

/// Comprueba que el texto sea una marca de tiempo RFC 3339 en UTC (`...Z`).
///
/// Reutilizable por otros documentos firmados (catálogo de addons opcionales).
pub fn is_rfc3339_utc(value: &str) -> bool {
    let Some(timestamp) = value.strip_suffix('Z') else {
        return false;
    };
    let Some((date, time)) = timestamp.split_once('T') else {
        return false;
    };
    let date_parts: Vec<_> = date.split('-').collect();
    let time_parts: Vec<_> = time.split(':').collect();
    date_parts.len() == 3
        && date_parts[0].len() == 4
        && date_parts
            .iter()
            .all(|part| part.bytes().all(|byte| byte.is_ascii_digit()))
        && time_parts.len() == 3
        && time_parts[0].len() == 2
        && time_parts[1].len() == 2
        && time_parts[0..2]
            .iter()
            .all(|part| part.bytes().all(|byte| byte.is_ascii_digit()))
        && time_parts[2]
            .split_once('.')
            .map_or(time_parts[2].len() == 2, |(seconds, fraction)| {
                seconds.len() == 2
                    && !fraction.is_empty()
                    && fraction.bytes().all(|byte| byte.is_ascii_digit())
            })
        && time_parts[2].split_once('.').map_or_else(
            || time_parts[2].bytes().all(|byte| byte.is_ascii_digit()),
            |(seconds, _)| seconds.bytes().all(|byte| byte.is_ascii_digit()),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn signed_manifest() -> (Vec<u8>, BTreeMap<String, VerifyingKey>) {
        let signing_key = SigningKey::from_bytes(&[7; 32]);
        let mut document = serde_json::json!({
            "schemaVersion": 1,
            "realm": "icetracks",
            "channel": "production",
            "clientBuild": 12340,
            "manifestVersion": 7,
            "publishedAt": "2026-10-07T12:00:00Z",
            "minLauncherVersion": "1.0.0",
            "files": [{
                "path": "Data/patch-W.MPQ",
                "role": "required",
                "kind": "clientPatch",
                "sizeBytes": 52428800,
                "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "source": {
                    "url": "https://example.com/patch-W.MPQ",
                    "compressedSizeBytes": 20971520,
                    "compression": "none"
                }
            }]
        });
        let canonical = canonical_json(&document);
        let signature = signing_key.sign(&canonical);
        document["signature"] = serde_json::json!({
            "keyId": "test-key",
            "algorithm": "ed25519",
            "value": signature.to_bytes().iter().map(|byte| format!("{byte:02x}")).collect::<String>()
        });
        let keys = BTreeMap::from([("test-key".into(), signing_key.verifying_key())]);
        (serde_json::to_vec(&document).unwrap(), keys)
    }

    #[test]
    fn parses_and_verifies_valid_manifest() {
        let (document, keys) = signed_manifest();
        let manifest = parse_manifest(&document, &keys).unwrap();
        assert_eq!(manifest.realm, "icetracks");
        assert_eq!(manifest.files[0].role, FileRole::Required);
        assert_eq!(manifest.files[0].kind, FileKind::ClientPatch);
    }

    #[test]
    fn parses_signed_manifest_with_archive_file() {
        let signing_key = SigningKey::from_bytes(&[8; 32]);
        let document = serde_json::json!({
            "schemaVersion": 1,
            "realm": "icetracks",
            "channel": "production",
            "clientBuild": 12340,
            "manifestVersion": 8,
            "publishedAt": "2026-10-07T12:00:00Z",
            "minLauncherVersion": "1.0.0",
            "files": [{
                "path": "Interface/AddOns/RuneEngraver.tar",
                "role": "required",
                "kind": "archive",
                "sizeBytes": 1024,
                "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "source": {
                    "url": "https://example.com/RuneEngraver.tar",
                    "compressedSizeBytes": 1024,
                    "compression": "none"
                }
            }]
        });
        let document = sign_document(document, &signing_key);
        let keys = BTreeMap::from([("test-key".into(), signing_key.verifying_key())]);

        let manifest = parse_manifest(&document, &keys).unwrap();

        assert_eq!(manifest.files[0].kind, FileKind::Archive);
        assert_eq!(
            serde_json::to_value(manifest.files[0].kind).unwrap(),
            Value::String("archive".into())
        );
    }

    fn sign_document(mut document: Value, signing_key: &SigningKey) -> Vec<u8> {
        let canonical = canonical_json(&document);
        let signature = signing_key.sign(&canonical);
        document["signature"] = serde_json::json!({
            "keyId": "test-key",
            "algorithm": "ed25519",
            "value": signature.to_bytes().iter().map(|byte| format!("{byte:02x}")).collect::<String>()
        });
        serde_json::to_vec(&document).unwrap()
    }

    #[test]
    fn parses_manifest_with_assembled_file() {
        let signing_key = SigningKey::from_bytes(&[9; 32]);
        let document = serde_json::json!({
            "schemaVersion": 1,
            "realm": "icetracks",
            "channel": "production",
            "clientBuild": 12340,
            "manifestVersion": 1,
            "publishedAt": "2026-10-07T12:00:00Z",
            "minLauncherVersion": "1.0.0",
            "files": [{
                "path": "Data/client-base.pkg",
                "role": "required",
                "kind": "clientBase",
                "sizeBytes": 4000000000_u64,
                "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "assembly": {
                    "partSizeBytes": 2000000000,
                    "parts": [
                        {
                            "sha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                            "sizeBytes": 2000000000,
                            "source": { "url": "https://example.com/part-000", "compressedSizeBytes": 2000000000, "compression": "none" }
                        },
                        {
                            "sha256": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
                            "sizeBytes": 2000000000,
                            "source": { "url": "https://example.com/part-001", "compressedSizeBytes": 2000000000, "compression": "none" }
                        }
                    ]
                }
            }]
        });
        let document = sign_document(document, &signing_key);
        let keys = BTreeMap::from([("test-key".into(), signing_key.verifying_key())]);
        let manifest = parse_manifest(&document, &keys).unwrap();
        let assembly = manifest.files[0].assembly.as_ref().unwrap();
        assert!(manifest.files[0].source.is_none());
        assert_eq!(assembly.parts.len(), 2);
        assert!(validate_source_hosts(&manifest, &["example.com"]).is_ok());
        assert!(validate_source_hosts(&manifest, &["other.com"]).is_err());
    }

    #[test]
    fn rejects_file_declaring_both_or_neither_source_and_assembly() {
        let signing_key = SigningKey::from_bytes(&[11; 32]);
        let base_file = serde_json::json!({
            "path": "Data/ambiguous.pkg",
            "role": "required",
            "kind": "clientBase",
            "sizeBytes": 10,
            "sha256": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
        });

        let mut neither = base_file.clone();
        let document = serde_json::json!({
            "schemaVersion": 1, "realm": "icetracks", "channel": "production",
            "clientBuild": 12340, "manifestVersion": 1,
            "publishedAt": "2026-10-07T12:00:00Z", "minLauncherVersion": "1.0.0",
            "files": [neither.take()]
        });
        let document = sign_document(document, &signing_key);
        let keys = BTreeMap::from([("test-key".into(), signing_key.verifying_key())]);
        assert!(matches!(
            parse_manifest(&document, &keys),
            Err(ManifestError::InvalidField(_))
        ));

        let mut both = base_file;
        both["source"] = serde_json::json!({ "url": "https://example.com/x", "compressedSizeBytes": 1, "compression": "none" });
        both["assembly"] = serde_json::json!({ "partSizeBytes": 1, "parts": [] });
        let document = serde_json::json!({
            "schemaVersion": 1, "realm": "icetracks", "channel": "production",
            "clientBuild": 12340, "manifestVersion": 1,
            "publishedAt": "2026-10-07T12:00:00Z", "minLauncherVersion": "1.0.0",
            "files": [both]
        });
        let document = sign_document(document, &signing_key);
        assert!(matches!(
            parse_manifest(&document, &keys),
            Err(ManifestError::InvalidField(_))
        ));
    }

    #[test]
    fn rejects_invalid_signature() {
        let (mut document, keys) = signed_manifest();
        let mut value: Value = serde_json::from_slice(&document).unwrap();
        value["files"][0]["sizeBytes"] = serde_json::json!(12);
        document = serde_json::to_vec(&value).unwrap();
        assert!(matches!(
            parse_manifest(&document, &keys),
            Err(ManifestError::InvalidSignature(_))
        ));
    }

    #[test]
    fn rejects_rollback_and_too_new_launcher_requirement() {
        let (document, keys) = signed_manifest();
        let manifest = parse_manifest(&document, &keys).unwrap();
        assert!(matches!(
            validate_versions(&manifest, 7, "1.0.0"),
            Err(ManifestError::ManifestRollback { .. })
        ));
        assert!(matches!(
            validate_versions(&manifest, 8, "1.0.0"),
            Err(ManifestError::ManifestRollback { .. })
        ));
        assert!(matches!(
            validate_versions(&manifest, 6, "0.9.9"),
            Err(ManifestError::LauncherTooOld { .. })
        ));
    }

    #[test]
    fn source_hosts_must_be_explicitly_allowed() {
        let (document, keys) = signed_manifest();
        let manifest = parse_manifest(&document, &keys).unwrap();
        assert!(validate_source_hosts(&manifest, &["EXAMPLE.com"]).is_ok());
        assert!(validate_source_hosts(&manifest, &["github.com"]).is_err());
    }

    #[test]
    fn normalizes_safe_paths_and_rejects_traversal_forms() {
        assert_eq!(
            normalize_manifest_path("Data//./patch.MPQ").unwrap(),
            "Data/patch.MPQ"
        );
        assert_eq!(
            resolve_manifest_path(Path::new("/game"), "Data/patch.MPQ").unwrap(),
            PathBuf::from("/game/Data/patch.MPQ")
        );
        for path in [
            "../escape",
            "Data/../escape",
            "/absolute",
            "C:/game/file",
            "\\\\server\\share",
            "\\rooted",
        ] {
            assert!(normalize_manifest_path(path).is_err(), "se aceptó {path}");
        }
    }
}

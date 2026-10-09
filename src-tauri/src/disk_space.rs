use std::path::Path;

/// Margen sobre lo pendiente para el staging: cada archivo se descarga completo antes de
/// sustituir al anterior, y un fragmentado necesita sitio para las piezas y el resultado.
const MARGIN_PERCENT: u64 = 10;

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

pub(crate) fn required_bytes(pending_bytes: u64) -> u64 {
    pending_bytes.saturating_add(pending_bytes / 100 * MARGIN_PERCENT)
}

pub(crate) fn ensure_enough(pending_bytes: u64, free_bytes: u64) -> Result<(), String> {
    let required = required_bytes(pending_bytes);
    if free_bytes >= required {
        return Ok(());
    }
    Err(format!(
        "No hay espacio suficiente en el disco: se necesitan {} y hay {} libres. Libera espacio o elige otra carpeta.",
        format_gib(required),
        format_gib(free_bytes)
    ))
}

pub(crate) fn format_gib(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / GIB).replace('.', ",")
}

/// Espacio libre del volumen que contiene `path`; si la carpeta aún no existe, el de su
/// primer ancestro existente.
pub(crate) fn free_bytes(path: &Path) -> Result<u64, String> {
    let existing = path
        .ancestors()
        .find(|candidate| candidate.exists())
        .ok_or_else(|| "no se pudo localizar el disco de la carpeta del cliente".to_string())?;
    fs4::available_space(existing)
        .map_err(|error| format!("no se pudo consultar el espacio libre del disco: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn requires_ten_percent_over_the_pending_bytes() {
        assert_eq!(required_bytes(1000), 1100);
        assert_eq!(required_bytes(0), 0);
        assert_eq!(required_bytes(u64::MAX), u64::MAX);
    }

    #[test]
    fn accepts_exactly_the_required_space_and_rejects_less() {
        assert!(ensure_enough(100 * GB, 110 * GB).is_ok());
        assert!(ensure_enough(100 * GB, 110 * GB - 1).is_err());
        assert!(ensure_enough(0, 0).is_ok());
    }

    #[test]
    fn the_error_states_needed_and_free_space() {
        let message = ensure_enough(10 * GB, 5 * GB).unwrap_err();
        assert!(message.contains("11,0 GB"), "{message}");
        assert!(message.contains("5,0 GB"), "{message}");
    }

    #[test]
    fn free_bytes_uses_the_nearest_existing_ancestor() {
        let missing = std::env::temp_dir().join("warcrafted-no-existe").join("hijo");
        assert!(free_bytes(&missing).unwrap() > 0);
    }
}

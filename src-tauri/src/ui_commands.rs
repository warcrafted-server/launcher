#[tauri::command]
pub(crate) fn greet(name: &str) -> String {
    format!("¡Hola, {}! Te saludamos desde Rust.", name)
}

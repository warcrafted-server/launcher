mod ui_commands;
pub mod update_engine;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![ui_commands::greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

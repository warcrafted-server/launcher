mod disk_space;
mod download_progress;
mod settings;
mod ui_commands;
pub mod update_engine;
mod verify_cache;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![
            ui_commands::get_settings,
            ui_commands::set_client_dir,
            ui_commands::create_install_dir,
            ui_commands::clear_cache,
            ui_commands::cancel_operation,
            ui_commands::check_client_status,
            ui_commands::update_client,
            ui_commands::launch_game,
            ui_commands::get_game_running,
            ui_commands::get_disk_space
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

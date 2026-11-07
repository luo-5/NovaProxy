pub mod commands;
pub mod core;
pub mod generator;
pub mod model;
pub mod monitor;
pub mod parser;
pub mod storage;
pub mod system;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![commands::get_app_version,])
        .run(tauri::generate_context!())
        .expect("error while running NovaProxy application");
}

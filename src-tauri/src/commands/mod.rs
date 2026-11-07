//! Tauri IPC commands
//! Bridge between frontend requests and backend services.

#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

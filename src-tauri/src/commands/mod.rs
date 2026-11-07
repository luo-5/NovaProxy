//! Tauri IPC commands
//! Thin bridge between frontend requests and backend services.

use crate::core::{default_minimal_singbox_config, CoreManager, CoreStartRequest, CoreStatusInfo};
use crate::monitor::CoreLogEntry;
use crate::storage::Database;
use tauri::State;

pub struct AppState {
    pub db: Database,
    pub core_manager: CoreManager,
}

#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
pub fn core_default_config(mixed_port: Option<u16>) -> String {
    default_minimal_singbox_config(mixed_port.unwrap_or(7890))
}

#[tauri::command]
pub async fn core_start(
    state: State<'_, AppState>,
    request: Option<CoreStartRequest>,
) -> Result<CoreStatusInfo, String> {
    let req = request.unwrap_or_default();
    state
        .core_manager
        .start(req)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn core_stop(state: State<'_, AppState>) -> Result<CoreStatusInfo, String> {
    state.core_manager.stop().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn core_restart(
    state: State<'_, AppState>,
    request: Option<CoreStartRequest>,
) -> Result<CoreStatusInfo, String> {
    let req = request.unwrap_or_default();
    state
        .core_manager
        .restart(req)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn core_status(state: State<'_, AppState>) -> Result<CoreStatusInfo, String> {
    Ok(state.core_manager.status())
}

#[tauri::command]
pub fn core_logs(
    state: State<'_, AppState>,
    since_seq: Option<u64>,
    limit: Option<usize>,
) -> Result<Vec<CoreLogEntry>, String> {
    Ok(state.core_manager.logs(since_seq, limit))
}

#[tauri::command]
pub fn core_clear_logs(state: State<'_, AppState>) -> Result<(), String> {
    state.core_manager.clear_logs();
    Ok(())
}

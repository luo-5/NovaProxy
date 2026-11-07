pub mod commands;
pub mod core;
pub mod generator;
pub mod model;
pub mod monitor;
pub mod parser;
pub mod storage;
pub mod system;

use commands::AppState;
use core::{CoreEventSink, CoreManager, CoreStatusInfo};
use monitor::CoreLogEntry;
use std::path::PathBuf;
use std::sync::Arc;
use storage::Database;
use tauri::{AppHandle, Emitter, Manager};

struct TauriEventSink {
    app_handle: AppHandle,
}

impl CoreEventSink for TauriEventSink {
    fn on_log(&self, entry: &CoreLogEntry) {
        let _ = self.app_handle.emit("core://log", entry);
    }

    fn on_status_change(&self, status: &CoreStatusInfo) {
        let _ = self.app_handle.emit("core://status", status);
    }
}

fn resolve_cores_dir(app: &tauri::App) -> PathBuf {
    if let Ok(res_dir) = app.path().resource_dir() {
        let bundled = res_dir.join("resources").join("cores");
        if bundled.exists() {
            return bundled;
        }
        let direct = res_dir.join("cores");
        if direct.exists() {
            return direct;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("cores")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let app_data_dir = dirs::data_local_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("NovaProxy");
            std::fs::create_dir_all(&app_data_dir)?;

            let db_path = app_data_dir.join("novaproxy.db");
            let db = Database::open(&db_path)
                .map_err(|e| std::io::Error::other(format!("Failed to open database: {}", e)))?;

            let cores_dir = resolve_cores_dir(app);
            let work_dir = app_data_dir.join("runtime");
            let core_manager = CoreManager::new(cores_dir, work_dir);

            let sink = Arc::new(TauriEventSink {
                app_handle: app.handle().clone(),
            });
            core_manager.set_event_sink(sink);

            app.manage(AppState { db, core_manager });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_version,
            commands::core_default_config,
            commands::core_start,
            commands::core_stop,
            commands::core_restart,
            commands::core_status,
            commands::core_logs,
            commands::core_clear_logs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running NovaProxy application");
}

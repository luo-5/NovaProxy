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
use system::SystemProxyManager;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
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

            let sysproxy_manager = SystemProxyManager::new();
            let _ = system::register_url_schemes();

            let toggle_i = MenuItemBuilder::with_id("toggle", "Show / Hide Window").build(app)?;
            let quit_i = MenuItemBuilder::with_id("quit", "Exit").build(app)?;
            let menu = MenuBuilder::new(app)
                .item(&toggle_i)
                .separator()
                .item(&quit_i)
                .build()?;

            let _tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("NovaProxy")
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "quit" => {
                        let state = app.state::<AppState>();
                        let _ = state.sysproxy_manager.restore_original();
                        app.exit(0);
                    }
                    "toggle" => {
                        if let Some(win) = app.get_webview_window("main") {
                            if win.is_visible().unwrap_or(false) {
                                let _ = win.hide();
                            } else {
                                let _ = win.show();
                                let _ = win.set_focus();
                            }
                        }
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(win) = app.get_webview_window("main") {
                            if win.is_visible().unwrap_or(false) {
                                let _ = win.hide();
                            } else {
                                let _ = win.show();
                                let _ = win.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;

            app.manage(AppState {
                db,
                core_manager,
                sysproxy_manager,
            });
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
            commands::parse_link,
            commands::export_node_link,
            commands::parse_subscription_text,
            commands::import_clipboard_nodes,
            commands::list_nodes,
            commands::upsert_node,
            commands::delete_node,
            commands::list_subscriptions,
            commands::upsert_subscription,
            commands::delete_subscription,
            commands::refresh_subscription,
            commands::encode_node_qr,
            commands::decode_qr_image_base64,
            commands::scan_screen_qr,
            commands::get_routing_settings,
            commands::save_routing_settings,
            commands::list_routing_rules,
            commands::upsert_routing_rule,
            commands::delete_routing_rule,
            commands::reorder_routing_rules,
            commands::get_dns_settings,
            commands::save_dns_settings,
            commands::generate_config,
            commands::get_sysproxy_status,
            commands::set_sysproxy,
            commands::is_process_elevated,
            commands::request_admin_relaunch,
            commands::get_autostart_status,
            commands::set_autostart_status,
            commands::handle_deep_link_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running NovaProxy application");
}

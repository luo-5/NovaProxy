pub mod commands;
pub mod core;
pub mod generator;
pub mod model;
pub mod monitor;
pub mod parser;
pub mod storage;
pub mod system;

use commands::{enqueue_deep_link_subscription, AppState};
use core::{CoreEventSink, CoreManager, CoreStatusInfo};
use monitor::{CoreLogEntry, HealthMonitor, NovaTracingLayer, PersistentLogWriter};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use storage::Database;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use system::{
    acquire_single_instance_or_forward, extract_deep_link_from_args,
    register_url_schemes_with_options, SingleInstanceGuard, SingleInstanceOutcome,
};
use system::{NetworkChangeListener, SystemProxyManager};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use tauri::menu::{MenuBuilder, MenuItemBuilder};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent};
#[cfg(target_os = "android")]
use tauri_plugin_android_vpn::AndroidVpnExt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

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

struct RuntimeHandles {
    health_monitor: Arc<HealthMonitor>,
    net_listener: Arc<NetworkChangeListener>,
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    _single_instance: Option<SingleInstanceGuard>,
    shutdown_done: AtomicBool,
}

static PANIC_SYSPROXY: OnceLock<SystemProxyManager> = OnceLock::new();

fn install_sysproxy_panic_hook(sysproxy: SystemProxyManager) {
    let _ = PANIC_SYSPROXY.set(sysproxy);
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(sp) = PANIC_SYSPROXY.get() {
            let _ = sp.restore_original();
        }
        previous_hook(info);
    }));
}

fn focus_main_window(app: &AppHandle) {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let _ = app;
}

fn handle_incoming_deep_link(app: &AppHandle, uri: Option<String>) {
    focus_main_window(app);
    if let Some(raw_uri) = uri {
        let state = app.state::<AppState>();
        match enqueue_deep_link_subscription(&state.db, &raw_uri) {
            Ok(Some(sub)) => {
                let _ = app.emit("app://deep-link", &sub);
            }
            Ok(None) => {
                tracing::warn!("Ignored unrecognized deep-link URI: {}", raw_uri);
            }
            Err(err) => {
                tracing::error!("Failed to import deep-link subscription: {}", err);
            }
        }
    }
}

fn perform_graceful_shutdown(app: &AppHandle) {
    if let Some(handles) = app.try_state::<RuntimeHandles>() {
        if handles.shutdown_done.swap(true, Ordering::SeqCst) {
            return;
        }
        handles.health_monitor.stop();
        handles.net_listener.stop();
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        if let Some(ref guard) = handles._single_instance {
            guard.shutdown();
        }
    }

    #[cfg(target_os = "android")]
    if let Ok(status) = app.android_vpn().get_vpn_status() {
        if status.running {
            let _ = app.android_vpn().stop_vpn();
        }
    }

    if let Some(state) = app.try_state::<AppState>() {
        let cm = state.core_manager.clone();
        let sp = state.sysproxy_manager.clone();
        tauri::async_runtime::block_on(async move {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(4), cm.stop()).await;
        });
        let _ = sp.restore_original();
    } else if let Some(sp) = PANIC_SYSPROXY.get() {
        let _ = sp.restore_original();
    }
}

fn resolve_cores_dir(app: &tauri::App) -> PathBuf {
    #[cfg(target_os = "android")]
    if let Ok(Some(native_dir)) = app.android_vpn().get_native_lib_dir() {
        let candidate = PathBuf::from(native_dir);
        if candidate.exists() {
            return candidate;
        }
    }

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
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let desktop_data_dir = dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("NovaProxy");

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    if let Err(err) = std::fs::create_dir_all(&desktop_data_dir) {
        eprintln!("Failed to create app data directory: {}", err);
        return;
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let initial_deep_link = extract_deep_link_from_args(std::env::args());
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let initial_deep_link: Option<String> = None;

    let deferred_app_handle: Arc<OnceLock<AppHandle>> = Arc::new(OnceLock::new());
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let callback_handle = deferred_app_handle.clone();

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let single_instance_guard = match acquire_single_instance_or_forward(
        &desktop_data_dir,
        initial_deep_link.as_deref(),
        move |incoming_uri| {
            if let Some(app) = callback_handle.get() {
                handle_incoming_deep_link(app, incoming_uri);
            }
        },
    ) {
        Ok(SingleInstanceOutcome::ForwardedToExisting) => {
            return;
        }
        Ok(SingleInstanceOutcome::Primary(guard)) => Some(guard),
        Err(err) => {
            eprintln!("Single-instance guard unavailable: {}", err);
            None
        }
    };

    let sysproxy_manager = SystemProxyManager::new();
    install_sysproxy_panic_hook(sysproxy_manager.clone());

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let app_data_setup_opt = Some(desktop_data_dir);
    #[cfg(any(target_os = "android", target_os = "ios"))]
    let app_data_setup_opt: Option<PathBuf> = None;

    let initial_link_setup = initial_deep_link;
    let handle_slot = deferred_app_handle;
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let mut guard_slot = single_instance_guard;

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_android_vpn::init())
        .setup(move |app| {
            let app_data_dir = match app_data_setup_opt {
                Some(dir) => dir,
                None => app
                    .path()
                    .app_local_data_dir()
                    .or_else(|_| app.path().app_data_dir())
                    .unwrap_or_else(|_| std::env::temp_dir().join("NovaProxy")),
            };
            std::fs::create_dir_all(&app_data_dir)?;

            let db_path = app_data_dir.join("novaproxy.db");
            let db = Database::open(&db_path)
                .map_err(|e| std::io::Error::other(format!("Failed to open database: {}", e)))?;

            // Periodic database integrity verification & snapshot
            let _ = db.check_integrity();
            let _ = db.backup_database(app_data_dir.join("novaproxy.backup.db"));

            let cores_dir = resolve_cores_dir(app);
            let work_dir = app_data_dir.join("runtime");
            let core_manager = CoreManager::new(cores_dir, work_dir);

            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            core_manager.set_override_cores_dir(app_data_dir.join("cores"));

            // Initialize persistent rolling log files + tracing layer before starting monitors
            let log_dir = app_data_dir.join("logs");
            match PersistentLogWriter::open(&log_dir) {
                Ok(writer) => core_manager.attach_log_writer(writer),
                Err(err) => eprintln!("Failed to initialize persistent log writer: {}", err),
            }
            let _ = tracing_subscriber::registry()
                .with(NovaTracingLayer::new(core_manager.logs_buffer().clone()))
                .try_init();

            let sink = Arc::new(TauriEventSink {
                app_handle: app.handle().clone(),
            });
            core_manager.set_event_sink(sink);

            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            {
                let register_clash = db
                    .get_app_settings()
                    .map(|s| s.register_clash_scheme)
                    .unwrap_or(false);
                let _ = register_url_schemes_with_options(register_clash);
            }

            // Background monitors: network interface changes & active node health probe
            let net_listener = Arc::new(NetworkChangeListener::with_telemetry(
                sysproxy_manager.clone(),
                core_manager.telemetry().clone(),
            ));
            if let Err(err) = net_listener.clone().start() {
                tracing::error!(error = %err, "Failed to start network change listener");
            }

            let health_monitor = Arc::new(HealthMonitor::new(db.clone(), core_manager.clone()));
            tauri::async_runtime::spawn(health_monitor.clone().run_loop());

            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            {
                let toggle_i =
                    MenuItemBuilder::with_id("toggle", "Show / Hide Window").build(app)?;
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
                            perform_graceful_shutdown(app);
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
            }

            app.manage(AppState {
                db,
                core_manager,
                sysproxy_manager: sysproxy_manager.clone(),
                app_data_dir,
            });

            app.manage(RuntimeHandles {
                health_monitor,
                net_listener,
                #[cfg(not(any(target_os = "android", target_os = "ios")))]
                _single_instance: guard_slot.take(),
                shutdown_done: AtomicBool::new(false),
            });

            let _ = handle_slot.set(app.handle().clone());

            if let Some(uri) = initial_link_setup {
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    handle_incoming_deep_link(&app_handle, Some(uri));
                });
            }

            tracing::info!(
                os = std::env::consts::OS,
                arch = std::env::consts::ARCH,
                "NovaProxy runtime initialized"
            );

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_version,
            commands::get_platform_info,
            commands::core_default_config,
            commands::core_start,
            commands::core_stop,
            commands::core_restart,
            commands::core_status,
            commands::core_logs,
            commands::core_clear_logs,
            commands::get_telemetry_summary,
            commands::get_log_files,
            commands::read_log_file,
            commands::log_frontend_event,
            commands::export_diagnostic_bundle,
            commands::prepare_android_vpn,
            commands::get_android_vpn_status,
            commands::parse_link,
            commands::export_node_link,
            commands::parse_subscription_text,
            commands::import_clipboard_nodes,
            commands::list_groups,
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
            commands::get_app_settings,
            commands::save_app_settings,
            commands::test_node_latency,
            commands::test_all_nodes_latency,
            commands::select_active_node,
            commands::get_core_traffic,
            commands::get_core_connections,
            commands::close_core_connection,
            commands::get_service_status,
            commands::check_wintun_status,
            commands::get_url_scheme_status,
            commands::set_clash_url_scheme,
            commands::get_core_versions,
            commands::check_core_update,
            commands::install_core_update,
            commands::check_database_integrity,
            commands::backup_database,
            commands::export_diagnostics_report,
        ]);

    match builder.build(tauri::generate_context!()) {
        Ok(app) => {
            app.run(|app_handle, event| match event {
                RunEvent::Exit => {
                    perform_graceful_shutdown(app_handle);
                }
                #[cfg(any(target_os = "macos", target_os = "ios"))]
                RunEvent::Opened { urls } => {
                    for url in urls {
                        handle_incoming_deep_link(app_handle, Some(url.to_string()));
                    }
                }
                _ => {}
            });
        }
        Err(err) => {
            tracing::error!(error = %err, "Failed to initialize NovaProxy Tauri runtime");
            eprintln!("Failed to initialize NovaProxy Tauri runtime: {}", err);
            if let Some(sp) = PANIC_SYSPROXY.get() {
                let _ = sp.restore_original();
            }
        }
    }
}

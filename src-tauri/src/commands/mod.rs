//! Tauri IPC commands
//! Thin bridge between frontend requests and backend services.

use crate::core::{
    default_minimal_singbox_config, CoreBinaryStatus, CoreLifecycleState, CoreManager,
    CoreStartRequest, CoreStatusInfo, CoreUpdateCheckResult, CoreUpdateResult, CoreUpdater,
};
use crate::model::{
    AppSettings, CoreType, DnsSettings, LogLevel, NodeGroup, ProxyNode, RoutingSettings, RuleItem,
    Subscription,
};
use crate::monitor::{
    close_clash_connection, fetch_clash_connections, fetch_clash_traffic, list_log_files,
    read_log_file_tail, sanitize_log_message, test_nodes_batch, test_single_node_latency,
    CoreLogEntry, DiagnosticBundleResult, LatencyTestMode, LogFileInfo, LogStream,
    TelemetrySummary, DEFAULT_LATENCY_TEST_URL, DEFAULT_TCP_TIMEOUT_MS, DEFAULT_URL_TIMEOUT_MS,
};
use crate::parser::base64::decode_base64_permissive;
use crate::parser::qrcode::{
    decode_qr_from_image_bytes, decode_qr_from_screen, encode_node_qr_svg,
};
use crate::parser::{
    apply_userinfo_to_subscription, fetch_and_parse_subscription, node_to_share_link,
    parse_share_link, parse_subscription_content, SubscriptionParseReport,
};
use crate::storage::Database;
use crate::system::{
    check_wintun_status as probe_wintun_status, detect_platform_info,
    get_url_scheme_status as probe_url_scheme_status, is_autostart_enabled, is_elevated,
    parse_deep_link, query_service_state, relaunch_as_admin, set_autostart,
    set_clash_url_scheme as apply_clash_url_scheme, PlatformInfo, ServiceState, SysProxySnapshot,
    SystemProxyManager, UrlSchemeStatus, WintunStatus,
};
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, State};
use tauri_plugin_android_vpn::{AndroidVpnExt, AndroidVpnStatus};

pub struct AppState {
    pub db: Database,
    pub core_manager: CoreManager,
    pub sysproxy_manager: SystemProxyManager,
    pub app_data_dir: PathBuf,
}

#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command]
pub fn get_platform_info() -> PlatformInfo {
    detect_platform_info()
}

#[tauri::command]
pub fn core_default_config(mixed_port: Option<u16>) -> String {
    default_minimal_singbox_config(mixed_port.unwrap_or(7890))
}

fn plan_core_request_from_db(
    state: &AppState,
    explicit_core: Option<CoreType>,
    mixed_port: Option<u16>,
    allow_lan: Option<bool>,
    auto_port_fallback: Option<bool>,
    active_node_id: Option<&str>,
) -> Result<CoreStartRequest, String> {
    let app_settings = state.db.get_app_settings().map_err(|e| e.to_string())?;
    let nodes = state.db.list_nodes().map_err(|e| e.to_string())?;
    let groups = state.db.list_groups().map_err(|e| e.to_string())?;
    let dns = state.db.get_dns_settings().map_err(|e| e.to_string())?;
    let routing = state.db.get_routing_settings().map_err(|e| e.to_string())?;

    state
        .core_manager
        .plan_start_request(
            explicit_core,
            mixed_port,
            allow_lan,
            auto_port_fallback,
            &nodes,
            &groups,
            &app_settings,
            &dns,
            &routing,
            active_node_id,
        )
        .map_err(|e| e.to_string())
}

fn sync_sysproxy_after_start(state: &AppState, status: &CoreStatusInfo) {
    if !matches!(status.lifecycle, CoreLifecycleState::Running) {
        return;
    }
    if let Ok(settings) = state.db.get_app_settings() {
        if settings.system_proxy {
            let port = status.actual_mixed_port.unwrap_or(settings.mixed_port);
            let srv = format!("127.0.0.1:{}", port);
            if let Err(e) = state.sysproxy_manager.set_proxy(&srv, None) {
                tracing::warn!("Failed to apply system proxy after core start: {}", e);
            }
        }
    }
}

fn clear_stale_active_node(state: &AppState, deleted_node_id: Option<&str>) {
    if let Ok(mut settings) = state.db.get_app_settings() {
        let should_clear = match (deleted_node_id, settings.active_node_id.as_deref()) {
            (Some(target), Some(active)) => target == active,
            (None, Some(active)) => matches!(state.db.get_node(active), Ok(None)),
            _ => false,
        };
        if should_clear {
            settings.active_node_id = None;
            let _ = state.db.save_app_settings(&settings);
        }
    }
}

fn sync_android_vpn_after_start(
    app: &AppHandle,
    state: &AppState,
    status: &CoreStatusInfo,
) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        if !matches!(status.lifecycle, CoreLifecycleState::Running) {
            return Ok(());
        }
        let settings = state.db.get_app_settings().map_err(|e| e.to_string())?;
        if !settings.tun_mode {
            return Ok(());
        }
        let vpn = app.android_vpn();
        if !vpn.prepare_vpn()? {
            return Err("Android VPN permission was denied by user".to_string());
        }
        let dns = state.db.get_dns_settings().ok();
        let dns_server = dns
            .and_then(|d| {
                d.remote_dns.into_iter().find_map(|entry| {
                    let trimmed = entry.trim();
                    if trimmed.parse::<std::net::IpAddr>().is_ok() {
                        Some(trimmed.to_string())
                    } else {
                        None
                    }
                })
            })
            .or_else(|| Some("1.1.1.1".to_string()));
        let port = status.actual_mixed_port.unwrap_or(settings.mixed_port);
        let log_file = Some(
            state
                .app_data_dir
                .join("logs")
                .join("hev-socks5-tunnel.log")
                .to_string_lossy()
                .to_string(),
        );
        vpn.start_vpn(tauri_plugin_android_vpn::StartVpnOptions {
            mixed_port: port,
            ipv6: settings.ipv6,
            mtu: 8500,
            dns_server,
            log_file,
        })?;
    }
    #[cfg(not(target_os = "android"))]
    let _ = (app, state, status);
    Ok(())
}

fn stop_android_vpn_if_active(app: &AppHandle) {
    #[cfg(target_os = "android")]
    if let Err(err) = app.android_vpn().stop_vpn() {
        tracing::warn!("Failed to stop Android VpnService: {}", err);
    }
    #[cfg(not(target_os = "android"))]
    let _ = app;
}

#[tauri::command]
pub async fn core_start(
    app: AppHandle,
    state: State<'_, AppState>,
    request: Option<CoreStartRequest>,
) -> Result<CoreStatusInfo, String> {
    let mut req = request.unwrap_or_default();
    if req.config_json.is_none() {
        req = plan_core_request_from_db(
            &state,
            req.core_type,
            req.mixed_port,
            req.allow_lan,
            req.auto_port_fallback,
            None,
        )?;
    }
    let status = state
        .core_manager
        .start(req)
        .await
        .map_err(|e| e.to_string())?;
    sync_sysproxy_after_start(&state, &status);
    if let Err(err) = sync_android_vpn_after_start(&app, &state, &status) {
        let _ = state.core_manager.stop().await;
        return Err(err);
    }
    Ok(status)
}

#[tauri::command]
pub async fn core_stop(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<CoreStatusInfo, String> {
    stop_android_vpn_if_active(&app);
    let status = state.core_manager.stop().await.map_err(|e| e.to_string())?;
    if state.sysproxy_manager.is_active() {
        if let Err(e) = state.sysproxy_manager.restore_original() {
            tracing::warn!(
                "Failed to restore original system proxy on core stop: {}",
                e
            );
        }
    }
    Ok(status)
}

#[tauri::command]
pub async fn core_restart(
    app: AppHandle,
    state: State<'_, AppState>,
    request: Option<CoreStartRequest>,
) -> Result<CoreStatusInfo, String> {
    stop_android_vpn_if_active(&app);
    let mut req = request.unwrap_or_default();
    if req.config_json.is_none() {
        req = plan_core_request_from_db(
            &state,
            req.core_type,
            req.mixed_port,
            req.allow_lan,
            req.auto_port_fallback,
            None,
        )?;
    }
    let status = state
        .core_manager
        .restart(req)
        .await
        .map_err(|e| e.to_string())?;
    sync_sysproxy_after_start(&state, &status);
    if let Err(err) = sync_android_vpn_after_start(&app, &state, &status) {
        let _ = state.core_manager.stop().await;
        return Err(err);
    }
    Ok(status)
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

// ==================== Phase P2: Share Links, Subscriptions & QR Code ====================

#[tauri::command]
pub fn parse_link(uri: String) -> Result<ProxyNode, String> {
    parse_share_link(&uri).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn export_node_link(node: ProxyNode) -> Result<String, String> {
    node_to_share_link(&node).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn parse_subscription_text(content: String) -> Result<SubscriptionParseReport, String> {
    parse_subscription_content(&content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn import_clipboard_nodes(
    state: State<'_, AppState>,
    content: String,
) -> Result<SubscriptionParseReport, String> {
    let mut report = parse_subscription_content(&content).map_err(|e| e.to_string())?;
    let parsed_nodes = report.nodes.clone();
    let saved = state
        .db
        .import_manual_nodes(report.nodes)
        .map_err(|e| e.to_string())?;
    report.groups = state
        .db
        .sync_groups(None, &parsed_nodes, &saved, report.groups)
        .map_err(|e| e.to_string())?;
    report.nodes = saved;
    state.core_manager.mark_config_dirty();
    Ok(report)
}

#[tauri::command]
pub fn list_groups(state: State<'_, AppState>) -> Result<Vec<NodeGroup>, String> {
    state.db.list_groups().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_nodes(
    state: State<'_, AppState>,
    subscription_id: Option<String>,
) -> Result<Vec<ProxyNode>, String> {
    match subscription_id {
        Some(sub_id) => state
            .db
            .list_nodes_by_subscription(&sub_id)
            .map_err(|e| e.to_string()),
        None => state.db.list_nodes().map_err(|e| e.to_string()),
    }
}

#[tauri::command]
pub fn upsert_node(state: State<'_, AppState>, node: ProxyNode) -> Result<ProxyNode, String> {
    state.db.upsert_node(&node).map_err(|e| e.to_string())?;
    state.core_manager.mark_config_dirty();
    Ok(node)
}

#[tauri::command]
pub fn delete_node(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let deleted = state.db.delete_node(&id).map_err(|e| e.to_string())?;
    if deleted {
        clear_stale_active_node(&state, Some(&id));
        state.core_manager.mark_config_dirty();
    }
    Ok(deleted)
}

#[tauri::command]
pub fn list_subscriptions(state: State<'_, AppState>) -> Result<Vec<Subscription>, String> {
    state.db.list_subscriptions().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn upsert_subscription(
    state: State<'_, AppState>,
    subscription: Subscription,
) -> Result<Subscription, String> {
    state
        .db
        .upsert_subscription(&subscription)
        .map_err(|e| e.to_string())?;
    state.core_manager.mark_config_dirty();
    Ok(subscription)
}

#[tauri::command]
pub fn delete_subscription(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let deleted = state
        .db
        .delete_subscription(&id)
        .map_err(|e| e.to_string())?;
    if deleted {
        clear_stale_active_node(&state, None);
        state.core_manager.mark_config_dirty();
    }
    Ok(deleted)
}

#[tauri::command]
pub async fn refresh_subscription(
    state: State<'_, AppState>,
    id: String,
) -> Result<SubscriptionParseReport, String> {
    let mut sub = state
        .db
        .get_subscription(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Subscription not found: {id}"))?;

    let local_port = state
        .core_manager
        .status()
        .actual_mixed_port
        .or_else(|| state.db.get_app_settings().ok().map(|s| s.mixed_port));

    let (mut report, user_info) = match fetch_and_parse_subscription(&sub, local_port, 15, 2).await
    {
        Ok(res) => {
            state
                .core_manager
                .telemetry()
                .record_subscription_refresh(true);
            res
        }
        Err(err) => {
            state
                .core_manager
                .telemetry()
                .record_subscription_refresh(false);
            tracing::warn!(
                target: "novaproxy::subscription",
                "Failed to refresh subscription '{}': {}",
                sub.name,
                err
            );
            return Err(err.to_string());
        }
    };
    tracing::info!(
        target: "novaproxy::subscription",
        "Refreshed subscription '{}': {} nodes",
        sub.name,
        report.nodes.len()
    );

    apply_userinfo_to_subscription(&mut sub, &user_info);
    state
        .db
        .upsert_subscription(&sub)
        .map_err(|e| e.to_string())?;

    let parsed_nodes = report.nodes.clone();
    let synced_nodes = state
        .db
        .sync_subscription_nodes(&sub.id, report.nodes)
        .map_err(|e| e.to_string())?;

    report.groups = state
        .db
        .sync_groups(Some(&sub.id), &parsed_nodes, &synced_nodes, report.groups)
        .map_err(|e| e.to_string())?;
    report.nodes = synced_nodes;
    clear_stale_active_node(&state, None);
    state.core_manager.mark_config_dirty();
    Ok(report)
}

#[tauri::command]
pub fn encode_node_qr(node: ProxyNode) -> Result<String, String> {
    encode_node_qr_svg(&node).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn decode_qr_image_base64(image_base64: String) -> Result<Vec<String>, String> {
    let payload = image_base64
        .split_once(',')
        .map(|(_, b64)| b64)
        .unwrap_or(&image_base64);
    let bytes = decode_base64_permissive(payload).map_err(|e| e.to_string())?;
    decode_qr_from_image_bytes(&bytes).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn scan_screen_qr() -> Result<Vec<String>, String> {
    decode_qr_from_screen().map_err(|e| e.to_string())
}

// ==================== Phase P3: Routing, DNS & Config Generation ====================

#[tauri::command]
pub fn get_routing_settings(state: State<'_, AppState>) -> Result<RoutingSettings, String> {
    state.db.get_routing_settings().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_routing_settings(
    state: State<'_, AppState>,
    settings: RoutingSettings,
) -> Result<RoutingSettings, String> {
    state
        .db
        .save_routing_settings(&settings)
        .map_err(|e| e.to_string())?;
    state.core_manager.mark_config_dirty();
    Ok(settings)
}

#[tauri::command]
pub fn list_routing_rules(state: State<'_, AppState>) -> Result<Vec<RuleItem>, String> {
    state.db.list_routing_rules().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn upsert_routing_rule(state: State<'_, AppState>, rule: RuleItem) -> Result<RuleItem, String> {
    state
        .db
        .upsert_routing_rule(&rule)
        .map_err(|e| e.to_string())?;
    state.core_manager.mark_config_dirty();
    Ok(rule)
}

#[tauri::command]
pub fn delete_routing_rule(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let deleted = state
        .db
        .delete_routing_rule(&id)
        .map_err(|e| e.to_string())?;
    if deleted {
        state.core_manager.mark_config_dirty();
    }
    Ok(deleted)
}

#[tauri::command]
pub fn reorder_routing_rules(state: State<'_, AppState>, ids: Vec<String>) -> Result<(), String> {
    state
        .db
        .reorder_routing_rules(&ids)
        .map_err(|e| e.to_string())?;
    state.core_manager.mark_config_dirty();
    Ok(())
}

#[tauri::command]
pub fn get_dns_settings(state: State<'_, AppState>) -> Result<DnsSettings, String> {
    state.db.get_dns_settings().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_dns_settings(
    state: State<'_, AppState>,
    settings: DnsSettings,
) -> Result<DnsSettings, String> {
    state
        .db
        .save_dns_settings(&settings)
        .map_err(|e| e.to_string())?;
    state.core_manager.mark_config_dirty();
    Ok(settings)
}

#[tauri::command]
pub fn generate_config(
    state: State<'_, AppState>,
    core_type: Option<CoreType>,
    active_node_id: Option<String>,
) -> Result<String, String> {
    let planned = plan_core_request_from_db(
        &state,
        core_type,
        None,
        None,
        None,
        active_node_id.as_deref(),
    )?;
    Ok(planned.config_json.unwrap_or_default())
}

// ==================== Phase P4: System Proxy, TUN & Deep Links ====================

#[tauri::command]
pub fn get_sysproxy_status() -> Result<SysProxySnapshot, String> {
    SystemProxyManager::query_current().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_sysproxy(state: State<'_, AppState>, enabled: bool) -> Result<SysProxySnapshot, String> {
    let mut settings = state.db.get_app_settings().map_err(|e| e.to_string())?;
    let status = state.core_manager.status();
    let is_running = matches!(status.lifecycle, CoreLifecycleState::Running);

    settings.system_proxy = enabled;
    state
        .db
        .save_app_settings(&settings)
        .map_err(|e| e.to_string())?;

    let snap = if enabled {
        if is_running {
            let active_port = status.actual_mixed_port.unwrap_or(settings.mixed_port);
            let srv = format!("127.0.0.1:{}", active_port);
            state
                .sysproxy_manager
                .set_proxy(&srv, None)
                .map_err(|e| e.to_string())?
        } else {
            // Core is stopped: persist preference without pointing Windows WinINet to a dead port.
            SystemProxyManager::query_current().map_err(|e| e.to_string())?
        }
    } else if state.sysproxy_manager.is_active() {
        state
            .sysproxy_manager
            .restore_original()
            .map_err(|e| e.to_string())?;
        SystemProxyManager::query_current().map_err(|e| e.to_string())?
    } else if is_running {
        state
            .sysproxy_manager
            .clear_proxy()
            .map_err(|e| e.to_string())?
    } else {
        SystemProxyManager::query_current().map_err(|e| e.to_string())?
    };

    Ok(snap)
}

#[tauri::command]
pub fn is_process_elevated() -> Result<bool, String> {
    Ok(is_elevated())
}

#[tauri::command]
pub fn request_admin_relaunch() -> Result<(), String> {
    relaunch_as_admin(&[]).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_autostart_status() -> Result<bool, String> {
    is_autostart_enabled().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_autostart_status(enabled: bool) -> Result<bool, String> {
    set_autostart(enabled).map_err(|e| e.to_string())
}

/// Helper used by both IPC command and background deep-link wakeup to store a subscription URL.
pub fn enqueue_deep_link_subscription(
    db: &Database,
    uri: &str,
) -> Result<Option<Subscription>, String> {
    let Some(action) = parse_deep_link(uri) else {
        return Ok(None);
    };

    let sub = Subscription {
        id: format!("sub-{}", uuid::Uuid::new_v4()),
        name: action
            .name
            .unwrap_or_else(|| "Imported Subscription".to_string()),
        url: action.subscription_url,
        enabled: true,
        user_agent: Some("clash.meta".to_string()),
        update_interval_mins: Some(1440),
        use_proxy: false,
        updated_at: None,
        upload: None,
        download: None,
        total: None,
        expire: None,
        home_url: None,
        sort_order: 0,
    };

    db.upsert_subscription(&sub).map_err(|e| e.to_string())?;
    Ok(Some(sub))
}

#[tauri::command]
pub async fn handle_deep_link_url(
    state: State<'_, AppState>,
    uri: String,
) -> Result<Option<Subscription>, String> {
    enqueue_deep_link_subscription(&state.db, &uri)
}

// ==================== Phase P5: App Settings, Latency & Traffic ====================

#[tauri::command]
pub fn get_app_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    state.db.get_app_settings().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_app_settings(
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    state
        .db
        .save_app_settings(&settings)
        .map_err(|e| e.to_string())?;
    state.core_manager.mark_config_dirty();
    Ok(settings)
}

#[tauri::command]
pub async fn test_node_latency(
    state: State<'_, AppState>,
    id: String,
    mode: Option<LatencyTestMode>,
    test_url: Option<String>,
) -> Result<Option<i64>, String> {
    let node = state
        .db
        .get_node(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Node not found: {id}"))?;
    let settings = state.db.get_app_settings().map_err(|e| e.to_string())?;
    let eff_mode = mode.unwrap_or(LatencyTestMode::Tcp);
    let url = test_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_LATENCY_TEST_URL);
    let timeout = match eff_mode {
        LatencyTestMode::Tcp => Duration::from_millis(DEFAULT_TCP_TIMEOUT_MS),
        LatencyTestMode::Url => Duration::from_millis(DEFAULT_URL_TIMEOUT_MS),
    };

    let res = test_single_node_latency(
        &state.core_manager,
        &node,
        &settings,
        eff_mode,
        url,
        timeout,
    )
    .await;
    let (lat, err) = match res {
        Ok(ms) => (Some(ms), None),
        Err(e) => (None, Some(e)),
    };
    state
        .core_manager
        .telemetry()
        .record_latency_probe(node.protocol, lat, err.as_deref());
    let test_type = if eff_mode == LatencyTestMode::Url {
        "url"
    } else {
        "tcp"
    };
    state
        .db
        .record_latency(&id, lat, test_type, err.as_deref())
        .map_err(|e| e.to_string())?;
    Ok(lat)
}

#[tauri::command]
pub async fn test_all_nodes_latency(
    state: State<'_, AppState>,
    mode: Option<LatencyTestMode>,
    test_url: Option<String>,
) -> Result<Vec<(String, Option<i64>)>, String> {
    let nodes = state.db.list_nodes().map_err(|e| e.to_string())?;
    let settings = state.db.get_app_settings().map_err(|e| e.to_string())?;
    let eff_mode = mode.unwrap_or(LatencyTestMode::Tcp);
    let url = test_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_LATENCY_TEST_URL);
    let timeout = match eff_mode {
        LatencyTestMode::Tcp => Duration::from_millis(DEFAULT_TCP_TIMEOUT_MS),
        LatencyTestMode::Url => Duration::from_millis(DEFAULT_URL_TIMEOUT_MS),
    };

    Ok(test_nodes_batch(
        &state.db,
        &state.core_manager,
        nodes,
        &settings,
        eff_mode,
        url,
        timeout,
    )
    .await)
}

#[tauri::command]
pub async fn select_active_node(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let mut settings = state.db.get_app_settings().map_err(|e| e.to_string())?;
    settings.active_node_id = Some(id.clone());
    state
        .db
        .save_app_settings(&settings)
        .map_err(|e| e.to_string())?;

    // If core is running, check whether existing engine topology matches the selected node
    let status = state.core_manager.status();
    if matches!(status.lifecycle, CoreLifecycleState::Running) {
        let nodes = state.db.list_nodes().map_err(|e| e.to_string())?;
        let groups = state.db.list_groups().map_err(|e| e.to_string())?;
        let dns = state.db.get_dns_settings().map_err(|e| e.to_string())?;
        let routing = state.db.get_routing_settings().map_err(|e| e.to_string())?;

        let planned = state
            .core_manager
            .plan_start_request(
                None,
                status.requested_port,
                Some(settings.allow_lan),
                Some(settings.auto_port_fallback),
                &nodes,
                &groups,
                &settings,
                &dns,
                &routing,
                Some(&id),
            )
            .map_err(|e| e.to_string())?;

        let topology_matches = !state.core_manager.is_config_dirty()
            && status.active_core == planned.core_type
            && status.upstream_core == planned.upstream_core_type
            && planned.upstream_core_type.is_none();

        if topology_matches {
            if let Some(node) = nodes.iter().find(|n| n.id == id) {
                let clash_port = status
                    .actual_clash_api_port
                    .unwrap_or(settings.clash_api_port);
                let mihomo_names = crate::generator::mihomo::resolve_mihomo_proxy_names(&nodes);
                let mut candidate_storage: Vec<String> = Vec::with_capacity(3);
                if matches!(status.active_core, Some(CoreType::Mihomo)) {
                    if let Some(mname) = mihomo_names.get(&node.id) {
                        candidate_storage.push(mname.clone());
                    }
                }
                candidate_storage.push(node.id.clone());
                candidate_storage.push(node.name.clone());
                let candidate_refs: Vec<&str> =
                    candidate_storage.iter().map(String::as_str).collect();

                let hot_res = crate::core::try_smart_hot_switch_candidates(
                    clash_port,
                    settings.clash_api_secret.as_deref(),
                    &candidate_refs,
                )
                .await;

                if hot_res.is_ok() {
                    tracing::info!(
                        "Successfully hot-switched active proxy node to {}",
                        node.name
                    );
                    return Ok(());
                }
                tracing::debug!(
                    "Hot-switch not supported or failed ({:?}), falling back to core restart",
                    hot_res
                );
            }
        }

        let new_status = state
            .core_manager
            .restart(planned)
            .await
            .map_err(|e| e.to_string())?;
        sync_sysproxy_after_start(&state, &new_status);
        sync_android_vpn_after_start(&app, &state, &new_status)?;
    }
    Ok(())
}

fn resolve_active_clash_api_port(state: &AppState, settings: &AppSettings) -> u16 {
    state
        .core_manager
        .status()
        .actual_clash_api_port
        .unwrap_or(settings.clash_api_port)
}

#[tauri::command]
pub async fn get_core_traffic(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let status = state.core_manager.status();
    if !matches!(status.lifecycle, CoreLifecycleState::Running) {
        return Ok(serde_json::json!({ "up": 0, "down": 0 }));
    }
    let settings = state.db.get_app_settings().map_err(|e| e.to_string())?;
    let port = resolve_active_clash_api_port(&state, &settings);
    let snap = fetch_clash_traffic(port, settings.clash_api_secret.as_deref()).await;
    serde_json::to_value(snap).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_core_connections(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let status = state.core_manager.status();
    if !matches!(status.lifecycle, CoreLifecycleState::Running) {
        return Ok(serde_json::json!({
            "downloadTotal": 0,
            "uploadTotal": 0,
            "connections": []
        }));
    }
    let settings = state.db.get_app_settings().map_err(|e| e.to_string())?;
    let port = resolve_active_clash_api_port(&state, &settings);
    Ok(fetch_clash_connections(port, settings.clash_api_secret.as_deref()).await)
}

#[tauri::command]
pub async fn close_core_connection(
    state: State<'_, AppState>,
    id: Option<String>,
) -> Result<(), String> {
    let settings = state.db.get_app_settings().map_err(|e| e.to_string())?;
    let port = resolve_active_clash_api_port(&state, &settings);
    close_clash_connection(port, settings.clash_api_secret.as_deref(), id.as_deref()).await
}

// ==================== Phase P6: System Status & Core Updater ====================

fn build_core_updater(state: &AppState) -> CoreUpdater {
    let work_dir = state.core_manager.work_dir();
    let lock_path = work_dir
        .parent()
        .unwrap_or(work_dir)
        .join("cores.lock.json");

    if !lock_path.exists() {
        let bundled_lock = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(|p| p.join("cores.lock.json"));
        if let Some(src) = bundled_lock.filter(|p| p.exists()) {
            let _ = std::fs::copy(src, &lock_path);
        }
    }

    CoreUpdater::new(state.core_manager.effective_updater_cores_dir(), lock_path)
}

fn resolve_updater_proxy_url(state: &AppState, use_proxy: Option<bool>) -> Option<String> {
    let status = state.core_manager.status();
    let running_port = if matches!(status.lifecycle, CoreLifecycleState::Running) {
        status.actual_mixed_port.or(status.requested_port)
    } else {
        None
    };
    let should_proxy = use_proxy.unwrap_or(running_port.is_some());
    if should_proxy {
        running_port.map(|p| format!("http://127.0.0.1:{p}"))
    } else {
        None
    }
}

#[tauri::command]
pub fn get_service_status() -> Result<ServiceState, String> {
    query_service_state().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn check_wintun_status(state: State<'_, AppState>) -> WintunStatus {
    probe_wintun_status(&state.core_manager.cores_dir())
}

#[tauri::command]
pub fn get_url_scheme_status() -> Result<UrlSchemeStatus, String> {
    probe_url_scheme_status().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_clash_url_scheme(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<UrlSchemeStatus, String> {
    let status = apply_clash_url_scheme(enabled).map_err(|e| e.to_string())?;
    let mut settings = state.db.get_app_settings().map_err(|e| e.to_string())?;
    settings.register_clash_scheme = enabled;
    state
        .db
        .save_app_settings(&settings)
        .map_err(|e| e.to_string())?;
    Ok(status)
}

#[tauri::command]
pub async fn get_core_versions(
    state: State<'_, AppState>,
) -> Result<Vec<CoreBinaryStatus>, String> {
    let updater = build_core_updater(&state);
    Ok(updater.get_installed_statuses().await)
}

#[tauri::command]
pub async fn check_core_update(
    state: State<'_, AppState>,
    core: CoreType,
    use_proxy: Option<bool>,
) -> Result<CoreUpdateCheckResult, String> {
    let updater = build_core_updater(&state);
    let proxy_url = resolve_updater_proxy_url(&state, use_proxy);
    updater
        .check_for_update(core, proxy_url.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn install_core_update(
    state: State<'_, AppState>,
    core: CoreType,
    use_proxy: Option<bool>,
    expected_sha256: Option<String>,
) -> Result<CoreUpdateResult, String> {
    let updater = build_core_updater(&state);
    let proxy_url = resolve_updater_proxy_url(&state, use_proxy);

    // Download & extract via running proxy BEFORE stopping a locked binary
    let (check, candidate_path) = updater
        .download_core_candidate(core, proxy_url.as_deref())
        .await
        .map_err(|e| e.to_string())?;

    let prev_status = state.core_manager.status();
    let restart_needed = matches!(
        prev_status.lifecycle,
        CoreLifecycleState::Running | CoreLifecycleState::Starting
    ) && (prev_status.active_core == Some(core)
        || prev_status.upstream_core == Some(core));

    if restart_needed {
        state.core_manager.stop().await.map_err(|e| e.to_string())?;
    }

    let install_res = updater
        .finalize_candidate_update(core, check, &candidate_path, expected_sha256.as_deref())
        .await
        .map_err(|e| e.to_string());

    if restart_needed {
        if let Ok(req) =
            plan_core_request_from_db(&state, None, prev_status.requested_port, None, None, None)
        {
            let _ = state.core_manager.start(req).await;
        }
    }

    install_res
}

// ==================== Phase P6: Database Integrity & Backup ====================

#[tauri::command]
pub fn check_database_integrity(state: State<'_, AppState>) -> Result<bool, String> {
    state.db.check_integrity().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn backup_database(state: State<'_, AppState>, target_path: String) -> Result<(), String> {
    state
        .db
        .backup_database(&target_path)
        .map_err(|e| e.to_string())
}

// ==================== Phase P7 & P8: Diagnostics, Telemetry & Android VPN ====================

#[tauri::command]
pub fn get_telemetry_summary(state: State<'_, AppState>) -> Result<TelemetrySummary, String> {
    let log_dir = state
        .core_manager
        .logs_buffer()
        .log_dir()
        .unwrap_or_else(|| state.app_data_dir.join("logs"));
    Ok(state.core_manager.telemetry().summarize(Some(&log_dir)))
}

#[tauri::command]
pub fn get_log_files(state: State<'_, AppState>) -> Result<Vec<LogFileInfo>, String> {
    let log_dir = state
        .core_manager
        .logs_buffer()
        .log_dir()
        .unwrap_or_else(|| state.app_data_dir.join("logs"));
    Ok(list_log_files(&log_dir))
}

#[tauri::command]
pub fn read_log_file(
    state: State<'_, AppState>,
    file_name: String,
    tail_lines: Option<usize>,
) -> Result<String, String> {
    let log_dir = state
        .core_manager
        .logs_buffer()
        .log_dir()
        .unwrap_or_else(|| state.app_data_dir.join("logs"));
    read_log_file_tail(&log_dir, &file_name, tail_lines.unwrap_or(400))
}

#[tauri::command]
pub fn log_frontend_event(
    state: State<'_, AppState>,
    level: LogLevel,
    component: String,
    message: String,
) -> Result<(), String> {
    let clean_component = if component.trim().is_empty() {
        "frontend".to_string()
    } else {
        format!("frontend::{}", component.trim())
    };
    state.core_manager.logs_buffer().push_detailed(
        None,
        LogStream::Frontend,
        level,
        Some(clean_component.clone()),
        None,
        message.clone(),
    );
    if matches!(level, LogLevel::Error | LogLevel::Warn) {
        state
            .core_manager
            .telemetry()
            .record_frontend_error(level, clean_component, message);
    }
    Ok(())
}

#[tauri::command]
pub fn prepare_android_vpn(app: AppHandle) -> Result<bool, String> {
    app.android_vpn().prepare_vpn()
}

#[tauri::command]
pub fn get_android_vpn_status(app: AppHandle) -> Result<AndroidVpnStatus, String> {
    app.android_vpn().get_vpn_status()
}

fn build_redacted_diagnostics_value(state: &AppState) -> Result<serde_json::Value, String> {
    let mut settings = state.db.get_app_settings().map_err(|e| e.to_string())?;
    if settings.clash_api_secret.is_some() {
        settings.clash_api_secret = Some("***REDACTED***".to_string());
    }
    let log_dir = state
        .core_manager
        .logs_buffer()
        .log_dir()
        .unwrap_or_else(|| state.app_data_dir.join("logs"));
    let telemetry = state.core_manager.telemetry().summarize(Some(&log_dir));

    Ok(serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "app_version": env!("CARGO_PKG_VERSION"),
        "platform": detect_platform_info(),
        "os_platform": std::env::consts::OS,
        "os_arch": std::env::consts::ARCH,
        "core_status": state.core_manager.status(),
        "available_cores": state.core_manager.available_cores(),
        "sysproxy_status": SystemProxyManager::query_current().ok(),
        "node_count": state.db.list_nodes().map(|v| v.len()).unwrap_or(0),
        "subscription_count": state.db.list_subscriptions().map(|v| v.len()).unwrap_or(0),
        "rule_count": state.db.list_routing_rules().map(|v| v.len()).unwrap_or(0),
        "app_settings": settings,
        "dns_settings": state.db.get_dns_settings().ok(),
        "routing_settings": state.db.get_routing_settings().ok(),
        "telemetry": telemetry,
        "recent_logs": state.core_manager.logs(None, Some(200)),
    }))
}

#[tauri::command]
pub fn export_diagnostics_report(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    build_redacted_diagnostics_value(&state)
}

#[tauri::command]
pub fn export_diagnostic_bundle(
    state: State<'_, AppState>,
) -> Result<DiagnosticBundleResult, String> {
    let exports_dir = state.app_data_dir.join("exports");
    std::fs::create_dir_all(&exports_dir).map_err(|e| e.to_string())?;

    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let filename = format!("novaproxy-diag-{stamp}.zip");
    let zip_path = exports_dir.join(&filename);
    let tmp_path = exports_dir.join(format!("{filename}.tmp"));

    let diag_val = build_redacted_diagnostics_value(&state)?;
    let diag_json = serde_json::to_string_pretty(&diag_val).map_err(|e| e.to_string())?;

    {
        let file = std::fs::File::create(&tmp_path).map_err(|e| e.to_string())?;
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        zip.start_file("diagnostics.json", opts)
            .map_err(|e| e.to_string())?;
        zip.write_all(diag_json.as_bytes())
            .map_err(|e| e.to_string())?;

        let log_dir = state
            .core_manager
            .logs_buffer()
            .log_dir()
            .unwrap_or_else(|| state.app_data_dir.join("logs"));
        for lf in list_log_files(&log_dir) {
            let src = log_dir.join(&lf.name);
            if let Ok(raw) = std::fs::read_to_string(&src) {
                let sanitized = sanitize_log_message(&raw);
                let archive_name = format!("logs/{}", lf.name);
                if zip.start_file(&archive_name, opts).is_ok() {
                    let _ = zip.write_all(sanitized.as_bytes());
                }
            }
        }

        zip.finish().map_err(|e| e.to_string())?;
    }

    if zip_path.exists() {
        let _ = std::fs::remove_file(&zip_path);
    }
    std::fs::rename(&tmp_path, &zip_path).map_err(|e| e.to_string())?;
    let size_bytes = std::fs::metadata(&zip_path).map(|m| m.len()).unwrap_or(0);

    tracing::info!(
        target: "novaproxy::diagnostics",
        "Exported redacted diagnostic bundle to {} ({} bytes)",
        zip_path.display(),
        size_bytes
    );

    Ok(DiagnosticBundleResult {
        path: zip_path.to_string_lossy().to_string(),
        filename,
        size_bytes,
    })
}

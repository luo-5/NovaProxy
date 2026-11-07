//! Tauri IPC commands
//! Thin bridge between frontend requests and backend services.

use crate::core::{
    default_minimal_singbox_config, CoreLifecycleState, CoreManager, CoreStartRequest,
    CoreStatusInfo,
};
use crate::model::{
    AppSettings, CoreType, DnsSettings, ProxyNode, RoutingSettings, RuleItem, Subscription,
};
use crate::monitor::CoreLogEntry;
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
    is_autostart_enabled, is_elevated, parse_deep_link, relaunch_as_admin, set_autostart,
    SysProxySnapshot, SystemProxyManager,
};
use tauri::State;

pub struct AppState {
    pub db: Database,
    pub core_manager: CoreManager,
    pub sysproxy_manager: SystemProxyManager,
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
    let mut req = request.unwrap_or_default();
    if req.config_json.is_none() {
        let app_settings = state.db.get_app_settings().map_err(|e| e.to_string())?;
        let core = req
            .core_type
            .or(app_settings.preferred_core)
            .unwrap_or(CoreType::SingBox);
        req.core_type = Some(core);
        let nodes = state.db.list_nodes().map_err(|e| e.to_string())?;
        let groups = state.db.list_groups().map_err(|e| e.to_string())?;
        let dns = state.db.get_dns_settings().map_err(|e| e.to_string())?;
        let routing = state.db.get_routing_settings().map_err(|e| e.to_string())?;

        let generated = crate::generator::generate_core_config(
            core,
            &nodes,
            &groups,
            &app_settings,
            &dns,
            &routing,
            app_settings.active_node_id.as_deref(),
        )
        .map_err(|e| e.to_string())?;
        req.config_json = Some(generated);
    }
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
    let mut req = request.unwrap_or_default();
    if req.config_json.is_none() {
        let app_settings = state.db.get_app_settings().map_err(|e| e.to_string())?;
        let core = req
            .core_type
            .or(app_settings.preferred_core)
            .unwrap_or(CoreType::SingBox);
        req.core_type = Some(core);
        let nodes = state.db.list_nodes().map_err(|e| e.to_string())?;
        let groups = state.db.list_groups().map_err(|e| e.to_string())?;
        let dns = state.db.get_dns_settings().map_err(|e| e.to_string())?;
        let routing = state.db.get_routing_settings().map_err(|e| e.to_string())?;

        let generated = crate::generator::generate_core_config(
            core,
            &nodes,
            &groups,
            &app_settings,
            &dns,
            &routing,
            app_settings.active_node_id.as_deref(),
        )
        .map_err(|e| e.to_string())?;
        req.config_json = Some(generated);
    }
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
    let saved = state
        .db
        .import_manual_nodes(report.nodes)
        .map_err(|e| e.to_string())?;
    report.nodes = saved;
    Ok(report)
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
    Ok(node)
}

#[tauri::command]
pub fn delete_node(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    state.db.delete_node(&id).map_err(|e| e.to_string())
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
    Ok(subscription)
}

#[tauri::command]
pub fn delete_subscription(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    state.db.delete_subscription(&id).map_err(|e| e.to_string())
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

    let (mut report, user_info) = fetch_and_parse_subscription(&sub, local_port, 15, 2)
        .await
        .map_err(|e| e.to_string())?;

    apply_userinfo_to_subscription(&mut sub, &user_info);
    state
        .db
        .upsert_subscription(&sub)
        .map_err(|e| e.to_string())?;

    let synced_nodes = state
        .db
        .sync_subscription_nodes(&sub.id, report.nodes)
        .map_err(|e| e.to_string())?;

    report.nodes = synced_nodes;
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
    Ok(rule)
}

#[tauri::command]
pub fn delete_routing_rule(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    state.db.delete_routing_rule(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reorder_routing_rules(state: State<'_, AppState>, ids: Vec<String>) -> Result<(), String> {
    state
        .db
        .reorder_routing_rules(&ids)
        .map_err(|e| e.to_string())
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
    Ok(settings)
}

#[tauri::command]
pub fn generate_config(
    state: State<'_, AppState>,
    core_type: Option<CoreType>,
    active_node_id: Option<String>,
) -> Result<String, String> {
    let app_settings = state.db.get_app_settings().map_err(|e| e.to_string())?;
    let core = core_type
        .or(app_settings.preferred_core)
        .unwrap_or(CoreType::SingBox);
    let nodes = state.db.list_nodes().map_err(|e| e.to_string())?;
    let groups = state.db.list_groups().map_err(|e| e.to_string())?;
    let dns = state.db.get_dns_settings().map_err(|e| e.to_string())?;
    let routing = state.db.get_routing_settings().map_err(|e| e.to_string())?;

    crate::generator::generate_core_config(
        core,
        &nodes,
        &groups,
        &app_settings,
        &dns,
        &routing,
        active_node_id
            .as_deref()
            .or(app_settings.active_node_id.as_deref()),
    )
    .map_err(|e| e.to_string())
}

// ==================== Phase P4: System Proxy, TUN & Deep Links ====================

#[tauri::command]
pub fn get_sysproxy_status() -> Result<SysProxySnapshot, String> {
    SystemProxyManager::query_current().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_sysproxy(state: State<'_, AppState>, enabled: bool) -> Result<SysProxySnapshot, String> {
    let mut settings = state.db.get_app_settings().map_err(|e| e.to_string())?;

    let snap = if enabled {
        let active_port = state
            .core_manager
            .status()
            .actual_mixed_port
            .unwrap_or(settings.mixed_port);
        let srv = format!("127.0.0.1:{}", active_port);
        let res = state
            .sysproxy_manager
            .set_proxy(&srv, None)
            .map_err(|e| e.to_string())?;
        settings.system_proxy = true;
        let _ = state.db.save_app_settings(&settings);
        res
    } else {
        let res = state
            .sysproxy_manager
            .clear_proxy()
            .map_err(|e| e.to_string())?;
        settings.system_proxy = false;
        let _ = state.db.save_app_settings(&settings);
        res
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

#[tauri::command]
pub async fn handle_deep_link_url(
    state: State<'_, AppState>,
    uri: String,
) -> Result<Option<Subscription>, String> {
    let Some(action) = parse_deep_link(&uri) else {
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

    state
        .db
        .upsert_subscription(&sub)
        .map_err(|e| e.to_string())?;

    Ok(Some(sub))
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
    Ok(settings)
}

async fn tcp_ping(
    host: &str,
    port: u16,
    timeout_duration: std::time::Duration,
) -> Result<i64, String> {
    let start = std::time::Instant::now();
    let addr = format!("{}:{}", host, port);
    let connect_fut = tokio::net::TcpStream::connect(&addr);
    match tokio::time::timeout(timeout_duration, connect_fut).await {
        Ok(Ok(_)) => Ok(start.elapsed().as_millis() as i64),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("Timeout".to_string()),
    }
}

#[tauri::command]
pub async fn test_node_latency(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<i64>, String> {
    let node = state
        .db
        .get_node(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Node not found: {id}"))?;

    let res = tcp_ping(
        &node.server,
        node.port,
        std::time::Duration::from_millis(2500),
    )
    .await;
    let (lat, err) = match res {
        Ok(ms) => (Some(ms), None),
        Err(e) => (None, Some(e)),
    };
    state
        .db
        .record_latency(&id, lat, "tcp", err.as_deref())
        .map_err(|e| e.to_string())?;
    Ok(lat)
}

#[tauri::command]
pub async fn test_all_nodes_latency(
    state: State<'_, AppState>,
) -> Result<Vec<(String, Option<i64>)>, String> {
    let nodes = state.db.list_nodes().map_err(|e| e.to_string())?;
    let mut handles = Vec::new();

    for node in nodes {
        let db = state.db.clone();
        handles.push(tokio::spawn(async move {
            let res = tcp_ping(
                &node.server,
                node.port,
                std::time::Duration::from_millis(3000),
            )
            .await;
            let (lat, err) = match res {
                Ok(ms) => (Some(ms), None),
                Err(e) => (None, Some(e)),
            };
            let _ = db.record_latency(&node.id, lat, "tcp", err.as_deref());
            (node.id, lat)
        }));
    }

    let mut results = Vec::new();
    for handle in handles {
        if let Ok(res) = handle.await {
            results.push(res);
        }
    }
    Ok(results)
}

#[tauri::command]
pub async fn select_active_node(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let mut settings = state.db.get_app_settings().map_err(|e| e.to_string())?;
    settings.active_node_id = Some(id.clone());
    state
        .db
        .save_app_settings(&settings)
        .map_err(|e| e.to_string())?;

    // If core is running, restart to immediately route via the newly selected active node
    let status = state.core_manager.status();
    if matches!(status.lifecycle, CoreLifecycleState::Running) {
        let core = status
            .active_core
            .or(settings.preferred_core)
            .unwrap_or(CoreType::SingBox);
        let nodes = state.db.list_nodes().map_err(|e| e.to_string())?;
        let groups = state.db.list_groups().map_err(|e| e.to_string())?;
        let dns = state.db.get_dns_settings().map_err(|e| e.to_string())?;
        let routing = state.db.get_routing_settings().map_err(|e| e.to_string())?;

        let generated = crate::generator::generate_core_config(
            core,
            &nodes,
            &groups,
            &settings,
            &dns,
            &routing,
            Some(&id),
        )
        .map_err(|e| e.to_string())?;

        let req = CoreStartRequest {
            core_type: Some(core),
            config_json: Some(generated),
            mixed_port: status.requested_port,
            allow_lan: Some(settings.allow_lan),
            auto_port_fallback: Some(settings.auto_port_fallback),
        };
        let _ = state.core_manager.restart(req).await;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_core_traffic(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let settings = state.db.get_app_settings().map_err(|e| e.to_string())?;
    let port = settings.clash_api_port;
    let url = format!("http://127.0.0.1:{}/traffic", port);
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(1500))
        .build()
    {
        Ok(c) => c,
        Err(_e) => return Ok(serde_json::json!({ "up": 0, "down": 0 })),
    };

    let mut req = client.get(&url);
    if let Some(secret) = &settings.clash_api_secret {
        if !secret.is_empty() {
            req = req.bearer_auth(secret);
        }
    }

    match req.send().await {
        Ok(resp) => {
            let val = resp
                .json::<serde_json::Value>()
                .await
                .unwrap_or_else(|_| serde_json::json!({ "up": 0, "down": 0 }));
            Ok(val)
        }
        Err(_) => Ok(serde_json::json!({ "up": 0, "down": 0 })),
    }
}

#[tauri::command]
pub async fn get_core_connections(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let settings = state.db.get_app_settings().map_err(|e| e.to_string())?;
    let port = settings.clash_api_port;
    let url = format!("http://127.0.0.1:{}/connections", port);
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(2000))
        .build()
    {
        Ok(c) => c,
        Err(_e) => {
            return Ok(serde_json::json!({
                "downloadTotal": 0,
                "uploadTotal": 0,
                "connections": []
            }))
        }
    };

    let mut req = client.get(&url);
    if let Some(secret) = &settings.clash_api_secret {
        if !secret.is_empty() {
            req = req.bearer_auth(secret);
        }
    }

    match req.send().await {
        Ok(resp) => {
            let val = resp.json::<serde_json::Value>().await.unwrap_or_else(|_| {
                serde_json::json!({
                    "downloadTotal": 0,
                    "uploadTotal": 0,
                    "connections": []
                })
            });
            Ok(val)
        }
        Err(_) => Ok(serde_json::json!({
            "downloadTotal": 0,
            "uploadTotal": 0,
            "connections": []
        })),
    }
}

#[tauri::command]
pub async fn close_core_connection(
    state: State<'_, AppState>,
    id: Option<String>,
) -> Result<(), String> {
    let settings = state.db.get_app_settings().map_err(|e| e.to_string())?;
    let port = settings.clash_api_port;
    let url = match &id {
        Some(cid) => format!("http://127.0.0.1:{}/connections/{}", port, cid),
        None => format!("http://127.0.0.1:{}/connections", port),
    };
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(2000))
        .build()
    {
        Ok(c) => c,
        Err(e) => return Err(e.to_string()),
    };

    let mut req = client.delete(&url);
    if let Some(secret) = &settings.clash_api_secret {
        if !secret.is_empty() {
            req = req.bearer_auth(secret);
        }
    }
    let _ = req.send().await;
    Ok(())
}

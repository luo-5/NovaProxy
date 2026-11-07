//! Tauri IPC commands
//! Thin bridge between frontend requests and backend services.

use crate::core::{default_minimal_singbox_config, CoreManager, CoreStartRequest, CoreStatusInfo};
use crate::model::{ProxyNode, Subscription};
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

//! Active node health probing and automatic failover monitor
//! Continuously checks active node connectivity and automatically switches
//! to an alternative healthy node if consecutive failures exceed threshold.

use crate::core::manager::CoreLifecycleState;
use crate::core::CoreManager;
use crate::model::CoreType;
use crate::monitor::latency::{
    probe_clash_api_delay, probe_url_via_socks_proxy, tcp_ping, DEFAULT_LATENCY_TEST_URL,
};
use crate::storage::Database;
use chrono::Utc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use ts_rs::TS;

#[derive(Debug, Error)]
pub enum HealthError {
    #[error("Database error: {0}")]
    Database(#[from] crate::storage::StorageError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("No healthy fallback node available")]
    NoHealthyNode,
    #[error("Monitor error: {0}")]
    General(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct HealthCheckResult {
    pub node_id: String,
    pub node_name: String,
    pub latency_ms: Option<i64>,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct FailoverEvent {
    pub previous_node_id: String,
    pub previous_node_name: String,
    pub new_node_id: String,
    pub new_node_name: String,
    pub failure_count: u32,
    pub reason: String,
}

pub struct HealthMonitor {
    db: Database,
    core_manager: CoreManager,
    running: Arc<AtomicBool>,
    consecutive_failures: Arc<AtomicU32>,
    last_failover_at: Arc<AtomicI64>,
    max_failures: u32,
    check_interval: Duration,
    probe_timeout: Duration,
    failover_cooldown: Duration,
    last_result: Arc<RwLock<Option<HealthCheckResult>>>,
}

impl HealthMonitor {
    pub fn new(db: Database, core_manager: CoreManager) -> Self {
        Self {
            db,
            core_manager,
            running: Arc::new(AtomicBool::new(false)),
            consecutive_failures: Arc::new(AtomicU32::new(0)),
            last_failover_at: Arc::new(AtomicI64::new(0)),
            max_failures: 2,
            check_interval: Duration::from_secs(30),
            probe_timeout: Duration::from_millis(3000),
            failover_cooldown: Duration::from_secs(30),
            last_result: Arc::new(RwLock::new(None)),
        }
    }

    pub fn with_config(
        mut self,
        interval: Duration,
        probe_timeout: Duration,
        max_failures: u32,
    ) -> Self {
        self.check_interval = interval;
        self.probe_timeout = probe_timeout;
        self.max_failures = max_failures;
        self
    }

    pub fn with_cooldown(mut self, cooldown: Duration) -> Self {
        self.failover_cooldown = cooldown;
        self
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn get_last_result(&self) -> Option<HealthCheckResult> {
        self.last_result.read().clone()
    }

    pub fn get_consecutive_failures(&self) -> u32 {
        self.consecutive_failures.load(Ordering::SeqCst)
    }

    /// Performs a single probe check of the currently active node.
    pub async fn check_once(&self) -> Result<HealthCheckResult, HealthError> {
        let settings = self.db.get_app_settings()?;
        let active_id = match settings.active_node_id {
            Some(ref id) => id.clone(),
            None => {
                return Err(HealthError::General("No active node configured".into()));
            }
        };

        let node = self
            .db
            .get_node(&active_id)?
            .ok_or_else(|| HealthError::General(format!("Active node not found: {active_id}")))?;

        let core_status = self.core_manager.status();
        let probe_res = if matches!(core_status.lifecycle, CoreLifecycleState::Running) {
            if core_status.active_core != Some(CoreType::Xray)
                && core_status.upstream_core.is_none()
            {
                let clash_port = core_status
                    .actual_clash_api_port
                    .unwrap_or(settings.clash_api_port);
                let relay_tag = format!("relay-{}", node.id);
                let mihomo_name = if core_status.active_core == Some(CoreType::Mihomo) {
                    self.db.list_nodes().ok().and_then(|all_nodes| {
                        crate::generator::mihomo::resolve_mihomo_proxy_names(&all_nodes)
                            .remove(&node.id)
                    })
                } else {
                    None
                };
                let mut candidates: Vec<&str> = Vec::with_capacity(4);
                if let Some(ref mname) = mihomo_name {
                    candidates.push(mname.as_str());
                }
                candidates.push(node.id.as_str());
                candidates.push(node.name.as_str());
                candidates.push(relay_tag.as_str());

                let clash_res = probe_clash_api_delay(
                    clash_port,
                    settings.clash_api_secret.as_deref(),
                    &candidates,
                    DEFAULT_LATENCY_TEST_URL,
                    self.probe_timeout,
                )
                .await;
                if clash_res.is_ok() {
                    clash_res
                } else {
                    let mixed_port = core_status.actual_mixed_port.unwrap_or(settings.mixed_port);
                    probe_url_via_socks_proxy(
                        mixed_port,
                        DEFAULT_LATENCY_TEST_URL,
                        self.probe_timeout,
                    )
                    .await
                }
            } else {
                let mixed_port = core_status.actual_mixed_port.unwrap_or(settings.mixed_port);
                probe_url_via_socks_proxy(mixed_port, DEFAULT_LATENCY_TEST_URL, self.probe_timeout)
                    .await
            }
        } else {
            tcp_ping(&node.server, node.port, self.probe_timeout).await
        };

        let (success, latency_ms, err_msg) = match probe_res {
            Ok(ms) => (true, Some(ms), None),
            Err(e) => (false, None, Some(e)),
        };

        let res = HealthCheckResult {
            node_id: node.id.clone(),
            node_name: node.name.clone(),
            latency_ms,
            success,
            error: err_msg.clone(),
        };

        // Record in DB
        let _ = self
            .db
            .record_latency(&node.id, latency_ms, "health_probe", err_msg.as_deref());

        *self.last_result.write() = Some(res.clone());

        if success {
            self.consecutive_failures.store(0, Ordering::SeqCst);
        } else {
            let count = self.consecutive_failures.fetch_add(1, Ordering::SeqCst) + 1;
            tracing::warn!(
                "Active node '{}' ({}) probe failed (failure {}/{})",
                node.name,
                node.id,
                count,
                self.max_failures
            );
        }

        Ok(res)
    }

    /// Evaluates if failover is required and switches to the next best node.
    pub async fn evaluate_failover(&self) -> Result<Option<FailoverEvent>, HealthError> {
        let failures = self.consecutive_failures.load(Ordering::SeqCst);
        if failures < self.max_failures {
            return Ok(None);
        }

        let cooldown_ms = self.failover_cooldown.as_millis() as i64;
        if cooldown_ms > 0 {
            let last_ts = self.last_failover_at.load(Ordering::SeqCst);
            let now_ts = Utc::now().timestamp_millis();
            if last_ts > 0 && now_ts.saturating_sub(last_ts) < cooldown_ms {
                tracing::debug!("Skipping failover evaluation: cooldown still active");
                return Ok(None);
            }
        }

        let mut settings = self.db.get_app_settings()?;
        let active_id = settings.active_node_id.clone().unwrap_or_default();
        let all_nodes = self.db.list_nodes()?;

        if all_nodes.is_empty() {
            return Err(HealthError::NoHealthyNode);
        }

        let current_node = all_nodes.iter().find(|n| n.id == active_id);
        let current_name = current_node
            .map(|n| n.name.clone())
            .unwrap_or_else(|| active_id.clone());

        // Find candidate nodes excluding the failing one
        let mut candidates: Vec<_> = all_nodes
            .into_iter()
            .filter(|n| n.id != active_id)
            .collect();

        if candidates.is_empty() {
            return Err(HealthError::NoHealthyNode);
        }

        // Rank candidates: nodes with lower latency first; None latency last
        candidates.sort_by(|a, b| match (a.latency, b.latency) {
            (Some(la), Some(lb)) => la.cmp(&lb),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });

        let chosen = candidates
            .first()
            .ok_or(HealthError::NoHealthyNode)?
            .clone();

        tracing::info!(
            "Auto-failover triggered: replacing '{}' with '{}' ({})",
            current_name,
            chosen.name,
            chosen.id
        );

        // Update settings in DB
        settings.active_node_id = Some(chosen.id.clone());
        self.db.save_app_settings(&settings)?;

        // Attempt Clash API hot-switch if running
        let status = self.core_manager.status();
        if matches!(status.lifecycle, CoreLifecycleState::Running) {
            let nodes = self.db.list_nodes()?;
            let groups = self.db.list_groups()?;
            let dns = self.db.get_dns_settings()?;
            let routing = self.db.get_routing_settings()?;

            if let Ok(req) = self.core_manager.plan_start_request(
                None,
                status.requested_port,
                Some(settings.allow_lan),
                Some(settings.auto_port_fallback),
                &nodes,
                &groups,
                &settings,
                &dns,
                &routing,
                Some(&chosen.id),
            ) {
                let topology_matches = !self.core_manager.is_config_dirty()
                    && status.active_core == req.core_type
                    && status.upstream_core == req.upstream_core_type
                    && req.upstream_core_type.is_none();

                let mut switched_hot = false;
                if topology_matches {
                    let clash_port = status
                        .actual_clash_api_port
                        .unwrap_or(settings.clash_api_port);
                    let mihomo_names = crate::generator::mihomo::resolve_mihomo_proxy_names(&nodes);
                    let mut candidate_storage: Vec<String> = Vec::with_capacity(3);
                    if matches!(status.active_core, Some(CoreType::Mihomo)) {
                        if let Some(mname) = mihomo_names.get(&chosen.id) {
                            candidate_storage.push(mname.clone());
                        }
                    }
                    candidate_storage.push(chosen.id.clone());
                    candidate_storage.push(chosen.name.clone());
                    let candidate_refs: Vec<&str> =
                        candidate_storage.iter().map(String::as_str).collect();

                    if crate::core::try_smart_hot_switch_candidates(
                        clash_port,
                        settings.clash_api_secret.as_deref(),
                        &candidate_refs,
                    )
                    .await
                    .is_ok()
                    {
                        switched_hot = true;
                    }
                }

                if !switched_hot {
                    let _ = self.core_manager.restart(req).await;
                }
            }
        }

        // Reset failure counter and record failover timestamp for flapping protection
        self.core_manager.telemetry().record_failover();
        self.consecutive_failures.store(0, Ordering::SeqCst);
        self.last_failover_at
            .store(Utc::now().timestamp_millis(), Ordering::SeqCst);

        let event = FailoverEvent {
            previous_node_id: active_id,
            previous_node_name: current_name,
            new_node_id: chosen.id,
            new_node_name: chosen.name,
            failure_count: failures,
            reason: format!("Active node failed {} consecutive health checks", failures),
        };

        Ok(Some(event))
    }

    /// Asynchronous background health monitor loop (safe to spawn via `tauri::async_runtime::spawn`)
    pub async fn run_loop(self: Arc<Self>) {
        self.running.store(true, Ordering::SeqCst);
        tracing::info!(
            "Health monitor loop started (interval: {:?})",
            self.check_interval
        );

        while self.running.load(Ordering::SeqCst) {
            tokio::time::sleep(self.check_interval).await;
            if !self.running.load(Ordering::SeqCst) {
                break;
            }

            // Only check when the core is actually running
            let status = self.core_manager.status();
            if matches!(status.lifecycle, CoreLifecycleState::Running) {
                let _ = self.check_once().await;
                let _ = self.evaluate_failover().await;
            }
        }

        tracing::info!("Health monitor loop exited");
    }

    /// Starts the background health monitor loop on an active Tokio runtime
    pub fn start(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        self.running.store(true, Ordering::SeqCst);
        let monitor = self.clone();
        tokio::spawn(monitor.run_loop())
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::node::{ProtocolConfig, ProtocolType, ProxyNode, VlessConfig};

    #[tokio::test]
    async fn test_health_monitor_failover_ranking() {
        let db = Database::open_in_memory().unwrap();
        let core_manager = CoreManager::new(std::env::temp_dir(), std::env::temp_dir());

        let node1 = ProxyNode {
            id: "node-1".into(),
            name: "Failing Node".into(),
            server: "127.0.0.1".into(),
            port: 19991, // offline
            protocol: ProtocolType::Vless,
            tls: None,
            transport: None,
            subscription_id: None,
            group_id: None,
            settings: ProtocolConfig::Vless(VlessConfig {
                uuid: "00000000-0000-0000-0000-000000000001".into(),
                flow: None,
                encryption: "none".into(),
                packet_encoding: None,
            }),
            latency: None,
            dialer_proxy: None,
            preferred_core: None,
            sort_order: 0,
        };

        let node2 = ProxyNode {
            id: "node-2".into(),
            name: "Fast Healthy Node".into(),
            server: "127.0.0.1".into(),
            port: 19992,
            protocol: ProtocolType::Vless,
            tls: None,
            transport: None,
            subscription_id: None,
            group_id: None,
            settings: ProtocolConfig::Vless(VlessConfig {
                uuid: "00000000-0000-0000-0000-000000000002".into(),
                flow: None,
                encryption: "none".into(),
                packet_encoding: None,
            }),
            latency: Some(42),
            dialer_proxy: None,
            preferred_core: None,
            sort_order: 1,
        };

        db.upsert_node(&node1).unwrap();
        db.upsert_node(&node2).unwrap();

        let mut settings = db.get_app_settings().unwrap();
        settings.active_node_id = Some("node-1".into());
        db.save_app_settings(&settings).unwrap();

        let monitor = HealthMonitor::new(db.clone(), core_manager).with_config(
            Duration::from_millis(50),
            Duration::from_millis(100),
            2,
        );

        // Before failures, no failover
        let failover = monitor.evaluate_failover().await.unwrap();
        assert!(failover.is_none());

        // Simulate 2 failures
        monitor.consecutive_failures.store(2, Ordering::SeqCst);

        let failover = monitor.evaluate_failover().await.unwrap();
        assert!(failover.is_some());
        let event = failover.unwrap();
        assert_eq!(event.previous_node_id, "node-1");
        assert_eq!(event.new_node_id, "node-2");

        // DB active node should now be updated to node-2
        let current_settings = db.get_app_settings().unwrap();
        assert_eq!(current_settings.active_node_id, Some("node-2".into()));

        // Consecutive failures should be reset
        assert_eq!(monitor.get_consecutive_failures(), 0);
    }

    #[tokio::test]
    async fn test_health_monitor_cooldown_prevents_flapping() {
        let db = Database::open_in_memory().unwrap();
        let core_manager = CoreManager::new(std::env::temp_dir(), std::env::temp_dir());

        let node1 = ProxyNode {
            id: "flap-1".into(),
            name: "Flap Node 1".into(),
            server: "127.0.0.1".into(),
            port: 19991,
            protocol: ProtocolType::Vless,
            tls: None,
            transport: None,
            subscription_id: None,
            group_id: None,
            settings: ProtocolConfig::Vless(VlessConfig {
                uuid: "00000000-0000-0000-0000-000000000001".into(),
                flow: None,
                encryption: "none".into(),
                packet_encoding: None,
            }),
            latency: Some(100),
            dialer_proxy: None,
            preferred_core: None,
            sort_order: 0,
        };
        let mut node2 = node1.clone();
        node2.id = "flap-2".into();
        node2.name = "Flap Node 2".into();
        node2.latency = Some(20);

        db.upsert_node(&node1).unwrap();
        db.upsert_node(&node2).unwrap();
        let mut settings = db.get_app_settings().unwrap();
        settings.active_node_id = Some("flap-1".into());
        db.save_app_settings(&settings).unwrap();

        let monitor = HealthMonitor::new(db, core_manager)
            .with_config(Duration::from_millis(50), Duration::from_millis(100), 2)
            .with_cooldown(Duration::from_secs(60));

        monitor.consecutive_failures.store(2, Ordering::SeqCst);
        let first = monitor.evaluate_failover().await.unwrap();
        assert!(first.is_some());

        // Immediate subsequent failure spike within cooldown must not flap again
        monitor.consecutive_failures.store(5, Ordering::SeqCst);
        let second = monitor.evaluate_failover().await.unwrap();
        assert!(second.is_none());
    }
}

//! Active node health probing and automatic failover monitor
//! Continuously checks active node connectivity and automatically switches
//! to an alternative healthy node if consecutive failures exceed threshold.

use crate::core::manager::CoreLifecycleState;
use crate::core::CoreManager;
use crate::storage::Database;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
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
    max_failures: u32,
    check_interval: Duration,
    probe_timeout: Duration,
    last_result: Arc<RwLock<Option<HealthCheckResult>>>,
}

impl HealthMonitor {
    pub fn new(db: Database, core_manager: CoreManager) -> Self {
        Self {
            db,
            core_manager,
            running: Arc::new(AtomicBool::new(false)),
            consecutive_failures: Arc::new(AtomicU32::new(0)),
            max_failures: 2,
            check_interval: Duration::from_secs(30),
            probe_timeout: Duration::from_millis(3000),
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

        let start = std::time::Instant::now();
        let target_addr = format!("{}:{}", node.server, node.port);
        let connect_fut = tokio::net::TcpStream::connect(&target_addr);

        let (success, latency_ms, err_msg) =
            match tokio::time::timeout(self.probe_timeout, connect_fut).await {
                Ok(Ok(_stream)) => (true, Some(start.elapsed().as_millis() as i64), None),
                Ok(Err(e)) => (false, None, Some(e.to_string())),
                Err(_) => (false, None, Some("Connection timeout".to_string())),
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
            let hot_res = crate::core::try_smart_hot_switch(
                settings.clash_api_port,
                settings.clash_api_secret.as_deref(),
                &chosen.id,
                &chosen.name,
            )
            .await;

            if hot_res.is_err() {
                // If hot-switch fails, trigger core restart with new config
                let core = status
                    .active_core
                    .or(settings.preferred_core)
                    .unwrap_or(crate::model::CoreType::SingBox);
                let nodes = self.db.list_nodes()?;
                let groups = self.db.list_groups()?;
                let dns = self.db.get_dns_settings()?;
                let routing = self.db.get_routing_settings()?;

                if let Ok(config_json) = crate::generator::generate_core_config(
                    core,
                    &nodes,
                    &groups,
                    &settings,
                    &dns,
                    &routing,
                    Some(&chosen.id),
                ) {
                    let req = crate::core::CoreStartRequest {
                        core_type: Some(core),
                        config_json: Some(config_json),
                        mixed_port: status.requested_port,
                        allow_lan: Some(settings.allow_lan),
                        auto_port_fallback: Some(settings.auto_port_fallback),
                    };
                    let _ = self.core_manager.restart(req).await;
                }
            }
        }

        // Reset failure counter
        self.consecutive_failures.store(0, Ordering::SeqCst);

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

    /// Starts the background health monitor loop
    pub fn start(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        self.running.store(true, Ordering::SeqCst);
        let monitor = self.clone();

        tokio::spawn(async move {
            tracing::info!(
                "Health monitor loop started (interval: {:?})",
                monitor.check_interval
            );

            while monitor.running.load(Ordering::SeqCst) {
                tokio::time::sleep(monitor.check_interval).await;
                if !monitor.running.load(Ordering::SeqCst) {
                    break;
                }

                // Only check when the core is actually running
                let status = monitor.core_manager.status();
                if matches!(status.lifecycle, CoreLifecycleState::Running) {
                    let _ = monitor.check_once().await;
                    let _ = monitor.evaluate_failover().await;
                }
            }

            tracing::info!("Health monitor loop exited");
        })
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
}

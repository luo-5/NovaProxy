use crate::model::{CoreType, LogLevel, ProtocolType};
use chrono::Utc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::path::Path;
use std::sync::Arc;
use ts_rs::TS;

const MAX_START_SAMPLES: usize = 64;
const MAX_FAILOVER_SAMPLES: usize = 32;
const MAX_CRASH_SAMPLES: usize = 32;
const MAX_FRONTEND_ERRORS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CoreStartSample {
    #[ts(type = "number")]
    pub timestamp: i64,
    pub core: CoreType,
    pub upstream_core: Option<CoreType>,
    #[ts(type = "number")]
    pub config_gen_ms: u64,
    #[ts(type = "number")]
    pub port_resolve_ms: u64,
    #[ts(type = "number")]
    pub spawn_ms: u64,
    #[ts(type = "number")]
    pub total_ms: u64,
    pub port_reassigned: bool,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ProtocolLatencyStats {
    pub protocol: String,
    #[ts(type = "number")]
    pub total: u64,
    #[ts(type = "number")]
    pub successes: u64,
    #[ts(type = "number")]
    pub timeouts: u64,
    #[ts(type = "number")]
    pub avg_ms: u64,
    #[ts(type = "number")]
    pub max_ms: u64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FrontendErrorSample {
    #[ts(type = "number")]
    pub timestamp: i64,
    pub level: LogLevel,
    pub component: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CrashSample {
    #[ts(type = "number")]
    pub timestamp: i64,
    pub core: CoreType,
    pub exit_code: Option<i32>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum InsightSeverity {
    Info,
    Warn,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OptimizationInsight {
    pub code: String,
    pub severity: InsightSeverity,
    pub title_zh: String,
    pub title_en: String,
    pub metric: String,
    pub suggestion_zh: String,
    pub suggestion_en: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TelemetrySummary {
    pub os: String,
    pub arch: String,
    pub log_dir: String,
    #[ts(type = "number")]
    pub total_log_bytes: u64,
    #[ts(type = "number")]
    pub total_core_starts: u64,
    #[ts(type = "number")]
    pub avg_core_start_ms: u64,
    #[ts(type = "number")]
    pub max_core_start_ms: u64,
    #[ts(type = "number")]
    pub port_reassigned_count: u64,
    #[ts(type = "number")]
    pub failover_count: u64,
    #[ts(type = "number")]
    pub crash_count: u64,
    #[ts(type = "number")]
    pub network_change_count: u64,
    #[ts(type = "number")]
    pub subscription_refresh_count: u64,
    #[ts(type = "number")]
    pub subscription_fail_count: u64,
    pub recent_starts: Vec<CoreStartSample>,
    pub latency_by_protocol: Vec<ProtocolLatencyStats>,
    pub recent_crashes: Vec<CrashSample>,
    pub recent_frontend_errors: Vec<FrontendErrorSample>,
    pub insights: Vec<OptimizationInsight>,
}

#[derive(Default)]
struct ProtoAccum {
    total: u64,
    successes: u64,
    timeouts: u64,
    sum_ms: u64,
    max_ms: u64,
    last_error: Option<String>,
}

#[derive(Default)]
struct TelemetryInner {
    total_core_starts: u64,
    port_reassigned_count: u64,
    failover_count: u64,
    crash_count: u64,
    network_change_count: u64,
    subscription_refresh_count: u64,
    subscription_fail_count: u64,
    starts: VecDeque<CoreStartSample>,
    crashes: VecDeque<CrashSample>,
    frontend_errors: VecDeque<FrontendErrorSample>,
    latency_stats: BTreeMap<String, ProtoAccum>,
}

/// Thread-safe performance & error telemetry collector for troubleshooting and optimization analysis.
#[derive(Clone, Default)]
pub struct TelemetryTracker {
    inner: Arc<RwLock<TelemetryInner>>,
}

impl TelemetryTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_core_start(&self, sample: CoreStartSample) {
        let mut g = self.inner.write();
        g.total_core_starts += 1;
        if sample.port_reassigned {
            g.port_reassigned_count += 1;
        }
        if g.starts.len() >= MAX_START_SAMPLES {
            g.starts.pop_front();
        }
        g.starts.push_back(sample);
    }

    pub fn record_latency_probe(
        &self,
        protocol: ProtocolType,
        latency_ms: Option<i64>,
        error: Option<&str>,
    ) {
        let key = format!("{:?}", protocol).to_ascii_lowercase();
        let mut g = self.inner.write();
        let entry = g.latency_stats.entry(key).or_default();
        entry.total += 1;
        match latency_ms {
            Some(ms) if ms >= 0 => {
                let u_ms = ms as u64;
                entry.successes += 1;
                entry.sum_ms = entry.sum_ms.saturating_add(u_ms);
                entry.max_ms = entry.max_ms.max(u_ms);
            }
            _ => {
                entry.timeouts += 1;
                if let Some(err) = error {
                    entry.last_error = Some(crate::monitor::logs::sanitize_log_message(err));
                }
            }
        }
    }

    pub fn record_failover(&self) {
        let mut g = self.inner.write();
        g.failover_count += 1;
        let _ = MAX_FAILOVER_SAMPLES;
    }

    pub fn record_crash(&self, core: CoreType, exit_code: Option<i32>, reason: &str) {
        let mut g = self.inner.write();
        g.crash_count += 1;
        if g.crashes.len() >= MAX_CRASH_SAMPLES {
            g.crashes.pop_front();
        }
        g.crashes.push_back(CrashSample {
            timestamp: Utc::now().timestamp_millis(),
            core,
            exit_code,
            reason: crate::monitor::logs::sanitize_log_message(reason),
        });
    }

    pub fn record_network_change(&self) {
        let mut g = self.inner.write();
        g.network_change_count += 1;
    }

    pub fn record_subscription_refresh(&self, success: bool) {
        let mut g = self.inner.write();
        g.subscription_refresh_count += 1;
        if !success {
            g.subscription_fail_count += 1;
        }
    }

    pub fn record_frontend_error(
        &self,
        level: LogLevel,
        component: impl Into<String>,
        message: impl Into<String>,
    ) {
        let mut g = self.inner.write();
        if g.frontend_errors.len() >= MAX_FRONTEND_ERRORS {
            g.frontend_errors.pop_front();
        }
        g.frontend_errors.push_back(FrontendErrorSample {
            timestamp: Utc::now().timestamp_millis(),
            level,
            component: component.into(),
            message: crate::monitor::logs::sanitize_log_message(&message.into()),
        });
    }

    pub fn summarize(&self, log_dir: Option<&Path>) -> TelemetrySummary {
        let total_log_bytes = log_dir
            .map(crate::monitor::logs::compute_log_dir_bytes)
            .unwrap_or(0);
        let log_dir_str = log_dir
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        let g = self.inner.read();
        let mut avg_start = 0u64;
        let mut max_start = 0u64;
        if !g.starts.is_empty() {
            let mut sum = 0u64;
            for s in &g.starts {
                sum = sum.saturating_add(s.total_ms);
                max_start = max_start.max(s.total_ms);
            }
            avg_start = sum / (g.starts.len() as u64);
        }

        let latency_by_protocol: Vec<ProtocolLatencyStats> = g
            .latency_stats
            .iter()
            .map(|(proto, acc)| ProtocolLatencyStats {
                protocol: proto.clone(),
                total: acc.total,
                successes: acc.successes,
                timeouts: acc.timeouts,
                avg_ms: acc.sum_ms.checked_div(acc.successes).unwrap_or(0),
                max_ms: acc.max_ms,
                last_error: acc.last_error.clone(),
            })
            .collect();

        let insights = build_optimization_insights(&g, avg_start, &latency_by_protocol);

        TelemetrySummary {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            log_dir: log_dir_str,
            total_log_bytes,
            total_core_starts: g.total_core_starts,
            avg_core_start_ms: avg_start,
            max_core_start_ms: max_start,
            port_reassigned_count: g.port_reassigned_count,
            failover_count: g.failover_count,
            crash_count: g.crash_count,
            network_change_count: g.network_change_count,
            subscription_refresh_count: g.subscription_refresh_count,
            subscription_fail_count: g.subscription_fail_count,
            recent_starts: g.starts.iter().cloned().collect(),
            latency_by_protocol,
            recent_crashes: g.crashes.iter().cloned().collect(),
            recent_frontend_errors: g.frontend_errors.iter().cloned().collect(),
            insights,
        }
    }
}

fn build_optimization_insights(
    g: &TelemetryInner,
    avg_start_ms: u64,
    latency_by_protocol: &[ProtocolLatencyStats],
) -> Vec<OptimizationInsight> {
    let mut insights = Vec::new();

    if g.crash_count > 0 {
        insights.push(OptimizationInsight {
            code: "CORE_CRASH_DETECTED".to_string(),
            severity: InsightSeverity::Critical,
            title_zh: "检测到代理内核异常退出".to_string(),
            title_en: "Proxy core crash detected".to_string(),
            metric: format!("{} crashes", g.crash_count),
            suggestion_zh: "请导出诊断日志包，重点检查 events.jsonl 中崩溃前的配置检验报错、端口权限或 TUN 提权状态。".to_string(),
            suggestion_en: "Export the diagnostic bundle and inspect events.jsonl before the crash timestamp for config validation errors, port permissions, or TUN privileges.".to_string(),
        });
    }

    if g.port_reassigned_count >= 2 {
        insights.push(OptimizationInsight {
            code: "FREQUENT_PORT_CONFLICT".to_string(),
            severity: InsightSeverity::Warn,
            title_zh: "本地混合端口频繁冲突自动顺延".to_string(),
            title_en: "Frequent local port conflicts detected".to_string(),
            metric: format!("{} reassignments", g.port_reassigned_count),
            suggestion_zh: "建议在「系统设置」中更换为高位冷门端口（例如 17890 / 27890），避免与其它本机代理工具冲突。".to_string(),
            suggestion_en: "Consider changing Mixed Port in Settings (e.g. 17890 or 27890) to avoid collisions with other local tools.".to_string(),
        });
    }

    if avg_start_ms > 2000 {
        insights.push(OptimizationInsight {
            code: "SLOW_CORE_STARTUP".to_string(),
            severity: InsightSeverity::Warn,
            title_zh: "内核启动与配置检查耗时偏高".to_string(),
            title_en: "Elevated core startup duration".to_string(),
            metric: format!("avg {} ms", avg_start_ms),
            suggestion_zh: "若使用 Windows Defender，建议将 NovaProxy 核心目录加入扫描排除列表；若节点数超千条，可精简未使用的路由规则集。".to_string(),
            suggestion_en: "Exclude NovaProxy core binaries from real-time AV scanning if on Windows, or trim unused routing rule-sets for faster config checks.".to_string(),
        });
    }

    for stat in latency_by_protocol {
        if stat.total >= 3 && stat.timeouts * 2 > stat.total {
            insights.push(OptimizationInsight {
                code: format!("HIGH_TIMEOUT_{}", stat.protocol.to_ascii_uppercase()),
                severity: InsightSeverity::Warn,
                title_zh: format!("{} 协议节点测速超时率过高", stat.protocol.to_uppercase()),
                title_en: format!("High probe timeout rate for {}", stat.protocol.to_uppercase()),
                metric: format!("{}/{} timeouts", stat.timeouts, stat.total),
                suggestion_zh: "对于 Hysteria2 / TUIC / WireGuard 等 UDP 协议，请确认使用「URL 204」真实测速而非 TCP Ping，并排查运营商 UDP QoS 或调整端口跳跃配置。".to_string(),
                suggestion_en: "For UDP-based protocols (Hysteria2/TUIC/WireGuard), verify URL 204 mode is selected instead of TCP Ping, and check ISP UDP QoS or port-hopping settings.".to_string(),
            });
        }
    }

    if g.failover_count >= 2 {
        insights.push(OptimizationInsight {
            code: "FREQUENT_FAILOVER".to_string(),
            severity: InsightSeverity::Warn,
            title_zh: "健康巡检频繁触发故障转移".to_string(),
            title_en: "Frequent automatic health failovers".to_string(),
            metric: format!("{} failovers", g.failover_count),
            suggestion_zh: "当前主用节点稳定性较差，建议查看测速排序后长期切换至平均延迟最低且波动小的节点。".to_string(),
            suggestion_en: "Current primary node shows instability; sort nodes by URL delay and switch to a lower-jitter endpoint.".to_string(),
        });
    }

    if g.subscription_fail_count > 0 {
        insights.push(OptimizationInsight {
            code: "SUBSCRIPTION_FETCH_FAILURE".to_string(),
            severity: InsightSeverity::Info,
            title_zh: "部分订阅刷新请求失败".to_string(),
            title_en: "Some subscription updates failed".to_string(),
            metric: format!(
                "{}/{} failed",
                g.subscription_fail_count, g.subscription_refresh_count
            ),
            suggestion_zh: "建议对被墙订阅域名开启「通过代理更新」，并检查订阅链接有效性。"
                .to_string(),
            suggestion_en:
                "Enable 'Use Proxy' on blocked subscription endpoints and verify subscription URLs."
                    .to_string(),
        });
    }

    if insights.is_empty() {
        insights.push(OptimizationInsight {
            code: "SYSTEM_HEALTHY".to_string(),
            severity: InsightSeverity::Info,
            title_zh: "当前运行指标良好，暂未发现性能瓶颈".to_string(),
            title_en: "Runtime telemetry looks healthy".to_string(),
            metric: format!("{} starts", g.total_core_starts),
            suggestion_zh: "保持开启 URL 204 测速与持久化滚动日志，遇到网络波动可一键导出诊断包分析。".to_string(),
            suggestion_en: "Keep persistent rolling logs enabled; export a diagnostic bundle anytime if network anomalies occur.".to_string(),
        });
    }

    insights
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_insights_generation() {
        let tracker = TelemetryTracker::new();
        tracker.record_core_start(CoreStartSample {
            timestamp: 1000,
            core: CoreType::SingBox,
            upstream_core: None,
            config_gen_ms: 12,
            port_resolve_ms: 8,
            spawn_ms: 95,
            total_ms: 2500,
            port_reassigned: true,
            success: true,
            error: None,
        });
        tracker.record_core_start(CoreStartSample {
            timestamp: 2000,
            core: CoreType::SingBox,
            upstream_core: None,
            config_gen_ms: 10,
            port_resolve_ms: 5,
            spawn_ms: 90,
            total_ms: 2300,
            port_reassigned: true,
            success: true,
            error: None,
        });
        tracker.record_latency_probe(ProtocolType::Hysteria2, None, Some("udp timeout"));
        tracker.record_latency_probe(ProtocolType::Hysteria2, None, Some("udp timeout"));
        tracker.record_latency_probe(ProtocolType::Hysteria2, Some(120), None);
        tracker.record_crash(CoreType::Mihomo, Some(1), "panic test");

        let summary = tracker.summarize(None);
        assert_eq!(summary.total_core_starts, 2);
        assert_eq!(summary.port_reassigned_count, 2);
        assert_eq!(summary.crash_count, 1);
        assert!(summary
            .insights
            .iter()
            .any(|i| i.code == "CORE_CRASH_DETECTED"));
        assert!(summary
            .insights
            .iter()
            .any(|i| i.code == "FREQUENT_PORT_CONFLICT"));
        assert!(summary
            .insights
            .iter()
            .any(|i| i.code == "SLOW_CORE_STARTUP"));
        assert!(summary
            .insights
            .iter()
            .any(|i| i.code == "HIGH_TIMEOUT_HYSTERIA2"));
    }
}

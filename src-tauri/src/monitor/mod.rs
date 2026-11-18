//! Traffic, connection, latency, telemetry, and log monitoring
//! Implemented in Phases P1-P8+.

pub mod connections;
pub mod health;
pub mod latency;
pub mod logs;
pub mod telemetry;
pub mod traffic;

pub use connections::{close_clash_connection, fetch_clash_connections};
pub use health::{FailoverEvent, HealthCheckResult, HealthError, HealthMonitor};
pub use latency::{
    tcp_ping, test_nodes_batch, test_single_node_latency, LatencyTestMode,
    DEFAULT_LATENCY_TEST_URL, DEFAULT_TCP_TIMEOUT_MS, DEFAULT_URL_TIMEOUT_MS,
};
pub use logs::{
    compute_log_dir_bytes, list_log_files, read_log_file_tail, sanitize_log_message, CoreLogEntry,
    DiagnosticBundleResult, LogFileInfo, LogRingBuffer, LogStream, NovaTracingLayer,
    PersistentLogWriter, DEFAULT_LOG_RING_CAPACITY,
};
pub use telemetry::{
    CoreStartSample, CrashSample, FrontendErrorSample, InsightSeverity, OptimizationInsight,
    ProtocolLatencyStats, TelemetrySummary, TelemetryTracker,
};
pub use traffic::{fetch_clash_traffic, TrafficSnapshot};

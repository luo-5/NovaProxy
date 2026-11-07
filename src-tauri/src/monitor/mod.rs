//! Traffic, connection, latency, and log monitoring
//! Implemented in Phases P1-P5.

pub mod connections;
pub mod health;
pub mod latency;
pub mod logs;
pub mod traffic;

pub use health::{FailoverEvent, HealthCheckResult, HealthError, HealthMonitor};
pub use logs::{CoreLogEntry, LogRingBuffer, LogStream, DEFAULT_LOG_RING_CAPACITY};

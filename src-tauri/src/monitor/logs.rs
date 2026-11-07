use crate::model::{CoreType, LogLevel};
use chrono::Utc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use ts_rs::TS;

pub const DEFAULT_LOG_RING_CAPACITY: usize = 2000;

/// Origin stream of a log line
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum LogStream {
    Stdout,
    Stderr,
    System,
}

/// Structured log entry stored in the ring buffer and emitted over Tauri IPC
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CoreLogEntry {
    #[ts(type = "number")]
    pub seq: u64,
    #[ts(type = "number")]
    pub timestamp: i64,
    pub core: Option<CoreType>,
    pub stream: LogStream,
    pub level: LogLevel,
    pub message: String,
}

/// Infers log level from a raw core output line (sing-box, mihomo, Xray)
pub fn infer_log_level(line: &str, stream: LogStream) -> LogLevel {
    let upper = line.to_ascii_uppercase();
    if upper.contains("FATAL")
        || upper.contains("ERROR")
        || upper.contains("LEVEL=ERROR")
        || upper.contains("[ERROR]")
        || upper.contains(" ERR ")
    {
        LogLevel::Error
    } else if upper.contains("WARN")
        || upper.contains("LEVEL=WARNING")
        || upper.contains("[WARNING]")
        || upper.contains(" WRN ")
    {
        LogLevel::Warn
    } else if upper.contains("DEBUG")
        || upper.contains("LEVEL=DEBUG")
        || upper.contains("[DEBUG]")
        || upper.contains(" DBG ")
    {
        LogLevel::Debug
    } else if upper.contains("TRACE") || upper.contains("LEVEL=TRACE") {
        LogLevel::Trace
    } else if upper.contains("INFO")
        || upper.contains("LEVEL=INFO")
        || upper.contains("[INFO]")
        || upper.contains(" INF ")
    {
        LogLevel::Info
    } else {
        match stream {
            LogStream::Stderr => LogLevel::Warn,
            LogStream::Stdout | LogStream::System => LogLevel::Info,
        }
    }
}

struct RingInner {
    capacity: usize,
    next_seq: u64,
    entries: VecDeque<CoreLogEntry>,
}

/// Thread-safe bounded ring buffer for core stdout/stderr/system logs
#[derive(Clone)]
pub struct LogRingBuffer {
    inner: Arc<RwLock<RingInner>>,
}

impl Default for LogRingBuffer {
    fn default() -> Self {
        Self::new(DEFAULT_LOG_RING_CAPACITY)
    }
}

impl LogRingBuffer {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.max(16);
        Self {
            inner: Arc::new(RwLock::new(RingInner {
                capacity: cap,
                next_seq: 1,
                entries: VecDeque::with_capacity(cap),
            })),
        }
    }

    pub fn push(
        &self,
        core: Option<CoreType>,
        stream: LogStream,
        message: impl Into<String>,
    ) -> CoreLogEntry {
        let msg = message.into();
        let level = infer_log_level(&msg, stream);
        self.push_with_level(core, stream, level, msg)
    }

    pub fn push_with_level(
        &self,
        core: Option<CoreType>,
        stream: LogStream,
        level: LogLevel,
        message: impl Into<String>,
    ) -> CoreLogEntry {
        let mut guard = self.inner.write();
        let seq = guard.next_seq;
        guard.next_seq += 1;

        let entry = CoreLogEntry {
            seq,
            timestamp: Utc::now().timestamp_millis(),
            core,
            stream,
            level,
            message: message.into(),
        };

        if guard.entries.len() >= guard.capacity {
            guard.entries.pop_front();
        }
        guard.entries.push_back(entry.clone());
        entry
    }

    /// Retrieves log entries optionally filtered by `since_seq` (exclusive) and `limit` (most recent N)
    pub fn list(&self, since_seq: Option<u64>, limit: Option<usize>) -> Vec<CoreLogEntry> {
        let guard = self.inner.read();
        let filtered: Vec<CoreLogEntry> = guard
            .entries
            .iter()
            .filter(|e| since_seq.map(|s| e.seq > s).unwrap_or(true))
            .cloned()
            .collect();

        match limit {
            Some(max_len) if filtered.len() > max_len => {
                filtered[filtered.len() - max_len..].to_vec()
            }
            _ => filtered,
        }
    }

    pub fn clear(&self) {
        let mut guard = self.inner.write();
        guard.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.inner.read().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.read().entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_capacity_and_filtering() {
        let buf = LogRingBuffer::new(16);
        for i in 1..=25 {
            buf.push(
                Some(CoreType::SingBox),
                LogStream::Stdout,
                format!("INFO[000{}] line {}", i, i),
            );
        }

        assert_eq!(buf.len(), 16);
        let all = buf.list(None, None);
        assert_eq!(all.first().unwrap().seq, 10);
        assert_eq!(all.last().unwrap().seq, 25);

        let since_20 = buf.list(Some(20), None);
        assert_eq!(since_20.len(), 5);
        assert_eq!(since_20[0].seq, 21);

        let last_3 = buf.list(None, Some(3));
        assert_eq!(last_3.len(), 3);
        assert_eq!(last_3[0].seq, 23);
    }

    #[test]
    fn test_log_level_inference() {
        assert_eq!(
            infer_log_level("FATAL[0000] decode config failed", LogStream::Stderr),
            LogLevel::Error
        );
        assert_eq!(
            infer_log_level("WARN[0001] deprecated option", LogStream::Stdout),
            LogLevel::Warn
        );
        assert_eq!(
            infer_log_level("INFO[0000] sing-box started", LogStream::Stdout),
            LogLevel::Info
        );
        assert_eq!(
            infer_log_level(
                "time=\"2026-11-07\" level=debug msg=\"dns\"",
                LogStream::Stdout
            ),
            LogLevel::Debug
        );
    }
}

use crate::model::{CoreType, LogLevel};
use chrono::{DateTime, Utc};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::field::{Field, Visit};
use tracing::Subscriber;
use tracing_subscriber::layer::{Context, Layer};
use ts_rs::TS;

pub const DEFAULT_LOG_RING_CAPACITY: usize = 2000;
const MAX_LOG_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_ROTATED_ARCHIVES: usize = 4;

/// Origin stream of a log line
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum LogStream {
    Stdout,
    Stderr,
    System,
    App,
    Frontend,
}

/// Structured log entry stored in the ring buffer, written to disk, and emitted over Tauri IPC
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CoreLogEntry {
    #[ts(type = "number")]
    pub seq: u64,
    #[ts(type = "number")]
    pub timestamp: i64,
    pub core: Option<CoreType>,
    pub stream: LogStream,
    pub level: LogLevel,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub duration_ms: Option<u64>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LogFileInfo {
    pub name: String,
    #[ts(type = "number")]
    pub size_bytes: u64,
    #[ts(type = "number")]
    pub modified_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DiagnosticBundleResult {
    pub path: String,
    pub filename: String,
    #[ts(type = "number")]
    pub size_bytes: u64,
}

/// Redacts common credential patterns (passwords, bearer tokens, URI user:pass, PSKs, private keys)
/// before storing or exporting logs while preserving host/port/latency troubleshooting metadata.
pub fn sanitize_log_message(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;

    // 1. Mask `scheme://credentials@host` userinfo payloads when non-empty
    while let Some(scheme_idx) = rest.find("://") {
        let split_at = scheme_idx + 3;
        out.push_str(&rest[..split_at]);
        rest = &rest[split_at..];

        let boundary = rest
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ',' | ')' | ']'))
            .unwrap_or(rest.len());
        let segment = &rest[..boundary];
        if let Some(at_idx) = segment.find('@') {
            if !segment[..at_idx].is_empty() {
                out.push_str("***");
                out.push_str(&segment[at_idx..]);
                rest = &rest[boundary..];
                continue;
            }
        }
        out.push_str(segment);
        rest = &rest[boundary..];
    }
    out.push_str(rest);

    // 2. Mask query / JSON secret key-value fields case-insensitively
    let mut masked = out;
    for key in [
        "password=",
        "secret=",
        "token=",
        "psk=",
        "private_key=",
        "preshared_key=",
        "clash_api_secret=",
    ] {
        masked = mask_query_key(&masked, key);
    }
    for json_key in [
        "\"password\":",
        "\"uuid\":",
        "\"psk\":",
        "\"private_key\":",
        "\"preshared_key\":",
        "\"clash_api_secret\":",
        "\"secret\":",
    ] {
        masked = mask_json_key(&masked, json_key);
    }

    if let Some(idx) = masked.to_ascii_lowercase().find("bearer ") {
        let token_start = idx + "bearer ".len();
        let tail = &masked[token_start..];
        let token_end = tail
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ',' | '}'))
            .unwrap_or(tail.len());
        if token_end > 0 {
            let mut replaced = String::with_capacity(masked.len());
            replaced.push_str(&masked[..token_start]);
            replaced.push_str("***REDACTED***");
            replaced.push_str(&tail[token_end..]);
            masked = replaced;
        }
    }

    masked
}

fn mask_query_key(text: &str, needle: &str) -> String {
    let lower = text.to_ascii_lowercase();
    if !lower.contains(needle) {
        return text.to_string();
    }
    let mut result = String::with_capacity(text.len());
    let mut cursor = 0;
    while let Some(rel) = lower[cursor..].find(needle) {
        let key_start = cursor + rel;
        let val_start = key_start + needle.len();
        result.push_str(&text[cursor..val_start]);
        let val_tail = &text[val_start..];
        let val_end = val_tail
            .find(|c: char| c.is_whitespace() || matches!(c, '&' | '#' | '"' | '\'' | ')' | ']'))
            .unwrap_or(val_tail.len());
        if val_end > 0 {
            result.push_str("***");
        }
        cursor = val_start + val_end;
    }
    result.push_str(&text[cursor..]);
    result
}

fn mask_json_key(text: &str, needle: &str) -> String {
    let lower = text.to_ascii_lowercase();
    if !lower.contains(needle) {
        return text.to_string();
    }
    let mut result = String::with_capacity(text.len());
    let mut cursor = 0;
    while let Some(rel) = lower[cursor..].find(needle) {
        let key_start = cursor + rel;
        let after_colon = key_start + needle.len();
        let tail = &text[after_colon..];
        let ws_len = tail.len() - tail.trim_start().len();
        let quote_idx = after_colon + ws_len;
        result.push_str(&text[cursor..quote_idx]);
        if text[quote_idx..].starts_with('"') {
            if let Some(close_rel) = text[quote_idx + 1..].find('"') {
                result.push_str("\"***REDACTED***\"");
                cursor = quote_idx + 1 + close_rel + 1;
                continue;
            }
        }
        cursor = quote_idx;
    }
    result.push_str(&text[cursor..]);
    result
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
            LogStream::Stdout | LogStream::System | LogStream::App | LogStream::Frontend => {
                LogLevel::Info
            }
        }
    }
}

struct FileSinkState {
    text_writer: Option<BufWriter<File>>,
    text_bytes: u64,
    json_writer: Option<BufWriter<File>>,
    json_bytes: u64,
}

/// Persistent size-rotating dual log writer (`novaproxy.log` + `events.jsonl`)
#[derive(Clone)]
pub struct PersistentLogWriter {
    log_dir: PathBuf,
    state: Arc<Mutex<FileSinkState>>,
}

impl PersistentLogWriter {
    pub fn open(log_dir: impl Into<PathBuf>) -> std::io::Result<Self> {
        let dir = log_dir.into();
        fs::create_dir_all(&dir)?;

        let text_path = dir.join("novaproxy.log");
        let text_bytes = fs::metadata(&text_path).map(|m| m.len()).unwrap_or(0);
        let text_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&text_path)?;

        let json_path = dir.join("events.jsonl");
        let json_bytes = fs::metadata(&json_path).map(|m| m.len()).unwrap_or(0);
        let json_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&json_path)?;

        Ok(Self {
            log_dir: dir,
            state: Arc::new(Mutex::new(FileSinkState {
                text_writer: Some(BufWriter::new(text_file)),
                text_bytes,
                json_writer: Some(BufWriter::new(json_file)),
                json_bytes,
            })),
        })
    }

    pub fn log_dir(&self) -> &Path {
        &self.log_dir
    }

    pub fn write_entry(&self, entry: &CoreLogEntry) {
        let mut g = self.state.lock();

        if g.text_bytes >= MAX_LOG_FILE_BYTES {
            g.text_writer = None;
            rotate_log_archive(&self.log_dir, "novaproxy", "log");
            if let Ok(f) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.log_dir.join("novaproxy.log"))
            {
                g.text_writer = Some(BufWriter::new(f));
                g.text_bytes = 0;
            }
        }

        if g.json_bytes >= MAX_LOG_FILE_BYTES {
            g.json_writer = None;
            rotate_log_archive(&self.log_dir, "events", "jsonl");
            if let Ok(f) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.log_dir.join("events.jsonl"))
            {
                g.json_writer = Some(BufWriter::new(f));
                g.json_bytes = 0;
            }
        }

        let ts_str = DateTime::<Utc>::from_timestamp_millis(entry.timestamp)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S%.3f").to_string())
            .unwrap_or_else(|| entry.timestamp.to_string());
        let src_tag = entry
            .source
            .as_deref()
            .or_else(|| entry.core.as_ref().map(|c| c.as_str()))
            .unwrap_or("system");
        let dur_suffix = entry
            .duration_ms
            .map(|d| format!(" ({d}ms)"))
            .unwrap_or_default();
        let level_label = format!("{:?}", entry.level).to_ascii_uppercase();
        let line = format!(
            "{ts_str} [{level_label:<5}] [{src_tag}]{dur_suffix} {}\n",
            entry.message
        );

        let wrote_text = if let Some(ref mut w) = g.text_writer {
            if w.write_all(line.as_bytes()).is_ok() {
                let _ = w.flush();
                true
            } else {
                false
            }
        } else {
            false
        };
        if wrote_text {
            g.text_bytes = g.text_bytes.saturating_add(line.len() as u64);
        }

        if let Ok(json_line) = serde_json::to_string(entry) {
            let bytes = format!("{json_line}\n");
            let wrote_json = if let Some(ref mut w) = g.json_writer {
                if w.write_all(bytes.as_bytes()).is_ok() {
                    let _ = w.flush();
                    true
                } else {
                    false
                }
            } else {
                false
            };
            if wrote_json {
                g.json_bytes = g.json_bytes.saturating_add(bytes.len() as u64);
            }
        }
    }
}

fn rotate_log_archive(dir: &Path, stem: &str, ext: &str) {
    for idx in (1..=MAX_ROTATED_ARCHIVES).rev() {
        let src = if idx == 1 {
            dir.join(format!("{stem}.{ext}"))
        } else {
            dir.join(format!("{stem}.{}.{ext}", idx - 1))
        };
        let dst = dir.join(format!("{stem}.{idx}.{ext}"));
        if src.exists() {
            if dst.exists() {
                let _ = fs::remove_file(&dst);
            }
            let _ = fs::rename(&src, &dst);
        }
    }
}

pub fn compute_log_dir_bytes(log_dir: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(log_dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}

pub fn list_log_files(log_dir: &Path) -> Vec<LogFileInfo> {
    let Ok(entries) = fs::read_dir(log_dir) else {
        return Vec::new();
    };
    let mut files: Vec<LogFileInfo> = entries
        .flatten()
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let modified_at = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            Some(LogFileInfo {
                name,
                size_bytes: meta.len(),
                modified_at,
            })
        })
        .collect();
    files.sort_by(|a, b| b.modified_at.cmp(&a.modified_at).then(a.name.cmp(&b.name)));
    files
}

pub fn read_log_file_tail(
    log_dir: &Path,
    file_name: &str,
    tail_lines: usize,
) -> Result<String, String> {
    let safe_name = Path::new(file_name)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "Invalid log file name".to_string())?;
    let target = log_dir.join(safe_name);
    if !target.exists() {
        return Err(format!("Log file not found: {safe_name}"));
    }
    let content = fs::read_to_string(&target).map_err(|e| e.to_string())?;
    let max_lines = tail_lines.clamp(1, 2000);
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    Ok(sanitize_log_message(&lines[start..].join("\n")))
}

pub type LiveLogEmitter = Arc<dyn Fn(&CoreLogEntry) + Send + Sync>;

struct RingInner {
    capacity: usize,
    next_seq: u64,
    entries: VecDeque<CoreLogEntry>,
    file_writer: Option<PersistentLogWriter>,
    live_emitter: Option<LiveLogEmitter>,
}

/// Thread-safe bounded ring buffer for core & application logs with optional persistent rolling disk writer.
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
                file_writer: None,
                live_emitter: None,
            })),
        }
    }

    pub fn attach_file_writer(&self, writer: PersistentLogWriter) {
        self.inner.write().file_writer = Some(writer);
    }

    pub fn set_live_emitter(&self, emitter: LiveLogEmitter) {
        self.inner.write().live_emitter = Some(emitter);
    }

    pub fn log_dir(&self) -> Option<PathBuf> {
        self.inner
            .read()
            .file_writer
            .as_ref()
            .map(|w| w.log_dir().to_path_buf())
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
        self.push_detailed(core, stream, level, None, None, message)
    }

    pub fn push_detailed(
        &self,
        core: Option<CoreType>,
        stream: LogStream,
        level: LogLevel,
        source: Option<String>,
        duration_ms: Option<u64>,
        message: impl Into<String>,
    ) -> CoreLogEntry {
        let sanitized = sanitize_log_message(&message.into());
        let (entry, writer_opt, emitter_opt) = {
            let mut guard = self.inner.write();
            let seq = guard.next_seq;
            guard.next_seq += 1;

            let entry = CoreLogEntry {
                seq,
                timestamp: Utc::now().timestamp_millis(),
                core,
                stream,
                level,
                source,
                duration_ms,
                message: sanitized,
            };

            if guard.entries.len() >= guard.capacity {
                guard.entries.pop_front();
            }
            guard.entries.push_back(entry.clone());
            (entry, guard.file_writer.clone(), guard.live_emitter.clone())
        };

        if let Some(writer) = writer_opt {
            writer.write_entry(&entry);
        }
        if let Some(emitter) = emitter_opt {
            emitter(&entry);
        }
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

/// Captures Rust `tracing` events from NovaProxy subsystems into `LogRingBuffer` + persistent rolling disk logs.
pub struct NovaTracingLayer {
    ring: LogRingBuffer,
}

impl NovaTracingLayer {
    pub fn new(ring: LogRingBuffer) -> Self {
        Self { ring }
    }
}

#[derive(Default)]
struct TracingFieldVisitor {
    message: Option<String>,
    duration_ms: Option<u64>,
    extra: Vec<String>,
}

impl Visit for TracingFieldVisitor {
    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name() == "duration_ms" {
            self.duration_ms = Some(value);
        } else {
            self.extra.push(format!("{}={}", field.name(), value));
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        if field.name() == "duration_ms" && value >= 0 {
            self.duration_ms = Some(value as u64);
        } else {
            self.extra.push(format!("{}={}", field.name(), value));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        } else {
            self.extra.push(format!("{}={}", field.name(), value));
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{:?}", value));
        } else {
            self.extra.push(format!("{}={:?}", field.name(), value));
        }
    }
}

impl<S: Subscriber> Layer<S> for NovaTracingLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        let target = meta.target();
        let is_ours = target.starts_with("novaproxy");
        let level = match *meta.level() {
            tracing::Level::ERROR => LogLevel::Error,
            tracing::Level::WARN => LogLevel::Warn,
            tracing::Level::INFO => LogLevel::Info,
            tracing::Level::DEBUG => LogLevel::Debug,
            tracing::Level::TRACE => LogLevel::Trace,
        };

        // Keep noisy third-party crate debug/info traces out of the user diagnostics stream
        if !is_ours && matches!(level, LogLevel::Info | LogLevel::Debug | LogLevel::Trace) {
            return;
        }

        let mut visitor = TracingFieldVisitor::default();
        event.record(&mut visitor);

        let short_source = target
            .strip_prefix("novaproxy_lib::")
            .or_else(|| target.strip_prefix("novaproxy::"))
            .unwrap_or(target)
            .to_string();

        let body = match (visitor.message, visitor.extra.is_empty()) {
            (Some(msg), true) => msg,
            (Some(msg), false) => format!("{msg} | {}", visitor.extra.join(", ")),
            (None, false) => visitor.extra.join(", "),
            (None, true) => meta.name().to_string(),
        };

        self.ring.push_detailed(
            None,
            LogStream::App,
            level,
            Some(short_source),
            visitor.duration_ms,
            body,
        );
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

    #[test]
    fn test_sanitize_log_message_redacts_secrets() {
        let raw = "Connecting vless://11111111-2222-3333-4444-555555555555@vpn.example.invalid:443?password=mySecretPw&token=abc123 Authorization: Bearer super_secret_token {\"uuid\":\"secret-uuid\"}";
        let clean = sanitize_log_message(raw);
        assert!(!clean.contains("11111111-2222-3333-4444-555555555555"));
        assert!(!clean.contains("mySecretPw"));
        assert!(!clean.contains("abc123"));
        assert!(!clean.contains("super_secret_token"));
        assert!(!clean.contains("secret-uuid"));
        assert!(clean.contains("vpn.example.invalid:443"));
    }

    #[test]
    fn test_persistent_log_writer_and_tail_read() {
        let dir = std::env::temp_dir().join("novaproxy_test_persistent_logs");
        let _ = fs::remove_dir_all(&dir);
        let writer = PersistentLogWriter::open(&dir).expect("open log writer");

        let ring = LogRingBuffer::new(32);
        ring.attach_file_writer(writer);
        ring.push_detailed(
            Some(CoreType::SingBox),
            LogStream::System,
            LogLevel::Info,
            Some("core::manager".to_string()),
            Some(142),
            "Started proxy with password=secret_pw",
        );

        let files = list_log_files(&dir);
        assert!(files.iter().any(|f| f.name == "novaproxy.log"));
        assert!(files.iter().any(|f| f.name == "events.jsonl"));

        let tail = read_log_file_tail(&dir, "novaproxy.log", 10).expect("read log tail");
        assert!(tail.contains("[core::manager] (142ms)"));
        assert!(!tail.contains("secret_pw"));
        let _ = fs::remove_dir_all(&dir);
    }
}

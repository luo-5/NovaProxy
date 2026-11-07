use crate::core::capabilities::CoreCapabilitiesRegistry;
use crate::core::process::{
    build_core_run_command, check_core_config, default_minimal_singbox_config,
    resolve_and_patch_config_port, JobObject, ProcessError,
};
use crate::model::{CoreType, LogLevel};
use crate::monitor::{CoreLogEntry, LogRingBuffer, LogStream};
use chrono::Utc;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Mutex as AsyncMutex;
use ts_rs::TS;

#[derive(Debug, Error)]
pub enum ManagerError {
    #[error("Invalid state transition from {from:?} to {to:?}")]
    InvalidStateTransition {
        from: CoreLifecycleState,
        to: CoreLifecycleState,
    },
    #[error("Process error: {0}")]
    Process(#[from] ProcessError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Core process lifecycle state machine (`Stopped → Starting → Running → Stopping / Crashed`)
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum CoreLifecycleState {
    #[default]
    Stopped,
    Starting,
    Running,
    Stopping,
    Crashed {
        exit_code: Option<i32>,
        reason: String,
    },
}

impl CoreLifecycleState {
    /// Validates whether transitioning from `self` to `next` is permitted by the state machine
    pub fn can_transition_to(&self, next: &CoreLifecycleState) -> bool {
        matches!(
            (self, next),
            (
                CoreLifecycleState::Stopped,
                CoreLifecycleState::Starting | CoreLifecycleState::Stopped
            ) | (
                CoreLifecycleState::Crashed { .. },
                CoreLifecycleState::Starting | CoreLifecycleState::Stopped,
            ) | (
                CoreLifecycleState::Starting,
                CoreLifecycleState::Running
                    | CoreLifecycleState::Stopping
                    | CoreLifecycleState::Stopped
                    | CoreLifecycleState::Crashed { .. },
            ) | (
                CoreLifecycleState::Running,
                CoreLifecycleState::Stopping | CoreLifecycleState::Crashed { .. },
            ) | (
                CoreLifecycleState::Stopping,
                CoreLifecycleState::Stopped | CoreLifecycleState::Crashed { .. },
            )
        )
    }

    pub fn transition_to(&mut self, next: CoreLifecycleState) -> Result<(), ManagerError> {
        if !self.can_transition_to(&next) {
            return Err(ManagerError::InvalidStateTransition {
                from: self.clone(),
                to: next,
            });
        }
        *self = next;
        Ok(())
    }
}

/// Snapshot of current core runtime status
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CoreStatusInfo {
    pub lifecycle: CoreLifecycleState,
    pub active_core: Option<CoreType>,
    pub pid: Option<u32>,
    #[ts(type = "number | null")]
    pub started_at: Option<i64>,
    pub requested_port: Option<u16>,
    pub actual_mixed_port: Option<u16>,
    pub port_reassigned: bool,
    pub config_path: Option<String>,
}

/// Request payload for `core_start`
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct CoreStartRequest {
    #[serde(default)]
    pub core_type: Option<CoreType>,
    #[serde(default)]
    pub config_json: Option<String>,
    #[serde(default)]
    pub mixed_port: Option<u16>,
    #[serde(default)]
    pub allow_lan: Option<bool>,
    #[serde(default)]
    pub auto_port_fallback: Option<bool>,
}

/// Event callback sink for broadcasting logs and status transitions to Tauri frontend
pub trait CoreEventSink: Send + Sync {
    fn on_log(&self, entry: &CoreLogEntry);
    fn on_status_change(&self, status: &CoreStatusInfo);
}

/// Default no-op event sink for unit tests
pub struct NoopEventSink;

impl CoreEventSink for NoopEventSink {
    fn on_log(&self, _entry: &CoreLogEntry) {}
    fn on_status_change(&self, _status: &CoreStatusInfo) {}
}

struct RunningChild {
    instance_id: u64,
    child: tokio::process::Child,
    _job: JobObject,
}

struct StateInner {
    status: CoreStatusInfo,
    stopping_intentionally: bool,
    active_instance_id: u64,
    last_start_request: Option<CoreStartRequest>,
    crash_count: u32,
    last_crash_at: Option<i64>,
    auto_heal: bool,
}

/// Central manager for proxy core processes, logs, and capabilities
#[derive(Clone)]
pub struct CoreManager {
    cores_dir: PathBuf,
    work_dir: PathBuf,
    state: Arc<Mutex<StateInner>>,
    child_holder: Arc<AsyncMutex<Option<RunningChild>>>,
    instance_seq: Arc<AtomicU64>,
    logs: LogRingBuffer,
    capabilities: CoreCapabilitiesRegistry,
    event_sink: Arc<Mutex<Arc<dyn CoreEventSink>>>,
}

impl CoreManager {
    pub fn new(cores_dir: impl Into<PathBuf>, work_dir: impl Into<PathBuf>) -> Self {
        let cores_dir = cores_dir.into();
        let work_dir = work_dir.into();
        let _ = std::fs::create_dir_all(&work_dir);

        Self {
            cores_dir,
            work_dir,
            state: Arc::new(Mutex::new(StateInner {
                status: CoreStatusInfo::default(),
                stopping_intentionally: false,
                active_instance_id: 0,
                last_start_request: None,
                crash_count: 0,
                last_crash_at: None,
                auto_heal: true,
            })),
            child_holder: Arc::new(AsyncMutex::new(None)),
            instance_seq: Arc::new(AtomicU64::new(1)),
            logs: LogRingBuffer::default(),
            capabilities: CoreCapabilitiesRegistry::new(),
            event_sink: Arc::new(Mutex::new(Arc::new(NoopEventSink))),
        }
    }

    pub fn set_auto_heal(&self, enabled: bool) {
        self.state.lock().auto_heal = enabled;
    }

    pub fn auto_heal(&self) -> bool {
        self.state.lock().auto_heal
    }

    pub fn reset_crash_count(&self) {
        let mut guard = self.state.lock();
        guard.crash_count = 0;
        guard.last_crash_at = None;
    }

    pub fn set_event_sink(&self, sink: Arc<dyn CoreEventSink>) {
        *self.event_sink.lock() = sink;
    }

    pub fn cores_dir(&self) -> &Path {
        &self.cores_dir
    }

    pub fn work_dir(&self) -> &Path {
        &self.work_dir
    }

    pub fn capabilities(&self) -> &CoreCapabilitiesRegistry {
        &self.capabilities
    }

    pub fn binary_path(&self, core: CoreType) -> PathBuf {
        self.cores_dir.join(core.binary_name())
    }

    pub fn available_cores(&self) -> Vec<CoreType> {
        [CoreType::SingBox, CoreType::Mihomo, CoreType::Xray]
            .into_iter()
            .filter(|&c| self.binary_path(c).exists())
            .collect()
    }

    pub fn status(&self) -> CoreStatusInfo {
        self.state.lock().status.clone()
    }

    pub fn logs(&self, since_seq: Option<u64>, limit: Option<usize>) -> Vec<CoreLogEntry> {
        self.logs.list(since_seq, limit)
    }

    pub fn clear_logs(&self) {
        self.logs.clear();
    }

    fn emit_log(
        &self,
        core: Option<CoreType>,
        stream: LogStream,
        level: LogLevel,
        msg: impl Into<String>,
    ) {
        let entry = self.logs.push_with_level(core, stream, level, msg);
        let sink = self.event_sink.lock().clone();
        sink.on_log(&entry);
    }

    fn emit_status(&self, status: &CoreStatusInfo) {
        let sink = self.event_sink.lock().clone();
        sink.on_status_change(status);
    }

    fn update_lifecycle(&self, next: CoreLifecycleState) -> Result<CoreStatusInfo, ManagerError> {
        let snapshot = {
            let mut guard = self.state.lock();
            guard.status.lifecycle.transition_to(next)?;
            if matches!(
                guard.status.lifecycle,
                CoreLifecycleState::Stopped | CoreLifecycleState::Crashed { .. }
            ) {
                guard.status.pid = None;
            }
            guard.status.clone()
        };
        self.emit_status(&snapshot);
        Ok(snapshot)
    }

    /// Starts the proxy core with pre-flight configuration check, automatic port conflict resolution,
    /// Windows Job Object binding, and real-time stdout/stderr capture.
    pub fn start(
        &self,
        req: CoreStartRequest,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<CoreStatusInfo, ManagerError>> + Send + '_>,
    > {
        Box::pin(self.start_inner(req))
    }

    async fn start_inner(&self, req: CoreStartRequest) -> Result<CoreStatusInfo, ManagerError> {
        // If already running or starting, stop cleanly first
        {
            let current = self.state.lock().status.lifecycle.clone();
            if matches!(
                current,
                CoreLifecycleState::Running | CoreLifecycleState::Starting
            ) {
                self.stop().await?;
            }
        }

        let core = req.core_type.unwrap_or(CoreType::SingBox);
        let preferred_port = req.mixed_port.unwrap_or(7890);
        let allow_lan = req.allow_lan.unwrap_or(false);
        let auto_fallback = req.auto_port_fallback.unwrap_or(true);

        // Transition to Starting
        {
            let mut guard = self.state.lock();
            guard
                .status
                .lifecycle
                .transition_to(CoreLifecycleState::Starting)?;
            guard.status.active_core = Some(core);
            guard.status.pid = None;
            guard.status.started_at = None;
            guard.status.requested_port = Some(preferred_port);
            guard.status.actual_mixed_port = None;
            guard.status.port_reassigned = false;
            guard.stopping_intentionally = false;
            guard.last_start_request = Some(req.clone());
            let snap = guard.status.clone();
            drop(guard);
            self.emit_status(&snap);
        }

        self.emit_log(
            Some(core),
            LogStream::System,
            LogLevel::Info,
            format!(
                "Starting core {} (requested port: {})...",
                core, preferred_port
            ),
        );

        let raw_config = req
            .config_json
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| default_minimal_singbox_config(preferred_port));

        // 1. Resolve port conflicts (for JSON configs like sing-box / xray)
        let port_res = match core {
            CoreType::SingBox | CoreType::Xray => {
                match resolve_and_patch_config_port(
                    &raw_config,
                    preferred_port,
                    allow_lan,
                    auto_fallback,
                ) {
                    Ok(res) => res,
                    Err(e) => {
                        let reason = e.to_string();
                        self.emit_log(Some(core), LogStream::System, LogLevel::Error, &reason);
                        let _ = self.update_lifecycle(CoreLifecycleState::Crashed {
                            exit_code: None,
                            reason,
                        });
                        return Err(ManagerError::Process(e));
                    }
                }
            }
            CoreType::Mihomo => crate::core::process::PortResolution {
                requested_port: preferred_port,
                actual_port: preferred_port,
                reassigned: false,
                config_content: raw_config.trim_start_matches('\u{feff}').to_string(),
            },
        };

        if port_res.reassigned {
            self.emit_log(
                Some(core),
                LogStream::System,
                LogLevel::Warn,
                format!(
                    "Port {} was in use; automatically switched inbound port to {}",
                    port_res.requested_port, port_res.actual_port
                ),
            );
        }

        // 2. Write runtime configuration file into work_dir (UTF-8 without BOM)
        std::fs::create_dir_all(&self.work_dir)?;
        let ext = if core == CoreType::Mihomo {
            "yaml"
        } else {
            "json"
        };
        let config_path = self
            .work_dir
            .join(format!("runtime_{}.{}", core.as_str(), ext));
        crate::storage::atomic_write_file(&config_path, port_res.config_content.as_bytes())?;

        // 3. Run pre-flight `check` command (`sing-box check`, `mihomo -t`, `xray run -test`)
        let binary_path = self.binary_path(core);
        if let Err(e) = check_core_config(core, &binary_path, &config_path, &self.work_dir).await {
            let reason = e.to_string();
            self.emit_log(Some(core), LogStream::System, LogLevel::Error, &reason);
            let _ = self.update_lifecycle(CoreLifecycleState::Crashed {
                exit_code: None,
                reason,
            });
            return Err(ManagerError::Process(e));
        }

        self.emit_log(
            Some(core),
            LogStream::System,
            LogLevel::Info,
            format!("Configuration check passed for {}", core),
        );

        // 4. Create Windows Job Object and spawn core child process
        let job = match JobObject::new() {
            Ok(j) => j,
            Err(e) => {
                let reason = e.to_string();
                self.emit_log(Some(core), LogStream::System, LogLevel::Error, &reason);
                let _ = self.update_lifecycle(CoreLifecycleState::Crashed {
                    exit_code: None,
                    reason,
                });
                return Err(ManagerError::Process(e));
            }
        };

        let mut cmd = build_core_run_command(core, &binary_path, &config_path, &self.work_dir);
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let reason = format!("Failed to spawn {}: {}", core, e);
                self.emit_log(Some(core), LogStream::System, LogLevel::Error, &reason);
                let _ = self.update_lifecycle(CoreLifecycleState::Crashed {
                    exit_code: None,
                    reason,
                });
                return Err(ManagerError::Io(e));
            }
        };

        let pid = child.id().unwrap_or(0);
        if pid > 0 {
            if let Err(e) = job.assign_pid(pid) {
                let _ = child.kill().await;
                let reason = e.to_string();
                self.emit_log(Some(core), LogStream::System, LogLevel::Error, &reason);
                let _ = self.update_lifecycle(CoreLifecycleState::Crashed {
                    exit_code: None,
                    reason,
                });
                return Err(ManagerError::Process(e));
            }
        }

        // 5. Attach stdout and stderr readers to ring buffer + event sink
        if let Some(stdout) = child.stdout.take() {
            let mgr = self.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let entry = mgr.logs.push(Some(core), LogStream::Stdout, line);
                    let sink = mgr.event_sink.lock().clone();
                    sink.on_log(&entry);
                }
            });
        }

        if let Some(stderr) = child.stderr.take() {
            let mgr = self.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let entry = mgr.logs.push(Some(core), LogStream::Stderr, line);
                    let sink = mgr.event_sink.lock().clone();
                    sink.on_log(&entry);
                }
            });
        }

        let instance_id = self.instance_seq.fetch_add(1, Ordering::SeqCst);

        {
            let mut holder = self.child_holder.lock().await;
            *holder = Some(RunningChild {
                instance_id,
                child,
                _job: job,
            });
        }

        // 6. Transition to Running
        let running_status = {
            let mut guard = self.state.lock();
            guard.active_instance_id = instance_id;
            guard
                .status
                .lifecycle
                .transition_to(CoreLifecycleState::Running)?;
            guard.status.pid = Some(pid);
            guard.status.started_at = Some(Utc::now().timestamp_millis());
            guard.status.actual_mixed_port = Some(port_res.actual_port);
            guard.status.port_reassigned = port_res.reassigned;
            guard.status.config_path = Some(config_path.display().to_string());
            guard.status.clone()
        };
        self.emit_status(&running_status);

        self.emit_log(
            Some(core),
            LogStream::System,
            LogLevel::Info,
            format!(
                "Core {} is running (PID: {}, mixed port: {})",
                core, pid, port_res.actual_port
            ),
        );

        // 7. Spawn background process exit monitor for unexpected crashes
        let mgr = self.clone();
        tokio::spawn(async move {
            mgr.monitor_child_exit(instance_id, core).await;
        });

        Ok(running_status)
    }

    async fn monitor_child_exit(&self, watched_instance_id: u64, core: CoreType) {
        loop {
            tokio::time::sleep(Duration::from_millis(150)).await;

            let mut holder = self.child_holder.lock().await;
            let Some(running) = holder.as_mut() else {
                break;
            };
            if running.instance_id != watched_instance_id {
                break;
            }

            match running.child.try_wait() {
                Ok(Some(exit_status)) => {
                    let code = exit_status.code();
                    *holder = None;
                    drop(holder);

                    let mut guard = self.state.lock();
                    if guard.active_instance_id != watched_instance_id {
                        break;
                    }

                    let (should_retry, retry_req, backoff_ms, attempt) = if guard
                        .stopping_intentionally
                        || matches!(
                            guard.status.lifecycle,
                            CoreLifecycleState::Stopping | CoreLifecycleState::Stopped
                        ) {
                        let _ = guard
                            .status
                            .lifecycle
                            .transition_to(CoreLifecycleState::Stopped);
                        guard.status.pid = None;
                        let snap = guard.status.clone();
                        drop(guard);
                        self.emit_status(&snap);
                        (false, None, 0, 0)
                    } else {
                        let now = Utc::now().timestamp_millis();
                        if let Some(last) = guard.last_crash_at {
                            if now - last > 60_000 {
                                guard.crash_count = 0;
                            }
                        }
                        guard.crash_count += 1;
                        guard.last_crash_at = Some(now);

                        let count = guard.crash_count;
                        let auto_heal = guard.auto_heal;
                        let req = guard.last_start_request.clone();

                        let reason =
                            format!("Core {} exited unexpectedly (exit code: {:?})", core, code);
                        let _ = guard
                            .status
                            .lifecycle
                            .transition_to(CoreLifecycleState::Crashed {
                                exit_code: code,
                                reason: reason.clone(),
                            });
                        guard.status.pid = None;
                        let snap = guard.status.clone();
                        drop(guard);
                        self.emit_log(Some(core), LogStream::System, LogLevel::Error, reason);
                        self.emit_status(&snap);

                        if auto_heal && count <= 3 && req.is_some() {
                            let backoff = match count {
                                1 => 1000,
                                2 => 2000,
                                _ => 4000,
                            };
                            (true, req, backoff, count)
                        } else {
                            if count > 3 {
                                self.emit_log(
                                    Some(core),
                                    LogStream::System,
                                    LogLevel::Error,
                                    format!(
                                        "[Supervisor] Core {} crashed {} times in 60s. Auto-recovery suspended.",
                                        core, count
                                    ),
                                );
                            }
                            (false, None, 0, count)
                        }
                    };

                    if should_retry {
                        if let Some(req) = retry_req {
                            self.emit_log(
                                Some(core),
                                LogStream::System,
                                LogLevel::Warn,
                                format!(
                                    "[Supervisor] Auto-recovering core {} in {}ms (attempt {}/3)...",
                                    core, backoff_ms, attempt
                                ),
                            );
                            let mgr = self.clone();
                            tokio::spawn(async move {
                                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                                let stopping = mgr.state.lock().stopping_intentionally;
                                if !stopping {
                                    let start_fut: std::pin::Pin<
                                        Box<
                                            dyn std::future::Future<
                                                    Output = Result<CoreStatusInfo, ManagerError>,
                                                > + Send,
                                        >,
                                    > = Box::pin(mgr.start(req));
                                    match start_fut.await {
                                        Ok(_) => {
                                            mgr.emit_log(
                                                Some(core),
                                                LogStream::System,
                                                LogLevel::Info,
                                                format!(
                                                    "[Supervisor] Core {} successfully auto-recovered and running.",
                                                    core
                                                ),
                                            );
                                        }
                                        Err(e) => {
                                            mgr.emit_log(
                                                Some(core),
                                                LogStream::System,
                                                LogLevel::Error,
                                                format!(
                                                    "[Supervisor] Auto-recovery attempt {} failed: {}",
                                                    attempt, e
                                                ),
                                            );
                                        }
                                    }
                                }
                            });
                        }
                    }
                    break;
                }
                Ok(None) => {
                    // Still running
                }
                Err(e) => {
                    *holder = None;
                    drop(holder);
                    let reason = format!("Error polling core {} process: {}", core, e);
                    let _ = self.update_lifecycle(CoreLifecycleState::Crashed {
                        exit_code: None,
                        reason: reason.clone(),
                    });
                    self.emit_log(Some(core), LogStream::System, LogLevel::Error, reason);
                    break;
                }
            }
        }
    }

    /// Stops the currently running core process cleanly
    pub async fn stop(&self) -> Result<CoreStatusInfo, ManagerError> {
        let current_core = {
            let mut guard = self.state.lock();
            match guard.status.lifecycle {
                CoreLifecycleState::Stopped => return Ok(guard.status.clone()),
                CoreLifecycleState::Crashed { .. } => {
                    guard
                        .status
                        .lifecycle
                        .transition_to(CoreLifecycleState::Stopped)?;
                    guard.status.pid = None;
                    let snap = guard.status.clone();
                    drop(guard);
                    self.emit_status(&snap);
                    return Ok(snap);
                }
                _ => {
                    guard.stopping_intentionally = true;
                    guard.crash_count = 0;
                    guard.last_crash_at = None;
                    guard
                        .status
                        .lifecycle
                        .transition_to(CoreLifecycleState::Stopping)?;
                    let snap = guard.status.clone();
                    let active_core = guard.status.active_core;
                    drop(guard);
                    self.emit_status(&snap);
                    active_core
                }
            }
        };

        if let Some(mut running) = self.child_holder.lock().await.take() {
            let _ = running.child.kill().await;
            let _ = tokio::time::timeout(Duration::from_secs(3), running.child.wait()).await;
            running._job.terminate();
        }

        let stopped_status = {
            let mut guard = self.state.lock();
            let _ = guard
                .status
                .lifecycle
                .transition_to(CoreLifecycleState::Stopped);
            guard.status.pid = None;
            guard.status.started_at = None;
            guard.stopping_intentionally = false;
            guard.status.clone()
        };

        self.emit_log(
            current_core,
            LogStream::System,
            LogLevel::Info,
            "Core stopped.",
        );
        self.emit_status(&stopped_status);

        Ok(stopped_status)
    }

    /// Restarts the core with the given request
    pub async fn restart(&self, req: CoreStartRequest) -> Result<CoreStatusInfo, ManagerError> {
        self.stop().await?;
        self.start(req).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::process::find_available_port;

    #[test]
    fn test_lifecycle_state_machine_transitions() {
        let mut state = CoreLifecycleState::Stopped;

        // Stopped -> Running directly is invalid (must go through Starting)
        assert!(!state.can_transition_to(&CoreLifecycleState::Running));
        assert!(state.transition_to(CoreLifecycleState::Running).is_err());

        // Valid happy path: Stopped -> Starting -> Running -> Stopping -> Stopped
        state.transition_to(CoreLifecycleState::Starting).unwrap();
        state.transition_to(CoreLifecycleState::Running).unwrap();
        state.transition_to(CoreLifecycleState::Stopping).unwrap();
        state.transition_to(CoreLifecycleState::Stopped).unwrap();

        // Crash path: Stopped -> Starting -> Running -> Crashed -> Starting -> Running
        state.transition_to(CoreLifecycleState::Starting).unwrap();
        state.transition_to(CoreLifecycleState::Running).unwrap();
        state
            .transition_to(CoreLifecycleState::Crashed {
                exit_code: Some(1),
                reason: "unexpected exit".to_string(),
            })
            .unwrap();
        state.transition_to(CoreLifecycleState::Starting).unwrap();
        state.transition_to(CoreLifecycleState::Running).unwrap();
    }

    #[tokio::test]
    async fn test_core_manager_start_stop_and_realtime_logs() {
        let cores_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("cores");
        if !cores_dir.join(CoreType::SingBox.binary_name()).exists() {
            return;
        }

        let work_dir = std::env::temp_dir().join("novaproxy_test_mgr_work");
        let mgr = CoreManager::new(cores_dir, work_dir);

        let port = find_available_port(29890, false).unwrap();
        let status = mgr
            .start(CoreStartRequest {
                core_type: Some(CoreType::SingBox),
                config_json: Some(default_minimal_singbox_config(port)),
                mixed_port: Some(port),
                allow_lan: Some(false),
                auto_port_fallback: Some(true),
            })
            .await
            .expect("start sing-box minimal config");

        assert_eq!(status.lifecycle, CoreLifecycleState::Running);
        assert_eq!(status.active_core, Some(CoreType::SingBox));
        assert_eq!(status.actual_mixed_port, Some(port));
        assert!(status.pid.unwrap_or(0) > 0);

        // Give sing-box a moment to flush initial startup logs
        tokio::time::sleep(Duration::from_millis(300)).await;

        let logs = mgr.logs(None, None);
        assert!(!logs.is_empty());

        let stopped = mgr.stop().await.expect("stop sing-box");
        assert_eq!(stopped.lifecycle, CoreLifecycleState::Stopped);
        assert!(stopped.pid.is_none());
    }
}

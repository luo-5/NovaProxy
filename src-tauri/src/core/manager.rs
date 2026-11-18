use crate::core::capabilities::{CapabilityError, CoreCapabilitiesRegistry};
use crate::core::process::{
    build_core_run_command, check_core_config, default_minimal_singbox_config,
    extract_clash_api_port, find_available_port, patch_relay_outbound_port,
    resolve_and_patch_core_ports, JobObject, ProcessError,
};
use crate::generator::{generate_core_config, generate_multi_core_relay, GeneratorError};
use crate::model::{
    AppSettings, CoreType, DnsSettings, LogLevel, NodeGroup, ProxyNode, RoutingSettings,
};
use crate::monitor::{
    CoreLogEntry, CoreStartSample, LogRingBuffer, LogStream, PersistentLogWriter, TelemetryTracker,
};
use chrono::Utc;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
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
    #[error("Capability error: {0}")]
    Capability(#[from] CapabilityError),
    #[error("Config generation error: {0}")]
    Generator(#[from] GeneratorError),
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
    #[serde(default)]
    pub upstream_core: Option<CoreType>,
    #[serde(default)]
    pub upstream_pid: Option<u32>,
    #[serde(default)]
    pub relay_port: Option<u16>,
    #[serde(default)]
    pub actual_clash_api_port: Option<u16>,
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
    #[serde(default)]
    pub upstream_core_type: Option<CoreType>,
    #[serde(default)]
    pub upstream_config_json: Option<String>,
    #[serde(default)]
    pub relay_port: Option<u16>,
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
    upstream_child: Option<tokio::process::Child>,
    upstream_core: Option<CoreType>,
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
    config_generation: u64,
    started_generation: Option<u64>,
}

/// Central manager for proxy core processes, logs, telemetry, and capabilities
#[derive(Clone)]
pub struct CoreManager {
    cores_dir: Arc<RwLock<PathBuf>>,
    override_cores_dir: Arc<RwLock<Option<PathBuf>>>,
    work_dir: PathBuf,
    state: Arc<Mutex<StateInner>>,
    child_holder: Arc<AsyncMutex<Option<RunningChild>>>,
    instance_seq: Arc<AtomicU64>,
    logs: LogRingBuffer,
    telemetry: TelemetryTracker,
    capabilities: CoreCapabilitiesRegistry,
    event_sink: Arc<Mutex<Arc<dyn CoreEventSink>>>,
}

impl CoreManager {
    pub fn new(cores_dir: impl Into<PathBuf>, work_dir: impl Into<PathBuf>) -> Self {
        let cores_dir = cores_dir.into();
        let work_dir = work_dir.into();
        let _ = std::fs::create_dir_all(&work_dir);

        Self {
            cores_dir: Arc::new(RwLock::new(cores_dir)),
            override_cores_dir: Arc::new(RwLock::new(None)),
            work_dir,
            state: Arc::new(Mutex::new(StateInner {
                status: CoreStatusInfo::default(),
                stopping_intentionally: false,
                active_instance_id: 0,
                last_start_request: None,
                crash_count: 0,
                last_crash_at: None,
                auto_heal: true,
                config_generation: 1,
                started_generation: None,
            })),
            child_holder: Arc::new(AsyncMutex::new(None)),
            instance_seq: Arc::new(AtomicU64::new(1)),
            logs: LogRingBuffer::default(),
            telemetry: TelemetryTracker::new(),
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

    /// Bumps the configuration generation counter after DB edits (nodes, routing, DNS, settings)
    /// so active switching avoids using stale in-memory Clash API outbounds.
    pub fn mark_config_dirty(&self) {
        let mut guard = self.state.lock();
        guard.config_generation = guard.config_generation.wrapping_add(1);
    }

    pub fn is_config_dirty(&self) -> bool {
        let guard = self.state.lock();
        match guard.started_generation {
            Some(started) => started != guard.config_generation,
            None => true,
        }
    }

    pub fn set_event_sink(&self, sink: Arc<dyn CoreEventSink>) {
        *self.event_sink.lock() = sink.clone();
        self.logs.set_live_emitter(Arc::new(move |entry| {
            sink.on_log(entry);
        }));
    }

    pub fn attach_log_writer(&self, writer: PersistentLogWriter) {
        self.logs.attach_file_writer(writer);
    }

    pub fn logs_buffer(&self) -> &LogRingBuffer {
        &self.logs
    }

    pub fn telemetry(&self) -> &TelemetryTracker {
        &self.telemetry
    }

    pub fn set_cores_dir(&self, dir: impl Into<PathBuf>) {
        *self.cores_dir.write() = dir.into();
    }

    pub fn set_override_cores_dir(&self, dir: impl Into<PathBuf>) {
        *self.override_cores_dir.write() = Some(dir.into());
    }

    pub fn cores_dir(&self) -> PathBuf {
        self.cores_dir.read().clone()
    }

    pub fn effective_updater_cores_dir(&self) -> PathBuf {
        self.override_cores_dir
            .read()
            .clone()
            .unwrap_or_else(|| self.cores_dir())
    }

    pub fn work_dir(&self) -> &Path {
        &self.work_dir
    }

    pub fn capabilities(&self) -> &CoreCapabilitiesRegistry {
        &self.capabilities
    }

    pub fn binary_path(&self, core: CoreType) -> PathBuf {
        let name = core.binary_name();
        if let Some(ref override_dir) = *self.override_cores_dir.read() {
            let candidate = override_dir.join(name);
            if candidate.exists() {
                return candidate;
            }
        }
        self.cores_dir.read().join(name)
    }

    pub fn available_cores(&self) -> Vec<CoreType> {
        [CoreType::SingBox, CoreType::Mihomo, CoreType::Xray]
            .into_iter()
            .filter(|&c| self.binary_path(c).exists())
            .collect()
    }

    /// Plans an optimal single-core or two-tier cooperative relay `CoreStartRequest`
    /// for the target node, TUN setting, and installed core binaries.
    #[allow(clippy::too_many_arguments)]
    pub fn plan_start_request(
        &self,
        explicit_core: Option<CoreType>,
        mixed_port: Option<u16>,
        allow_lan: Option<bool>,
        auto_port_fallback: Option<bool>,
        nodes: &[ProxyNode],
        groups: &[NodeGroup],
        settings: &AppSettings,
        dns: &DnsSettings,
        routing: &RoutingSettings,
        active_node_id: Option<&str>,
    ) -> Result<CoreStartRequest, ManagerError> {
        let target_node_id = active_node_id.or(settings.active_node_id.as_deref());
        let active_node = target_node_id
            .and_then(|id| nodes.iter().find(|n| n.id == id))
            .or_else(|| nodes.first());

        let req_port = Some(mixed_port.unwrap_or(settings.mixed_port));
        let req_lan = Some(allow_lan.unwrap_or(settings.allow_lan));
        let req_fallback = Some(auto_port_fallback.unwrap_or(settings.auto_port_fallback));

        let available = self.available_cores();
        let is_avail = |core: CoreType| available.is_empty() || available.contains(&core);
        let requested_preferred = explicit_core.or(settings.preferred_core);

        // On Android, VpnService captures system TUN traffic via hev-socks5-tunnel into our local
        // mixed inbound, so child core configs never spawn a privileged kernel TUN inbound.
        #[cfg(target_os = "android")]
        let effective_settings = {
            let mut s = settings.clone();
            s.tun_mode = false;
            s
        };
        #[cfg(not(target_os = "android"))]
        let effective_settings = settings.clone();
        let settings = &effective_settings;

        let pick_tun_primary = || -> Option<CoreType> {
            if explicit_core == Some(CoreType::Mihomo) && is_avail(CoreType::Mihomo) {
                Some(CoreType::Mihomo)
            } else if is_avail(CoreType::SingBox) {
                Some(CoreType::SingBox)
            } else if is_avail(CoreType::Mihomo) {
                Some(CoreType::Mihomo)
            } else {
                None
            }
        };

        let Some(node) = active_node else {
            let mut chosen = requested_preferred
                .filter(|c| is_avail(*c))
                .or_else(|| {
                    [CoreType::SingBox, CoreType::Mihomo, CoreType::Xray]
                        .into_iter()
                        .find(|&c| is_avail(c))
                })
                .unwrap_or(CoreType::SingBox);

            if settings.tun_mode && chosen == CoreType::Xray {
                chosen = pick_tun_primary().ok_or_else(|| {
                    GeneratorError::Relay(
                        "TUN mode requires sing-box or mihomo binary, which is not installed"
                            .to_string(),
                    )
                })?;
            }

            let config_json = generate_core_config(
                chosen,
                nodes,
                groups,
                settings,
                dns,
                routing,
                target_node_id,
            )?;
            return Ok(CoreStartRequest {
                core_type: Some(chosen),
                config_json: Some(config_json),
                mixed_port: req_port,
                allow_lan: req_lan,
                auto_port_fallback: req_fallback,
                ..Default::default()
            });
        };

        let selection =
            self.capabilities
                .select_core_for_node(node, requested_preferred, &available)?;

        let relay_pair: Option<(CoreType, CoreType)> = match selection.core {
            CoreType::Xray => {
                let strict_xray = !settings.tun_mode
                    && (explicit_core == Some(CoreType::Xray)
                        || (explicit_core.is_none()
                            && (node.preferred_core == Some(CoreType::Xray)
                                || settings.preferred_core == Some(CoreType::Xray))));
                if strict_xray {
                    None
                } else if let Some(primary) = pick_tun_primary() {
                    Some((primary, CoreType::Xray))
                } else if settings.tun_mode {
                    return Err(GeneratorError::Relay(
                        "Selected node requires Xray and TUN mode requires sing-box or mihomo as primary router"
                            .to_string(),
                    )
                    .into());
                } else {
                    None
                }
            }
            CoreType::Mihomo => {
                if explicit_core == Some(CoreType::SingBox)
                    && is_avail(CoreType::SingBox)
                    && node.preferred_core != Some(CoreType::Mihomo)
                {
                    Some((CoreType::SingBox, CoreType::Mihomo))
                } else {
                    None
                }
            }
            CoreType::SingBox => None,
        };

        if let Some((primary_core, upstream_core)) = relay_pair {
            let relay_port = find_available_port(18889, false)?;
            let relay = generate_multi_core_relay(
                primary_core,
                upstream_core,
                node,
                nodes,
                groups,
                settings,
                dns,
                routing,
                relay_port,
            )?;
            return Ok(CoreStartRequest {
                core_type: Some(relay.primary_core),
                config_json: Some(relay.primary_config),
                mixed_port: req_port,
                allow_lan: req_lan,
                auto_port_fallback: req_fallback,
                upstream_core_type: Some(relay.upstream_core),
                upstream_config_json: Some(relay.upstream_config),
                relay_port: Some(relay.relay_port),
            });
        }

        let config_json = generate_core_config(
            selection.core,
            nodes,
            groups,
            settings,
            dns,
            routing,
            Some(&node.id),
        )?;
        Ok(CoreStartRequest {
            core_type: Some(selection.core),
            config_json: Some(config_json),
            mixed_port: req_port,
            allow_lan: req_lan,
            auto_port_fallback: req_fallback,
            ..Default::default()
        })
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
        let _ = self.logs.push_with_level(core, stream, level, msg);
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

    fn mark_crashed(&self, core: Option<CoreType>, exit_code: Option<i32>, reason: &str) {
        if let Some(c) = core {
            self.telemetry.record_crash(c, exit_code, reason);
        }
        self.emit_log(core, LogStream::System, LogLevel::Error, reason);
        let _ = self.update_lifecycle(CoreLifecycleState::Crashed {
            exit_code,
            reason: reason.to_string(),
        });
    }

    fn attach_pipe_readers(&self, core: CoreType, child: &mut tokio::process::Child) {
        if let Some(stdout) = child.stdout.take() {
            let mgr = self.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let _ = mgr.logs.push(Some(core), LogStream::Stdout, line);
                }
            });
        }

        if let Some(stderr) = child.stderr.take() {
            let mgr = self.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let _ = mgr.logs.push(Some(core), LogStream::Stderr, line);
                }
            });
        }
    }

    async fn start_inner(&self, req: CoreStartRequest) -> Result<CoreStatusInfo, ManagerError> {
        let start_total_timer = Instant::now();
        let target_generation = self.state.lock().config_generation;

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
            guard.status.upstream_core = req.upstream_core_type;
            guard.status.upstream_pid = None;
            guard.status.relay_port = req.relay_port;
            guard.status.actual_clash_api_port = None;
            guard.started_generation = None;
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

        let mut raw_config = req
            .config_json
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| default_minimal_singbox_config(preferred_port));

        std::fs::create_dir_all(&self.work_dir)?;

        // Optional Stage 0: Prepare, check, and resolve ports for upstream cooperative relay core
        let mut prepared_upstream: Option<(CoreType, PathBuf, PathBuf, u16)> = None;
        if let (Some(up_core), Some(up_raw)) = (
            req.upstream_core_type,
            req.upstream_config_json
                .as_ref()
                .filter(|s| !s.trim().is_empty()),
        ) {
            let req_relay = req.relay_port.unwrap_or(18889);
            let up_port_res = match resolve_and_patch_core_ports(
                up_core,
                up_raw,
                req_relay,
                false,
                auto_fallback,
            ) {
                Ok(r) => r,
                Err(e) => {
                    self.mark_crashed(Some(up_core), None, &e.to_string());
                    return Err(ManagerError::Process(e));
                }
            };
            if up_port_res.actual_port != req_relay {
                self.emit_log(
                    Some(up_core),
                    LogStream::System,
                    LogLevel::Warn,
                    format!(
                        "Upstream relay port {} was in use; reassigned to {}",
                        req_relay, up_port_res.actual_port
                    ),
                );
                raw_config = patch_relay_outbound_port(
                    core,
                    &raw_config,
                    req_relay,
                    up_port_res.actual_port,
                );
            }
            let up_ext = if up_core == CoreType::Mihomo {
                "yaml"
            } else {
                "json"
            };
            let up_cfg_path =
                self.work_dir
                    .join(format!("runtime_upstream_{}.{}", up_core.as_str(), up_ext));
            crate::storage::atomic_write_file(&up_cfg_path, up_port_res.config_content.as_bytes())?;
            let up_bin = self.binary_path(up_core);
            if let Err(e) = check_core_config(up_core, &up_bin, &up_cfg_path, &self.work_dir).await
            {
                self.mark_crashed(Some(up_core), None, &e.to_string());
                return Err(ManagerError::Process(e));
            }
            self.emit_log(
                Some(up_core),
                LogStream::System,
                LogLevel::Info,
                format!(
                    "Upstream cooperative relay check passed for {} (port {})",
                    up_core, up_port_res.actual_port
                ),
            );
            prepared_upstream = Some((up_core, up_bin, up_cfg_path, up_port_res.actual_port));
        }

        // 1. Resolve port conflicts for primary engine (both JSON and Mihomo YAML)
        let port_timer = Instant::now();
        let port_res = match resolve_and_patch_core_ports(
            core,
            &raw_config,
            preferred_port,
            allow_lan,
            auto_fallback,
        ) {
            Ok(res) => res,
            Err(e) => {
                self.mark_crashed(Some(core), None, &e.to_string());
                self.telemetry.record_core_start(CoreStartSample {
                    timestamp: Utc::now().timestamp_millis(),
                    core,
                    upstream_core: req.upstream_core_type,
                    config_gen_ms: 0,
                    port_resolve_ms: port_timer.elapsed().as_millis() as u64,
                    spawn_ms: 0,
                    total_ms: start_total_timer.elapsed().as_millis() as u64,
                    port_reassigned: false,
                    success: false,
                    error: Some(e.to_string()),
                });
                return Err(ManagerError::Process(e));
            }
        };
        let port_resolve_ms = port_timer.elapsed().as_millis() as u64;
        let actual_clash_api_port = extract_clash_api_port(core, &port_res.config_content);

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
        let spawn_timer = Instant::now();
        let binary_path = self.binary_path(core);
        if let Err(e) = check_core_config(core, &binary_path, &config_path, &self.work_dir).await {
            self.mark_crashed(Some(core), None, &e.to_string());
            self.telemetry.record_core_start(CoreStartSample {
                timestamp: Utc::now().timestamp_millis(),
                core,
                upstream_core: req.upstream_core_type,
                config_gen_ms: 0,
                port_resolve_ms,
                spawn_ms: spawn_timer.elapsed().as_millis() as u64,
                total_ms: start_total_timer.elapsed().as_millis() as u64,
                port_reassigned: port_res.reassigned,
                success: false,
                error: Some(e.to_string()),
            });
            return Err(ManagerError::Process(e));
        }

        self.emit_log(
            Some(core),
            LogStream::System,
            LogLevel::Info,
            format!("Configuration check passed for {}", core),
        );

        // 4. Create shared Windows Job Object and spawn upstream + primary processes
        let job = match JobObject::new() {
            Ok(j) => j,
            Err(e) => {
                self.mark_crashed(Some(core), None, &e.to_string());
                return Err(ManagerError::Process(e));
            }
        };

        let mut upstream_child: Option<tokio::process::Child> = None;
        let mut upstream_pid: Option<u32> = None;
        let mut upstream_core_out: Option<CoreType> = None;
        let mut actual_relay_port: Option<u16> = None;

        if let Some((up_core, up_bin, up_cfg_path, relay_p)) = prepared_upstream {
            let mut up_cmd = build_core_run_command(up_core, &up_bin, &up_cfg_path, &self.work_dir);
            let mut up_proc = match up_cmd.spawn() {
                Ok(c) => c,
                Err(e) => {
                    let reason = format!("Failed to spawn upstream {}: {}", up_core, e);
                    self.mark_crashed(Some(up_core), None, &reason);
                    return Err(ManagerError::Io(e));
                }
            };
            let up_pid = up_proc.id().unwrap_or(0);
            if up_pid > 0 {
                if let Err(e) = job.assign_pid(up_pid) {
                    let _ = up_proc.kill().await;
                    job.terminate();
                    self.mark_crashed(Some(up_core), None, &e.to_string());
                    return Err(ManagerError::Process(e));
                }
                upstream_pid = Some(up_pid);
            }
            self.attach_pipe_readers(up_core, &mut up_proc);
            // Allow upstream listener socket brief time to bind before primary dials it
            tokio::time::sleep(Duration::from_millis(60)).await;
            if let Ok(Some(early_exit)) = up_proc.try_wait() {
                let reason = format!(
                    "Upstream {} exited prematurely with code {:?}",
                    up_core,
                    early_exit.code()
                );
                self.mark_crashed(Some(up_core), early_exit.code(), &reason);
                return Err(ManagerError::Process(ProcessError::SpawnFailed(
                    std::io::Error::other(reason),
                )));
            }
            upstream_child = Some(up_proc);
            upstream_core_out = Some(up_core);
            actual_relay_port = Some(relay_p);
            self.emit_log(
                Some(up_core),
                LogStream::System,
                LogLevel::Info,
                format!(
                    "Upstream core {} running (PID: {:?}, relay port: {})",
                    up_core, upstream_pid, relay_p
                ),
            );
        }

        let mut cmd = build_core_run_command(core, &binary_path, &config_path, &self.work_dir);
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                if let Some(mut up) = upstream_child {
                    let _ = up.kill().await;
                }
                job.terminate();
                let reason = format!("Failed to spawn {}: {}", core, e);
                self.mark_crashed(Some(core), None, &reason);
                return Err(ManagerError::Io(e));
            }
        };

        let pid = child.id().unwrap_or(0);
        if pid > 0 {
            if let Err(e) = job.assign_pid(pid) {
                let _ = child.kill().await;
                if let Some(mut up) = upstream_child {
                    let _ = up.kill().await;
                }
                job.terminate();
                self.mark_crashed(Some(core), None, &e.to_string());
                return Err(ManagerError::Process(e));
            }
        }

        // 5. Attach primary stdout and stderr readers to ring buffer + event sink
        self.attach_pipe_readers(core, &mut child);

        let instance_id = self.instance_seq.fetch_add(1, Ordering::SeqCst);

        {
            let mut holder = self.child_holder.lock().await;
            *holder = Some(RunningChild {
                instance_id,
                child,
                upstream_child,
                upstream_core: upstream_core_out,
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
            guard.status.upstream_core = upstream_core_out;
            guard.status.upstream_pid = upstream_pid;
            guard.status.relay_port = actual_relay_port;
            guard.status.actual_clash_api_port = actual_clash_api_port;
            guard.started_generation = Some(target_generation);
            guard.status.clone()
        };
        self.emit_status(&running_status);

        let total_ms = start_total_timer.elapsed().as_millis() as u64;
        let spawn_ms = spawn_timer.elapsed().as_millis() as u64;
        self.telemetry.record_core_start(CoreStartSample {
            timestamp: Utc::now().timestamp_millis(),
            core,
            upstream_core: upstream_core_out,
            config_gen_ms: 0,
            port_resolve_ms,
            spawn_ms,
            total_ms,
            port_reassigned: port_res.reassigned,
            success: true,
            error: None,
        });

        let _ = self.logs.push_detailed(
            Some(core),
            LogStream::System,
            LogLevel::Info,
            Some("core::manager".to_string()),
            Some(total_ms),
            format!(
                "Core {} is running (PID: {}, mixed port: {}, port_resolve: {}ms, check+spawn: {}ms)",
                core, pid, port_res.actual_port, port_resolve_ms, spawn_ms
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

            let primary_poll = running.child.try_wait();
            let upstream_poll = match running.upstream_child.as_mut() {
                Some(up) => match up.try_wait() {
                    Ok(opt) => Ok((running.upstream_core, opt)),
                    Err(e) => Err(e),
                },
                None => Ok((None, None)),
            };

            let crashed_info = match (primary_poll, upstream_poll) {
                (Ok(Some(exit_status)), _) => Ok(Some((core, exit_status))),
                (_, Ok((Some(up_core), Some(exit_status)))) => Ok(Some((up_core, exit_status))),
                (Err(e), _) | (_, Err(e)) => Err(e),
                _ => Ok(None),
            };

            match crashed_info {
                Ok(Some((exited_core, exit_status))) => {
                    let code = exit_status.code();
                    if let Some(finished) = holder.take() {
                        finished._job.terminate();
                    }
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
                        guard.status.upstream_pid = None;
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

                        let reason = format!(
                            "Core {} exited unexpectedly (exit code: {:?})",
                            exited_core, code
                        );
                        self.telemetry.record_crash(exited_core, code, &reason);
                        let _ = guard
                            .status
                            .lifecycle
                            .transition_to(CoreLifecycleState::Crashed {
                                exit_code: code,
                                reason: reason.clone(),
                            });
                        guard.status.pid = None;
                        guard.status.upstream_pid = None;
                        let snap = guard.status.clone();
                        drop(guard);
                        self.emit_log(
                            Some(exited_core),
                            LogStream::System,
                            LogLevel::Error,
                            reason,
                        );
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
                    if let Some(finished) = holder.take() {
                        finished._job.terminate();
                    }
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
            if let Some(mut up) = running.upstream_child.take() {
                let _ = up.kill().await;
                let _ = tokio::time::timeout(Duration::from_secs(3), up.wait()).await;
            }
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
            guard.status.upstream_core = None;
            guard.status.upstream_pid = None;
            guard.status.relay_port = None;
            guard.status.actual_clash_api_port = None;
            guard.started_generation = None;
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
    use crate::model::{
        DnsSettings, ProtocolConfig, RoutingMode, RoutingSettings, ShadowsocksRConfig,
        TransportConfig, VlessConfig,
    };

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

    #[test]
    fn test_plan_start_request_auto_and_multi_core_relay() {
        let cores_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("cores");
        let work_dir = std::env::temp_dir().join("novaproxy_test_plan_work");
        let mgr = CoreManager::new(cores_dir, work_dir);

        let xhttp_node = ProxyNode {
            id: "node-xhttp".to_string(),
            name: "VLESS XHTTP".to_string(),
            server: "example.invalid".to_string(),
            port: 443,
            protocol: crate::model::ProtocolType::Vless,
            tls: Some(crate::model::TlsConfig {
                enabled: true,
                server_name: Some("example.invalid".to_string()),
                ..Default::default()
            }),
            transport: Some(TransportConfig::Xhttp {
                host: Some("cdn.example.invalid".to_string()),
                path: "/xhttp".to_string(),
                mode: Some("auto".to_string()),
                extra: None,
            }),
            settings: ProtocolConfig::Vless(VlessConfig {
                uuid: "11111111-1111-1111-1111-111111111111".to_string(),
                flow: None,
                encryption: "none".to_string(),
                packet_encoding: None,
            }),
            subscription_id: None,
            group_id: None,
            latency: None,
            dialer_proxy: None,
            preferred_core: None,
            sort_order: 0,
        };

        let ssr_node = ProxyNode {
            id: "node-ssr".to_string(),
            name: "SSR Node".to_string(),
            server: "ssr.example.invalid".to_string(),
            port: 8388,
            protocol: crate::model::ProtocolType::ShadowsocksR,
            tls: None,
            transport: None,
            settings: ProtocolConfig::ShadowsocksR(ShadowsocksRConfig {
                method: "aes-256-cfb".to_string(),
                password: "dummy-password".to_string(),
                protocol: "origin".to_string(),
                protocol_param: None,
                obfs: "plain".to_string(),
                obfs_param: None,
            }),
            subscription_id: None,
            group_id: None,
            latency: None,
            dialer_proxy: None,
            preferred_core: None,
            sort_order: 1,
        };

        let settings = AppSettings::default();
        let dns = DnsSettings::default();
        let routing = RoutingSettings {
            mode: RoutingMode::Rule,
            ..Default::default()
        };

        // 1. XHTTP in auto mode relays via sing-box primary + xray upstream
        let req_xhttp = mgr
            .plan_start_request(
                None,
                None,
                None,
                None,
                &[xhttp_node.clone(), ssr_node.clone()],
                &[],
                &settings,
                &dns,
                &routing,
                Some("node-xhttp"),
            )
            .expect("plan xhttp relay");
        assert_eq!(req_xhttp.core_type, Some(CoreType::SingBox));
        assert_eq!(req_xhttp.upstream_core_type, Some(CoreType::Xray));
        assert!(req_xhttp.upstream_config_json.is_some());
        assert!(req_xhttp.relay_port.is_some());

        // 2. XHTTP with explicit Xray and TUN=false runs solo Xray
        let req_xray_solo = mgr
            .plan_start_request(
                Some(CoreType::Xray),
                None,
                None,
                None,
                std::slice::from_ref(&xhttp_node),
                &[],
                &settings,
                &dns,
                &routing,
                Some("node-xhttp"),
            )
            .expect("plan solo xray");
        assert_eq!(req_xray_solo.core_type, Some(CoreType::Xray));
        assert_eq!(req_xray_solo.upstream_core_type, None);

        // 3. XHTTP with explicit Xray + TUN=true upgrades to sing-box primary + xray upstream
        let mut tun_settings = settings.clone();
        tun_settings.tun_mode = true;
        let req_xray_tun = mgr
            .plan_start_request(
                Some(CoreType::Xray),
                None,
                None,
                None,
                &[xhttp_node],
                &[],
                &tun_settings,
                &dns,
                &routing,
                Some("node-xhttp"),
            )
            .expect("plan xhttp under TUN");
        assert_eq!(req_xray_tun.core_type, Some(CoreType::SingBox));
        assert_eq!(req_xray_tun.upstream_core_type, Some(CoreType::Xray));

        // 4. SSR in auto mode switches directly to Mihomo
        let req_ssr = mgr
            .plan_start_request(
                None,
                None,
                None,
                None,
                std::slice::from_ref(&ssr_node),
                &[],
                &settings,
                &dns,
                &routing,
                Some("node-ssr"),
            )
            .expect("plan ssr mihomo");
        assert_eq!(req_ssr.core_type, Some(CoreType::Mihomo));
        assert_eq!(req_ssr.upstream_core_type, None);

        // 5. SSR with explicit SingBox primary relays via SingBox + Mihomo
        let req_ssr_relay = mgr
            .plan_start_request(
                Some(CoreType::SingBox),
                None,
                None,
                None,
                &[ssr_node],
                &[],
                &settings,
                &dns,
                &routing,
                Some("node-ssr"),
            )
            .expect("plan ssr relay via sing-box");
        assert_eq!(req_ssr_relay.core_type, Some(CoreType::SingBox));
        assert_eq!(req_ssr_relay.upstream_core_type, Some(CoreType::Mihomo));
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
                ..Default::default()
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

    #[tokio::test]
    async fn test_core_manager_multi_core_relay_start_and_stop() {
        let cores_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("cores");
        if !cores_dir.join(CoreType::SingBox.binary_name()).exists()
            || !cores_dir.join(CoreType::Xray.binary_name()).exists()
        {
            return;
        }

        let work_dir = std::env::temp_dir().join("novaproxy_test_mgr_relay_work");
        let mgr = CoreManager::new(cores_dir, work_dir);

        let mixed_port = find_available_port(29910, false).unwrap();
        let api_port = find_available_port(29960, false).unwrap();
        let mut settings = AppSettings {
            mixed_port,
            clash_api_port: api_port,
            ..Default::default()
        };
        settings.tun_mode = false;
        let dns = DnsSettings {
            fake_ip: false,
            ..Default::default()
        };
        let routing = RoutingSettings::default();

        let xhttp_node = ProxyNode {
            id: "relay-live-xhttp".to_string(),
            name: "Live XHTTP Relay".to_string(),
            server: "example.invalid".to_string(),
            port: 443,
            protocol: crate::model::ProtocolType::Vless,
            tls: Some(crate::model::TlsConfig {
                enabled: true,
                server_name: Some("example.invalid".to_string()),
                ..Default::default()
            }),
            transport: Some(TransportConfig::Xhttp {
                host: Some("cdn.example.invalid".to_string()),
                path: "/xhttp".to_string(),
                mode: Some("auto".to_string()),
                extra: None,
            }),
            settings: ProtocolConfig::Vless(VlessConfig {
                uuid: "11111111-1111-1111-1111-111111111111".to_string(),
                flow: None,
                encryption: "none".to_string(),
                packet_encoding: None,
            }),
            subscription_id: None,
            group_id: None,
            latency: None,
            dialer_proxy: None,
            preferred_core: None,
            sort_order: 0,
        };

        let req = mgr
            .plan_start_request(
                None,
                Some(mixed_port),
                Some(false),
                Some(true),
                &[xhttp_node],
                &[],
                &settings,
                &dns,
                &routing,
                Some("relay-live-xhttp"),
            )
            .expect("plan relay start");

        let status = mgr.start(req).await.expect("start dual-core relay");
        assert_eq!(status.lifecycle, CoreLifecycleState::Running);
        assert_eq!(status.active_core, Some(CoreType::SingBox));
        assert_eq!(status.upstream_core, Some(CoreType::Xray));
        assert!(status.pid.unwrap_or(0) > 0);
        assert!(status.upstream_pid.unwrap_or(0) > 0);

        tokio::time::sleep(Duration::from_millis(350)).await;
        let still_running = mgr.status();
        assert_eq!(
            still_running.lifecycle,
            CoreLifecycleState::Running,
            "both sing-box and xray must stay alive past startup: logs={:?}",
            mgr.logs(None, None)
        );

        let stopped = mgr.stop().await.expect("stop dual-core relay");
        assert_eq!(stopped.lifecycle, CoreLifecycleState::Stopped);
        assert!(stopped.pid.is_none());
        assert!(stopped.upstream_pid.is_none());
    }
}

use crate::model::CoreType;
use serde_json::Value;
use std::net::TcpListener;
use std::path::Path;
use std::time::Duration;
use thiserror::Error;
use tokio::process::Command;

#[cfg(target_os = "windows")]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("Core binary not found at '{0}'")]
    BinaryNotFound(String),
    #[error("Failed to spawn core process: {0}")]
    SpawnFailed(#[from] std::io::Error),
    #[error("Configuration check failed for {core}: {details}")]
    ConfigCheckFailed { core: CoreType, details: String },
    #[error("Configuration check timed out for {0}")]
    ConfigCheckTimeout(CoreType),
    #[error("Port {0} is already in use and automatic port fallback is disabled")]
    PortInUse(u16),
    #[error("No available TCP port found starting from {0}")]
    NoAvailablePort(u16),
    #[error("Windows Job Object error: {0}")]
    JobObjectError(String),
    #[error("Invalid JSON configuration: {0}")]
    InvalidConfigJson(String),
}

/// Result of checking and optionally reassigning an inbound port
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortResolution {
    pub requested_port: u16,
    pub actual_port: u16,
    pub reassigned: bool,
    pub config_content: String,
}

/// Checks if a local TCP port is available for binding
pub fn is_port_available(port: u16, allow_lan: bool) -> bool {
    if port == 0 {
        return false;
    }
    let loopback_ok = TcpListener::bind(("127.0.0.1", port)).is_ok();
    if !loopback_ok {
        return false;
    }
    if allow_lan {
        TcpListener::bind(("0.0.0.0", port)).is_ok()
    } else {
        true
    }
}

/// Finds the first available TCP port starting at `start_port`
pub fn find_available_port(start_port: u16, allow_lan: bool) -> Result<u16, ProcessError> {
    let base = start_port.max(1024);
    for port in base..=65000 {
        if is_port_available(port, allow_lan) {
            return Ok(port);
        }
    }
    Err(ProcessError::NoAvailablePort(start_port))
}

/// Checks and resolves port conflicts across JSON (sing-box / Xray) and YAML (Mihomo) configs.
pub fn resolve_and_patch_core_ports(
    core: CoreType,
    config_raw: &str,
    preferred_port: u16,
    allow_lan: bool,
    auto_fallback: bool,
) -> Result<PortResolution, ProcessError> {
    match core {
        CoreType::SingBox | CoreType::Xray => {
            resolve_and_patch_config_port(config_raw, preferred_port, allow_lan, auto_fallback)
        }
        CoreType::Mihomo => resolve_and_patch_mihomo_yaml_ports(
            config_raw,
            preferred_port,
            allow_lan,
            auto_fallback,
        ),
    }
}

/// Inspects a JSON configuration (sing-box or Xray) for the primary inbound port
/// (plus secondary http-in / Clash API controller ports), verifies port availability,
/// and rewrites conflicting ports if `auto_fallback` is enabled.
pub fn resolve_and_patch_config_port(
    config_raw: &str,
    preferred_port: u16,
    allow_lan: bool,
    auto_fallback: bool,
) -> Result<PortResolution, ProcessError> {
    let clean = config_raw.trim_start_matches('\u{feff}');
    let mut json_val: Value =
        serde_json::from_str(clean).map_err(|e| ProcessError::InvalidConfigJson(e.to_string()))?;

    // Determine the primary TCP/mixed socket inbound (skipping TUN inbound which has no port).
    let primary_idx = json_val
        .get("inbounds")
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter()
                .position(|inb| inb.get("listen_port").is_some() || inb.get("port").is_some())
        });

    let detected_port = primary_idx
        .and_then(|idx| {
            json_val
                .get("inbounds")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.get(idx))
        })
        .and_then(|inb| {
            inb.get("listen_port")
                .or_else(|| inb.get("port"))
                .and_then(|p| p.as_u64())
        })
        .and_then(|p| u16::try_from(p).ok())
        .unwrap_or(preferred_port);

    let (actual_port, reassigned) = if is_port_available(detected_port, allow_lan) {
        (detected_port, false)
    } else {
        if !auto_fallback {
            return Err(ProcessError::PortInUse(detected_port));
        }
        let next_start = detected_port.saturating_add(1);
        let new_port = find_available_port(next_start, allow_lan)?;
        if let (Some(idx), Some(inbounds)) = (
            primary_idx,
            json_val.get_mut("inbounds").and_then(|v| v.as_array_mut()),
        ) {
            if let Some(primary) = inbounds.get_mut(idx) {
                if primary.get("listen_port").is_some() {
                    primary["listen_port"] = Value::from(new_port);
                } else if primary.get("port").is_some() {
                    primary["port"] = Value::from(new_port);
                }
            }
        }
        (new_port, true)
    };

    // Check secondary JSON socket inbounds (excluding primary_idx) against conflict or collision
    if auto_fallback {
        let mut used_ports = vec![actual_port];
        if let Some(inbounds) = json_val.get_mut("inbounds").and_then(|v| v.as_array_mut()) {
            for (idx, inb) in inbounds.iter_mut().enumerate() {
                if Some(idx) == primary_idx {
                    continue;
                }
                let key = if inb.get("listen_port").is_some() {
                    "listen_port"
                } else if inb.get("port").is_some() {
                    "port"
                } else {
                    continue;
                };
                if let Some(p) = inb
                    .get(key)
                    .and_then(|v| v.as_u64())
                    .and_then(|p| u16::try_from(p).ok())
                {
                    let mut chosen = p;
                    if used_ports.contains(&p) || !is_port_available(p, allow_lan) {
                        let mut start = p.max(actual_port).saturating_add(1);
                        while let Ok(free) = find_available_port(start, allow_lan) {
                            if !used_ports.contains(&free) {
                                chosen = free;
                                inb[key] = Value::from(free);
                                break;
                            }
                            start = free.saturating_add(1);
                        }
                    }
                    used_ports.push(chosen);
                }
            }
        }

        // Check sing-box experimental.clash_api.external_controller port on conflict
        if let Some(ctrl) = json_val
            .pointer("/experimental/clash_api/external_controller")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
        {
            if let Some((host, api_port)) = parse_host_port(&ctrl) {
                if api_port == actual_port || !is_port_available(api_port, false) {
                    if let Ok(free) =
                        find_available_port(api_port.max(actual_port).saturating_add(1), false)
                    {
                        if let Some(slot) =
                            json_val.pointer_mut("/experimental/clash_api/external_controller")
                        {
                            *slot = Value::String(format!("{host}:{free}"));
                        }
                    }
                }
            }
        }
    }

    let formatted = serde_json::to_string_pretty(&json_val)
        .map_err(|e| ProcessError::InvalidConfigJson(e.to_string()))?;

    Ok(PortResolution {
        requested_port: detected_port,
        actual_port,
        reassigned,
        config_content: formatted,
    })
}

fn parse_host_port(addr: &str) -> Option<(&str, u16)> {
    let trimmed = addr.trim().trim_matches('"').trim_matches('\'');
    let (host, port_str) = trimmed.rsplit_once(':')?;
    let port = port_str.trim().parse::<u16>().ok()?;
    Some((host, port))
}

fn resolve_and_patch_mihomo_yaml_ports(
    config_raw: &str,
    preferred_port: u16,
    allow_lan: bool,
    auto_fallback: bool,
) -> Result<PortResolution, ProcessError> {
    let clean = config_raw.trim_start_matches('\u{feff}');
    let mut detected_mixed: Option<u16> = None;
    let mut detected_ctrl: Option<(String, u16)> = None;

    for line in clean.lines() {
        let trimmed = line.trim();
        if line.starts_with(char::is_whitespace) || trimmed.starts_with('#') {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("mixed-port:") {
            if let Ok(p) = rest.trim().parse::<u16>() {
                detected_mixed = Some(p);
            }
        } else if let Some(rest) = trimmed.strip_prefix("external-controller:") {
            if let Some((h, p)) = parse_host_port(rest) {
                detected_ctrl = Some((h.to_string(), p));
            }
        }
    }

    let req_port = detected_mixed.unwrap_or(preferred_port);
    let (actual_port, reassigned) = if is_port_available(req_port, allow_lan) {
        (req_port, false)
    } else {
        if !auto_fallback {
            return Err(ProcessError::PortInUse(req_port));
        }
        let next = req_port.saturating_add(1);
        (find_available_port(next, allow_lan)?, true)
    };

    let new_ctrl = if auto_fallback {
        detected_ctrl.as_ref().and_then(|(host, api_port)| {
            if *api_port == actual_port || !is_port_available(*api_port, false) {
                find_available_port((*api_port).max(actual_port).saturating_add(1), false)
                    .ok()
                    .map(|p| format!("{host}:{p}"))
            } else {
                None
            }
        })
    } else {
        None
    };

    if !reassigned && new_ctrl.is_none() {
        return Ok(PortResolution {
            requested_port: req_port,
            actual_port,
            reassigned: false,
            config_content: clean.to_string(),
        });
    }

    let mut patched_lines = Vec::new();
    for line in clean.lines() {
        if !line.starts_with(char::is_whitespace) {
            let t = line.trim_start();
            if reassigned && t.starts_with("mixed-port:") {
                patched_lines.push(format!("mixed-port: {actual_port}"));
                continue;
            }
            if let Some(ref ctrl_val) = new_ctrl {
                if t.starts_with("external-controller:") {
                    patched_lines.push(format!("external-controller: {ctrl_val}"));
                    continue;
                }
            }
        }
        patched_lines.push(line.to_string());
    }
    let mut output = patched_lines.join("\n");
    if clean.ends_with('\n') {
        output.push('\n');
    }

    Ok(PortResolution {
        requested_port: req_port,
        actual_port,
        reassigned,
        config_content: output,
    })
}

/// Extracts the Clash API controller port from a generated sing-box JSON or Mihomo YAML config.
pub fn extract_clash_api_port(core: CoreType, config_raw: &str) -> Option<u16> {
    let clean = config_raw.trim_start_matches('\u{feff}');
    match core {
        CoreType::SingBox => {
            let v: Value = serde_json::from_str(clean).ok()?;
            let ctrl = v
                .pointer("/experimental/clash_api/external_controller")?
                .as_str()?;
            parse_host_port(ctrl).map(|(_, p)| p)
        }
        CoreType::Mihomo => {
            for line in clean.lines() {
                if !line.starts_with(char::is_whitespace) {
                    if let Some(rest) = line.trim().strip_prefix("external-controller:") {
                        if let Some((_, p)) = parse_host_port(rest) {
                            return Some(p);
                        }
                    }
                }
            }
            None
        }
        CoreType::Xray => None,
    }
}

/// Patches a primary core configuration's upstream relay outbound port if the upstream relay's
/// local port was auto-reassigned due to conflict.
pub fn patch_relay_outbound_port(
    core: CoreType,
    config_raw: &str,
    old_port: u16,
    new_port: u16,
) -> String {
    if old_port == new_port {
        return config_raw.to_string();
    }
    match core {
        CoreType::SingBox => {
            let Ok(mut val) = serde_json::from_str::<Value>(config_raw) else {
                return config_raw.to_string();
            };
            if let Some(outbounds) = val.get_mut("outbounds").and_then(|v| v.as_array_mut()) {
                for ob in outbounds {
                    let is_relay = ob
                        .get("tag")
                        .and_then(|t| t.as_str())
                        .is_some_and(|t| t.starts_with("relay-"));
                    let matches_port = ob
                        .get("server_port")
                        .and_then(|p| p.as_u64())
                        .is_some_and(|p| p == u64::from(old_port));
                    if is_relay && matches_port {
                        ob["server_port"] = Value::from(new_port);
                    }
                }
            }
            serde_json::to_string_pretty(&val).unwrap_or_else(|_| config_raw.to_string())
        }
        CoreType::Mihomo => {
            let old_line = format!("    port: {old_port}");
            let new_line = format!("    port: {new_port}");
            let mut in_relay_block = false;
            let mut replaced = false;
            let mut out_lines = Vec::new();
            for line in config_raw.lines() {
                let trimmed = line.trim_start();
                if line.starts_with("  - name:") {
                    in_relay_block = trimmed.contains("(Relay via ") || trimmed.contains("relay-");
                } else if !line.starts_with(' ') {
                    in_relay_block = false;
                }
                if in_relay_block && !replaced && line == old_line {
                    out_lines.push(new_line.clone());
                    replaced = true;
                } else {
                    out_lines.push(line.to_string());
                }
            }
            if replaced {
                let mut joined = out_lines.join("\n");
                if config_raw.ends_with('\n') {
                    joined.push('\n');
                }
                joined
            } else {
                config_raw.replacen(&old_line, &new_line, 1)
            }
        }
        CoreType::Xray => config_raw.to_string(),
    }
}

/// Ensures a binary file has executable permissions on Unix targets (safe no-op on read-only mounts).
pub fn ensure_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            let mode = meta.permissions().mode();
            if (mode & 0o111) != 0o111 {
                let mut perms = meta.permissions();
                perms.set_mode(mode | 0o755);
                let _ = std::fs::set_permissions(path, perms);
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

/// Applies `CREATE_NO_WINDOW` on Windows to prevent console window popups
pub fn configure_command_no_window(cmd: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = cmd;
    }
}

/// Runs the core's built-in configuration validator (`sing-box check`, `mihomo -t`, `xray run -test`)
pub async fn check_core_config(
    core: CoreType,
    binary_path: &Path,
    config_path: &Path,
    work_dir: &Path,
) -> Result<(), ProcessError> {
    if !binary_path.exists() {
        return Err(ProcessError::BinaryNotFound(
            binary_path.display().to_string(),
        ));
    }
    ensure_executable(binary_path);

    let mut cmd = Command::new(binary_path);
    configure_command_no_window(&mut cmd);
    cmd.current_dir(work_dir);

    match core {
        CoreType::SingBox => {
            cmd.arg("check")
                .arg("-c")
                .arg(config_path)
                .arg("-D")
                .arg(work_dir)
                .arg("--disable-color");
        }
        CoreType::Mihomo => {
            cmd.arg("-t")
                .arg("-f")
                .arg(config_path)
                .arg("-d")
                .arg(work_dir);
        }
        CoreType::Xray => {
            cmd.arg("run").arg("-test").arg("-c").arg(config_path);
        }
    }

    let output = tokio::time::timeout(Duration::from_secs(10), cmd.output())
        .await
        .map_err(|_| ProcessError::ConfigCheckTimeout(core))??;

    if output.status.success() {
        Ok(())
    } else {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{}\n{}", stdout.trim(), stderr.trim())
            .trim()
            .to_string();
        Err(ProcessError::ConfigCheckFailed {
            core,
            details: if combined.is_empty() {
                format!("Exit status: {}", output.status)
            } else {
                combined
            },
        })
    }
}

/// Builds the runtime `tokio::process::Command` to start the specified core
pub fn build_core_run_command(
    core: CoreType,
    binary_path: &Path,
    config_path: &Path,
    work_dir: &Path,
) -> Command {
    ensure_executable(binary_path);
    let mut cmd = Command::new(binary_path);
    configure_command_no_window(&mut cmd);
    cmd.current_dir(work_dir);
    cmd.kill_on_drop(true);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    #[cfg(unix)]
    unsafe {
        cmd.pre_exec(|| {
            // Isolate into its own process group so JobObject cleanup can terminate the tree.
            let _ = libc::setpgid(0, 0);
            #[cfg(any(target_os = "linux", target_os = "android"))]
            {
                let _ = libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
            }
            Ok(())
        });
    }

    match core {
        CoreType::SingBox => {
            cmd.arg("run")
                .arg("-c")
                .arg(config_path)
                .arg("-D")
                .arg(work_dir)
                .arg("--disable-color");
        }
        CoreType::Mihomo => {
            cmd.arg("-f").arg(config_path).arg("-d").arg(work_dir);
        }
        CoreType::Xray => {
            cmd.arg("run").arg("-c").arg(config_path);
        }
    }

    cmd
}

// ==================== Windows Job Object Implementation ====================

#[cfg(target_os = "windows")]
mod win_job {
    use super::ProcessError;
    use std::ffi::c_void;
    use std::ptr::null_mut;

    type Handle = *mut c_void;
    type Bool = i32;
    type Dword = u32;

    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS: i32 = 9;
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: Dword = 0x0000_2000;
    const PROCESS_SET_QUOTA: Dword = 0x0100;
    const PROCESS_TERMINATE: Dword = 0x0001;

    #[repr(C)]
    #[derive(Default)]
    struct IoCounters {
        read_operation_count: u64,
        write_operation_count: u64,
        other_operation_count: u64,
        read_transfer_count: u64,
        write_transfer_count: u64,
        other_transfer_count: u64,
    }

    #[repr(C)]
    #[derive(Default)]
    struct JobObjectBasicLimitInformation {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: Dword,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: Dword,
        affinity: usize,
        priority_class: Dword,
        scheduling_class: Dword,
    }

    #[repr(C)]
    #[derive(Default)]
    struct JobObjectExtendedLimitInformation {
        basic_limit_information: JobObjectBasicLimitInformation,
        io_info: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateJobObjectW(lp_job_attributes: *mut c_void, lp_name: *const u16) -> Handle;
        fn SetInformationJobObject(
            h_job: Handle,
            job_object_info_class: i32,
            lp_job_object_info: *const c_void,
            cb_job_object_info_length: Dword,
        ) -> Bool;
        fn AssignProcessToJobObject(h_job: Handle, h_process: Handle) -> Bool;
        fn TerminateJobObject(h_job: Handle, u_exit_code: u32) -> Bool;
        fn OpenProcess(
            dw_desired_access: Dword,
            b_inherit_handle: Bool,
            dw_process_id: Dword,
        ) -> Handle;
        fn CloseHandle(h_object: Handle) -> Bool;
        fn GetLastError() -> Dword;
    }

    /// RAII wrapper around a Windows Job Object configured with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
    /// When this handle is closed (including when the parent UI process crashes or is killed),
    /// Windows automatically terminates all assigned child processes.
    pub struct JobObject {
        handle: Handle,
    }

    // Safety: Win32 Job Object HANDLE is thread-safe for kernel operations.
    unsafe impl Send for JobObject {}
    unsafe impl Sync for JobObject {}

    impl JobObject {
        pub fn new() -> Result<Self, ProcessError> {
            unsafe {
                let handle = CreateJobObjectW(null_mut(), null_mut());
                if handle.is_null() {
                    return Err(ProcessError::JobObjectError(format!(
                        "CreateJobObjectW failed with error {}",
                        GetLastError()
                    )));
                }

                let mut info = JobObjectExtendedLimitInformation::default();
                info.basic_limit_information.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

                let ok = SetInformationJobObject(
                    handle,
                    JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS,
                    &info as *const _ as *const c_void,
                    std::mem::size_of::<JobObjectExtendedLimitInformation>() as Dword,
                );

                if ok == 0 {
                    let err = GetLastError();
                    CloseHandle(handle);
                    return Err(ProcessError::JobObjectError(format!(
                        "SetInformationJobObject failed with error {}",
                        err
                    )));
                }

                Ok(Self { handle })
            }
        }

        pub fn assign_pid(&self, pid: u32) -> Result<(), ProcessError> {
            unsafe {
                let proc_handle = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
                if proc_handle.is_null() {
                    return Err(ProcessError::JobObjectError(format!(
                        "OpenProcess({}) failed with error {}",
                        pid,
                        GetLastError()
                    )));
                }

                let ok = AssignProcessToJobObject(self.handle, proc_handle);
                let err = GetLastError();
                CloseHandle(proc_handle);

                if ok == 0 {
                    return Err(ProcessError::JobObjectError(format!(
                        "AssignProcessToJobObject(pid={}) failed with error {}",
                        pid, err
                    )));
                }
                Ok(())
            }
        }

        pub fn terminate(&self) {
            unsafe {
                let _ = TerminateJobObject(self.handle, 1);
            }
        }
    }

    impl Drop for JobObject {
        fn drop(&mut self) {
            if !self.handle.is_null() {
                unsafe {
                    let _ = CloseHandle(self.handle);
                }
                self.handle = null_mut();
            }
        }
    }
}

#[cfg(target_os = "windows")]
pub use win_job::JobObject;

#[cfg(unix)]
mod unix_job {
    use super::ProcessError;
    use parking_lot::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Unix process-group supervisor mirroring Win32 JobObject termination semantics.
    pub struct JobObject {
        pids: Mutex<Vec<u32>>,
        terminated: AtomicBool,
    }

    impl JobObject {
        pub fn new() -> Result<Self, ProcessError> {
            Ok(Self {
                pids: Mutex::new(Vec::new()),
                terminated: AtomicBool::new(false),
            })
        }

        pub fn assign_pid(&self, pid: u32) -> Result<(), ProcessError> {
            self.pids.lock().push(pid);
            Ok(())
        }

        pub fn terminate(&self) {
            if self.terminated.swap(true, Ordering::SeqCst) {
                return;
            }
            let pids = self.pids.lock().clone();
            for pid in &pids {
                let raw = *pid as i32;
                if raw > 0 {
                    unsafe {
                        // Signal both process group and direct PID gracefully first.
                        let _ = libc::kill(-raw, libc::SIGTERM);
                        let _ = libc::kill(raw, libc::SIGTERM);
                    }
                }
            }
            if !pids.is_empty() {
                std::thread::sleep(std::time::Duration::from_millis(60));
                for pid in &pids {
                    let raw = *pid as i32;
                    if raw > 0 {
                        unsafe {
                            let _ = libc::kill(-raw, libc::SIGKILL);
                            let _ = libc::kill(raw, libc::SIGKILL);
                        }
                    }
                }
            }
        }
    }

    impl Drop for JobObject {
        fn drop(&mut self) {
            self.terminate();
        }
    }
}

#[cfg(unix)]
pub use unix_job::JobObject;

#[cfg(all(not(target_os = "windows"), not(unix)))]
pub struct JobObject;

#[cfg(all(not(target_os = "windows"), not(unix)))]
impl JobObject {
    pub fn new() -> Result<Self, ProcessError> {
        Ok(Self)
    }

    pub fn assign_pid(&self, _pid: u32) -> Result<(), ProcessError> {
        Ok(())
    }

    pub fn terminate(&self) {}
}

/// Returns the default handwritten minimal sing-box configuration (mixed inbound + direct outbound)
pub fn default_minimal_singbox_config(mixed_port: u16) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "log": {
            "level": "info",
            "timestamp": true
        },
        "inbounds": [
            {
                "type": "mixed",
                "tag": "mixed-in",
                "listen": "127.0.0.1",
                "listen_port": mixed_port
            }
        ],
        "outbounds": [
            {
                "type": "direct",
                "tag": "direct"
            }
        ]
    }))
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn locate_singbox_binary() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("cores")
            .join(CoreType::SingBox.binary_name())
    }

    #[test]
    fn test_port_conflict_and_auto_fallback() {
        // Bind a temporary listener on an available port to simulate port conflict
        let occupied_listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind ephemeral port");
        let occupied_port = occupied_listener.local_addr().unwrap().port();

        assert!(!is_port_available(occupied_port, false));

        let raw_cfg = default_minimal_singbox_config(occupied_port);

        // When auto_fallback is disabled, should return PortInUse error
        let err = resolve_and_patch_config_port(&raw_cfg, occupied_port, false, false).unwrap_err();
        assert!(matches!(err, ProcessError::PortInUse(p) if p == occupied_port));

        // When auto_fallback is enabled, should reassign to a free port and rewrite JSON
        let res = resolve_and_patch_config_port(&raw_cfg, occupied_port, false, true).unwrap();
        assert_eq!(res.requested_port, occupied_port);
        assert!(res.reassigned);
        assert_ne!(res.actual_port, occupied_port);
        assert!(res
            .config_content
            .contains(&format!("\"listen_port\": {}", res.actual_port)));
    }

    #[tokio::test]
    async fn test_singbox_config_check_valid_and_invalid() {
        let bin = locate_singbox_binary();
        if !bin.exists() {
            return;
        }

        let temp_dir = std::env::temp_dir().join("novaproxy_test_cfg_check");
        std::fs::create_dir_all(&temp_dir).unwrap();

        let valid_path = temp_dir.join("valid_sb.json");
        std::fs::write(&valid_path, default_minimal_singbox_config(17890)).unwrap();

        check_core_config(CoreType::SingBox, &bin, &valid_path, &temp_dir)
            .await
            .expect("valid minimal sing-box config should pass check");

        let invalid_path = temp_dir.join("invalid_sb.json");
        std::fs::write(&invalid_path, r#"{"inbounds": [{"type": "non_existent"}]}"#).unwrap();

        let err = check_core_config(CoreType::SingBox, &bin, &invalid_path, &temp_dir)
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            ProcessError::ConfigCheckFailed {
                core: CoreType::SingBox,
                ..
            }
        ));
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn test_job_object_kills_child_on_drop() {
        let bin = locate_singbox_binary();
        if !bin.exists() {
            return;
        }

        let temp_dir = std::env::temp_dir().join("novaproxy_test_job_obj");
        std::fs::create_dir_all(&temp_dir).unwrap();

        let free_port = find_available_port(28900, false).unwrap();
        let cfg_path = temp_dir.join("job_test_sb.json");
        std::fs::write(&cfg_path, default_minimal_singbox_config(free_port)).unwrap();

        let job = JobObject::new().expect("create Windows JobObject");
        let mut cmd = build_core_run_command(CoreType::SingBox, &bin, &cfg_path, &temp_dir);
        let mut child = cmd.spawn().expect("spawn sing-box");
        let pid = child.id().expect("child pid");
        job.assign_pid(pid).expect("assign pid to JobObject");

        // Wait up to 2s for sing-box to bind the listening port
        let mut bound = false;
        for _ in 0..40 {
            if !is_port_available(free_port, false) {
                bound = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(bound, "sing-box should bind listening port");
        assert!(
            child.try_wait().expect("try_wait").is_none(),
            "child must still be running before JobObject drop"
        );

        // Dropping the JobObject simulates the parent UI process dying and closing its handles
        drop(job);

        // Child must exit almost immediately once the JobObject handle is closed
        let _status = tokio::time::timeout(Duration::from_secs(3), child.wait())
            .await
            .expect("child process should be terminated by JobObject")
            .expect("wait result");
        assert!(is_port_available(free_port, false));
    }

    #[test]
    fn test_mihomo_yaml_port_and_controller_fallback() {
        let mixed_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let occupied_mixed = mixed_listener.local_addr().unwrap().port();
        let ctrl_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let occupied_ctrl = ctrl_listener.local_addr().unwrap().port();

        let yaml_cfg = format!(
            "mixed-port: {}\nallow-lan: false\nexternal-controller: \"127.0.0.1:{}\"\nmode: rule\n",
            occupied_mixed, occupied_ctrl
        );

        let res =
            resolve_and_patch_core_ports(CoreType::Mihomo, &yaml_cfg, occupied_mixed, false, true)
                .expect("mihomo yaml fallback");
        assert!(res.reassigned);
        assert_ne!(res.actual_port, occupied_mixed);
        assert!(res
            .config_content
            .contains(&format!("mixed-port: {}", res.actual_port)));

        let new_api_port = extract_clash_api_port(CoreType::Mihomo, &res.config_content)
            .expect("extract mihomo clash api port");
        assert_ne!(new_api_port, occupied_ctrl);
        assert_ne!(new_api_port, res.actual_port);
    }

    #[test]
    fn test_patch_relay_outbound_port_singbox_and_mihomo() {
        let sb_cfg = r#"{
  "outbounds": [
    { "type": "socks", "tag": "relay-node", "server": "127.0.0.1", "server_port": 18889 },
    { "type": "direct", "tag": "direct" }
  ]
}"#;
        let patched_sb = patch_relay_outbound_port(CoreType::SingBox, sb_cfg, 18889, 18999);
        assert!(patched_sb.contains("18999"));
        assert!(!patched_sb.contains("18889"));

        let mh_cfg = "proxies:\n  - name: relay-node\n    type: socks5\n    server: 127.0.0.1\n    port: 18889\n";
        let patched_mh = patch_relay_outbound_port(CoreType::Mihomo, mh_cfg, 18889, 18999);
        assert!(patched_mh.contains("port: 18999"));
        assert!(!patched_mh.contains("port: 18889"));
    }

    #[test]
    fn test_singbox_tun_first_inbound_does_not_block_mixed_port_fallback() {
        let mixed_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let occupied_mixed = mixed_listener.local_addr().unwrap().port();
        let cfg = serde_json::json!({
            "inbounds": [
                { "type": "tun", "tag": "tun-in" },
                { "type": "mixed", "tag": "mixed-in", "listen": "127.0.0.1", "listen_port": occupied_mixed }
            ],
            "experimental": {
                "clash_api": { "external_controller": "127.0.0.1:0" }
            }
        });
        let raw = serde_json::to_string(&cfg).unwrap();
        let res =
            resolve_and_patch_core_ports(CoreType::SingBox, &raw, occupied_mixed, false, true)
                .expect("tun + occupied mixed port should reassign mixed port");
        assert!(res.reassigned);
        assert_ne!(res.actual_port, occupied_mixed);
        let parsed: serde_json::Value = serde_json::from_str(&res.config_content).unwrap();
        assert_eq!(
            parsed["inbounds"][1]["listen_port"].as_u64(),
            Some(u64::from(res.actual_port))
        );
    }
}

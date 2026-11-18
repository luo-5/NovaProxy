//! Latency testing engines: bounded concurrent TCP ping and real proxy URL delay probing.
//! Supports Clash API `/proxies/{tag}/delay` against the running core as well as ephemeral
//! temporary core probing (sing-box / mihomo / xray) so QUIC/UDP protocols (Hysteria2,
//! TUIC, WireGuard) and Xray-exclusive transports measure real end-to-end tunnel latency.

use crate::core::process::{
    build_core_run_command, find_available_port, is_port_available, JobObject,
};
use crate::core::{CoreLifecycleState, CoreManager};
use crate::generator::generate_core_config;
use crate::model::{AppSettings, CoreType, DnsSettings, ProxyNode, RoutingMode, RoutingSettings};
use crate::storage::{atomic_write_file, Database};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use ts_rs::TS;

pub const DEFAULT_LATENCY_TEST_URL: &str = "https://www.gstatic.com/generate_204";
pub const DEFAULT_TCP_TIMEOUT_MS: u64 = 3000;
pub const DEFAULT_URL_TIMEOUT_MS: u64 = 4500;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum LatencyTestMode {
    #[default]
    Tcp,
    Url,
}

/// Measures raw TCP handshake latency to `host:port`.
pub async fn tcp_ping(host: &str, port: u16, timeout_duration: Duration) -> Result<i64, String> {
    let start = Instant::now();
    let formatted_host = crate::parser::uri_common::format_host_for_uri(host);
    let addr = format!("{formatted_host}:{port}");
    let connect_fut = tokio::net::TcpStream::connect(&addr);
    match tokio::time::timeout(timeout_duration, connect_fut).await {
        Ok(Ok(_)) => Ok(start.elapsed().as_millis() as i64),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("TCP connect timeout".to_string()),
    }
}

/// Queries a running Clash API controller for real proxy delay (`/proxies/{name}/delay`).
pub async fn probe_clash_api_delay(
    clash_api_port: u16,
    clash_api_secret: Option<&str>,
    candidate_names: &[&str],
    test_url: &str,
    timeout_duration: Duration,
) -> Result<i64, String> {
    let client_timeout = timeout_duration.saturating_add(Duration::from_millis(1200));
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(client_timeout)
        .build()
        .map_err(|e| e.to_string())?;

    let timeout_ms = timeout_duration.as_millis().max(500);
    let encoded_url = urlencoding::encode(test_url);
    let mut last_err = String::from("No matching proxy tag found in Clash API");

    for candidate in candidate_names {
        let trimmed = candidate.trim();
        if trimmed.is_empty() {
            continue;
        }
        let encoded_name = urlencoding::encode(trimmed);
        let endpoint = format!(
            "http://127.0.0.1:{}/proxies/{}/delay?timeout={}&url={}",
            clash_api_port, encoded_name, timeout_ms, encoded_url
        );

        let mut req = client.get(&endpoint);
        if let Some(secret) = clash_api_secret.filter(|s| !s.is_empty()) {
            req = req.bearer_auth(secret);
        }

        match req.send().await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(json) = resp.json::<serde_json::Value>().await {
                    if let Some(delay) = json.get("delay").and_then(|v| v.as_i64()) {
                        return Ok(delay.max(1));
                    }
                    if let Some(msg) = json.get("message").and_then(|v| v.as_str()) {
                        last_err = msg.to_string();
                    }
                }
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                let err_msg = format!("Clash API delay {} ({}): {}", trimmed, status, body.trim());
                if status != reqwest::StatusCode::NOT_FOUND
                    || last_err == "No matching proxy tag found in Clash API"
                    || last_err.contains("404 Not Found")
                {
                    last_err = err_msg;
                }
            }
            Err(e) => {
                last_err = e.to_string();
            }
        }
    }

    Err(last_err)
}

/// Measures full end-to-end HTTP delay through a local SOCKS5/mixed proxy port.
pub async fn probe_url_via_socks_proxy(
    socks_port: u16,
    test_url: &str,
    timeout_duration: Duration,
) -> Result<i64, String> {
    let proxy = reqwest::Proxy::all(format!("socks5h://127.0.0.1:{socks_port}"))
        .map_err(|e| e.to_string())?;
    let client = reqwest::Client::builder()
        .proxy(proxy)
        .timeout(timeout_duration)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;

    let start = Instant::now();
    let resp = client
        .get(test_url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    if status.is_success() || status.is_redirection() {
        Ok(start.elapsed().as_millis().max(1) as i64)
    } else {
        Err(format!("Proxy URL test returned HTTP {}", status))
    }
}

struct EphemeralCoreProcess {
    child: tokio::process::Child,
    upstream_child: Option<tokio::process::Child>,
    _job: JobObject,
    temp_dir: PathBuf,
    mixed_port: u16,
    clash_api_port: u16,
    recent_stderr: Arc<parking_lot::Mutex<String>>,
}

impl EphemeralCoreProcess {
    async fn shutdown(mut self) {
        let _ = self.child.kill().await;
        let _ = tokio::time::timeout(Duration::from_secs(2), self.child.wait()).await;
        if let Some(mut up) = self.upstream_child.take() {
            let _ = up.kill().await;
            let _ = tokio::time::timeout(Duration::from_secs(2), up.wait()).await;
        }
        self._job.terminate();
        let _ = std::fs::remove_dir_all(&self.temp_dir);
    }
}

async fn spawn_ephemeral_probe_core(
    core_manager: &CoreManager,
    core: CoreType,
    config_content: &str,
    mixed_port: u16,
    clash_api_port: u16,
) -> Result<EphemeralCoreProcess, String> {
    let tag = uuid::Uuid::new_v4().to_string();
    let temp_dir = core_manager.work_dir().join(format!("probe-{tag}"));
    std::fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;

    let ext = if core == CoreType::Mihomo {
        "yaml"
    } else {
        "json"
    };
    let cfg_path = temp_dir.join(format!("probe.{ext}"));
    atomic_write_file(&cfg_path, config_content.as_bytes()).map_err(|e| e.to_string())?;

    let bin_path = core_manager.binary_path(core);
    if !bin_path.exists() {
        let _ = std::fs::remove_dir_all(&temp_dir);
        return Err(format!("Core binary not found: {}", bin_path.display()));
    }

    let job = JobObject::new().map_err(|e| e.to_string())?;
    let mut cmd = build_core_run_command(core, &bin_path, &cfg_path, &temp_dir);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| {
        let _ = std::fs::remove_dir_all(&temp_dir);
        format!("Failed to spawn ephemeral {core}: {e}")
    })?;

    if let Some(pid) = child.id() {
        let _ = job.assign_pid(pid);
    }

    // Wait briefly until mixed_port (and clash_api_port for non-Xray cores) starts listening
    let mut ready = false;
    for _ in 0..30 {
        if let Ok(Some(exit)) = child.try_wait() {
            let mut err_buf = Vec::new();
            if let Some(mut se) = child.stderr.take() {
                let _ = tokio::io::AsyncReadExt::read_to_end(&mut se, &mut err_buf).await;
            }
            let _ = std::fs::remove_dir_all(&temp_dir);
            let stderr_txt = String::from_utf8_lossy(&err_buf);
            return Err(format!(
                "Ephemeral {core} exited prematurely ({exit}): {}",
                stderr_txt.trim()
            ));
        }
        let mixed_ready = !is_port_available(mixed_port, false);
        let api_ready = core == CoreType::Xray || !is_port_available(clash_api_port, false);
        if mixed_ready && api_ready {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(60)).await;
    }

    if !ready {
        let _ = child.kill().await;
        let _ = std::fs::remove_dir_all(&temp_dir);
        return Err(format!(
            "Ephemeral {core} failed to bind readiness ports (mixed={mixed_port}, api={clash_api_port})"
        ));
    }

    let recent_stderr = Arc::new(parking_lot::Mutex::new(String::new()));

    // Drain stdout/stderr pipes asynchronously so child output buffers can never deadlock
    if let Some(mut so) = child.stdout.take() {
        let log_buf = recent_stderr.clone();
        tokio::spawn(async move {
            let mut sink = [0u8; 4096];
            while let Ok(n) = tokio::io::AsyncReadExt::read(&mut so, &mut sink).await {
                if n == 0 {
                    break;
                }
                let mut g = log_buf.lock();
                if g.len() < 4096 {
                    g.push_str(&String::from_utf8_lossy(&sink[..n]));
                }
            }
        });
    }
    if let Some(mut se) = child.stderr.take() {
        let log_buf = recent_stderr.clone();
        tokio::spawn(async move {
            let mut sink = [0u8; 4096];
            while let Ok(n) = tokio::io::AsyncReadExt::read(&mut se, &mut sink).await {
                if n == 0 {
                    break;
                }
                let mut g = log_buf.lock();
                if g.len() < 4096 {
                    g.push_str(&String::from_utf8_lossy(&sink[..n]));
                }
            }
        });
    }

    Ok(EphemeralCoreProcess {
        child,
        upstream_child: None,
        _job: job,
        temp_dir,
        mixed_port,
        clash_api_port,
        recent_stderr,
    })
}

fn build_ephemeral_probe_settings(
    base: &AppSettings,
    mixed_port: u16,
    clash_api_port: u16,
) -> (AppSettings, DnsSettings, RoutingSettings) {
    let mut settings = base.clone();
    settings.mixed_port = mixed_port;
    settings.socks_port = None;
    settings.http_port = None;
    settings.clash_api_port = clash_api_port;
    settings.clash_api_secret = None;
    settings.allow_lan = false;
    settings.tun_mode = false;
    settings.system_proxy = false;

    let dns = DnsSettings {
        fake_ip: false,
        ..Default::default()
    };
    let routing = RoutingSettings {
        mode: RoutingMode::Global,
        ..Default::default()
    };
    (settings, dns, routing)
}

/// Probes a single node in either TCP or real proxy URL mode.
pub async fn test_single_node_latency(
    core_manager: &CoreManager,
    node: &ProxyNode,
    settings: &AppSettings,
    mode: LatencyTestMode,
    test_url: &str,
    timeout_duration: Duration,
) -> Result<i64, String> {
    if mode == LatencyTestMode::Tcp {
        return tcp_ping(&node.server, node.port, timeout_duration).await;
    }

    // 1. If a primary core with Clash API is running and supports this node, probe via Clash API first.
    let status = core_manager.status();
    if matches!(status.lifecycle, CoreLifecycleState::Running) {
        if let Some(active_core) = status.active_core {
            if active_core != CoreType::Xray {
                let clash_port = status
                    .actual_clash_api_port
                    .unwrap_or(settings.clash_api_port);
                let relay_tag = format!("relay-{}", node.id);
                let candidates = [node.id.as_str(), node.name.as_str(), relay_tag.as_str()];
                if let Ok(delay) = probe_clash_api_delay(
                    clash_port,
                    settings.clash_api_secret.as_deref(),
                    &candidates,
                    test_url,
                    timeout_duration,
                )
                .await
                {
                    return Ok(delay);
                }
            }
        }
    }

    // 2. Fall back to an isolated ephemeral core instance so UDP/QUIC/XHTTP/offline nodes test accurately.
    let available = core_manager.available_cores();
    let sel = core_manager
        .capabilities()
        .select_core_for_node(node, node.preferred_core, &available)
        .map_err(|e| e.to_string())?;

    let mixed_port = find_available_port(31200, false).map_err(|e| e.to_string())?;
    let api_port =
        find_available_port(mixed_port.saturating_add(1), false).map_err(|e| e.to_string())?;
    let (probe_settings, probe_dns, probe_routing) =
        build_ephemeral_probe_settings(settings, mixed_port, api_port);

    let cfg = generate_core_config(
        sel.core,
        std::slice::from_ref(node),
        &[],
        &probe_settings,
        &probe_dns,
        &probe_routing,
        Some(&node.id),
    )
    .map_err(|e| e.to_string())?;

    let proc =
        spawn_ephemeral_probe_core(core_manager, sel.core, &cfg, mixed_port, api_port).await?;

    // Measure full end-to-end delay over the ephemeral single-node core's mixed SOCKS5 inbound
    // so custom/local test_url values and QUIC/UDP/XHTTP transports work identically across cores.
    let res = probe_url_via_socks_proxy(proc.mixed_port, test_url, timeout_duration)
        .await
        .map_err(|err| {
            let stderr_tail = proc.recent_stderr.lock().trim().to_string();
            if stderr_tail.is_empty() {
                err
            } else {
                format!("{err} | stderr: {stderr_tail}")
            }
        });

    proc.shutdown().await;
    res
}

async fn batch_probe_with_clash_api(
    nodes: &[ProxyNode],
    mihomo_names: &HashMap<String, String>,
    clash_port: u16,
    secret: Option<&str>,
    test_url: &str,
    timeout_duration: Duration,
    concurrency: usize,
) -> HashMap<String, Result<i64, String>> {
    let sem = Arc::new(Semaphore::new(concurrency.max(1)));
    let mut handles = Vec::with_capacity(nodes.len());

    for node in nodes {
        let permit_sem = sem.clone();
        let id = node.id.clone();
        let name = node.name.clone();
        let mihomo_name = mihomo_names.get(&node.id).cloned();
        let secret_owned = secret.map(|s| s.to_string());
        let url_owned = test_url.to_string();
        handles.push(tokio::spawn(async move {
            let _permit = permit_sem.acquire_owned().await;
            let relay_tag = format!("relay-{id}");
            let mut candidates: Vec<&str> = Vec::with_capacity(4);
            if let Some(ref mname) = mihomo_name {
                candidates.push(mname.as_str());
            }
            candidates.push(id.as_str());
            candidates.push(name.as_str());
            candidates.push(relay_tag.as_str());
            let res = probe_clash_api_delay(
                clash_port,
                secret_owned.as_deref(),
                &candidates,
                &url_owned,
                timeout_duration,
            )
            .await;
            (id, res)
        }));
    }

    let mut out = HashMap::with_capacity(handles.len());
    for h in handles {
        if let Ok((id, res)) = h.await {
            out.insert(id, res);
        }
    }
    out
}

/// Tests all provided nodes concurrently and persists results to `Database`.
pub async fn test_nodes_batch(
    db: &Database,
    core_manager: &CoreManager,
    nodes: Vec<ProxyNode>,
    settings: &AppSettings,
    mode: LatencyTestMode,
    test_url: &str,
    timeout_duration: Duration,
) -> Vec<(String, Option<i64>)> {
    if nodes.is_empty() {
        return Vec::new();
    }

    let mut result_map: HashMap<String, (Option<i64>, Option<String>)> =
        HashMap::with_capacity(nodes.len());

    if mode == LatencyTestMode::Tcp {
        let sem = Arc::new(Semaphore::new(32));
        let mut handles = Vec::with_capacity(nodes.len());
        for node in &nodes {
            let permit_sem = sem.clone();
            let id = node.id.clone();
            let server = node.server.clone();
            let port = node.port;
            handles.push(tokio::spawn(async move {
                let _permit = permit_sem.acquire_owned().await;
                let res = tcp_ping(&server, port, timeout_duration).await;
                (id, res)
            }));
        }
        for h in handles {
            if let Ok((id, res)) = h.await {
                let (lat, err) = match res {
                    Ok(ms) => (Some(ms), None),
                    Err(e) => (None, Some(e)),
                };
                result_map.insert(id, (lat, err));
            }
        }
    } else {
        let available = core_manager.available_cores();
        let live_mihomo_names = crate::generator::mihomo::resolve_mihomo_proxy_names(&nodes);
        let mut singbox_nodes = Vec::new();
        let mut mihomo_nodes = Vec::new();
        let mut xray_nodes = Vec::new();

        for node in &nodes {
            match core_manager
                .capabilities()
                .select_core_for_node(node, node.preferred_core, &available)
                .map(|s| s.core)
            {
                Ok(CoreType::SingBox) => singbox_nodes.push(node.clone()),
                Ok(CoreType::Mihomo) => mihomo_nodes.push(node.clone()),
                Ok(CoreType::Xray) => xray_nodes.push(node.clone()),
                Err(e) => {
                    result_map.insert(node.id.clone(), (None, Some(e.to_string())));
                }
            }
        }

        // Batch-probe bucket using one ephemeral instance per engine so thousands of nodes
        // do not spawn thousands of processes.
        for (engine, bucket) in [
            (CoreType::SingBox, singbox_nodes),
            (CoreType::Mihomo, mihomo_nodes),
        ] {
            if bucket.is_empty() {
                continue;
            }
            let running = core_manager.status();
            if !core_manager.is_config_dirty()
                && matches!(running.lifecycle, CoreLifecycleState::Running)
                && running.active_core == Some(engine)
            {
                let clash_port = running
                    .actual_clash_api_port
                    .unwrap_or(settings.clash_api_port);
                let live_results = batch_probe_with_clash_api(
                    &bucket,
                    &live_mihomo_names,
                    clash_port,
                    settings.clash_api_secret.as_deref(),
                    test_url,
                    timeout_duration,
                    12,
                )
                .await;
                for (id, res) in live_results {
                    match res {
                        Ok(ms) => {
                            result_map.insert(id, (Some(ms), None));
                        }
                        Err(e) if !e.contains("404") => {
                            result_map.insert(id, (None, Some(e)));
                        }
                        Err(_) => {
                            // 404: fallback node wasn't inside live config (e.g. newly added offline),
                            // fall through? Handled right below if still absent.
                        }
                    }
                }
            }

            let mut remaining: Vec<ProxyNode> = bucket
                .into_iter()
                .filter(|n| !result_map.contains_key(&n.id))
                .collect();
            if remaining.is_empty() {
                continue;
            }

            // Sanitize dangling `dialer_proxy` references that point outside `remaining`
            // so one missing/cross-core upstream does not abort ephemeral batch generation.
            let remaining_ids: std::collections::HashSet<String> =
                remaining.iter().map(|n| n.id.clone()).collect();
            let remaining_names: std::collections::HashSet<String> =
                remaining.iter().map(|n| n.name.clone()).collect();
            for node in &mut remaining {
                if let Some(ref dialer) = node.dialer_proxy {
                    let exists = remaining_ids.contains(dialer) || remaining_names.contains(dialer);
                    if !exists || dialer == &node.id || dialer == &node.name {
                        node.dialer_proxy = None;
                    }
                }
            }
            let remaining_mihomo_names =
                crate::generator::mihomo::resolve_mihomo_proxy_names(&remaining);

            match (
                find_available_port(31300, false),
                find_available_port(31400, false),
            ) {
                (Ok(mixed_port), Ok(api_port)) => {
                    let (probe_settings, probe_dns, probe_routing) =
                        build_ephemeral_probe_settings(settings, mixed_port, api_port);
                    match generate_core_config(
                        engine,
                        &remaining,
                        &[],
                        &probe_settings,
                        &probe_dns,
                        &probe_routing,
                        remaining.first().map(|n| n.id.as_str()),
                    ) {
                        Ok(cfg) => {
                            match spawn_ephemeral_probe_core(
                                core_manager,
                                engine,
                                &cfg,
                                mixed_port,
                                api_port,
                            )
                            .await
                            {
                                Ok(proc) => {
                                    let probes = batch_probe_with_clash_api(
                                        &remaining,
                                        &remaining_mihomo_names,
                                        proc.clash_api_port,
                                        None,
                                        test_url,
                                        timeout_duration,
                                        12,
                                    )
                                    .await;
                                    proc.shutdown().await;
                                    for (id, res) in probes {
                                        let (lat, err) = match res {
                                            Ok(ms) => (Some(ms), None),
                                            Err(e) => (None, Some(e)),
                                        };
                                        result_map.insert(id, (lat, err));
                                    }
                                }
                                Err(e) => {
                                    for node in remaining {
                                        result_map.insert(node.id, (None, Some(e.clone())));
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            for node in remaining {
                                result_map.insert(node.id, (None, Some(e.to_string())));
                            }
                        }
                    }
                }
                _ => {
                    for node in remaining {
                        result_map
                            .insert(node.id, (None, Some("No free ephemeral ports".to_string())));
                    }
                }
            }
        }

        // Xray-exclusive nodes (XHTTP / mKCP): bounded ephemeral SOCKS probes
        if !xray_nodes.is_empty() {
            let sem = Arc::new(Semaphore::new(4));
            let mut handles = Vec::with_capacity(xray_nodes.len());
            for node in xray_nodes {
                let permit_sem = sem.clone();
                let cm = core_manager.clone();
                let s = settings.clone();
                let url = test_url.to_string();
                handles.push(tokio::spawn(async move {
                    let _permit = permit_sem.acquire_owned().await;
                    let res = test_single_node_latency(
                        &cm,
                        &node,
                        &s,
                        LatencyTestMode::Url,
                        &url,
                        timeout_duration,
                    )
                    .await;
                    (node.id, res)
                }));
            }
            for h in handles {
                if let Ok((id, res)) = h.await {
                    let (lat, err) = match res {
                        Ok(ms) => (Some(ms), None),
                        Err(e) => (None, Some(e)),
                    };
                    result_map.insert(id, (lat, err));
                }
            }
        }
    }

    let test_type = match mode {
        LatencyTestMode::Tcp => "tcp",
        LatencyTestMode::Url => "url",
    };

    let mut ordered_results = Vec::with_capacity(nodes.len());
    for node in nodes {
        let (lat, err) = result_map
            .remove(&node.id)
            .unwrap_or((None, Some("Test did not complete".to_string())));
        core_manager
            .telemetry()
            .record_latency_probe(node.protocol, lat, err.as_deref());
        let _ = db.record_latency(&node.id, lat, test_type, err.as_deref());
        ordered_results.push((node.id, lat));
    }
    ordered_results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ProtocolConfig, ProtocolType, Socks5Config};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn test_tcp_ping_loopback_success_and_refused() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let open_port = listener.local_addr().unwrap().port();
        let _accept_task = tokio::spawn(async move {
            let _ = listener.accept().await;
        });

        let ok_ms = tcp_ping("127.0.0.1", open_port, Duration::from_secs(2))
            .await
            .expect("loopback tcp ping should succeed");
        assert!(ok_ms >= 0);

        let free_port = find_available_port(39100, false).unwrap();
        let err = tcp_ping("127.0.0.1", free_port, Duration::from_millis(500)).await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn test_url_latency_via_ephemeral_singbox_and_local_socks5_http204() {
        let cores_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("cores");
        if !cores_dir.join(CoreType::SingBox.binary_name()).exists() {
            return;
        }

        // 1. Minimal local HTTP/1.1 204 endpoint
        let http_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let http_port = http_listener.local_addr().unwrap().port();
        let http_task = tokio::spawn(async move {
            while let Ok((mut sock, _)) = http_listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = sock.read(&mut buf).await;
                    let _ = sock
                        .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                        .await;
                });
            }
        });

        // 2. Minimal no-auth SOCKS5 server forwarding ONLY to 127.0.0.1:{http_port}
        let socks_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mock_socks_port = socks_listener.local_addr().unwrap().port();
        let socks_task = tokio::spawn(async move {
            while let Ok((mut client, _)) = socks_listener.accept().await {
                tokio::spawn(async move {
                    let mut greeting = [0u8; 2];
                    if client.read_exact(&mut greeting).await.is_err() || greeting[0] != 5 {
                        return;
                    }
                    let mut methods = vec![0u8; greeting[1] as usize];
                    if client.read_exact(&mut methods).await.is_err() {
                        return;
                    }
                    let _ = client.write_all(&[0x05, 0x00]).await;

                    let mut req_hdr = [0u8; 4];
                    if client.read_exact(&mut req_hdr).await.is_err() || req_hdr[1] != 0x01 {
                        return;
                    }
                    let target_host = match req_hdr[3] {
                        0x01 => {
                            let mut ip = [0u8; 4];
                            if client.read_exact(&mut ip).await.is_err() {
                                return;
                            }
                            format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3])
                        }
                        0x03 => {
                            let mut len = [0u8; 1];
                            if client.read_exact(&mut len).await.is_err() {
                                return;
                            }
                            let mut dom = vec![0u8; len[0] as usize];
                            if client.read_exact(&mut dom).await.is_err() {
                                return;
                            }
                            String::from_utf8_lossy(&dom).to_string()
                        }
                        _ => return,
                    };
                    let mut port_buf = [0u8; 2];
                    if client.read_exact(&mut port_buf).await.is_err() {
                        return;
                    }
                    let target_port = u16::from_be_bytes(port_buf);
                    if target_host != "127.0.0.1" || target_port != http_port {
                        let _ = client
                            .write_all(&[0x05, 0x05, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                            .await;
                        return;
                    }

                    if let Ok(mut upstream) =
                        tokio::net::TcpStream::connect(("127.0.0.1", http_port)).await
                    {
                        let _ = client
                            .write_all(&[0x05, 0x00, 0x00, 0x01, 127, 0, 0, 1, 0, 0])
                            .await;
                        let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
                    }
                });
            }
        });

        let work_dir = std::env::temp_dir().join("novaproxy_test_url_latency");
        let mgr = CoreManager::new(cores_dir, work_dir);
        let node = ProxyNode {
            id: "socks-loopback".to_string(),
            name: "Loopback SOCKS5".to_string(),
            server: "127.0.0.1".to_string(),
            port: mock_socks_port,
            protocol: ProtocolType::Socks5,
            tls: None,
            transport: None,
            settings: ProtocolConfig::Socks5(Socks5Config {
                username: None,
                password: None,
                udp: false,
            }),
            subscription_id: None,
            group_id: None,
            latency: None,
            dialer_proxy: None,
            preferred_core: Some(CoreType::SingBox),
            sort_order: 0,
        };

        let test_url = format!("http://127.0.0.1:{http_port}/generate_204");
        let delay = test_single_node_latency(
            &mgr,
            &node,
            &AppSettings::default(),
            LatencyTestMode::Url,
            &test_url,
            Duration::from_secs(4),
        )
        .await
        .expect("real URL latency probe through ephemeral sing-box should succeed");
        assert!(delay >= 1);

        http_task.abort();
        socks_task.abort();
    }
}

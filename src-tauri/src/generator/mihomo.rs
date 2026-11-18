//! Mihomo (Clash.Meta) YAML configuration generator

use crate::generator::dns::generate_mihomo_dns_yaml_for_routing;
use crate::generator::routing::generate_mihomo_rules_yaml_with_options;
use crate::model::node::{ProtocolConfig, ProxyNode, RealityConfig, TlsConfig, TransportConfig};
use crate::model::routing::{DnsSettings, RoutingSettings};
use crate::model::settings::{AppSettings, RoutingMode};
use crate::model::subscription::{GroupType, NodeGroup};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MihomoGeneratorError {
    #[error("Node {0} has an invalid configuration for mihomo: {1}")]
    InvalidNodeConfig(String, String),
    #[error("YAML formatting error: {0}")]
    Format(String),
}

/// Computes the exact deduplicated Mihomo YAML proxy name mapping (`node.id -> yaml_name`)
/// used during config generation so Clash API hot-switching and latency queries stay in sync.
pub fn resolve_mihomo_proxy_names(nodes: &[ProxyNode]) -> HashMap<String, String> {
    let mut used_names: HashSet<String> =
        ["PROXY", "AUTO", "FALLBACK", "DIRECT", "REJECT", "GLOBAL"]
            .iter()
            .map(|s| s.to_string())
            .collect();
    let mut id_to_name: HashMap<String, String> = HashMap::with_capacity(nodes.len());

    for node in nodes {
        let base = node.name.trim();
        let base_str = if base.is_empty() {
            format!("{}:{}", node.server, node.port)
        } else {
            base.to_string()
        };
        let mut unique = base_str.clone();
        let mut suffix = 2u32;
        while !used_names.insert(unique.clone()) {
            unique = format!("{base_str} ({suffix})");
            suffix += 1;
        }
        id_to_name.insert(node.id.clone(), unique);
    }

    id_to_name
}

pub fn generate_mihomo_config(
    nodes: &[ProxyNode],
    groups: &[NodeGroup],
    settings: &AppSettings,
    dns: &DnsSettings,
    routing: &RoutingSettings,
    active_node_id: Option<&str>,
) -> Result<String, MihomoGeneratorError> {
    let mut out = String::new();

    // 1. Basic configuration header
    out.push_str(&format!("mixed-port: {}\n", settings.mixed_port));
    out.push_str(&format!("allow-lan: {}\n", settings.allow_lan));
    out.push_str(&format!(
        "mode: {}\n",
        match routing.mode {
            crate::model::settings::RoutingMode::Direct => "direct",
            crate::model::settings::RoutingMode::Global => "global",
            crate::model::settings::RoutingMode::Rule => "rule",
        }
    ));
    out.push_str(&format!("log-level: {}\n", settings.log_level.as_str()));
    out.push_str(&format!("ipv6: {}\n", settings.ipv6));
    out.push_str(&format!(
        "external-controller: 127.0.0.1:{}\n",
        settings.clash_api_port
    ));
    if let Some(secret) = &settings.clash_api_secret {
        if !secret.is_empty() {
            out.push_str(&format!("secret: \"{}\"\n", escape_yaml(secret)));
        }
    }

    if settings.tun_mode {
        out.push_str("tun:\n");
        out.push_str("  enable: true\n");
        out.push_str("  stack: system\n");
        out.push_str("  device: NovaProxy-TUN\n");
        out.push_str("  auto-route: true\n");
        out.push_str("  auto-detect-interface: true\n");
        out.push_str("  dns-hijack:\n");
        out.push_str("    - any:53\n");
        out.push_str("    - tcp://any:53\n");
    }

    // 2. DNS section
    out.push_str(&generate_mihomo_dns_yaml_for_routing(dns, routing));

    // 3. Proxies section (ensure unique YAML proxy names)
    out.push_str("proxies:\n");
    let id_to_name = resolve_mihomo_proxy_names(nodes);
    let candidate_names: Vec<String> = nodes
        .iter()
        .map(|n| {
            id_to_name
                .get(&n.id)
                .cloned()
                .unwrap_or_else(|| n.name.clone())
        })
        .collect();
    let mut used_names: HashSet<String> =
        ["PROXY", "AUTO", "FALLBACK", "DIRECT", "REJECT", "GLOBAL"]
            .iter()
            .map(|s| s.to_string())
            .collect();
    used_names.extend(candidate_names.iter().cloned());

    let mut valid_proxy_names = Vec::new();
    let mut valid_id_to_name: HashMap<String, String> = HashMap::new();
    let valid_supported_ids: HashSet<String> = nodes
        .iter()
        .filter(|n| !matches!(n.settings, ProtocolConfig::ShadowTls(_)))
        .map(|n| n.id.clone())
        .collect();

    for (node, yaml_name) in nodes.iter().zip(candidate_names) {
        let is_active = active_node_id == Some(node.id.as_str());
        if let Some(yaml_block) =
            compile_mihomo_proxy_yaml(node, &yaml_name, &id_to_name, &valid_supported_ids)
        {
            out.push_str(&yaml_block);
            valid_proxy_names.push(yaml_name.clone());
            valid_id_to_name.insert(node.id.clone(), yaml_name);
        } else if is_active && routing.mode != RoutingMode::Direct {
            return Err(MihomoGeneratorError::InvalidNodeConfig(
                node.name.clone(),
                format!(
                    "protocol '{}' is not supported natively by mihomo",
                    node.protocol
                ),
            ));
        }
    }

    if valid_proxy_names.is_empty() {
        // Fallback placeholder proxy so YAML parser does not fail on empty proxies array
        out.push_str("  - name: DIRECT-PLACEHOLDER\n");
        out.push_str("    type: direct\n");
        valid_proxy_names.push("DIRECT-PLACEHOLDER".to_string());
    }

    // 4. Proxy Groups section
    out.push_str("proxy-groups:\n");

    let auto_candidates = valid_proxy_names.clone();

    // Disambiguate user group names against built-in groups and proxy names
    let mut group_used = used_names;
    let mut resolved_groups: Vec<(&NodeGroup, String)> = Vec::with_capacity(groups.len());
    for g in groups {
        let mut g_name = g.name.trim().to_string();
        if g_name.is_empty() {
            g_name = g.id.clone();
        }
        let orig = g_name.clone();
        let mut suffix = 2u32;
        while !group_used.insert(g_name.clone()) {
            g_name = format!("{orig} ({suffix})");
            suffix += 1;
        }
        resolved_groups.push((g, g_name));
    }

    // Top-level PROXY group
    out.push_str("  - name: PROXY\n");
    out.push_str("    type: select\n");
    out.push_str("    proxies:\n");

    let active_yaml_name = active_node_id
        .and_then(|id| valid_id_to_name.get(id))
        .cloned();
    if let Some(ref active_name) = active_yaml_name {
        out.push_str(&format!("      - \"{}\"\n", escape_yaml(active_name)));
    }
    out.push_str("      - AUTO\n");
    out.push_str("      - FALLBACK\n");
    for (_, g_name) in &resolved_groups {
        out.push_str(&format!("      - \"{}\"\n", escape_yaml(g_name)));
    }
    for name in &valid_proxy_names {
        if active_yaml_name.as_deref() == Some(name.as_str()) {
            continue;
        }
        out.push_str(&format!("      - \"{}\"\n", escape_yaml(name)));
    }
    out.push_str("      - DIRECT\n");

    // AUTO group (url-test)
    out.push_str("  - name: AUTO\n");
    out.push_str("    type: url-test\n");
    out.push_str("    url: https://www.gstatic.com/generate_204\n");
    out.push_str("    interval: 300\n");
    out.push_str("    proxies:\n");
    for name in &auto_candidates {
        out.push_str(&format!("      - \"{}\"\n", escape_yaml(name)));
    }

    // FALLBACK group
    out.push_str("  - name: FALLBACK\n");
    out.push_str("    type: fallback\n");
    out.push_str("    url: https://www.gstatic.com/generate_204\n");
    out.push_str("    interval: 300\n");
    out.push_str("    proxies:\n");
    for name in &auto_candidates {
        out.push_str(&format!("      - \"{}\"\n", escape_yaml(name)));
    }

    // Custom user groups
    for (g, g_name) in &resolved_groups {
        let g_type = match g.group_type {
            GroupType::Selector => "select",
            GroupType::UrlTest => "url-test",
            GroupType::Fallback => "fallback",
            GroupType::LoadBalance => "load-balance",
        };
        out.push_str(&format!("  - name: \"{}\"\n", escape_yaml(g_name)));
        out.push_str(&format!("    type: {}\n", g_type));
        if g.group_type != GroupType::Selector {
            out.push_str(&format!(
                "    url: {}\n",
                g.test_url
                    .as_deref()
                    .unwrap_or("https://www.gstatic.com/generate_204")
            ));
            out.push_str(&format!(
                "    interval: {}\n",
                g.interval_secs.unwrap_or(300)
            ));
        }
        if g.group_type == GroupType::LoadBalance {
            out.push_str("    strategy: consistent-hashing\n");
        }
        out.push_str("    proxies:\n");
        let mut members = Vec::new();
        for node_id in &g.node_ids {
            if let Some(target_name) = valid_id_to_name.get(node_id) {
                if !members.contains(target_name) {
                    members.push(target_name.clone());
                }
            }
        }
        if members.is_empty() {
            members.push("DIRECT".to_string());
        }
        for m in members {
            out.push_str(&format!("      - \"{}\"\n", escape_yaml(&m)));
        }
    }

    // 5. Rules & Rule Providers
    out.push_str(&generate_mihomo_rules_yaml_with_options(
        routing,
        "PROXY",
        settings.tun_mode,
    ));

    Ok(out)
}

fn compile_mihomo_proxy_yaml(
    node: &ProxyNode,
    yaml_name: &str,
    id_to_name: &HashMap<String, String>,
    valid_supported_ids: &HashSet<String>,
) -> Option<String> {
    let mut s = String::new();
    s.push_str(&format!("  - name: \"{}\"\n", escape_yaml(yaml_name)));

    match &node.settings {
        ProtocolConfig::Shadowsocks(ss) => {
            s.push_str("    type: ss\n");
            s.push_str(&format!("    server: \"{}\"\n", escape_yaml(&node.server)));
            s.push_str(&format!("    port: {}\n", node.port));
            s.push_str(&format!("    cipher: {}\n", ss.method));
            s.push_str(&format!(
                "    password: \"{}\"\n",
                escape_yaml(&ss.password)
            ));
            s.push_str("    udp: true\n");
            if let Some(plugin) = &ss.plugin {
                let mihomo_plugin = match plugin.trim() {
                    "obfs-local" | "simple-obfs" => "obfs",
                    other => other,
                };
                if !mihomo_plugin.is_empty() {
                    s.push_str(&format!("    plugin: {}\n", mihomo_plugin));
                    if let Some(opts) = &ss.plugin_opts {
                        let parsed_opts = parse_ss_plugin_opts_map(mihomo_plugin, opts);
                        if !parsed_opts.is_empty() {
                            s.push_str("    plugin-opts:\n");
                            for (k, v) in parsed_opts {
                                if v == "true" || v == "false" {
                                    s.push_str(&format!("      {k}: {v}\n"));
                                } else {
                                    s.push_str(&format!("      {k}: \"{}\"\n", escape_yaml(&v)));
                                }
                            }
                        }
                    }
                }
            }
        }
        ProtocolConfig::ShadowsocksR(ssr) => {
            s.push_str("    type: ssr\n");
            s.push_str(&format!("    server: \"{}\"\n", escape_yaml(&node.server)));
            s.push_str(&format!("    port: {}\n", node.port));
            s.push_str(&format!("    cipher: {}\n", ssr.method));
            s.push_str(&format!(
                "    password: \"{}\"\n",
                escape_yaml(&ssr.password)
            ));
            s.push_str(&format!("    protocol: {}\n", ssr.protocol));
            if let Some(pp) = &ssr.protocol_param {
                s.push_str(&format!("    protocol-param: \"{}\"\n", escape_yaml(pp)));
            }
            s.push_str(&format!("    obfs: {}\n", ssr.obfs));
            if let Some(op) = &ssr.obfs_param {
                s.push_str(&format!("    obfs-param: \"{}\"\n", escape_yaml(op)));
            }
            s.push_str("    udp: true\n");
        }
        ProtocolConfig::Vmess(vm) => {
            s.push_str("    type: vmess\n");
            s.push_str(&format!("    server: \"{}\"\n", escape_yaml(&node.server)));
            s.push_str(&format!("    port: {}\n", node.port));
            s.push_str(&format!("    uuid: {}\n", vm.uuid));
            s.push_str(&format!("    alterId: {}\n", vm.alter_id));
            s.push_str(&format!("    cipher: {}\n", vm.security));
            s.push_str("    udp: true\n");
            append_mihomo_tls_yaml(&mut s, node.tls.as_ref(), &node.server);
            append_mihomo_transport_yaml(&mut s, node.transport.as_ref());
        }
        ProtocolConfig::Vless(vl) => {
            s.push_str("    type: vless\n");
            s.push_str(&format!("    server: \"{}\"\n", escape_yaml(&node.server)));
            s.push_str(&format!("    port: {}\n", node.port));
            s.push_str(&format!("    uuid: {}\n", vl.uuid));
            if let Some(flow) = &vl.flow {
                if !flow.is_empty() {
                    s.push_str(&format!("    flow: {}\n", flow));
                }
            }
            s.push_str("    udp: true\n");
            append_mihomo_tls_yaml(&mut s, node.tls.as_ref(), &node.server);
            append_mihomo_transport_yaml(&mut s, node.transport.as_ref());
        }
        ProtocolConfig::Trojan(tr) => {
            s.push_str("    type: trojan\n");
            s.push_str(&format!("    server: \"{}\"\n", escape_yaml(&node.server)));
            s.push_str(&format!("    port: {}\n", node.port));
            s.push_str(&format!(
                "    password: \"{}\"\n",
                escape_yaml(&tr.password)
            ));
            s.push_str("    udp: true\n");
            append_mihomo_tls_fields(&mut s, node.tls.as_ref(), &node.server, true);
            append_mihomo_transport_yaml(&mut s, node.transport.as_ref());
        }
        ProtocolConfig::Hysteria(hy) => {
            s.push_str("    type: hysteria\n");
            s.push_str(&format!("    server: \"{}\"\n", escape_yaml(&node.server)));
            s.push_str(&format!("    port: {}\n", node.port));
            if let Some(auth) = &hy.auth_str {
                s.push_str(&format!("    auth-str: \"{}\"\n", escape_yaml(auth)));
            }
            if let Some(proto) = &hy.protocol {
                if !proto.is_empty() {
                    s.push_str(&format!("    protocol: {}\n", proto));
                }
            }
            s.push_str(&format!("    up: \"{} Mbps\"\n", hy.up_mbps));
            s.push_str(&format!("    down: \"{} Mbps\"\n", hy.down_mbps));
            if let Some(obfs) = &hy.obfs {
                if !obfs.is_empty() {
                    s.push_str(&format!("    obfs: \"{}\"\n", escape_yaml(obfs)));
                }
            }
            if let Some(rwc) = hy.recv_window_conn {
                s.push_str(&format!("    recv-window-conn: {}\n", rwc));
            }
            if let Some(rw) = hy.recv_window {
                s.push_str(&format!("    recv-window: {}\n", rw));
            }
            if let Some(dmd) = hy.disable_mtu_discovery {
                s.push_str(&format!("    disable_mtu_discovery: {}\n", dmd));
            }
            append_mihomo_tls_fields(&mut s, node.tls.as_ref(), &node.server, true);
        }
        ProtocolConfig::Hysteria2(hy2) => {
            s.push_str("    type: hysteria2\n");
            s.push_str(&format!("    server: \"{}\"\n", escape_yaml(&node.server)));
            s.push_str(&format!("    port: {}\n", node.port));
            if let Some(ports_raw) = &hy2.ports {
                let normalized = ports_raw
                    .split(',')
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                    .map(|p| p.replace(':', "-"))
                    .collect::<Vec<_>>()
                    .join(",");
                if !normalized.is_empty() {
                    s.push_str(&format!("    ports: \"{}\"\n", escape_yaml(&normalized)));
                }
            }
            if let Some(hop) = hy2.hop_interval {
                if hop > 0 {
                    s.push_str(&format!("    hop-interval: {}\n", hop));
                }
            }
            s.push_str(&format!(
                "    password: \"{}\"\n",
                escape_yaml(&hy2.password)
            ));
            if let Some(up) = hy2.up_mbps {
                s.push_str(&format!("    up: \"{} Mbps\"\n", up));
            }
            if let Some(down) = hy2.down_mbps {
                s.push_str(&format!("    down: \"{} Mbps\"\n", down));
            }
            if let (Some(ot), Some(op)) = (&hy2.obfs_type, &hy2.obfs_password) {
                s.push_str(&format!("    obfs: {}\n", ot));
                s.push_str(&format!("    obfs-password: \"{}\"\n", escape_yaml(op)));
            }
            append_mihomo_tls_fields(&mut s, node.tls.as_ref(), &node.server, true);
        }
        ProtocolConfig::Tuic(tuic) => {
            s.push_str("    type: tuic\n");
            s.push_str(&format!("    server: \"{}\"\n", escape_yaml(&node.server)));
            s.push_str(&format!("    port: {}\n", node.port));
            s.push_str(&format!("    uuid: {}\n", tuic.uuid));
            s.push_str(&format!(
                "    password: \"{}\"\n",
                escape_yaml(&tuic.password)
            ));
            if let Some(cc) = &tuic.congestion_control {
                s.push_str(&format!("    congestion-controller: {}\n", cc));
            }
            if let Some(mode) = &tuic.udp_relay_mode {
                s.push_str(&format!("    udp-relay-mode: {}\n", mode));
            }
            if let Some(hb) = &tuic.heartbeat {
                if let Ok(ms_or_s) = hb.trim().trim_end_matches('s').parse::<u64>() {
                    let ms = if hb.trim().ends_with('s') {
                        ms_or_s * 1000
                    } else {
                        ms_or_s
                    };
                    s.push_str(&format!("    heartbeat-interval: {}\n", ms));
                }
            }
            s.push_str(&format!(
                "    reduce-rtt: {}\n",
                tuic.zero_rtt_handshake.unwrap_or(false)
            ));
            append_mihomo_tls_fields(&mut s, node.tls.as_ref(), &node.server, true);
        }
        ProtocolConfig::WireGuard(wg) => {
            s.push_str("    type: wireguard\n");
            s.push_str(&format!("    server: \"{}\"\n", escape_yaml(&node.server)));
            s.push_str(&format!("    port: {}\n", node.port));
            let ipv4 = wg
                .local_address
                .iter()
                .find(|a| !a.contains(':'))
                .and_then(|a| a.split('/').next())
                .unwrap_or("10.0.0.2");
            s.push_str(&format!("    ip: {}\n", ipv4));
            if let Some(ipv6) = wg
                .local_address
                .iter()
                .find(|a| a.contains(':'))
                .and_then(|a| a.split('/').next())
            {
                s.push_str(&format!("    ipv6: {}\n", ipv6));
            }
            s.push_str(&format!("    private-key: {}\n", wg.private_key));
            s.push_str(&format!("    public-key: {}\n", wg.public_key));
            if let Some(psk) = &wg.preshared_key {
                s.push_str(&format!("    preshared-key: {}\n", psk));
            }
            if let Some(res) = wg.reserved {
                s.push_str(&format!("    reserved: {:?}\n", res));
            }
            if let Some(mtu) = wg.mtu {
                s.push_str(&format!("    mtu: {}\n", mtu));
            }
            s.push_str("    udp: true\n");
        }
        ProtocolConfig::AnyTls(atls) => {
            s.push_str("    type: anytls\n");
            s.push_str(&format!("    server: \"{}\"\n", escape_yaml(&node.server)));
            s.push_str(&format!("    port: {}\n", node.port));
            s.push_str(&format!(
                "    password: \"{}\"\n",
                escape_yaml(&atls.password)
            ));
            if let Some(chk) = &atls.idle_session_check_interval {
                if !chk.trim().is_empty() {
                    s.push_str(&format!(
                        "    idle-session-check-interval: {}\n",
                        chk.trim().trim_end_matches('s')
                    ));
                }
            }
            if let Some(timeout) = &atls.idle_session_timeout {
                if !timeout.trim().is_empty() {
                    s.push_str(&format!(
                        "    idle-session-timeout: {}\n",
                        timeout.trim().trim_end_matches('s')
                    ));
                }
            }
            if let Some(min_idle) = atls.min_idle_session {
                s.push_str(&format!("    min-idle-session: {}\n", min_idle));
            }
            append_mihomo_tls_fields(&mut s, node.tls.as_ref(), &node.server, true);
        }
        ProtocolConfig::Snell(snell) => {
            s.push_str("    type: snell\n");
            s.push_str(&format!("    server: \"{}\"\n", escape_yaml(&node.server)));
            s.push_str(&format!("    port: {}\n", node.port));
            s.push_str(&format!("    psk: \"{}\"\n", escape_yaml(&snell.psk)));
            s.push_str(&format!("    version: {}\n", snell.version));
            if let (Some(m), Some(h)) = (&snell.obfs_mode, &snell.obfs_host) {
                s.push_str("    obfs-opts:\n");
                s.push_str(&format!("      mode: {}\n", m));
                s.push_str(&format!("      host: \"{}\"\n", escape_yaml(h)));
            }
        }
        ProtocolConfig::Mieru(mieru) => {
            s.push_str("    type: mieru\n");
            s.push_str(&format!("    server: \"{}\"\n", escape_yaml(&node.server)));
            s.push_str(&format!("    port: {}\n", node.port));
            s.push_str(&format!("    transport: {}\n", mieru.transport));
            if let Some(pr) = &mieru.port_range {
                if !pr.is_empty() {
                    s.push_str(&format!("    port-range: \"{}\"\n", escape_yaml(pr)));
                }
            }
            if let Some(mux) = &mieru.multiplexing {
                if !mux.is_empty() {
                    s.push_str(&format!("    multiplexing: {}\n", mux));
                }
            }
            s.push_str(&format!(
                "    username: \"{}\"\n",
                escape_yaml(&mieru.username)
            ));
            s.push_str(&format!(
                "    password: \"{}\"\n",
                escape_yaml(&mieru.password)
            ));
        }
        ProtocolConfig::Ssh(ssh) => {
            s.push_str("    type: ssh\n");
            s.push_str(&format!("    server: \"{}\"\n", escape_yaml(&node.server)));
            s.push_str(&format!("    port: {}\n", node.port));
            s.push_str(&format!("    username: \"{}\"\n", escape_yaml(&ssh.user)));
            if let Some(p) = &ssh.password {
                s.push_str(&format!("    password: \"{}\"\n", escape_yaml(p)));
            }
            if let Some(pk) = &ssh.private_key {
                s.push_str(&format!("    private-key: \"{}\"\n", escape_yaml(pk)));
            }
            if let Some(pp) = &ssh.private_key_passphrase {
                s.push_str(&format!(
                    "    private-key-passphrase: \"{}\"\n",
                    escape_yaml(pp)
                ));
            }
            if !ssh.host_key.is_empty() {
                s.push_str(&format!("    host-key: {:?}\n", ssh.host_key));
            }
            if !ssh.host_key_algorithms.is_empty() {
                s.push_str(&format!(
                    "    host-key-algorithms: {:?}\n",
                    ssh.host_key_algorithms
                ));
            }
        }
        ProtocolConfig::Socks5(s5) => {
            s.push_str("    type: socks5\n");
            s.push_str(&format!("    server: \"{}\"\n", escape_yaml(&node.server)));
            s.push_str(&format!("    port: {}\n", node.port));
            if let (Some(u), Some(p)) = (&s5.username, &s5.password) {
                s.push_str(&format!("    username: \"{}\"\n", escape_yaml(u)));
                s.push_str(&format!("    password: \"{}\"\n", escape_yaml(p)));
            }
            s.push_str(&format!("    udp: {}\n", s5.udp));
            append_mihomo_tls_fields(&mut s, node.tls.as_ref(), &node.server, false);
        }
        ProtocolConfig::Http(http) => {
            s.push_str("    type: http\n");
            s.push_str(&format!("    server: \"{}\"\n", escape_yaml(&node.server)));
            s.push_str(&format!("    port: {}\n", node.port));
            if let (Some(u), Some(p)) = (&http.username, &http.password) {
                s.push_str(&format!("    username: \"{}\"\n", escape_yaml(u)));
                s.push_str(&format!("    password: \"{}\"\n", escape_yaml(p)));
            }
            append_mihomo_tls_fields(&mut s, node.tls.as_ref(), &node.server, false);
        }
        ProtocolConfig::Naive(naive) => {
            s.push_str("    type: http\n");
            s.push_str(&format!("    server: \"{}\"\n", escape_yaml(&node.server)));
            s.push_str(&format!("    port: {}\n", node.port));
            s.push_str(&format!(
                "    username: \"{}\"\n",
                escape_yaml(&naive.username)
            ));
            s.push_str(&format!(
                "    password: \"{}\"\n",
                escape_yaml(&naive.password)
            ));
            if let Some(tls) = &node.tls {
                append_mihomo_tls_fields(&mut s, Some(tls), &node.server, false);
            } else {
                s.push_str("    tls: true\n");
                s.push_str(&format!("    sni: \"{}\"\n", escape_yaml(&node.server)));
            }
            if !naive.extra_headers.is_empty() {
                s.push_str("    headers:\n");
                for (k, v) in &naive.extra_headers {
                    s.push_str(&format!("      {}: \"{}\"\n", k, escape_yaml(v)));
                }
            }
        }
        ProtocolConfig::ShadowTls(_) => {
            // Mihomo doesn't support standalone raw shadow-tls outbound
            return None;
        }
    }

    if let Some(dialer) = &node.dialer_proxy {
        let trimmed = dialer.trim();
        if !trimmed.is_empty() && trimmed != node.id && trimmed != yaml_name {
            if let Some(resolved_name) = valid_supported_ids
                .contains(trimmed)
                .then(|| id_to_name.get(trimmed))
                .flatten()
                .cloned()
                .or_else(|| {
                    valid_supported_ids
                        .iter()
                        .filter_map(|id| id_to_name.get(id))
                        .find(|name| name.as_str() == trimmed)
                        .cloned()
                })
            {
                s.push_str(&format!(
                    "    dialer-proxy: \"{}\"\n",
                    escape_yaml(&resolved_name)
                ));
            }
        }
    }

    Some(s)
}

fn append_mihomo_tls_yaml(s: &mut String, tls: Option<&TlsConfig>, default_sni: &str) {
    append_mihomo_tls_fields(s, tls, default_sni, false);
}

fn append_mihomo_tls_fields(
    s: &mut String,
    tls: Option<&TlsConfig>,
    default_sni: &str,
    implicit_tls: bool,
) {
    let Some(tls) = tls else {
        if implicit_tls {
            s.push_str(&format!("    sni: \"{}\"\n", escape_yaml(default_sni)));
        }
        return;
    };
    if !tls.enabled && !implicit_tls {
        return;
    }
    let sni = tls.server_name.as_deref().unwrap_or(default_sni);
    if implicit_tls {
        s.push_str(&format!("    sni: \"{}\"\n", escape_yaml(sni)));
    } else {
        s.push_str("    tls: true\n");
        s.push_str(&format!("    servername: \"{}\"\n", escape_yaml(sni)));
    }
    if tls.insecure {
        s.push_str("    skip-cert-verify: true\n");
    }
    if !tls.alpn.is_empty() {
        s.push_str(&format!("    alpn: {:?}\n", tls.alpn));
    }
    if let Some(fp) = &tls.fingerprint {
        s.push_str(&format!("    client-fingerprint: {}\n", fp));
    }
    if let Some(RealityConfig {
        public_key,
        short_id,
        ..
    }) = &tls.reality
    {
        s.push_str("    reality-opts:\n");
        s.push_str(&format!(
            "      public-key: \"{}\"\n",
            escape_yaml(public_key)
        ));
        s.push_str(&format!("      short-id: \"{}\"\n", escape_yaml(short_id)));
    }
}

fn append_mihomo_transport_yaml(s: &mut String, transport: Option<&TransportConfig>) {
    let Some(transport) = transport else { return };
    match transport {
        TransportConfig::Ws {
            path,
            host,
            headers,
            max_early_data,
            early_data_header_name,
        } => {
            s.push_str("    network: ws\n");
            s.push_str("    ws-opts:\n");
            s.push_str(&format!("      path: \"{}\"\n", escape_yaml(path)));
            let mut merged_headers = headers.clone();
            if let Some(h) = host {
                merged_headers.insert("Host".to_string(), h.clone());
            }
            if !merged_headers.is_empty() {
                s.push_str("      headers:\n");
                for (k, v) in merged_headers {
                    s.push_str(&format!("        {}: \"{}\"\n", k, escape_yaml(&v)));
                }
            }
            if let Some(ed) = max_early_data {
                s.push_str(&format!("      max-early-data: {ed}\n"));
            }
            if let Some(eh) = early_data_header_name {
                if !eh.trim().is_empty() {
                    s.push_str(&format!(
                        "      early-data-header-name: \"{}\"\n",
                        escape_yaml(eh.trim())
                    ));
                }
            }
        }
        TransportConfig::Grpc { service_name, .. } => {
            s.push_str("    network: grpc\n");
            s.push_str("    grpc-opts:\n");
            s.push_str(&format!(
                "      grpc-service-name: \"{}\"\n",
                escape_yaml(service_name)
            ));
        }
        TransportConfig::Http { host, path, .. } => {
            s.push_str("    network: http\n");
            s.push_str("    http-opts:\n");
            s.push_str(&format!("      path: [\"{}\"]\n", escape_yaml(path)));
            if !host.is_empty() {
                s.push_str(&format!("      headers:\n        Host: {:?}\n", host));
            }
        }
        TransportConfig::HttpUpgrade { host, path, .. } => {
            s.push_str("    network: httpupgrade\n");
            s.push_str("    httpupgrade-opts:\n");
            s.push_str(&format!("      path: \"{}\"\n", escape_yaml(path)));
            if let Some(h) = host {
                s.push_str(&format!("      host: \"{}\"\n", escape_yaml(h)));
            }
        }
        TransportConfig::Tcp {
            header_type,
            http_host,
            http_path,
        } if header_type
            .as_deref()
            .is_some_and(|t| t.eq_ignore_ascii_case("http")) =>
        {
            s.push_str("    network: http\n");
            s.push_str("    http-opts:\n");
            s.push_str(&format!(
                "      path: [\"{}\"]\n",
                escape_yaml(http_path.as_deref().unwrap_or("/"))
            ));
            if !http_host.is_empty() {
                s.push_str(&format!("      headers:\n        Host: {:?}\n", http_host));
            }
        }
        _ => {}
    }
}

fn parse_ss_plugin_opts_map(plugin: &str, raw_opts: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for pair in raw_opts.split(';') {
        let trimmed = pair.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (raw_k, raw_v) = match trimmed.split_once('=') {
            Some((k, v)) => (k.trim(), v.trim()),
            None if trimmed.eq_ignore_ascii_case("tls") && plugin == "v2ray-plugin" => {
                ("tls", "true")
            }
            None => continue,
        };
        let key = match (plugin, raw_k) {
            ("obfs", "obfs") => "mode",
            ("obfs", "obfs-host") => "host",
            _ => raw_k,
        };
        if !key.is_empty() && !raw_v.is_empty() {
            out.push((key.to_string(), raw_v.to_string()));
        }
    }
    out
}

fn escape_yaml(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
        .replace('\0', "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::node::{ShadowsocksConfig, ShadowsocksRConfig};

    #[test]
    fn test_mihomo_config_generation() {
        let node_ss = ProxyNode {
            id: "node-ss".to_string(),
            name: "SS Node".to_string(),
            server: "example.invalid".to_string(),
            port: 8388,
            protocol: crate::model::node::ProtocolType::Shadowsocks,
            tls: None,
            transport: None,
            settings: ProtocolConfig::Shadowsocks(ShadowsocksConfig {
                method: "2022-blake3-aes-128-gcm".to_string(),
                password: "password123".to_string(),
                plugin: None,
                plugin_opts: None,
                uot: Some(true),
            }),
            subscription_id: None,
            group_id: None,
            latency: None,
            dialer_proxy: None,
            preferred_core: None,
            sort_order: 0,
        };

        let node_ssr = ProxyNode {
            id: "node-ssr".to_string(),
            name: "SSR Node".to_string(),
            server: "example.invalid".to_string(),
            port: 8389,
            protocol: crate::model::node::ProtocolType::ShadowsocksR,
            tls: None,
            transport: None,
            settings: ProtocolConfig::ShadowsocksR(ShadowsocksRConfig {
                method: "aes-256-cfb".to_string(),
                password: "password123".to_string(),
                protocol: "auth_aes128_md5".to_string(),
                protocol_param: None,
                obfs: "tls1.2_ticket_auth".to_string(),
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
        let routing = RoutingSettings::default();

        let cfg = generate_mihomo_config(
            &[node_ss, node_ssr],
            &[],
            &settings,
            &dns,
            &routing,
            Some("node-ss"),
        )
        .unwrap();

        assert!(cfg.contains("mixed-port: 7890"));
        assert!(cfg.contains("name: \"SS Node\""));
        assert!(cfg.contains("name: \"SSR Node\""));
        assert!(cfg.contains("name: PROXY"));
    }
}

//! Mihomo (Clash.Meta) YAML configuration generator

use crate::generator::dns::generate_mihomo_dns_yaml;
use crate::generator::routing::generate_mihomo_rules_yaml;
use crate::model::node::{ProtocolConfig, ProxyNode, RealityConfig, TlsConfig, TransportConfig};
use crate::model::routing::{DnsSettings, RoutingSettings};
use crate::model::settings::AppSettings;
use crate::model::subscription::{GroupType, NodeGroup};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MihomoGeneratorError {
    #[error("Node {0} has an invalid configuration for mihomo: {1}")]
    InvalidNodeConfig(String, String),
    #[error("YAML formatting error: {0}")]
    Format(String),
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
    }

    // 2. DNS section
    out.push_str(&generate_mihomo_dns_yaml(dns));

    // 3. Proxies section
    out.push_str("proxies:\n");
    let mut valid_proxy_names = Vec::new();

    for node in nodes {
        if let Some(yaml_block) = compile_mihomo_proxy_yaml(node) {
            out.push_str(&yaml_block);
            valid_proxy_names.push(node.name.clone());
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

    let mut auto_candidates = valid_proxy_names.clone();
    if auto_candidates.is_empty() {
        auto_candidates.push("DIRECT".to_string());
    }

    // Top-level PROXY group
    out.push_str("  - name: PROXY\n");
    out.push_str("    type: select\n");
    out.push_str("    proxies:\n");

    if let Some(active_id) = active_node_id {
        if let Some(active_node) = nodes.iter().find(|n| n.id == active_id) {
            out.push_str(&format!("      - \"{}\"\n", escape_yaml(&active_node.name)));
        }
    }
    out.push_str("      - AUTO\n");
    out.push_str("      - FALLBACK\n");
    for g in groups {
        out.push_str(&format!("      - \"{}\"\n", escape_yaml(&g.name)));
    }
    for name in &valid_proxy_names {
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
    for g in groups {
        let g_type = match g.group_type {
            GroupType::Selector => "select",
            GroupType::UrlTest => "url-test",
            GroupType::Fallback => "fallback",
            GroupType::LoadBalance => "load-balance",
        };
        out.push_str(&format!("  - name: \"{}\"\n", escape_yaml(&g.name)));
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
            if let Some(target) = nodes.iter().find(|n| &n.id == node_id) {
                members.push(target.name.clone());
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
    out.push_str(&generate_mihomo_rules_yaml(routing, "PROXY"));

    Ok(out)
}

fn compile_mihomo_proxy_yaml(node: &ProxyNode) -> Option<String> {
    let mut s = String::new();
    s.push_str(&format!("  - name: \"{}\"\n", escape_yaml(&node.name)));

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
                s.push_str(&format!("    plugin: {}\n", plugin));
                if let Some(opts) = &ss.plugin_opts {
                    s.push_str(&format!("    plugin-opts: {}\n", opts));
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
            let sni = node
                .tls
                .as_ref()
                .and_then(|t| t.server_name.as_deref())
                .unwrap_or(&node.server);
            s.push_str(&format!("    sni: \"{}\"\n", escape_yaml(sni)));
            append_mihomo_transport_yaml(&mut s, node.transport.as_ref());
        }
        ProtocolConfig::Hysteria(hy) => {
            s.push_str("    type: hysteria\n");
            s.push_str(&format!("    server: \"{}\"\n", escape_yaml(&node.server)));
            s.push_str(&format!("    port: {}\n", node.port));
            if let Some(auth) = &hy.auth_str {
                s.push_str(&format!("    auth-str: \"{}\"\n", escape_yaml(auth)));
            }
            s.push_str(&format!("    up: \"{} Mbps\"\n", hy.up_mbps));
            s.push_str(&format!("    down: \"{} Mbps\"\n", hy.down_mbps));
            let sni = node
                .tls
                .as_ref()
                .and_then(|t| t.server_name.as_deref())
                .unwrap_or(&node.server);
            s.push_str(&format!("    sni: \"{}\"\n", escape_yaml(sni)));
        }
        ProtocolConfig::Hysteria2(hy2) => {
            s.push_str("    type: hysteria2\n");
            s.push_str(&format!("    server: \"{}\"\n", escape_yaml(&node.server)));
            s.push_str(&format!("    port: {}\n", node.port));
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
            let sni = node
                .tls
                .as_ref()
                .and_then(|t| t.server_name.as_deref())
                .unwrap_or(&node.server);
            s.push_str(&format!("    sni: \"{}\"\n", escape_yaml(sni)));
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
            let sni = node
                .tls
                .as_ref()
                .and_then(|t| t.server_name.as_deref())
                .unwrap_or(&node.server);
            s.push_str(&format!("    sni: \"{}\"\n", escape_yaml(sni)));
            s.push_str("    reduce-rtt: true\n");
        }
        ProtocolConfig::WireGuard(wg) => {
            s.push_str("    type: wireguard\n");
            s.push_str(&format!("    server: \"{}\"\n", escape_yaml(&node.server)));
            s.push_str(&format!("    port: {}\n", node.port));
            let ip = wg
                .local_address
                .first()
                .and_then(|a| a.split('/').next())
                .unwrap_or("10.0.0.2");
            s.push_str(&format!("    ip: {}\n", ip));
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
            let sni = node
                .tls
                .as_ref()
                .and_then(|t| t.server_name.as_deref())
                .unwrap_or(&node.server);
            s.push_str(&format!("    sni: \"{}\"\n", escape_yaml(sni)));
            s.push_str("    client-fingerprint: chrome\n");
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
        }
        ProtocolConfig::Socks5(s5) => {
            s.push_str("    type: socks5\n");
            s.push_str(&format!("    server: \"{}\"\n", escape_yaml(&node.server)));
            s.push_str(&format!("    port: {}\n", node.port));
            if let (Some(u), Some(p)) = (&s5.username, &s5.password) {
                s.push_str(&format!("    username: \"{}\"\n", escape_yaml(u)));
                s.push_str(&format!("    password: \"{}\"\n", escape_yaml(p)));
            }
            s.push_str("    udp: true\n");
        }
        ProtocolConfig::Http(http) => {
            s.push_str("    type: http\n");
            s.push_str(&format!("    server: \"{}\"\n", escape_yaml(&node.server)));
            s.push_str(&format!("    port: {}\n", node.port));
            if let (Some(u), Some(p)) = (&http.username, &http.password) {
                s.push_str(&format!("    username: \"{}\"\n", escape_yaml(u)));
                s.push_str(&format!("    password: \"{}\"\n", escape_yaml(p)));
            }
            if let Some(tls) = &node.tls {
                if tls.enabled {
                    s.push_str("    tls: true\n");
                    if let Some(sni) = &tls.server_name {
                        s.push_str(&format!("    sni: \"{}\"\n", escape_yaml(sni)));
                    }
                }
            }
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
            s.push_str("    tls: true\n");
            s.push_str(&format!("    sni: \"{}\"\n", escape_yaml(&node.server)));
        }
        ProtocolConfig::ShadowTls(_) => {
            // Mihomo doesn't support raw shadow-tls outbound, return None or fallback
            return None;
        }
    }

    if let Some(dialer) = &node.dialer_proxy {
        if !dialer.is_empty() {
            s.push_str(&format!("    dialer-proxy: \"{}\"\n", escape_yaml(dialer)));
        }
    }

    Some(s)
}

fn append_mihomo_tls_yaml(s: &mut String, tls: Option<&TlsConfig>, default_sni: &str) {
    let Some(tls) = tls else { return };
    if !tls.enabled {
        return;
    }
    s.push_str("    tls: true\n");
    let sni = tls.server_name.as_deref().unwrap_or(default_sni);
    s.push_str(&format!("    servername: \"{}\"\n", escape_yaml(sni)));
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
        TransportConfig::Ws { path, host, .. } => {
            s.push_str("    network: ws\n");
            s.push_str("    ws-opts:\n");
            s.push_str(&format!("      path: \"{}\"\n", escape_yaml(path)));
            if let Some(h) = host {
                s.push_str("      headers:\n");
                s.push_str(&format!("        Host: \"{}\"\n", escape_yaml(h)));
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
        _ => {}
    }
}

fn escape_yaml(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
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

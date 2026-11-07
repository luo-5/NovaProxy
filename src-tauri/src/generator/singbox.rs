//! sing-box 1.14 configuration generator

use crate::generator::dns::generate_singbox_dns;
use crate::generator::routing::generate_singbox_route;
use crate::model::node::{ProtocolConfig, ProxyNode, RealityConfig, TlsConfig, TransportConfig};
use crate::model::routing::{DnsSettings, RoutingSettings};
use crate::model::settings::AppSettings;
use crate::model::subscription::{GroupType, NodeGroup};
use serde_json::{json, Map, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SingBoxGeneratorError {
    #[error("Node {0} has an invalid configuration for sing-box: {1}")]
    InvalidNodeConfig(String, String),
    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn generate_singbox_config(
    nodes: &[ProxyNode],
    groups: &[NodeGroup],
    settings: &AppSettings,
    dns: &DnsSettings,
    routing: &RoutingSettings,
    active_node_id: Option<&str>,
) -> Result<String, SingBoxGeneratorError> {
    let mut root = Map::new();

    // 1. Log section
    root.insert(
        "log".to_string(),
        json!({
            "level": settings.log_level.as_str(),
            "timestamp": true
        }),
    );

    // 2. Experimental / Clash API section
    let mut clash_api = json!({
        "external_controller": format!("127.0.0.1:{}", settings.clash_api_port)
    });
    if let Some(secret) = &settings.clash_api_secret {
        if !secret.is_empty() {
            clash_api["secret"] = Value::String(secret.clone());
        }
    }
    root.insert(
        "experimental".to_string(),
        json!({
            "clash_api": clash_api
        }),
    );

    // 3. DNS section
    root.insert("dns".to_string(), generate_singbox_dns(dns));

    // 4. Inbounds section
    let listen_host = if settings.allow_lan {
        "0.0.0.0"
    } else {
        "127.0.0.1"
    };

    let mut inbounds = Vec::new();

    if settings.tun_mode {
        inbounds.push(json!({
            "type": "tun",
            "tag": "tun-in",
            "interface_name": "NovaProxy-TUN",
            "address": ["172.19.0.1/30"],
            "auto_route": true,
            "strict_route": true,
            "stack": "system"
        }));
    }

    inbounds.push(json!({
        "type": "mixed",
        "tag": "mixed-in",
        "listen": listen_host,
        "listen_port": settings.mixed_port
    }));

    if let Some(sp) = settings.socks_port {
        if sp > 0 && sp != settings.mixed_port {
            inbounds.push(json!({
                "type": "socks",
                "tag": "socks-in",
                "listen": listen_host,
                "listen_port": sp
            }));
        }
    }

    if let Some(hp) = settings.http_port {
        if hp > 0 && hp != settings.mixed_port {
            inbounds.push(json!({
                "type": "http",
                "tag": "http-in",
                "listen": listen_host,
                "listen_port": hp
            }));
        }
    }
    root.insert("inbounds".to_string(), Value::Array(inbounds));

    // 5. Endpoints & Outbounds sections
    let mut endpoints = Vec::new();
    let mut outbounds = Vec::new();
    let mut all_proxy_tags = Vec::new();

    // First collect wireguard endpoints vs standard outbounds
    for node in nodes {
        if let ProtocolConfig::WireGuard(wg) = &node.settings {
            let mut peers = Vec::new();
            let mut peer_obj = json!({
                "address": node.server,
                "port": node.port,
                "public_key": wg.public_key,
                "allowed_ips": ["0.0.0.0/0", "::/0"]
            });
            if let Some(psk) = &wg.preshared_key {
                peer_obj["preshared_key"] = Value::String(psk.clone());
            }
            if let Some(res) = wg.reserved {
                peer_obj["reserved"] = json!(res);
            }
            peers.push(peer_obj);

            let ep = json!({
                "type": "wireguard",
                "tag": node.id,
                "address": wg.local_address,
                "private_key": wg.private_key,
                "peers": peers,
                "mtu": wg.mtu.unwrap_or(1408)
            });
            endpoints.push(ep);
            all_proxy_tags.push(node.id.clone());
        } else {
            let (node_outbounds, primary_tag) = compile_singbox_outbound(node);
            for ob in node_outbounds {
                outbounds.push(ob);
            }
            all_proxy_tags.push(primary_tag);
        }
    }

    if !endpoints.is_empty() {
        root.insert("endpoints".to_string(), Value::Array(endpoints));
    }

    // Proxy groups
    let mut group_tags = Vec::new();

    // Auto urltest group
    let auto_proxies = if all_proxy_tags.is_empty() {
        vec!["direct".to_string()]
    } else {
        all_proxy_tags.clone()
    };
    outbounds.push(json!({
        "type": "urltest",
        "tag": "auto",
        "outbounds": auto_proxies,
        "url": "https://www.gstatic.com/generate_204",
        "interval": "5m"
    }));
    group_tags.push("auto".to_string());

    // User-defined groups
    for g in groups {
        let members: Vec<String> = g
            .node_ids
            .iter()
            .filter(|id| all_proxy_tags.contains(id))
            .cloned()
            .collect();
        let target_members = if members.is_empty() {
            vec!["direct".to_string()]
        } else {
            members
        };

        let g_type = match g.group_type {
            GroupType::Selector => "selector",
            GroupType::UrlTest => "urltest",
            GroupType::Fallback => "urltest", // sing-box urltest acts with fallback
            GroupType::LoadBalance => "urltest",
        };

        let mut g_obj = json!({
            "type": g_type,
            "tag": g.id,
            "outbounds": target_members
        });

        if g.group_type != GroupType::Selector {
            g_obj["url"] = json!(g
                .test_url
                .as_deref()
                .unwrap_or("https://www.gstatic.com/generate_204"));
            let int_secs = g.interval_secs.unwrap_or(300);
            g_obj["interval"] = json!(format!("{}s", int_secs));
        }

        outbounds.push(g_obj);
        group_tags.push(g.id.clone());
    }

    // Top-level "proxy" selector group
    let mut main_selector_outbounds = Vec::new();
    // Prioritize active node or groups
    if let Some(active_id) = active_node_id {
        if all_proxy_tags.contains(&active_id.to_string())
            || group_tags.contains(&active_id.to_string())
        {
            main_selector_outbounds.push(active_id.to_string());
        }
    }
    for gt in &group_tags {
        if !main_selector_outbounds.contains(gt) {
            main_selector_outbounds.push(gt.clone());
        }
    }
    for pt in &all_proxy_tags {
        if !main_selector_outbounds.contains(pt) {
            main_selector_outbounds.push(pt.clone());
        }
    }
    main_selector_outbounds.push("direct".to_string());

    let mut final_outbounds = Vec::new();
    final_outbounds.push(json!({
        "type": "selector",
        "tag": "proxy",
        "outbounds": main_selector_outbounds
    }));
    final_outbounds.extend(outbounds);

    // Standard built-ins
    final_outbounds.push(json!({
        "type": "direct",
        "tag": "direct"
    }));
    final_outbounds.push(json!({
        "type": "block",
        "tag": "block"
    }));

    root.insert("outbounds".to_string(), Value::Array(final_outbounds));

    // 6. Route section
    root.insert(
        "route".to_string(),
        generate_singbox_route(routing, "proxy"),
    );

    let formatted = serde_json::to_string_pretty(&root)?;
    Ok(formatted)
}

fn compile_singbox_outbound(node: &ProxyNode) -> (Vec<Value>, String) {
    let mut list = Vec::new();
    let tag = node.id.clone();

    match &node.settings {
        ProtocolConfig::Shadowsocks(ss) => {
            let mut ob = json!({
                "type": "shadowsocks",
                "tag": tag,
                "server": node.server,
                "server_port": node.port,
                "method": ss.method,
                "password": ss.password
            });
            if let Some(plugin) = &ss.plugin {
                ob["plugin"] = json!(plugin);
                if let Some(opts) = &ss.plugin_opts {
                    ob["plugin_opts"] = json!(opts);
                }
            }
            apply_detour_if_any(&mut ob, node);
            list.push(ob);
        }
        ProtocolConfig::ShadowsocksR(_) => {
            // sing-box lacks native SSR; map to local socks placeholder if dialed or direct
            let mut ob = json!({
                "type": "direct",
                "tag": tag
            });
            apply_detour_if_any(&mut ob, node);
            list.push(ob);
        }
        ProtocolConfig::Vmess(vm) => {
            let mut ob = json!({
                "type": "vmess",
                "tag": tag,
                "server": node.server,
                "server_port": node.port,
                "uuid": vm.uuid,
                "security": vm.security,
                "alter_id": vm.alter_id
            });
            if let Some(tls) = &node.tls {
                if tls.enabled {
                    ob["tls"] = compile_singbox_tls(tls);
                }
            }
            if let Some(tr) = &node.transport {
                if let Some(tr_val) = compile_singbox_transport(tr) {
                    ob["transport"] = tr_val;
                }
            }
            apply_detour_if_any(&mut ob, node);
            list.push(ob);
        }
        ProtocolConfig::Vless(vl) => {
            let mut ob = json!({
                "type": "vless",
                "tag": tag,
                "server": node.server,
                "server_port": node.port,
                "uuid": vl.uuid
            });
            if let Some(flow) = &vl.flow {
                if !flow.is_empty() {
                    ob["flow"] = json!(flow);
                }
            }
            if let Some(tls) = &node.tls {
                if tls.enabled {
                    ob["tls"] = compile_singbox_tls(tls);
                }
            }
            if let Some(tr) = &node.transport {
                if let Some(tr_val) = compile_singbox_transport(tr) {
                    ob["transport"] = tr_val;
                }
            }
            apply_detour_if_any(&mut ob, node);
            list.push(ob);
        }
        ProtocolConfig::Trojan(tr) => {
            let mut ob = json!({
                "type": "trojan",
                "tag": tag,
                "server": node.server,
                "server_port": node.port,
                "password": tr.password
            });
            if let Some(tls) = &node.tls {
                if tls.enabled {
                    ob["tls"] = compile_singbox_tls(tls);
                }
            } else {
                ob["tls"] = json!({ "enabled": true, "server_name": node.server });
            }
            if let Some(transport) = &node.transport {
                if let Some(tr_val) = compile_singbox_transport(transport) {
                    ob["transport"] = tr_val;
                }
            }
            apply_detour_if_any(&mut ob, node);
            list.push(ob);
        }
        ProtocolConfig::Hysteria(hy) => {
            let mut ob = json!({
                "type": "hysteria",
                "tag": tag,
                "server": node.server,
                "server_port": node.port,
                "up_mbps": hy.up_mbps,
                "down_mbps": hy.down_mbps
            });
            if let Some(auth) = &hy.auth_str {
                ob["auth_str"] = json!(auth);
            }
            if let Some(obfs) = &hy.obfs {
                ob["obfs"] = json!(obfs);
            }
            if let Some(tls) = &node.tls {
                if tls.enabled {
                    ob["tls"] = compile_singbox_tls(tls);
                }
            } else {
                ob["tls"] = json!({ "enabled": true, "server_name": node.server });
            }
            apply_detour_if_any(&mut ob, node);
            list.push(ob);
        }
        ProtocolConfig::Hysteria2(hy2) => {
            let mut ob = json!({
                "type": "hysteria2",
                "tag": tag,
                "server": node.server,
                "server_port": node.port,
                "password": hy2.password
            });
            if let Some(up) = hy2.up_mbps {
                ob["up_mbps"] = json!(up);
            }
            if let Some(down) = hy2.down_mbps {
                ob["down_mbps"] = json!(down);
            }
            if let (Some(ot), Some(op)) = (&hy2.obfs_type, &hy2.obfs_password) {
                ob["obfs"] = json!({
                    "type": ot,
                    "password": op
                });
            }
            if let Some(tls) = &node.tls {
                if tls.enabled {
                    ob["tls"] = compile_singbox_tls(tls);
                }
            } else {
                ob["tls"] = json!({ "enabled": true, "server_name": node.server });
            }
            apply_detour_if_any(&mut ob, node);
            list.push(ob);
        }
        ProtocolConfig::Tuic(tuic) => {
            let mut ob = json!({
                "type": "tuic",
                "tag": tag,
                "server": node.server,
                "server_port": node.port,
                "uuid": tuic.uuid,
                "password": tuic.password
            });
            if let Some(cc) = &tuic.congestion_control {
                ob["congestion_control"] = json!(cc);
            }
            if let Some(mode) = &tuic.udp_relay_mode {
                ob["udp_relay_mode"] = json!(mode);
            }
            if let Some(tls) = &node.tls {
                if tls.enabled {
                    ob["tls"] = compile_singbox_tls(tls);
                }
            } else {
                ob["tls"] = json!({ "enabled": true, "server_name": node.server });
            }
            apply_detour_if_any(&mut ob, node);
            list.push(ob);
        }
        ProtocolConfig::AnyTls(atls) => {
            let mut ob = json!({
                "type": "anytls",
                "tag": tag,
                "server": node.server,
                "server_port": node.port,
                "password": atls.password
            });
            if let Some(tls) = &node.tls {
                if tls.enabled {
                    ob["tls"] = compile_singbox_tls(tls);
                }
            } else {
                ob["tls"] = json!({ "enabled": true, "server_name": node.server });
            }
            apply_detour_if_any(&mut ob, node);
            list.push(ob);
        }
        ProtocolConfig::ShadowTls(stls) => {
            let outer_tag = format!("{}-outer", tag);
            let mut outer_tls = json!({
                "enabled": true,
                "server_name": node.server
            });
            if let Some(tls) = &node.tls {
                if let Some(sni) = &tls.server_name {
                    outer_tls["server_name"] = json!(sni);
                }
            }
            let outer_ob = json!({
                "type": "shadowtls",
                "tag": outer_tag,
                "server": node.server,
                "server_port": node.port,
                "version": stls.version,
                "password": stls.password,
                "tls": outer_tls
            });
            list.push(outer_ob);

            let inner_method = stls
                .inner_method
                .as_deref()
                .unwrap_or("2022-blake3-aes-128-gcm");
            let inner_pass = stls.inner_password.as_deref().unwrap_or(&stls.password);

            let inner_ob = json!({
                "type": "shadowsocks",
                "tag": tag,
                "method": inner_method,
                "password": inner_pass,
                "detour": outer_tag
            });
            list.push(inner_ob);
        }
        ProtocolConfig::Naive(naive) => {
            // NaiveProxy is an HTTP/2 CONNECT proxy with TLS. Sing-box HTTP outbound handles it seamlessly without cronet!
            let mut ob = json!({
                "type": "http",
                "tag": tag,
                "server": node.server,
                "server_port": node.port,
                "username": naive.username,
                "password": naive.password,
                "tls": {
                    "enabled": true,
                    "server_name": node.server,
                    "alpn": ["h2", "http/1.1"],
                    "utls": {
                        "enabled": true,
                        "fingerprint": "chrome"
                    }
                }
            });
            apply_detour_if_any(&mut ob, node);
            list.push(ob);
        }
        ProtocolConfig::Ssh(ssh) => {
            let mut ob = json!({
                "type": "ssh",
                "tag": tag,
                "server": node.server,
                "server_port": node.port,
                "user": ssh.user
            });
            if let Some(p) = &ssh.password {
                ob["password"] = json!(p);
            }
            if let Some(pk) = &ssh.private_key {
                ob["private_key"] = json!(pk);
            }
            apply_detour_if_any(&mut ob, node);
            list.push(ob);
        }
        ProtocolConfig::Socks5(s5) => {
            let mut ob = json!({
                "type": "socks",
                "tag": tag,
                "server": node.server,
                "server_port": node.port,
                "version": "5"
            });
            if let (Some(u), Some(p)) = (&s5.username, &s5.password) {
                ob["username"] = json!(u);
                ob["password"] = json!(p);
            }
            apply_detour_if_any(&mut ob, node);
            list.push(ob);
        }
        ProtocolConfig::Http(http) => {
            let mut ob = json!({
                "type": "http",
                "tag": tag,
                "server": node.server,
                "server_port": node.port
            });
            if let (Some(u), Some(p)) = (&http.username, &http.password) {
                ob["username"] = json!(u);
                ob["password"] = json!(p);
            }
            if let Some(tls) = &node.tls {
                if tls.enabled {
                    ob["tls"] = compile_singbox_tls(tls);
                }
            }
            apply_detour_if_any(&mut ob, node);
            list.push(ob);
        }
        ProtocolConfig::Snell(_) | ProtocolConfig::Mieru(_) => {
            // These protocols are mihomo-exclusive; provide direct fallback in sing-box
            let mut ob = json!({
                "type": "direct",
                "tag": tag
            });
            apply_detour_if_any(&mut ob, node);
            list.push(ob);
        }
        ProtocolConfig::WireGuard(_) => {
            // Handled as endpoint
        }
    }

    (list, tag)
}

fn apply_detour_if_any(ob: &mut Value, node: &ProxyNode) {
    if let Some(dialer) = &node.dialer_proxy {
        if !dialer.is_empty() {
            ob["detour"] = json!(dialer);
        }
    }
}

fn compile_singbox_tls(tls: &TlsConfig) -> Value {
    let mut obj = json!({
        "enabled": true
    });
    if let Some(sni) = &tls.server_name {
        obj["server_name"] = json!(sni);
    }
    if tls.insecure {
        obj["insecure"] = json!(true);
    }
    if !tls.alpn.is_empty() {
        obj["alpn"] = json!(tls.alpn);
    }

    if let Some(fp) = &tls.fingerprint {
        obj["utls"] = json!({
            "enabled": true,
            "fingerprint": fp
        });
    }

    if let Some(RealityConfig {
        public_key,
        short_id,
        ..
    }) = &tls.reality
    {
        obj["reality"] = json!({
            "enabled": true,
            "public_key": public_key,
            "short_id": short_id
        });
    }

    obj
}

fn compile_singbox_transport(transport: &TransportConfig) -> Option<Value> {
    match transport {
        TransportConfig::Ws {
            path,
            host,
            headers,
            max_early_data,
            early_data_header_name,
        } => {
            let mut ws_obj = json!({
                "type": "ws",
                "path": path
            });
            let mut all_headers = headers.clone();
            if let Some(h) = host {
                all_headers.insert("Host".to_string(), h.clone());
            }
            if !all_headers.is_empty() {
                ws_obj["headers"] = json!(all_headers);
            }
            if let Some(ed) = max_early_data {
                ws_obj["max_early_data"] = json!(ed);
            }
            if let Some(ed_name) = early_data_header_name {
                ws_obj["early_data_header_name"] = json!(ed_name);
            }
            Some(ws_obj)
        }
        TransportConfig::Grpc {
            service_name,
            multi_mode,
            ..
        } => Some(json!({
            "type": "grpc",
            "service_name": service_name,
            "idle_timeout": "15s",
            "ping_timeout": "15s",
            "permit_without_stream": multi_mode
        })),
        TransportConfig::Http {
            host, path, method, ..
        } => {
            let mut h_obj = json!({
                "type": "http",
                "path": path
            });
            if !host.is_empty() {
                h_obj["host"] = json!(host);
            }
            if let Some(m) = method {
                h_obj["method"] = json!(m);
            }
            Some(h_obj)
        }
        TransportConfig::HttpUpgrade { host, path, .. } => {
            let mut up_obj = json!({
                "type": "httpupgrade",
                "path": path
            });
            if let Some(h) = host {
                up_obj["host"] = json!(h);
            }
            Some(up_obj)
        }
        TransportConfig::Quic { .. } => Some(json!({
            "type": "quic"
        })),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::node::{VlessConfig, WireGuardConfig};

    #[test]
    fn test_singbox_config_generation() {
        let node_vless = ProxyNode {
            id: "node-vless".to_string(),
            name: "VLESS Reality".to_string(),
            server: "example.invalid".to_string(),
            port: 443,
            protocol: crate::model::node::ProtocolType::Vless,
            tls: Some(TlsConfig {
                enabled: true,
                server_name: Some("example.invalid".to_string()),
                insecure: false,
                alpn: vec!["h2".to_string()],
                fingerprint: Some("chrome".to_string()),
                reality: Some(RealityConfig {
                    public_key: "dGVzdC1wdWJsaWMta2V5LTAwMDAwMDAwMDAwMDAwMDA".to_string(),
                    short_id: "01234567".to_string(),
                    spider_x: None,
                }),
                ech_config: None,
                fragment: None,
            }),
            transport: None,
            settings: ProtocolConfig::Vless(VlessConfig {
                uuid: "00000000-0000-0000-0000-000000000001".to_string(),
                flow: Some("xtls-rprx-vision".to_string()),
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

        let node_wg = ProxyNode {
            id: "node-wg".to_string(),
            name: "WireGuard".to_string(),
            server: "198.51.100.1".to_string(),
            port: 51820,
            protocol: crate::model::node::ProtocolType::WireGuard,
            tls: None,
            transport: None,
            settings: ProtocolConfig::WireGuard(WireGuardConfig {
                private_key: "YWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWE=".to_string(),
                public_key: "YmJiYmJiYmJiYmJiYmJiYmJiYmJiYmJiYmJiYmJiYmI=".to_string(),
                preshared_key: None,
                local_address: vec!["10.0.0.2/32".to_string()],
                reserved: None,
                mtu: Some(1408),
                workers: None,
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

        let cfg = generate_singbox_config(
            &[node_vless, node_wg],
            &[],
            &settings,
            &dns,
            &routing,
            Some("node-vless"),
        )
        .unwrap();

        assert!(cfg.contains("\"mixed-in\""));
        assert!(cfg.contains("\"node-vless\""));
        assert!(cfg.contains("\"endpoints\""));
        assert!(cfg.contains("\"node-wg\""));
    }
}

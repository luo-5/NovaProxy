//! Xray-core 26.10 configuration generator

use crate::generator::routing::generate_xray_routing;
use crate::model::node::{ProtocolConfig, ProxyNode, RealityConfig, TransportConfig};
use crate::model::routing::RoutingSettings;
use crate::model::settings::AppSettings;
use serde_json::{json, Map, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum XrayGeneratorError {
    #[error("Node {0} has an invalid configuration for Xray: {1}")]
    InvalidNodeConfig(String, String),
    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn generate_xray_config(
    nodes: &[ProxyNode],
    settings: &AppSettings,
    routing: &RoutingSettings,
    active_node_id: Option<&str>,
) -> Result<String, XrayGeneratorError> {
    let mut root = Map::new();

    // 1. Log section
    let log_level_str = match settings.log_level {
        crate::model::settings::LogLevel::Trace | crate::model::settings::LogLevel::Debug => {
            "debug"
        }
        crate::model::settings::LogLevel::Info => "info",
        crate::model::settings::LogLevel::Warn => "warning",
        crate::model::settings::LogLevel::Error => "error",
    };
    root.insert(
        "log".to_string(),
        json!({
            "loglevel": log_level_str
        }),
    );

    // 2. Inbounds section
    let listen_host = if settings.allow_lan {
        "0.0.0.0"
    } else {
        "127.0.0.1"
    };

    let mut inbounds = vec![json!({
        "tag": "mixed-in",
        "port": settings.mixed_port,
        "listen": listen_host,
        "protocol": "socks",
        "settings": {
            "auth": "noauth",
            "udp": true
        },
        "sniffing": {
            "enabled": true,
            "destOverride": ["http", "tls", "quic"]
        }
    })];

    if let Some(hp) = settings.http_port {
        if hp > 0 && hp != settings.mixed_port {
            inbounds.push(json!({
                "tag": "http-in",
                "port": hp,
                "listen": listen_host,
                "protocol": "http",
                "settings": {
                    "allowTransparent": false
                }
            }));
        }
    }
    root.insert("inbounds".to_string(), Value::Array(inbounds));

    // 3. Outbounds section
    let mut outbounds = Vec::new();

    // Determine target active node tag
    let primary_node = active_node_id
        .and_then(|id| nodes.iter().find(|n| n.id == id))
        .or_else(|| nodes.first());

    let mut primary_tag = primary_node.map(|n| n.id.as_str()).unwrap_or("direct");

    if let Some(target_node) = primary_node {
        if let Some(ob) = compile_xray_outbound(target_node, &target_node.id) {
            outbounds.push(ob);
        } else if routing.mode != crate::model::settings::RoutingMode::Direct {
            return Err(XrayGeneratorError::InvalidNodeConfig(
                target_node.name.clone(),
                format!(
                    "protocol '{}' is not supported natively by Xray",
                    target_node.protocol
                ),
            ));
        } else {
            primary_tag = "direct";
        }
    }

    // Add remaining nodes as alternate outbounds
    for node in nodes {
        if primary_node.map(|p| p.id.as_str()) != Some(&node.id) {
            if let Some(ob) = compile_xray_outbound(node, &node.id) {
                outbounds.push(ob);
            }
        }
    }

    // Direct and Block outbounds
    outbounds.push(json!({
        "tag": "direct",
        "protocol": "freedom",
        "settings": {}
    }));
    outbounds.push(json!({
        "tag": "block",
        "protocol": "blackhole",
        "settings": {}
    }));

    root.insert("outbounds".to_string(), Value::Array(outbounds));

    // 4. Routing section
    root.insert(
        "routing".to_string(),
        generate_xray_routing(routing, primary_tag),
    );

    let formatted = serde_json::to_string_pretty(&root)?;
    Ok(formatted)
}

fn compile_xray_outbound(node: &ProxyNode, tag: &str) -> Option<Value> {
    if matches!(
        node.transport_kind(),
        crate::model::node::TransportKind::Http | crate::model::node::TransportKind::Quic
    ) {
        return None;
    }

    match &node.settings {
        ProtocolConfig::Vless(vl) => {
            let mut user_obj = json!({
                "id": vl.uuid,
                "encryption": vl.encryption
            });
            if let Some(flow) = &vl.flow {
                if !flow.is_empty() {
                    user_obj["flow"] = json!(flow);
                }
            }

            let mut ob = json!({
                "tag": tag,
                "protocol": "vless",
                "settings": {
                    "vnext": [
                        {
                            "address": node.server,
                            "port": node.port,
                            "users": [ user_obj ]
                        }
                    ]
                }
            });

            ob["streamSettings"] = compile_xray_stream_settings(node);
            Some(ob)
        }
        ProtocolConfig::Vmess(vm) => {
            let user_obj = json!({
                "id": vm.uuid,
                "alterId": vm.alter_id,
                "security": vm.security
            });

            let mut ob = json!({
                "tag": tag,
                "protocol": "vmess",
                "settings": {
                    "vnext": [
                        {
                            "address": node.server,
                            "port": node.port,
                            "users": [ user_obj ]
                        }
                    ]
                }
            });

            ob["streamSettings"] = compile_xray_stream_settings(node);
            Some(ob)
        }
        ProtocolConfig::Trojan(tr) => {
            let mut ob = json!({
                "tag": tag,
                "protocol": "trojan",
                "settings": {
                    "servers": [
                        {
                            "address": node.server,
                            "port": node.port,
                            "password": tr.password
                        }
                    ]
                }
            });

            ob["streamSettings"] = compile_xray_stream_settings(node);
            Some(ob)
        }
        ProtocolConfig::Shadowsocks(ss) => {
            let ob = json!({
                "tag": tag,
                "protocol": "shadowsocks",
                "settings": {
                    "servers": [
                        {
                            "address": node.server,
                            "port": node.port,
                            "method": ss.method,
                            "password": ss.password,
                            "uot": ss.uot.unwrap_or(true)
                        }
                    ]
                }
            });
            Some(ob)
        }
        ProtocolConfig::WireGuard(wg) => {
            let mut peer_obj = json!({
                "publicKey": wg.public_key,
                "endpoint": format!("{}:{}", node.server, node.port)
            });
            if let Some(psk) = &wg.preshared_key {
                peer_obj["preSharedKey"] = json!(psk);
            }

            let ob = json!({
                "tag": tag,
                "protocol": "wireguard",
                "settings": {
                    "secretKey": wg.private_key,
                    "address": wg.local_address,
                    "peers": [ peer_obj ],
                    "mtu": wg.mtu.unwrap_or(1408)
                }
            });
            Some(ob)
        }
        ProtocolConfig::Socks5(s5) => {
            let mut server_obj = json!({
                "address": node.server,
                "port": node.port
            });
            if let (Some(u), Some(p)) = (&s5.username, &s5.password) {
                server_obj["users"] = json!([{ "user": u, "pass": p }]);
            }

            let ob = json!({
                "tag": tag,
                "protocol": "socks",
                "settings": {
                    "servers": [ server_obj ]
                }
            });
            Some(ob)
        }
        ProtocolConfig::Http(http) => {
            let mut server_obj = json!({
                "address": node.server,
                "port": node.port
            });
            if let (Some(u), Some(p)) = (&http.username, &http.password) {
                server_obj["users"] = json!([{ "user": u, "pass": p }]);
            }

            let mut ob = json!({
                "tag": tag,
                "protocol": "http",
                "settings": {
                    "servers": [ server_obj ]
                }
            });
            if node.tls.as_ref().is_some_and(|t| t.enabled) {
                ob["streamSettings"] = compile_xray_stream_settings(node);
            }
            Some(ob)
        }
        _ => None,
    }
}

fn compile_xray_stream_settings(node: &ProxyNode) -> Value {
    let mut stream = Map::new();

    let network_str = match node.transport_kind() {
        crate::model::node::TransportKind::Tcp => "tcp",
        crate::model::node::TransportKind::Ws => "ws",
        crate::model::node::TransportKind::Grpc => "grpc",
        crate::model::node::TransportKind::Http => "http",
        crate::model::node::TransportKind::HttpUpgrade => "httpupgrade",
        crate::model::node::TransportKind::Xhttp => "xhttp",
        crate::model::node::TransportKind::Mkcp => "mkcp",
        crate::model::node::TransportKind::Quic => "quic",
    };
    stream.insert("network".to_string(), json!(network_str));

    // Security & TLS / REALITY
    if let Some(tls) = &node.tls {
        if tls.enabled {
            if let Some(RealityConfig {
                public_key,
                short_id,
                spider_x,
            }) = &tls.reality
            {
                stream.insert("security".to_string(), json!("reality"));
                let sni = tls.server_name.as_deref().unwrap_or(&node.server);
                stream.insert(
                    "realitySettings".to_string(),
                    json!({
                        "show": false,
                        "fingerprint": tls.fingerprint.as_deref().unwrap_or("chrome"),
                        "serverName": sni,
                        "publicKey": public_key,
                        "shortId": short_id,
                        "spiderX": spider_x.as_deref().unwrap_or("")
                    }),
                );
            } else {
                stream.insert("security".to_string(), json!("tls"));
                let sni = tls.server_name.as_deref().unwrap_or(&node.server);
                let mut tls_settings = json!({
                    "serverName": sni,
                    "allowInsecure": tls.insecure
                });
                if !tls.alpn.is_empty() {
                    tls_settings["alpn"] = json!(tls.alpn);
                }
                if let Some(fp) = &tls.fingerprint {
                    tls_settings["fingerprint"] = json!(fp);
                }
                stream.insert("tlsSettings".to_string(), tls_settings);
            }
        } else {
            stream.insert("security".to_string(), json!("none"));
        }
    } else {
        stream.insert("security".to_string(), json!("none"));
    }

    // Transport specific settings
    if let Some(transport) = &node.transport {
        match transport {
            TransportConfig::Ws {
                path,
                host,
                headers,
                ..
            } => {
                let mut ws_headers = headers.clone();
                if let Some(h) = host {
                    ws_headers.insert("Host".to_string(), h.clone());
                }
                stream.insert(
                    "wsSettings".to_string(),
                    json!({
                        "path": path,
                        "headers": ws_headers
                    }),
                );
            }
            TransportConfig::Grpc {
                service_name,
                multi_mode,
                authority,
            } => {
                let mut grpc_obj = json!({
                    "serviceName": service_name,
                    "multiMode": multi_mode
                });
                if let Some(auth) = authority {
                    grpc_obj["authority"] = json!(auth);
                }
                stream.insert("grpcSettings".to_string(), grpc_obj);
            }
            TransportConfig::Xhttp {
                path,
                host,
                mode,
                extra,
            } => {
                let mut xhttp_obj = json!({
                    "path": path
                });
                if let Some(h) = host {
                    xhttp_obj["host"] = json!(h);
                }
                if let Some(m) = mode {
                    xhttp_obj["mode"] = json!(m);
                }
                if let Some(ex) = extra {
                    xhttp_obj["extra"] = json!(ex);
                }
                stream.insert("xhttpSettings".to_string(), xhttp_obj);
            }
            TransportConfig::Mkcp {
                seed,
                header_type,
                mtu,
                tti,
                uplink_capacity,
                downlink_capacity,
                congestion,
            } => {
                let mut mkcp_obj = json!({
                    "congestion": congestion
                });
                if let Some(s) = seed {
                    mkcp_obj["seed"] = json!(s);
                }
                if let Some(ht) = header_type {
                    mkcp_obj["header"] = json!({ "type": ht });
                }
                if let Some(m) = mtu {
                    mkcp_obj["mtu"] = json!(m);
                }
                if let Some(t) = tti {
                    mkcp_obj["tti"] = json!(t);
                }
                if let Some(up) = uplink_capacity {
                    mkcp_obj["uplinkCapacity"] = json!(up);
                }
                if let Some(down) = downlink_capacity {
                    mkcp_obj["downlinkCapacity"] = json!(down);
                }
                stream.insert("kcpSettings".to_string(), mkcp_obj);
            }
            TransportConfig::HttpUpgrade { host, path, .. } => {
                let mut up_obj = json!({
                    "path": path
                });
                if let Some(h) = host {
                    up_obj["host"] = json!(h);
                }
                stream.insert("httpupgradeSettings".to_string(), up_obj);
            }
            TransportConfig::Tcp {
                header_type,
                http_host,
                http_path,
            } if header_type
                .as_deref()
                .is_some_and(|t| t.eq_ignore_ascii_case("http")) =>
            {
                let mut req = Map::new();
                req.insert(
                    "path".to_string(),
                    json!([http_path.as_deref().unwrap_or("/")]),
                );
                if !http_host.is_empty() {
                    req.insert("headers".to_string(), json!({ "Host": http_host }));
                }
                stream.insert(
                    "tcpSettings".to_string(),
                    json!({
                        "header": {
                            "type": "http",
                            "request": Value::Object(req)
                        }
                    }),
                );
            }
            _ => {}
        }
    }

    Value::Object(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::node::{TlsConfig, VlessConfig};

    #[test]
    fn test_xray_config_generation() {
        let node_xhttp = ProxyNode {
            id: "node-xhttp".to_string(),
            name: "VLESS XHTTP Reality".to_string(),
            server: "example.invalid".to_string(),
            port: 443,
            protocol: crate::model::node::ProtocolType::Vless,
            tls: Some(TlsConfig {
                enabled: true,
                server_name: Some("example.invalid".to_string()),
                insecure: false,
                alpn: vec![],
                fingerprint: Some("chrome".to_string()),
                reality: Some(RealityConfig {
                    public_key: "dGVzdC1wdWJsaWMta2V5LTAwMDAwMDAwMDAwMDAwMDA".to_string(),
                    short_id: "01234567".to_string(),
                    spider_x: None,
                }),
                ech_config: None,
                fragment: None,
            }),
            transport: Some(TransportConfig::Xhttp {
                host: Some("example.invalid".to_string()),
                path: "/xhttp".to_string(),
                mode: Some("auto".to_string()),
                extra: None,
            }),
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

        let settings = AppSettings::default();
        let routing = RoutingSettings::default();

        let cfg =
            generate_xray_config(&[node_xhttp], &settings, &routing, Some("node-xhttp")).unwrap();

        assert!(cfg.contains("\"protocol\": \"vless\""));
        assert!(cfg.contains("\"network\": \"xhttp\""));
        assert!(cfg.contains("\"security\": \"reality\""));
        assert!(cfg.contains("\"xhttpSettings\""));
    }
}

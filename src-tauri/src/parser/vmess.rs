use crate::model::{
    ProtocolConfig, ProtocolType, ProxyNode, TlsConfig, TransportConfig, VmessConfig,
};
use crate::parser::base64::{decode_base64_to_string, encode_base64_standard};
use crate::parser::error::{ParseError, ParseResult};
use crate::parser::uri_common::{
    extract_remarks, extract_url_host_port, normalize_host, parse_host_port, parse_query_map,
    parse_raw_query_map, parse_v2ray_transport_and_tls,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use url::Url;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VmessV2Json {
    #[serde(default = "default_v2")]
    v: Value,
    #[serde(default)]
    ps: String,
    #[serde(default)]
    add: String,
    #[serde(default)]
    port: Value,
    #[serde(default)]
    id: String,
    #[serde(default)]
    aid: Value,
    #[serde(default)]
    scy: String,
    #[serde(default)]
    net: String,
    #[serde(default, rename = "type")]
    header_type: String,
    #[serde(default)]
    host: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    tls: String,
    #[serde(default)]
    sni: String,
    #[serde(default)]
    alpn: String,
    #[serde(default)]
    fp: String,
    #[serde(default, rename = "allowInsecure")]
    allow_insecure: Value,
}

fn default_v2() -> Value {
    Value::String("2".to_string())
}

fn value_to_u16(val: &Value) -> Option<u16> {
    match val {
        Value::Number(n) => n.as_u64().and_then(|v| u16::try_from(v).ok()),
        Value::String(s) => s.trim().parse::<u16>().ok(),
        _ => None,
    }
}

fn value_to_bool(val: &Value) -> bool {
    match val {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_i64().unwrap_or(0) != 0,
        Value::String(s) => matches!(s.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"),
        _ => false,
    }
}

/// Parses a `vmess://` share link into a `ProxyNode`
pub fn parse(uri: &str) -> ParseResult<ProxyNode> {
    let trimmed = uri.trim();
    let payload = trimmed
        .strip_prefix("vmess://")
        .or_else(|| trimmed.strip_prefix("VMESS://"))
        .ok_or_else(|| ParseError::UnsupportedScheme(trimmed.to_string()))?;

    if payload.is_empty() {
        return Err(ParseError::EmptyInput);
    }

    // 1. Try standard v2rayN Base64 JSON format first (if no '@' in raw payload before '?')
    let before_query = payload.split('?').next().unwrap_or(payload);
    let before_frag = before_query.split('#').next().unwrap_or(before_query);

    if !before_frag.contains('@') {
        if let Ok(decoded) = decode_base64_to_string(before_frag) {
            let decoded_trim = decoded.trim();
            if decoded_trim.starts_with('{') {
                return parse_vmess_json(decoded_trim);
            }
            // Shadowrocket format: vmess://base64(security:uuid@host:port)?remarks=...
            if decoded_trim.contains('@') {
                let query_part = payload.find('?').map(|idx| &payload[idx + 1..]);
                return parse_shadowrocket_vmess(decoded_trim, query_part);
            }
        }
    }

    // 2. Xray / standard URL-style vmess://uuid@host:port?type=ws&security=tls#name
    parse_url_style_vmess(trimmed)
}

fn parse_vmess_json(json_str: &str) -> ParseResult<ProxyNode> {
    let raw: VmessV2Json = serde_json::from_str(json_str)?;
    let server = normalize_host(&raw.add);
    if server.is_empty() {
        return Err(ParseError::MissingField("add (server)"));
    }
    let port = value_to_u16(&raw.port)
        .filter(|&p| p > 0)
        .ok_or_else(|| ParseError::InvalidPort(raw.port.to_string()))?;

    let uuid = raw.id.trim().to_string();
    if uuid.is_empty() {
        return Err(ParseError::MissingField("id (uuid)"));
    }

    let alter_id = value_to_u16(&raw.aid).unwrap_or(0);
    let security = if raw.scy.trim().is_empty() {
        "auto".to_string()
    } else {
        raw.scy.trim().to_string()
    };

    let name = if raw.ps.trim().is_empty() {
        format!("{server}:{port}")
    } else {
        raw.ps.trim().to_string()
    };

    let mut params = BTreeMap::new();
    let net = if raw.net.trim().is_empty() {
        "tcp".to_string()
    } else {
        raw.net.trim().to_ascii_lowercase()
    };
    params.insert("type".to_string(), net.clone());

    if !raw.host.trim().is_empty() {
        params.insert("host".to_string(), raw.host.trim().to_string());
    }
    if !raw.path.trim().is_empty() {
        if net == "grpc" {
            params.insert("serviceName".to_string(), raw.path.trim().to_string());
        } else if net == "kcp" || net == "mkcp" {
            params.insert("seed".to_string(), raw.path.trim().to_string());
        } else {
            params.insert("path".to_string(), raw.path.trim().to_string());
        }
    }
    if !raw.header_type.trim().is_empty() {
        if net == "grpc" {
            params.insert("mode".to_string(), raw.header_type.trim().to_string());
        } else {
            params.insert("headerType".to_string(), raw.header_type.trim().to_string());
        }
    }
    if !raw.tls.trim().is_empty() {
        params.insert("security".to_string(), raw.tls.trim().to_string());
    }
    let sni = if !raw.sni.trim().is_empty() {
        raw.sni.trim().to_string()
    } else if !raw.host.trim().is_empty() && !raw.host.contains(',') {
        raw.host.trim().to_string()
    } else {
        String::new()
    };
    if !sni.is_empty() {
        params.insert("sni".to_string(), sni);
    }
    if !raw.alpn.trim().is_empty() {
        params.insert("alpn".to_string(), raw.alpn.trim().to_string());
    }
    if !raw.fp.trim().is_empty() {
        params.insert("fp".to_string(), raw.fp.trim().to_string());
    }
    if value_to_bool(&raw.allow_insecure) {
        params.insert("allowInsecure".to_string(), "1".to_string());
    }

    let (transport, tls) = parse_v2ray_transport_and_tls(&params, false);

    Ok(ProxyNode {
        id: Uuid::new_v4().to_string(),
        name,
        server,
        port,
        protocol: ProtocolType::Vmess,
        tls,
        transport,
        settings: ProtocolConfig::Vmess(VmessConfig {
            uuid,
            alter_id,
            security,
            global_padding: None,
            authenticated_length: None,
            packet_encoding: None,
        }),
        subscription_id: None,
        group_id: None,
        latency: None,
        dialer_proxy: None,
        preferred_core: None,
        sort_order: 0,
    })
}

fn parse_shadowrocket_vmess(decoded: &str, query_part: Option<&str>) -> ParseResult<ProxyNode> {
    // Format: method:uuid@host:port
    let (userinfo, host_port) = decoded
        .rsplit_once('@')
        .ok_or_else(|| ParseError::InvalidUri("missing '@' in shadowrocket vmess".to_string()))?;
    let (security, uuid) = match userinfo.split_once(':') {
        Some((m, u)) => (m.trim().to_string(), u.trim().to_string()),
        None => ("auto".to_string(), userinfo.trim().to_string()),
    };
    if uuid.is_empty() {
        return Err(ParseError::MissingField("uuid"));
    }
    let (server, port) = parse_host_port(host_port)?;

    let params = query_part.map(parse_raw_query_map).unwrap_or_default();
    let name = params
        .get("remarks")
        .cloned()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{server}:{port}"));

    let obfs = params
        .get("obfs")
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    let transport = if obfs == "websocket" || obfs == "ws" {
        let host = params.get("obfsParam").cloned().filter(|s| !s.is_empty());
        let path = params
            .get("path")
            .cloned()
            .unwrap_or_else(|| "/".to_string());
        let mut headers = BTreeMap::new();
        if let Some(ref h) = host {
            headers.insert("Host".to_string(), h.clone());
        }
        Some(TransportConfig::Ws {
            path,
            host,
            headers,
            max_early_data: None,
            early_data_header_name: None,
        })
    } else {
        None
    };

    let tls_enabled = matches!(params.get("tls").map(|s| s.as_str()), Some("1" | "true"));
    let tls = if tls_enabled {
        Some(TlsConfig {
            enabled: true,
            server_name: params
                .get("peer")
                .or_else(|| params.get("sni"))
                .cloned()
                .filter(|s| !s.is_empty()),
            insecure: matches!(
                params.get("allowInsecure").map(|s| s.as_str()),
                Some("1" | "true")
            ),
            alpn: Vec::new(),
            fingerprint: None,
            reality: None,
            ech_config: None,
            fragment: None,
        })
    } else {
        None
    };

    Ok(ProxyNode {
        id: Uuid::new_v4().to_string(),
        name,
        server,
        port,
        protocol: ProtocolType::Vmess,
        tls,
        transport,
        settings: ProtocolConfig::Vmess(VmessConfig {
            uuid,
            alter_id: params
                .get("alterId")
                .and_then(|v| v.parse::<u16>().ok())
                .unwrap_or(0),
            security: if security.is_empty() {
                "auto".to_string()
            } else {
                security
            },
            global_padding: None,
            authenticated_length: None,
            packet_encoding: None,
        }),
        subscription_id: None,
        group_id: None,
        latency: None,
        dialer_proxy: None,
        preferred_core: None,
        sort_order: 0,
    })
}

fn parse_url_style_vmess(uri: &str) -> ParseResult<ProxyNode> {
    let url = Url::parse(uri).map_err(|e| ParseError::InvalidUri(e.to_string()))?;
    let uuid = urlencoding::decode(url.username())
        .map(|s| s.into_owned())
        .unwrap_or_else(|_| url.username().to_string());
    if uuid.trim().is_empty() {
        return Err(ParseError::MissingField("uuid"));
    }
    let (server, port) = extract_url_host_port(&url, None)?;
    let params = parse_query_map(&url);
    let (transport, tls) = parse_v2ray_transport_and_tls(&params, false);
    let name = extract_remarks(&url, &server, port);

    let security = params
        .get("encryption")
        .or_else(|| params.get("scy"))
        .cloned()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "auto".to_string());
    let alter_id = params
        .get("aid")
        .or_else(|| params.get("alterId"))
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);

    Ok(ProxyNode {
        id: Uuid::new_v4().to_string(),
        name,
        server,
        port,
        protocol: ProtocolType::Vmess,
        tls,
        transport,
        settings: ProtocolConfig::Vmess(VmessConfig {
            uuid,
            alter_id,
            security,
            global_padding: None,
            authenticated_length: None,
            packet_encoding: None,
        }),
        subscription_id: None,
        group_id: None,
        latency: None,
        dialer_proxy: None,
        preferred_core: None,
        sort_order: 0,
    })
}

/// Serializes a `ProxyNode` of type `Vmess` into a standard v2rayN `vmess://<base64>` link
pub fn to_uri(node: &ProxyNode) -> ParseResult<String> {
    let vmess = match &node.settings {
        ProtocolConfig::Vmess(c) => c,
        _ => {
            return Err(ParseError::InvalidUri(
                "node is not a VMess node".to_string(),
            ))
        }
    };

    let mut net = "tcp".to_string();
    let mut header_type = "none".to_string();
    let mut host = String::new();
    let mut path = String::new();

    if let Some(ref transport) = node.transport {
        match transport {
            TransportConfig::Tcp {
                header_type: ht,
                http_host,
                http_path,
            } => {
                net = "tcp".to_string();
                if let Some(h) = ht {
                    header_type = h.clone();
                }
                if !http_host.is_empty() {
                    host = http_host.join(",");
                }
                if let Some(p) = http_path {
                    path = p.clone();
                }
            }
            TransportConfig::Ws {
                path: p,
                host: h,
                headers,
                ..
            } => {
                net = "ws".to_string();
                path = p.clone();
                host = h
                    .clone()
                    .or_else(|| headers.get("Host").cloned())
                    .unwrap_or_default();
            }
            TransportConfig::Grpc {
                service_name,
                multi_mode,
                authority,
            } => {
                net = "grpc".to_string();
                path = service_name.clone();
                header_type = if *multi_mode {
                    "multi".to_string()
                } else {
                    "gun".to_string()
                };
                host = authority.clone().unwrap_or_default();
            }
            TransportConfig::Http {
                host: h, path: p, ..
            } => {
                net = "h2".to_string();
                host = h.join(",");
                path = p.clone();
            }
            TransportConfig::HttpUpgrade {
                host: h, path: p, ..
            } => {
                net = "httpupgrade".to_string();
                host = h.clone().unwrap_or_default();
                path = p.clone();
            }
            TransportConfig::Xhttp {
                host: h,
                path: p,
                mode,
                ..
            } => {
                net = "xhttp".to_string();
                host = h.clone().unwrap_or_default();
                path = p.clone();
                if let Some(m) = mode {
                    header_type = m.clone();
                }
            }
            TransportConfig::Mkcp {
                seed,
                header_type: ht,
                ..
            } => {
                net = "kcp".to_string();
                path = seed.clone().unwrap_or_default();
                if let Some(h) = ht {
                    header_type = h.clone();
                }
            }
            TransportConfig::Quic {
                security,
                key,
                header_type: ht,
            } => {
                net = "quic".to_string();
                host = security.clone().unwrap_or_default();
                path = key.clone().unwrap_or_default();
                if let Some(h) = ht {
                    header_type = h.clone();
                }
            }
        }
    }

    let (tls_str, sni, alpn, fp, allow_insecure) = match &node.tls {
        Some(t) if t.enabled => (
            "tls".to_string(),
            t.server_name.clone().unwrap_or_default(),
            t.alpn.join(","),
            t.fingerprint.clone().unwrap_or_default(),
            Value::Bool(t.insecure),
        ),
        _ => (
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            Value::Bool(false),
        ),
    };

    let payload = VmessV2Json {
        v: Value::String("2".to_string()),
        ps: node.name.clone(),
        add: normalize_host(&node.server),
        port: Value::String(node.port.to_string()),
        id: vmess.uuid.clone(),
        aid: Value::String(vmess.alter_id.to_string()),
        scy: vmess.security.clone(),
        net,
        header_type,
        host,
        path,
        tls: tls_str,
        sni,
        alpn,
        fp,
        allow_insecure,
    };

    let json = serde_json::to_string(&payload)?;
    Ok(format!(
        "vmess://{}",
        encode_base64_standard(json.as_bytes())
    ))
}

use crate::model::{
    AnyTlsConfig, HttpConfig, Hysteria2Config, HysteriaConfig, NaiveConfig, ProtocolConfig,
    ProtocolType, ProxyNode, RealityConfig, ShadowTlsConfig, ShadowsocksConfig, ShadowsocksRConfig,
    Socks5Config, SshConfig, TlsConfig, TransportConfig, TrojanConfig, TuicConfig, VlessConfig,
    VmessConfig, WireGuardConfig,
};
use crate::parser::error::{ParseError, ParseResult};
use crate::parser::uri_common::normalize_host;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use uuid::Uuid;

fn j_str<'a>(obj: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    obj.get(key).and_then(Value::as_str)
}

fn j_string(obj: &Map<String, Value>, key: &str) -> Option<String> {
    match obj.get(key)? {
        Value::String(s) => Some(s.trim().to_string()).filter(|s| !s.is_empty()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn j_u16(obj: &Map<String, Value>, key: &str) -> Option<u16> {
    match obj.get(key)? {
        Value::Number(n) => n.as_u64().and_then(|v| u16::try_from(v).ok()),
        Value::String(s) => s.trim().parse::<u16>().ok(),
        _ => None,
    }
}

fn j_u32(obj: &Map<String, Value>, key: &str) -> Option<u32> {
    match obj.get(key)? {
        Value::Number(n) => n.as_u64().and_then(|v| u32::try_from(v).ok()),
        Value::String(s) => s.trim().parse::<u32>().ok(),
        _ => None,
    }
}

fn j_bool(obj: &Map<String, Value>, key: &str) -> Option<bool> {
    match obj.get(key)? {
        Value::Bool(b) => Some(*b),
        Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => Some(true),
            "false" | "0" | "no" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn j_string_vec(obj: &Map<String, Value>, key: &str) -> Vec<String> {
    match obj.get(key) {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
            .collect(),
        Some(Value::String(s)) if !s.trim().is_empty() => vec![s.trim().to_string()],
        _ => Vec::new(),
    }
}

fn parse_sb_tls(obj: &Map<String, Value>) -> Option<TlsConfig> {
    let tls_obj = obj.get("tls")?.as_object()?;
    let enabled = j_bool(tls_obj, "enabled").unwrap_or(true);
    if !enabled {
        return None;
    }

    let server_name = j_string(tls_obj, "server_name");
    let insecure = j_bool(tls_obj, "insecure").unwrap_or(false);
    let alpn = j_string_vec(tls_obj, "alpn");
    let fingerprint = tls_obj
        .get("utls")
        .and_then(Value::as_object)
        .and_then(|u| j_string(u, "fingerprint"));

    let reality = tls_obj
        .get("reality")
        .and_then(Value::as_object)
        .and_then(|r| {
            if !j_bool(r, "enabled").unwrap_or(true) {
                return None;
            }
            let public_key = j_string(r, "public_key")?;
            let short_id = j_string(r, "short_id").unwrap_or_default();
            Some(RealityConfig {
                public_key,
                short_id,
                spider_x: None,
            })
        });

    Some(TlsConfig {
        enabled: true,
        server_name,
        insecure,
        alpn,
        fingerprint,
        reality,
        ech_config: None,
        fragment: None,
    })
}

fn parse_sb_transport(obj: &Map<String, Value>) -> Option<TransportConfig> {
    let tr = obj.get("transport")?.as_object()?;
    let t_type = j_str(tr, "type")?.trim().to_ascii_lowercase();

    let mut headers = BTreeMap::new();
    let mut host_from_header = None;
    if let Some(hdrs) = tr.get("headers").and_then(Value::as_object) {
        for (k, v) in hdrs {
            let val_str = match v {
                Value::String(s) => Some(s.clone()),
                Value::Array(arr) => arr.first().and_then(Value::as_str).map(ToString::to_string),
                _ => None,
            };
            if let Some(vs) = val_str {
                if k.eq_ignore_ascii_case("host") {
                    host_from_header = Some(vs.clone());
                }
                headers.insert(k.clone(), vs);
            }
        }
    }

    match t_type.as_str() {
        "ws" => Some(TransportConfig::Ws {
            path: j_string(tr, "path").unwrap_or_else(|| "/".to_string()),
            host: host_from_header,
            headers,
            max_early_data: j_u32(tr, "max_early_data"),
            early_data_header_name: j_string(tr, "early_data_header_name"),
        }),
        "grpc" => Some(TransportConfig::Grpc {
            service_name: j_string(tr, "service_name").unwrap_or_default(),
            multi_mode: false,
            authority: None,
        }),
        "http" => Some(TransportConfig::Http {
            host: j_string_vec(tr, "host"),
            path: j_string(tr, "path").unwrap_or_else(|| "/".to_string()),
            method: j_string(tr, "method"),
            headers,
        }),
        "httpupgrade" => Some(TransportConfig::HttpUpgrade {
            host: j_string(tr, "host").or(host_from_header),
            path: j_string(tr, "path").unwrap_or_else(|| "/".to_string()),
            headers,
        }),
        "quic" => Some(TransportConfig::Quic {
            security: None,
            key: None,
            header_type: None,
        }),
        _ => None,
    }
}

fn parse_sb_outbound(obj: &Map<String, Value>, sort_order: i32) -> Option<ParseResult<ProxyNode>> {
    let ob_type = j_str(obj, "type")?.trim().to_ascii_lowercase();
    // Skip non-node outbounds
    if matches!(
        ob_type.as_str(),
        "direct" | "block" | "dns" | "selector" | "urltest"
    ) {
        return None;
    }

    let parse_inner = || -> ParseResult<ProxyNode> {
        let raw_server = j_string(obj, "server").ok_or(ParseError::MissingField("server"))?;
        let server = normalize_host(&raw_server);
        let server_ports = j_string_vec(obj, "server_ports");
        let fallback_hy2_port = if ob_type == "hysteria2" {
            server_ports
                .first()
                .and_then(|spec| {
                    spec.split(&[':', '-'][..])
                        .next()
                        .and_then(|s| s.trim().parse::<u16>().ok())
                })
                .or(Some(443))
        } else {
            None
        };
        let port = j_u16(obj, "server_port")
            .or_else(|| j_u16(obj, "port"))
            .or(fallback_hy2_port)
            .filter(|&p| p > 0)
            .ok_or_else(|| ParseError::InvalidPort("missing server_port".to_string()))?;

        let name = j_string(obj, "tag").unwrap_or_else(|| format!("{server}:{port}"));
        let dialer_proxy = j_string(obj, "detour");
        let tls = parse_sb_tls(obj);
        let transport = parse_sb_transport(obj);

        let (protocol, settings) = match ob_type.as_str() {
            "shadowsocks" => (
                ProtocolType::Shadowsocks,
                ProtocolConfig::Shadowsocks(ShadowsocksConfig {
                    method: j_string(obj, "method").ok_or(ParseError::MissingField("method"))?,
                    password: j_string(obj, "password")
                        .ok_or(ParseError::MissingField("password"))?,
                    plugin: j_string(obj, "plugin"),
                    plugin_opts: j_string(obj, "plugin_opts"),
                    uot: obj.get("udp_over_tcp").and_then(|v| match v {
                        Value::Bool(b) => Some(*b),
                        Value::Object(m) => j_bool(m, "enabled"),
                        _ => None,
                    }),
                }),
            ),
            "shadowsocksr" => (
                ProtocolType::ShadowsocksR,
                ProtocolConfig::ShadowsocksR(ShadowsocksRConfig {
                    method: j_string(obj, "method").ok_or(ParseError::MissingField("method"))?,
                    password: j_string(obj, "password")
                        .ok_or(ParseError::MissingField("password"))?,
                    protocol: j_string(obj, "protocol")
                        .ok_or(ParseError::MissingField("protocol"))?,
                    protocol_param: j_string(obj, "protocol_param"),
                    obfs: j_string(obj, "obfs").ok_or(ParseError::MissingField("obfs"))?,
                    obfs_param: j_string(obj, "obfs_param"),
                }),
            ),
            "vmess" => (
                ProtocolType::Vmess,
                ProtocolConfig::Vmess(VmessConfig {
                    uuid: j_string(obj, "uuid").ok_or(ParseError::MissingField("uuid"))?,
                    alter_id: j_u16(obj, "alter_id").unwrap_or(0),
                    security: j_string(obj, "security").unwrap_or_else(|| "auto".to_string()),
                    global_padding: j_bool(obj, "global_padding"),
                    authenticated_length: j_bool(obj, "authenticated_length"),
                    packet_encoding: j_string(obj, "packet_encoding"),
                }),
            ),
            "vless" => (
                ProtocolType::Vless,
                ProtocolConfig::Vless(VlessConfig {
                    uuid: j_string(obj, "uuid").ok_or(ParseError::MissingField("uuid"))?,
                    flow: j_string(obj, "flow"),
                    encryption: "none".to_string(),
                    packet_encoding: j_string(obj, "packet_encoding"),
                }),
            ),
            "trojan" => (
                ProtocolType::Trojan,
                ProtocolConfig::Trojan(TrojanConfig {
                    password: j_string(obj, "password")
                        .ok_or(ParseError::MissingField("password"))?,
                }),
            ),
            "hysteria" => (
                ProtocolType::Hysteria,
                ProtocolConfig::Hysteria(HysteriaConfig {
                    auth_str: j_string(obj, "auth_str").or_else(|| j_string(obj, "auth")),
                    protocol: Some("udp".to_string()),
                    up_mbps: j_u32(obj, "up_mbps").unwrap_or(100),
                    down_mbps: j_u32(obj, "down_mbps").unwrap_or(100),
                    obfs: j_string(obj, "obfs"),
                    recv_window_conn: None,
                    recv_window: None,
                    disable_mtu_discovery: j_bool(obj, "disable_mtu_discovery"),
                }),
            ),
            "hysteria2" => {
                let (obfs_type, obfs_password) = match obj.get("obfs").and_then(Value::as_object) {
                    Some(o) => (j_string(o, "type"), j_string(o, "password")),
                    None => (None, None),
                };
                let normalized_ports = if server_ports.is_empty() {
                    None
                } else {
                    Some(
                        server_ports
                            .iter()
                            .map(|p| p.replace(':', "-"))
                            .collect::<Vec<_>>()
                            .join(","),
                    )
                };
                let hop_interval = j_string(obj, "hop_interval").and_then(|s| {
                    let t = s.trim().to_ascii_lowercase();
                    if let Some(m) = t.strip_suffix('m') {
                        m.trim().parse::<u32>().ok().map(|v| v * 60)
                    } else if let Some(sec) = t.strip_suffix('s') {
                        sec.trim().parse::<u32>().ok()
                    } else {
                        t.parse::<u32>().ok()
                    }
                });
                (
                    ProtocolType::Hysteria2,
                    ProtocolConfig::Hysteria2(Hysteria2Config {
                        password: j_string(obj, "password")
                            .ok_or(ParseError::MissingField("password"))?,
                        up_mbps: j_u32(obj, "up_mbps"),
                        down_mbps: j_u32(obj, "down_mbps"),
                        obfs_type,
                        obfs_password,
                        ports: normalized_ports,
                        hop_interval,
                    }),
                )
            }
            "tuic" => (
                ProtocolType::Tuic,
                ProtocolConfig::Tuic(TuicConfig {
                    uuid: j_string(obj, "uuid").ok_or(ParseError::MissingField("uuid"))?,
                    password: j_string(obj, "password").unwrap_or_default(),
                    congestion_control: j_string(obj, "congestion_control"),
                    udp_relay_mode: j_string(obj, "udp_relay_mode"),
                    zero_rtt_handshake: j_bool(obj, "zero_rtt_handshake"),
                    heartbeat: j_string(obj, "heartbeat"),
                }),
            ),
            "wireguard" => (
                ProtocolType::WireGuard,
                ProtocolConfig::WireGuard(WireGuardConfig {
                    private_key: j_string(obj, "private_key")
                        .ok_or(ParseError::MissingField("private_key"))?,
                    public_key: j_string(obj, "peer_public_key")
                        .ok_or(ParseError::MissingField("peer_public_key"))?,
                    preshared_key: j_string(obj, "pre_shared_key"),
                    local_address: {
                        let addrs = j_string_vec(obj, "local_address");
                        if addrs.is_empty() {
                            vec!["10.0.0.2/32".to_string()]
                        } else {
                            addrs
                        }
                    },
                    reserved: None,
                    mtu: j_u16(obj, "mtu"),
                    workers: j_u16(obj, "workers").map(|v| v as u8),
                }),
            ),
            "anytls" => (
                ProtocolType::AnyTls,
                ProtocolConfig::AnyTls(AnyTlsConfig {
                    password: j_string(obj, "password")
                        .ok_or(ParseError::MissingField("password"))?,
                    idle_session_check_interval: j_string(obj, "idle_session_check_interval"),
                    idle_session_timeout: j_string(obj, "idle_session_timeout"),
                    min_idle_session: j_u32(obj, "min_idle_session"),
                }),
            ),
            "shadowtls" => (
                ProtocolType::ShadowTls,
                ProtocolConfig::ShadowTls(ShadowTlsConfig {
                    version: j_u16(obj, "version").unwrap_or(3) as u8,
                    password: j_string(obj, "password")
                        .ok_or(ParseError::MissingField("password"))?,
                    inner_protocol: None,
                    inner_method: None,
                    inner_password: None,
                }),
            ),
            "naive" => (
                ProtocolType::Naive,
                ProtocolConfig::Naive(NaiveConfig {
                    username: j_string(obj, "username")
                        .ok_or(ParseError::MissingField("username"))?,
                    password: j_string(obj, "password").unwrap_or_default(),
                    extra_headers: BTreeMap::new(),
                    insecure_concurrency: j_u16(obj, "insecure_concurrency").map(|v| v as u8),
                    quic: j_bool(obj, "quic").unwrap_or(false),
                }),
            ),
            "ssh" => (
                ProtocolType::Ssh,
                ProtocolConfig::Ssh(SshConfig {
                    user: j_string(obj, "user").unwrap_or_else(|| "root".to_string()),
                    password: j_string(obj, "password"),
                    private_key: j_string(obj, "private_key"),
                    private_key_passphrase: j_string(obj, "private_key_passphrase"),
                    host_key: j_string_vec(obj, "host_key"),
                    host_key_algorithms: j_string_vec(obj, "host_key_algorithms"),
                    client_version: j_string(obj, "client_version"),
                }),
            ),
            "socks" => (
                ProtocolType::Socks5,
                ProtocolConfig::Socks5(Socks5Config {
                    username: j_string(obj, "username"),
                    password: j_string(obj, "password"),
                    udp: true,
                }),
            ),
            "http" => (
                ProtocolType::Http,
                ProtocolConfig::Http(HttpConfig {
                    username: j_string(obj, "username"),
                    password: j_string(obj, "password"),
                    headers: BTreeMap::new(),
                }),
            ),
            other => return Err(ParseError::UnsupportedScheme(format!("sing-box:{other}"))),
        };

        Ok(ProxyNode {
            id: Uuid::new_v4().to_string(),
            name,
            server,
            port,
            protocol,
            tls,
            transport,
            settings,
            subscription_id: None,
            group_id: None,
            latency: None,
            dialer_proxy,
            preferred_core: None,
            sort_order,
        })
    };

    Some(parse_inner())
}

/// Parses a `sing-box` JSON configuration (`outbounds:` array) or `SIP008` JSON (`servers:` array)
pub fn parse_json_subscription(json_text: &str) -> ParseResult<(Vec<ProxyNode>, Vec<String>)> {
    let val: Value = serde_json::from_str(json_text)?;

    // Check if root is an array of SIP008 servers or sing-box outbounds
    if let Some(arr) = val.as_array() {
        return parse_json_array(arr);
    }

    let obj = val
        .as_object()
        .ok_or_else(|| ParseError::InvalidUri("JSON root is not an object".to_string()))?;

    if let Some(outbounds) = obj.get("outbounds").and_then(Value::as_array) {
        let mut nodes = Vec::new();
        let mut warnings = Vec::new();
        for (idx, item) in outbounds.iter().enumerate() {
            let Some(ob_map) = item.as_object() else {
                continue;
            };
            if let Some(res) = parse_sb_outbound(ob_map, idx as i32) {
                match res {
                    Ok(node) => nodes.push(node),
                    Err(e) => {
                        let tag = j_string(ob_map, "tag").unwrap_or_else(|| format!("#{idx}"));
                        warnings.push(format!("skipped sing-box outbound '{tag}': {e}"));
                    }
                }
            }
        }
        return Ok((nodes, warnings));
    }

    if let Some(servers) = obj.get("servers").and_then(Value::as_array) {
        return parse_sip008_servers(servers);
    }

    // Single outbound object?
    if obj.contains_key("type") && obj.contains_key("server") {
        if let Some(res) = parse_sb_outbound(obj, 0) {
            return Ok((vec![res?], Vec::new()));
        }
    }

    Err(ParseError::InvalidUri(
        "JSON document contains neither 'outbounds' (sing-box) nor 'servers' (SIP008)".to_string(),
    ))
}

fn parse_json_array(arr: &[Value]) -> ParseResult<(Vec<ProxyNode>, Vec<String>)> {
    if arr.first().and_then(Value::as_object).is_some_and(|o| {
        o.contains_key("server_port") && o.contains_key("method") && !o.contains_key("type")
    }) {
        return parse_sip008_servers(arr);
    }

    let mut nodes = Vec::new();
    let mut warnings = Vec::new();
    for (idx, item) in arr.iter().enumerate() {
        if let Some(ob_map) = item.as_object() {
            if let Some(res) = parse_sb_outbound(ob_map, idx as i32) {
                match res {
                    Ok(n) => nodes.push(n),
                    Err(e) => warnings.push(format!("skipped item[{idx}]: {e}")),
                }
            }
        }
    }
    Ok((nodes, warnings))
}

/// Parses SIP008 Shadowsocks `servers` array
pub fn parse_sip008_servers(servers: &[Value]) -> ParseResult<(Vec<ProxyNode>, Vec<String>)> {
    let mut nodes = Vec::new();
    let mut warnings = Vec::new();

    for (idx, item) in servers.iter().enumerate() {
        let Some(obj) = item.as_object() else {
            warnings.push(format!("SIP008 server[{idx}] is not an object"));
            continue;
        };

        let parse_one = || -> ParseResult<ProxyNode> {
            let raw_server = j_string(obj, "server").ok_or(ParseError::MissingField("server"))?;
            let server = normalize_host(&raw_server);
            let port = j_u16(obj, "server_port")
                .or_else(|| j_u16(obj, "port"))
                .filter(|&p| p > 0)
                .ok_or_else(|| ParseError::InvalidPort("missing server_port".to_string()))?;
            let method = j_string(obj, "method").ok_or(ParseError::MissingField("method"))?;
            let password = j_string(obj, "password").ok_or(ParseError::MissingField("password"))?;
            let name = j_string(obj, "remarks")
                .or_else(|| j_string(obj, "name"))
                .unwrap_or_else(|| format!("{server}:{port}"));
            let plugin = j_string(obj, "plugin");
            let plugin_opts = j_string(obj, "plugin_opts");

            Ok(ProxyNode {
                id: Uuid::new_v4().to_string(),
                name,
                server,
                port,
                protocol: ProtocolType::Shadowsocks,
                tls: None,
                transport: None,
                settings: ProtocolConfig::Shadowsocks(ShadowsocksConfig {
                    method,
                    password,
                    plugin,
                    plugin_opts,
                    uot: None,
                }),
                subscription_id: None,
                group_id: None,
                latency: None,
                dialer_proxy: None,
                preferred_core: None,
                sort_order: idx as i32,
            })
        };

        match parse_one() {
            Ok(n) => nodes.push(n),
            Err(e) => warnings.push(format!("skipped SIP008 server[{idx}]: {e}")),
        }
    }

    Ok((nodes, warnings))
}

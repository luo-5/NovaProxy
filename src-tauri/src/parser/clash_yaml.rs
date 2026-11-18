use crate::model::{
    AnyTlsConfig, GroupType, HttpConfig, Hysteria2Config, HysteriaConfig, MieruConfig, NodeGroup,
    ProtocolConfig, ProtocolType, ProxyNode, RealityConfig, ShadowTlsConfig, ShadowsocksConfig,
    ShadowsocksRConfig, SnellConfig, Socks5Config, SshConfig, TlsConfig, TransportConfig,
    TrojanConfig, TuicConfig, VlessConfig, VmessConfig, WireGuardConfig,
};
use crate::parser::error::{ParseError, ParseResult};
use crate::parser::uri_common::normalize_host;
use serde_yaml::Value;
use std::collections::BTreeMap;
use uuid::Uuid;

fn get_str<'a>(map: &'a serde_yaml::Mapping, key: &str) -> Option<&'a str> {
    map.get(Value::String(key.to_string()))
        .and_then(Value::as_str)
}

fn get_string(map: &serde_yaml::Mapping, key: &str) -> Option<String> {
    let val = map.get(Value::String(key.to_string()))?;
    match val {
        Value::String(s) => Some(s.trim().to_string()).filter(|s| !s.is_empty()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn get_u64(map: &serde_yaml::Mapping, key: &str) -> Option<u64> {
    let val = map.get(Value::String(key.to_string()))?;
    match val {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.trim().parse::<u64>().ok(),
        _ => None,
    }
}

fn get_u16(map: &serde_yaml::Mapping, key: &str) -> Option<u16> {
    get_u64(map, key).and_then(|v| u16::try_from(v).ok())
}

fn get_u32_mbps(map: &serde_yaml::Mapping, key: &str) -> Option<u32> {
    let val = map.get(Value::String(key.to_string()))?;
    match val {
        Value::Number(n) => n.as_u64().and_then(|v| u32::try_from(v).ok()),
        Value::String(s) => {
            let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse::<u32>().ok()
        }
        _ => None,
    }
}

fn get_bool(map: &serde_yaml::Mapping, key: &str) -> Option<bool> {
    let val = map.get(Value::String(key.to_string()))?;
    match val {
        Value::Bool(b) => Some(*b),
        Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => Some(true),
            "false" | "0" | "no" => Some(false),
            _ => None,
        },
        Value::Number(n) => n.as_i64().map(|v| v != 0),
        _ => None,
    }
}

fn get_string_vec(map: &serde_yaml::Mapping, key: &str) -> Vec<String> {
    match map.get(Value::String(key.to_string())) {
        Some(Value::Sequence(seq)) => seq
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
            .collect(),
        Some(Value::String(s)) if !s.trim().is_empty() => s
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

fn get_submap<'a>(map: &'a serde_yaml::Mapping, key: &str) -> Option<&'a serde_yaml::Mapping> {
    map.get(Value::String(key.to_string()))
        .and_then(Value::as_mapping)
}

fn parse_clash_tls(map: &serde_yaml::Mapping, default_enabled: bool) -> Option<TlsConfig> {
    let tls_enabled = get_bool(map, "tls")
        .or_else(|| {
            if get_submap(map, "reality-opts").is_some() {
                Some(true)
            } else {
                None
            }
        })
        .unwrap_or(default_enabled);

    if !tls_enabled {
        return None;
    }

    let server_name = get_string(map, "servername").or_else(|| get_string(map, "sni"));
    let insecure = get_bool(map, "skip-cert-verify").unwrap_or(false);
    let alpn = get_string_vec(map, "alpn");
    let fingerprint =
        get_string(map, "client-fingerprint").or_else(|| get_string(map, "fingerprint"));

    let reality = get_submap(map, "reality-opts").and_then(|ro| {
        let pbk = get_string(ro, "public-key")?;
        let sid = get_string(ro, "short-id").unwrap_or_default();
        let spx = get_string(ro, "spider-x");
        Some(RealityConfig {
            public_key: pbk,
            short_id: sid,
            spider_x: spx,
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

fn parse_clash_transport(map: &serde_yaml::Mapping) -> Option<TransportConfig> {
    let network = get_str(map, "network")
        .unwrap_or("tcp")
        .trim()
        .to_ascii_lowercase();

    match network.as_str() {
        "ws" | "websocket" => {
            let ws_opts = get_submap(map, "ws-opts");
            let path = ws_opts
                .and_then(|o| get_string(o, "path"))
                .or_else(|| get_string(map, "ws-path"))
                .unwrap_or_else(|| "/".to_string());

            let mut headers = BTreeMap::new();
            let mut host = None;
            if let Some(opts) = ws_opts {
                if let Some(hdrs) = get_submap(opts, "headers") {
                    for (k, v) in hdrs {
                        if let (Some(ks), Some(vs)) = (k.as_str(), v.as_str()) {
                            if ks.eq_ignore_ascii_case("host") {
                                host = Some(vs.trim().to_string());
                            }
                            headers.insert(ks.trim().to_string(), vs.trim().to_string());
                        }
                    }
                }
            }
            let max_early_data = ws_opts
                .and_then(|o| get_u64(o, "max-early-data"))
                .and_then(|v| u32::try_from(v).ok());
            let early_data_header_name =
                ws_opts.and_then(|o| get_string(o, "early-data-header-name"));

            if get_bool(ws_opts.unwrap_or(map), "v2ray-http-upgrade").unwrap_or(false) {
                Some(TransportConfig::HttpUpgrade {
                    host,
                    path,
                    headers,
                })
            } else {
                Some(TransportConfig::Ws {
                    path,
                    host,
                    headers,
                    max_early_data,
                    early_data_header_name,
                })
            }
        }
        "grpc" => {
            let grpc_opts = get_submap(map, "grpc-opts");
            let service_name = grpc_opts
                .and_then(|o| get_string(o, "grpc-service-name"))
                .unwrap_or_default();
            Some(TransportConfig::Grpc {
                service_name,
                multi_mode: false,
                authority: None,
            })
        }
        "h2" => {
            let h2_opts = get_submap(map, "h2-opts");
            let host = h2_opts
                .map(|o| get_string_vec(o, "host"))
                .unwrap_or_default();
            let path = h2_opts
                .and_then(|o| get_string(o, "path"))
                .unwrap_or_else(|| "/".to_string());
            Some(TransportConfig::Http {
                host,
                path,
                method: None,
                headers: BTreeMap::new(),
            })
        }
        "http" => {
            let http_opts = get_submap(map, "http-opts");
            let path = http_opts
                .map(|o| get_string_vec(o, "path"))
                .and_then(|v| v.into_iter().next());
            let mut http_host = Vec::new();
            if let Some(opts) = http_opts {
                if let Some(hdrs) = get_submap(opts, "headers") {
                    if let Some(h_seq) = hdrs.get(Value::String("Host".to_string())) {
                        if let Some(seq) = h_seq.as_sequence() {
                            for item in seq {
                                if let Some(s) = item.as_str() {
                                    http_host.push(s.to_string());
                                }
                            }
                        }
                    }
                }
            }
            Some(TransportConfig::Tcp {
                header_type: Some("http".to_string()),
                http_host,
                http_path: path,
            })
        }
        _ => None,
    }
}

fn parse_clash_wireguard_reserved(map: &serde_yaml::Mapping) -> Option<[u8; 3]> {
    let val = map.get(Value::String("reserved".to_string()))?;
    if let Some(seq) = val.as_sequence() {
        if seq.len() >= 3 {
            let b0 = seq[0].as_u64().and_then(|v| u8::try_from(v).ok())?;
            let b1 = seq[1].as_u64().and_then(|v| u8::try_from(v).ok())?;
            let b2 = seq[2].as_u64().and_then(|v| u8::try_from(v).ok())?;
            return Some([b0, b1, b2]);
        }
    } else if let Some(s) = val.as_str() {
        let parts: Vec<u8> = s
            .split(',')
            .filter_map(|p| p.trim().parse::<u8>().ok())
            .collect();
        if parts.len() >= 3 {
            return Some([parts[0], parts[1], parts[2]]);
        }
    }
    None
}

/// Parses a single Clash / Mihomo proxy mapping into a `ProxyNode`
pub fn parse_clash_proxy_map(map: &serde_yaml::Mapping, sort_order: i32) -> ParseResult<ProxyNode> {
    let proxy_type = get_str(map, "type")
        .ok_or(ParseError::MissingField("type"))?
        .trim()
        .to_ascii_lowercase();

    let first_wg_peer = if proxy_type == "wireguard" {
        map.get(Value::String("peers".to_string()))
            .and_then(|v| v.as_sequence())
            .and_then(|seq| seq.first())
            .and_then(|v| v.as_mapping())
    } else {
        None
    };

    let raw_server = get_string(map, "server")
        .or_else(|| first_wg_peer.and_then(|p| get_string(p, "server")))
        .ok_or(ParseError::MissingField("server"))?;
    let server = normalize_host(&raw_server);
    if server.is_empty() {
        return Err(ParseError::MissingField("server"));
    }

    let port = get_u16(map, "port")
        .or_else(|| first_wg_peer.and_then(|p| get_u16(p, "port")))
        .filter(|&p| p > 0)
        .ok_or_else(|| ParseError::InvalidPort("missing or invalid port".to_string()))?;

    let name = get_string(map, "name").unwrap_or_else(|| format!("{server}:{port}"));
    let dialer_proxy = get_string(map, "dialer-proxy");

    let (protocol, settings, tls, transport) = match proxy_type.as_str() {
        "ss" | "shadowsocks" => {
            let method = get_string(map, "cipher")
                .or_else(|| get_string(map, "method"))
                .ok_or(ParseError::MissingField("cipher"))?;
            let password =
                get_string(map, "password").ok_or(ParseError::MissingField("password"))?;
            let plugin = get_string(map, "plugin");
            let plugin_opts = get_submap(map, "plugin-opts").map(|opts| {
                let mut parts = Vec::new();
                if let Some(mode) = get_string(opts, "mode") {
                    parts.push(format!("obfs={mode}"));
                }
                if let Some(host) = get_string(opts, "host") {
                    parts.push(format!("obfs-host={host}"));
                }
                parts.join(";")
            });
            let uot = get_bool(map, "udp-over-tcp");
            (
                ProtocolType::Shadowsocks,
                ProtocolConfig::Shadowsocks(ShadowsocksConfig {
                    method,
                    password,
                    plugin,
                    plugin_opts: plugin_opts.filter(|s| !s.is_empty()),
                    uot,
                }),
                None,
                None,
            )
        }
        "ssr" => {
            let method = get_string(map, "cipher").ok_or(ParseError::MissingField("cipher"))?;
            let password =
                get_string(map, "password").ok_or(ParseError::MissingField("password"))?;
            let protocol =
                get_string(map, "protocol").ok_or(ParseError::MissingField("protocol"))?;
            let obfs = get_string(map, "obfs").ok_or(ParseError::MissingField("obfs"))?;
            let protocol_param =
                get_string(map, "protocol-param").or_else(|| get_string(map, "protoparam"));
            let obfs_param = get_string(map, "obfs-param").or_else(|| get_string(map, "obfsparam"));
            (
                ProtocolType::ShadowsocksR,
                ProtocolConfig::ShadowsocksR(ShadowsocksRConfig {
                    method,
                    password,
                    protocol,
                    protocol_param,
                    obfs,
                    obfs_param,
                }),
                None,
                None,
            )
        }
        "vmess" => {
            let uuid = get_string(map, "uuid").ok_or(ParseError::MissingField("uuid"))?;
            let alter_id = get_u16(map, "alterId").unwrap_or(0);
            let security = get_string(map, "cipher").unwrap_or_else(|| "auto".to_string());
            let tls = parse_clash_tls(map, false);
            let transport = parse_clash_transport(map);
            (
                ProtocolType::Vmess,
                ProtocolConfig::Vmess(VmessConfig {
                    uuid,
                    alter_id,
                    security,
                    global_padding: get_bool(map, "global-padding"),
                    authenticated_length: get_bool(map, "authenticated-length"),
                    packet_encoding: get_string(map, "packet-encoding"),
                }),
                tls,
                transport,
            )
        }
        "vless" => {
            let uuid = get_string(map, "uuid").ok_or(ParseError::MissingField("uuid"))?;
            let flow = get_string(map, "flow");
            let tls = parse_clash_tls(map, false);
            let transport = parse_clash_transport(map);
            (
                ProtocolType::Vless,
                ProtocolConfig::Vless(VlessConfig {
                    uuid,
                    flow,
                    encryption: "none".to_string(),
                    packet_encoding: get_string(map, "packet-encoding"),
                }),
                tls,
                transport,
            )
        }
        "trojan" => {
            let password =
                get_string(map, "password").ok_or(ParseError::MissingField("password"))?;
            let tls = parse_clash_tls(map, true);
            let transport = parse_clash_transport(map);
            (
                ProtocolType::Trojan,
                ProtocolConfig::Trojan(TrojanConfig { password }),
                tls,
                transport,
            )
        }
        "hysteria" => {
            let auth_str = get_string(map, "auth-str").or_else(|| get_string(map, "auth_str"));
            let up_mbps = get_u32_mbps(map, "up").unwrap_or(100);
            let down_mbps = get_u32_mbps(map, "down").unwrap_or(100);
            let tls = parse_clash_tls(map, true);
            (
                ProtocolType::Hysteria,
                ProtocolConfig::Hysteria(HysteriaConfig {
                    auth_str,
                    protocol: get_string(map, "protocol"),
                    up_mbps,
                    down_mbps,
                    obfs: get_string(map, "obfs"),
                    recv_window_conn: get_u64(map, "recv-window-conn"),
                    recv_window: get_u64(map, "recv-window"),
                    disable_mtu_discovery: get_bool(map, "disable_mtu_discovery"),
                }),
                tls,
                None,
            )
        }
        "hysteria2" => {
            let password =
                get_string(map, "password").ok_or(ParseError::MissingField("password"))?;
            let tls = parse_clash_tls(map, true);
            (
                ProtocolType::Hysteria2,
                ProtocolConfig::Hysteria2(Hysteria2Config {
                    password,
                    up_mbps: get_u32_mbps(map, "up"),
                    down_mbps: get_u32_mbps(map, "down"),
                    obfs_type: get_string(map, "obfs"),
                    obfs_password: get_string(map, "obfs-password"),
                    ports: get_string(map, "ports").or_else(|| get_string(map, "mport")),
                    hop_interval: get_u32_mbps(map, "hop-interval")
                        .or_else(|| get_u32_mbps(map, "hop_interval")),
                }),
                tls,
                None,
            )
        }
        "tuic" => {
            let uuid = get_string(map, "uuid").ok_or(ParseError::MissingField("uuid"))?;
            let password = get_string(map, "password").unwrap_or_default();
            let tls = parse_clash_tls(map, true);
            (
                ProtocolType::Tuic,
                ProtocolConfig::Tuic(TuicConfig {
                    uuid,
                    password,
                    congestion_control: get_string(map, "congestion-controller"),
                    udp_relay_mode: get_string(map, "udp-relay-mode"),
                    zero_rtt_handshake: get_bool(map, "reduce-rtt"),
                    heartbeat: get_string(map, "heartbeat-interval"),
                }),
                tls,
                None,
            )
        }
        "wireguard" => {
            let private_key =
                get_string(map, "private-key").ok_or(ParseError::MissingField("private-key"))?;
            let public_key = get_string(map, "public-key")
                .or_else(|| first_wg_peer.and_then(|p| get_string(p, "public-key")))
                .ok_or(ParseError::MissingField("public-key"))?;
            let mut local_address = Vec::new();
            if let Some(ip) = get_string(map, "ip") {
                local_address.push(if ip.contains('/') {
                    ip
                } else {
                    format!("{ip}/32")
                });
            }
            if let Some(ipv6) = get_string(map, "ipv6") {
                local_address.push(if ipv6.contains('/') {
                    ipv6
                } else {
                    format!("{ipv6}/128")
                });
            }
            if local_address.is_empty() {
                local_address.push("10.0.0.2/32".to_string());
            }
            let preshared_key = get_string(map, "pre-shared-key")
                .or_else(|| get_string(map, "preshared-key"))
                .or_else(|| {
                    first_wg_peer.and_then(|p| {
                        get_string(p, "pre-shared-key").or_else(|| get_string(p, "preshared-key"))
                    })
                });
            let reserved = parse_clash_wireguard_reserved(map)
                .or_else(|| first_wg_peer.and_then(parse_clash_wireguard_reserved));
            (
                ProtocolType::WireGuard,
                ProtocolConfig::WireGuard(WireGuardConfig {
                    private_key,
                    public_key,
                    preshared_key,
                    local_address,
                    reserved,
                    mtu: get_u16(map, "mtu"),
                    workers: None,
                }),
                None,
                None,
            )
        }
        "anytls" => {
            let password =
                get_string(map, "password").ok_or(ParseError::MissingField("password"))?;
            let tls = parse_clash_tls(map, true);
            (
                ProtocolType::AnyTls,
                ProtocolConfig::AnyTls(AnyTlsConfig {
                    password,
                    idle_session_check_interval: get_string(map, "idle-session-check-interval"),
                    idle_session_timeout: get_string(map, "idle-session-timeout"),
                    min_idle_session: get_u32_mbps(map, "min-idle-session"),
                }),
                tls,
                None,
            )
        }
        "shadow-tls" | "shadowtls" => {
            let password =
                get_string(map, "password").ok_or(ParseError::MissingField("password"))?;
            let version = get_u16(map, "version").unwrap_or(3) as u8;
            let tls = parse_clash_tls(map, true);
            (
                ProtocolType::ShadowTls,
                ProtocolConfig::ShadowTls(ShadowTlsConfig {
                    version,
                    password,
                    inner_protocol: None,
                    inner_method: None,
                    inner_password: None,
                }),
                tls,
                None,
            )
        }
        "snell" => {
            let psk = get_string(map, "psk").ok_or(ParseError::MissingField("psk"))?;
            let version = get_u16(map, "version").unwrap_or(4) as u8;
            let obfs_opts = get_submap(map, "obfs-opts");
            (
                ProtocolType::Snell,
                ProtocolConfig::Snell(SnellConfig {
                    psk,
                    version,
                    obfs_mode: obfs_opts.and_then(|o| get_string(o, "mode")),
                    obfs_host: obfs_opts.and_then(|o| get_string(o, "host")),
                }),
                None,
                None,
            )
        }
        "mieru" => {
            let username =
                get_string(map, "username").ok_or(ParseError::MissingField("username"))?;
            let password =
                get_string(map, "password").ok_or(ParseError::MissingField("password"))?;
            let transport = get_string(map, "transport").unwrap_or_else(|| "TCP".to_string());
            (
                ProtocolType::Mieru,
                ProtocolConfig::Mieru(MieruConfig {
                    username,
                    password,
                    transport,
                    port_range: get_string(map, "port-range"),
                    multiplexing: get_string(map, "multiplexing"),
                }),
                None,
                None,
            )
        }
        "ssh" => {
            let user = get_string(map, "username").unwrap_or_else(|| "root".to_string());
            (
                ProtocolType::Ssh,
                ProtocolConfig::Ssh(SshConfig {
                    user,
                    password: get_string(map, "password"),
                    private_key: get_string(map, "private-key"),
                    private_key_passphrase: get_string(map, "private-key-passphrase"),
                    host_key: get_string_vec(map, "host-key"),
                    host_key_algorithms: get_string_vec(map, "host-key-algorithms"),
                    client_version: None,
                }),
                None,
                None,
            )
        }
        "socks5" => {
            let tls = parse_clash_tls(map, false);
            (
                ProtocolType::Socks5,
                ProtocolConfig::Socks5(Socks5Config {
                    username: get_string(map, "username"),
                    password: get_string(map, "password"),
                    udp: get_bool(map, "udp").unwrap_or(true),
                }),
                tls,
                None,
            )
        }
        "http" => {
            let tls = parse_clash_tls(map, false);
            (
                ProtocolType::Http,
                ProtocolConfig::Http(HttpConfig {
                    username: get_string(map, "username"),
                    password: get_string(map, "password"),
                    headers: BTreeMap::new(),
                }),
                tls,
                None,
            )
        }
        other => return Err(ParseError::UnsupportedScheme(format!("clash:{other}"))),
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
}

/// Parses a Clash / Mihomo YAML subscription document (`proxies:` and optional `proxy-groups:`)
pub fn parse_clash_yaml_with_groups(
    yaml_text: &str,
) -> ParseResult<(Vec<ProxyNode>, Vec<NodeGroup>, Vec<String>)> {
    let doc: Value = serde_yaml::from_str(yaml_text)?;
    let mapping = doc
        .as_mapping()
        .ok_or_else(|| ParseError::InvalidUri("YAML root is not a mapping".to_string()))?;

    let proxies_val = mapping
        .get(Value::String("proxies".to_string()))
        .or_else(|| mapping.get(Value::String("Proxy".to_string())))
        .ok_or(ParseError::MissingField("proxies"))?;

    let seq = proxies_val
        .as_sequence()
        .ok_or_else(|| ParseError::InvalidUri("'proxies' is not a list".to_string()))?;

    let mut nodes = Vec::new();
    let mut warnings = Vec::new();

    for (idx, item) in seq.iter().enumerate() {
        let Some(proxy_map) = item.as_mapping() else {
            warnings.push(format!("proxy[{idx}] is not a YAML mapping, skipped"));
            continue;
        };
        match parse_clash_proxy_map(proxy_map, idx as i32) {
            Ok(node) => nodes.push(node),
            Err(err) => {
                let name = get_string(proxy_map, "name").unwrap_or_else(|| format!("#{idx}"));
                warnings.push(format!("skipped Clash proxy '{name}': {err}"));
            }
        }
    }

    let groups = parse_clash_proxy_groups(mapping, &nodes);

    Ok((nodes, groups, warnings))
}

fn parse_clash_proxy_groups(mapping: &serde_yaml::Mapping, nodes: &[ProxyNode]) -> Vec<NodeGroup> {
    let Some(pg_val) = mapping
        .get(Value::String("proxy-groups".to_string()))
        .or_else(|| mapping.get(Value::String("Proxy Group".to_string())))
    else {
        return Vec::new();
    };
    let Some(pg_seq) = pg_val.as_sequence() else {
        return Vec::new();
    };

    let name_to_id: std::collections::HashMap<&str, &str> = nodes
        .iter()
        .map(|n| (n.name.as_str(), n.id.as_str()))
        .collect();

    // First pass: capture raw group member names for nested group resolution
    struct RawGroup {
        name: String,
        group_type: GroupType,
        members: Vec<String>,
        test_url: Option<String>,
        interval_secs: Option<u32>,
    }

    let mut raw_groups = Vec::new();
    for item in pg_seq {
        let Some(g_map) = item.as_mapping() else {
            continue;
        };
        let Some(name) = get_string(g_map, "name") else {
            continue;
        };
        let g_type_str = get_string(g_map, "type").unwrap_or_else(|| "select".to_string());
        let group_type = GroupType::from_str_lossy(&g_type_str);
        let members = get_string_vec(g_map, "proxies");
        let test_url = get_string(g_map, "url");
        let interval_secs = get_u32_mbps(g_map, "interval");
        raw_groups.push(RawGroup {
            name,
            group_type,
            members,
            test_url,
            interval_secs,
        });
    }

    let raw_by_name: std::collections::HashMap<&str, &RawGroup> =
        raw_groups.iter().map(|g| (g.name.as_str(), g)).collect();

    fn collect_node_ids(
        group_name: &str,
        raw_by_name: &std::collections::HashMap<&str, &RawGroup>,
        name_to_id: &std::collections::HashMap<&str, &str>,
        visiting: &mut std::collections::HashSet<String>,
        out: &mut Vec<String>,
        seen_ids: &mut std::collections::HashSet<String>,
    ) {
        if !visiting.insert(group_name.to_string()) {
            return;
        }
        if let Some(rg) = raw_by_name.get(group_name) {
            for member in &rg.members {
                let upper = member.trim().to_ascii_uppercase();
                if matches!(upper.as_str(), "DIRECT" | "REJECT" | "PASS" | "COMPATIBLE") {
                    continue;
                }
                if let Some(&id) = name_to_id.get(member.as_str()) {
                    if seen_ids.insert(id.to_string()) {
                        out.push(id.to_string());
                    }
                } else if raw_by_name.contains_key(member.as_str()) {
                    collect_node_ids(member, raw_by_name, name_to_id, visiting, out, seen_ids);
                }
            }
        }
        visiting.remove(group_name);
    }

    let mut result = Vec::new();
    for (idx, rg) in raw_groups.iter().enumerate() {
        let mut node_ids = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();
        // Direct members first
        for m in &rg.members {
            if let Some(&id) = name_to_id.get(m.as_str()) {
                if seen_ids.insert(id.to_string()) {
                    node_ids.push(id.to_string());
                }
            }
        }
        // Fallback: flatten referenced sub-groups if no direct node members matched
        if node_ids.is_empty() {
            let mut visiting = std::collections::HashSet::new();
            collect_node_ids(
                &rg.name,
                &raw_by_name,
                &name_to_id,
                &mut visiting,
                &mut node_ids,
                &mut seen_ids,
            );
        }
        if node_ids.is_empty() {
            continue;
        }
        let deterministic_id = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(b"clash-group:");
            hasher.update(rg.name.as_bytes());
            let digest = hasher.finalize();
            let mut bytes = [0u8; 16];
            bytes.copy_from_slice(&digest[..16]);
            bytes[6] = (bytes[6] & 0x0f) | 0x50;
            bytes[8] = (bytes[8] & 0x3f) | 0x80;
            Uuid::from_bytes(bytes).to_string()
        };
        result.push(NodeGroup {
            id: deterministic_id,
            name: rg.name.clone(),
            group_type: rg.group_type,
            node_ids,
            test_url: rg.test_url.clone(),
            interval_secs: rg.interval_secs,
            sort_order: idx as i32,
        });
    }

    result
}

/// Parses a Clash / Mihomo YAML subscription document (`proxies:` list)
pub fn parse_clash_yaml(yaml_text: &str) -> ParseResult<(Vec<ProxyNode>, Vec<String>)> {
    let (nodes, _groups, warnings) = parse_clash_yaml_with_groups(yaml_text)?;
    Ok((nodes, warnings))
}

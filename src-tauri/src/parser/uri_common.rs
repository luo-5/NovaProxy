use crate::model::{RealityConfig, TlsConfig, TransportConfig};
use crate::parser::error::{ParseError, ParseResult};
use std::collections::BTreeMap;
use url::Url;

/// Normalizes a server host string by stripping surrounding `[` and `]` for IPv6 literals
pub fn normalize_host(host: &str) -> String {
    let trimmed = host.trim();
    if trimmed.starts_with('[') && trimmed.ends_with(']') && trimmed.len() > 2 {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

/// Formats a host for URI authority inclusion (adds `[` and `]` if the host is an IPv6 address)
pub fn format_host_for_uri(host: &str) -> String {
    let clean = normalize_host(host);
    if clean.contains(':') {
        format!("[{clean}]")
    } else {
        clean
    }
}

/// Parses `host:port` or `[ipv6]:port`
pub fn parse_host_port(input: &str) -> ParseResult<(String, u16)> {
    let trimmed = input.trim();
    if trimmed.starts_with('[') {
        let close_idx = trimmed
            .find(']')
            .ok_or_else(|| ParseError::InvalidUri(format!("unclosed IPv6 bracket in {trimmed}")))?;
        let host = trimmed[1..close_idx].to_string();
        let rest = &trimmed[close_idx + 1..];
        let port_str = rest
            .strip_prefix(':')
            .ok_or_else(|| ParseError::InvalidPort(format!("missing port in {trimmed}")))?;
        let port = port_str
            .parse::<u16>()
            .map_err(|_| ParseError::InvalidPort(port_str.to_string()))?;
        if host.is_empty() || port == 0 {
            return Err(ParseError::InvalidUri(format!(
                "invalid host/port in {trimmed}"
            )));
        }
        Ok((host, port))
    } else {
        let colon_idx = trimmed
            .rfind(':')
            .ok_or_else(|| ParseError::InvalidPort(format!("missing ':' in {trimmed}")))?;
        let host = trimmed[..colon_idx].trim().to_string();
        let port_str = trimmed[colon_idx + 1..].trim();
        let port = port_str
            .parse::<u16>()
            .map_err(|_| ParseError::InvalidPort(port_str.to_string()))?;
        if host.is_empty() || port == 0 {
            return Err(ParseError::InvalidUri(format!(
                "invalid host/port in {trimmed}"
            )));
        }
        Ok((host, port))
    }
}

/// Extracts normalized host and port from a parsed `url::Url`
pub fn extract_url_host_port(url: &Url, default_port: Option<u16>) -> ParseResult<(String, u16)> {
    let raw_host = url
        .host_str()
        .ok_or(ParseError::MissingField("server host"))?;
    let host = normalize_host(raw_host);
    if host.is_empty() {
        return Err(ParseError::MissingField("server host"));
    }
    let port = url
        .port()
        .or(default_port)
        .ok_or_else(|| ParseError::InvalidPort("missing port".to_string()))?;
    if port == 0 {
        return Err(ParseError::InvalidPort("0".to_string()));
    }
    Ok((host, port))
}

/// Parses a URL's query string into a map of decoded keys and values
pub fn parse_query_map(url: &Url) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for (k, v) in url.query_pairs() {
        map.insert(k.into_owned(), v.into_owned());
    }
    map
}

/// Parses a raw query string (`a=1&b=2`) into a map of decoded keys and values
pub fn parse_raw_query_map(query: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let trimmed = query.trim_start_matches('?');
    for (k, v) in url::form_urlencoded::parse(trimmed.as_bytes()) {
        map.insert(k.into_owned(), v.into_owned());
    }
    map
}

/// Encodes non-empty query key-value pairs into a query string (`k1=v1&k2=v2`)
pub fn encode_query_pairs(pairs: &[(&str, String)]) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (k, v) in pairs {
        if !v.is_empty() {
            serializer.append_pair(k, v);
        }
    }
    serializer.finish()
}

/// Decodes URL fragment (`#remarks`) or falls back to `server:port`
pub fn extract_remarks(url: &Url, server: &str, port: u16) -> String {
    if let Some(frag) = url.fragment() {
        let decoded = urlencoding::decode(frag)
            .map(|c| c.into_owned())
            .unwrap_or_else(|_| frag.to_string());
        let trimmed = decoded.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    format!("{}:{}", format_host_for_uri(server), port)
}

/// Percent-encodes node name for `#fragment`
pub fn encode_remarks(name: &str) -> String {
    urlencoding::encode(name).into_owned()
}

/// Parses common bool strings (`"1"`, `"true"`, `"yes"`)
pub fn parse_bool_flag(val: Option<&String>) -> bool {
    matches!(
        val.map(|s| s.trim().to_ascii_lowercase()).as_deref(),
        Some("1" | "true" | "yes")
    )
}

/// Parses standard v2rayN / Xray / sing-box query parameters for Transport & TLS/REALITY
pub fn parse_v2ray_transport_and_tls(
    params: &BTreeMap<String, String>,
    default_tls_enabled: bool,
) -> (Option<TransportConfig>, Option<TlsConfig>) {
    let net_type = params
        .get("type")
        .or_else(|| params.get("net"))
        .map(|s| s.trim().to_ascii_lowercase())
        .unwrap_or_else(|| "tcp".to_string());

    let host_param = params
        .get("host")
        .or_else(|| params.get("obfsParam"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let path_param = params
        .get("path")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let header_type = params
        .get("headerType")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "none");

    let transport = match net_type.as_str() {
        "tcp" | "raw" | "" => {
            if header_type.is_some() || host_param.is_some() || path_param.is_some() {
                let http_host = host_param
                    .as_deref()
                    .map(|h| {
                        h.split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect()
                    })
                    .unwrap_or_default();
                Some(TransportConfig::Tcp {
                    header_type,
                    http_host,
                    http_path: path_param,
                })
            } else {
                None
            }
        }
        "ws" | "websocket" => {
            let mut headers = BTreeMap::new();
            if let Some(ref h) = host_param {
                headers.insert("Host".to_string(), h.clone());
            }
            Some(TransportConfig::Ws {
                path: path_param.unwrap_or_else(|| "/".to_string()),
                host: host_param,
                headers,
                max_early_data: params.get("ed").and_then(|v| v.parse::<u32>().ok()),
                early_data_header_name: params.get("eh").filter(|s| !s.is_empty()).cloned(),
            })
        }
        "grpc" | "gun" => {
            let service_name = params
                .get("serviceName")
                .or(path_param.as_ref())
                .cloned()
                .unwrap_or_default();
            let mode = params
                .get("mode")
                .map(|m| m.to_ascii_lowercase())
                .unwrap_or_else(|| "gun".to_string());
            let authority = params
                .get("authority")
                .or(host_param.as_ref())
                .filter(|s| !s.is_empty())
                .cloned();
            Some(TransportConfig::Grpc {
                service_name,
                multi_mode: mode == "multi",
                authority,
            })
        }
        "http" | "h2" => {
            let host = host_param
                .as_deref()
                .map(|h| {
                    h.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            Some(TransportConfig::Http {
                host,
                path: path_param.unwrap_or_else(|| "/".to_string()),
                method: None,
                headers: BTreeMap::new(),
            })
        }
        "httpupgrade" => Some(TransportConfig::HttpUpgrade {
            host: host_param,
            path: path_param.unwrap_or_else(|| "/".to_string()),
            headers: BTreeMap::new(),
        }),
        "xhttp" | "splithttp" => Some(TransportConfig::Xhttp {
            host: host_param,
            path: path_param.unwrap_or_else(|| "/".to_string()),
            mode: params.get("mode").filter(|s| !s.is_empty()).cloned(),
            extra: params.get("extra").filter(|s| !s.is_empty()).cloned(),
        }),
        "kcp" | "mkcp" => Some(TransportConfig::Mkcp {
            seed: params
                .get("seed")
                .or(path_param.as_ref())
                .filter(|s| !s.is_empty())
                .cloned(),
            header_type,
            mtu: None,
            tti: None,
            uplink_capacity: None,
            downlink_capacity: None,
            congestion: false,
        }),
        "quic" => Some(TransportConfig::Quic {
            security: params
                .get("quicSecurity")
                .or(host_param.as_ref())
                .filter(|s| !s.is_empty())
                .cloned(),
            key: params
                .get("key")
                .or(path_param.as_ref())
                .filter(|s| !s.is_empty())
                .cloned(),
            header_type,
        }),
        _ => None,
    };

    // Security / TLS / REALITY
    let security = params
        .get("security")
        .or_else(|| params.get("tls"))
        .map(|s| s.trim().to_ascii_lowercase());

    let tls_enabled = match security.as_deref() {
        Some("tls" | "reality" | "xtls" | "1" | "true") => true,
        Some("none" | "0" | "false" | "") => false,
        None => default_tls_enabled,
        Some(_) => default_tls_enabled,
    };

    let tls = if tls_enabled {
        let sni = params
            .get("sni")
            .or_else(|| params.get("peer"))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let insecure = parse_bool_flag(params.get("allowInsecure"))
            || parse_bool_flag(params.get("insecure"))
            || parse_bool_flag(params.get("skip-cert-verify"));

        let alpn = params
            .get("alpn")
            .map(|a| {
                a.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let fingerprint = params
            .get("fp")
            .or_else(|| params.get("fingerprint"))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let reality = if security.as_deref() == Some("reality") || params.contains_key("pbk") {
            let pbk = params.get("pbk").cloned().unwrap_or_default();
            if !pbk.is_empty() {
                Some(RealityConfig {
                    public_key: pbk,
                    short_id: params.get("sid").cloned().unwrap_or_default(),
                    spider_x: params
                        .get("spx")
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty()),
                })
            } else {
                None
            }
        } else {
            None
        };

        Some(TlsConfig {
            enabled: true,
            server_name: sni,
            insecure,
            alpn,
            fingerprint,
            reality,
            ech_config: params.get("ech").filter(|s| !s.is_empty()).cloned(),
            fragment: None,
        })
    } else {
        None
    };

    (transport, tls)
}

/// Appends standard v2rayN / Xray query parameters for `TransportConfig` and `TlsConfig`
pub fn append_v2ray_transport_and_tls_params(
    pairs: &mut Vec<(&'static str, String)>,
    transport: &Option<TransportConfig>,
    tls: &Option<TlsConfig>,
    omit_default_tls: bool,
) {
    match transport {
        None => {
            pairs.push(("type", "tcp".to_string()));
        }
        Some(TransportConfig::Tcp {
            header_type,
            http_host,
            http_path,
        }) => {
            pairs.push(("type", "tcp".to_string()));
            if let Some(ht) = header_type {
                if !ht.is_empty() && ht != "none" {
                    pairs.push(("headerType", ht.clone()));
                }
            }
            if !http_host.is_empty() {
                pairs.push(("host", http_host.join(",")));
            }
            if let Some(p) = http_path {
                if !p.is_empty() {
                    pairs.push(("path", p.clone()));
                }
            }
        }
        Some(TransportConfig::Ws {
            path,
            host,
            headers,
            max_early_data,
            early_data_header_name,
        }) => {
            pairs.push(("type", "ws".to_string()));
            let effective_host = host
                .clone()
                .or_else(|| headers.get("Host").cloned())
                .or_else(|| headers.get("host").cloned());
            if let Some(h) = effective_host {
                if !h.is_empty() {
                    pairs.push(("host", h));
                }
            }
            if !path.is_empty() {
                pairs.push(("path", path.clone()));
            }
            if let Some(ed) = max_early_data {
                pairs.push(("ed", ed.to_string()));
            }
            if let Some(eh) = early_data_header_name {
                if !eh.is_empty() {
                    pairs.push(("eh", eh.clone()));
                }
            }
        }
        Some(TransportConfig::Grpc {
            service_name,
            multi_mode,
            authority,
        }) => {
            pairs.push(("type", "grpc".to_string()));
            if !service_name.is_empty() {
                pairs.push(("serviceName", service_name.clone()));
            }
            pairs.push((
                "mode",
                if *multi_mode {
                    "multi".to_string()
                } else {
                    "gun".to_string()
                },
            ));
            if let Some(auth) = authority {
                if !auth.is_empty() {
                    pairs.push(("authority", auth.clone()));
                }
            }
        }
        Some(TransportConfig::Http { host, path, .. }) => {
            pairs.push(("type", "http".to_string()));
            if !host.is_empty() {
                pairs.push(("host", host.join(",")));
            }
            if !path.is_empty() {
                pairs.push(("path", path.clone()));
            }
        }
        Some(TransportConfig::HttpUpgrade { host, path, .. }) => {
            pairs.push(("type", "httpupgrade".to_string()));
            if let Some(h) = host {
                if !h.is_empty() {
                    pairs.push(("host", h.clone()));
                }
            }
            if !path.is_empty() {
                pairs.push(("path", path.clone()));
            }
        }
        Some(TransportConfig::Xhttp {
            host,
            path,
            mode,
            extra,
        }) => {
            pairs.push(("type", "xhttp".to_string()));
            if let Some(h) = host {
                if !h.is_empty() {
                    pairs.push(("host", h.clone()));
                }
            }
            if !path.is_empty() {
                pairs.push(("path", path.clone()));
            }
            if let Some(m) = mode {
                if !m.is_empty() {
                    pairs.push(("mode", m.clone()));
                }
            }
            if let Some(ex) = extra {
                if !ex.is_empty() {
                    pairs.push(("extra", ex.clone()));
                }
            }
        }
        Some(TransportConfig::Mkcp {
            seed, header_type, ..
        }) => {
            pairs.push(("type", "kcp".to_string()));
            if let Some(ht) = header_type {
                if !ht.is_empty() && ht != "none" {
                    pairs.push(("headerType", ht.clone()));
                }
            }
            if let Some(s) = seed {
                if !s.is_empty() {
                    pairs.push(("seed", s.clone()));
                }
            }
        }
        Some(TransportConfig::Quic {
            security,
            key,
            header_type,
        }) => {
            pairs.push(("type", "quic".to_string()));
            if let Some(sec) = security {
                if !sec.is_empty() {
                    pairs.push(("quicSecurity", sec.clone()));
                }
            }
            if let Some(k) = key {
                if !k.is_empty() {
                    pairs.push(("key", k.clone()));
                }
            }
            if let Some(ht) = header_type {
                if !ht.is_empty() && ht != "none" {
                    pairs.push(("headerType", ht.clone()));
                }
            }
        }
    }

    match tls {
        Some(tls_cfg) if tls_cfg.enabled => {
            if let Some(ref reality) = tls_cfg.reality {
                pairs.push(("security", "reality".to_string()));
                if let Some(ref sni) = tls_cfg.server_name {
                    if !sni.is_empty() {
                        pairs.push(("sni", sni.clone()));
                    }
                }
                if let Some(ref fp) = tls_cfg.fingerprint {
                    if !fp.is_empty() {
                        pairs.push(("fp", fp.clone()));
                    }
                }
                pairs.push(("pbk", reality.public_key.clone()));
                if !reality.short_id.is_empty() {
                    pairs.push(("sid", reality.short_id.clone()));
                }
                if let Some(ref spx) = reality.spider_x {
                    if !spx.is_empty() {
                        pairs.push(("spx", spx.clone()));
                    }
                }
            } else {
                if !omit_default_tls {
                    pairs.push(("security", "tls".to_string()));
                }
                if let Some(ref sni) = tls_cfg.server_name {
                    if !sni.is_empty() {
                        pairs.push(("sni", sni.clone()));
                    }
                }
                if !tls_cfg.alpn.is_empty() {
                    pairs.push(("alpn", tls_cfg.alpn.join(",")));
                }
                if let Some(ref fp) = tls_cfg.fingerprint {
                    if !fp.is_empty() {
                        pairs.push(("fp", fp.clone()));
                    }
                }
                if tls_cfg.insecure {
                    pairs.push(("allowInsecure", "1".to_string()));
                }
                if let Some(ref ech) = tls_cfg.ech_config {
                    if !ech.is_empty() {
                        pairs.push(("ech", ech.clone()));
                    }
                }
            }
        }
        _ => {
            pairs.push(("security", "none".to_string()));
        }
    }
}

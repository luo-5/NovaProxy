use crate::model::{
    Hysteria2Config, HysteriaConfig, ProtocolConfig, ProtocolType, ProxyNode, TlsConfig,
};
use crate::parser::error::{ParseError, ParseResult};
use crate::parser::uri_common::{
    encode_query_pairs, encode_remarks, extract_remarks, extract_url_host_port,
    format_host_for_uri, parse_bool_flag, parse_query_map,
};
use url::Url;
use uuid::Uuid;

/// Parses a Hysteria v1 (`hysteria://` or `hy://`) share link into a `ProxyNode`
pub fn parse_v1(uri: &str) -> ParseResult<ProxyNode> {
    let trimmed = uri.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("hysteria://") && !lower.starts_with("hy://") {
        return Err(ParseError::UnsupportedScheme(trimmed.to_string()));
    }

    let url = Url::parse(trimmed).map_err(|e| ParseError::InvalidUri(e.to_string()))?;
    let (server, port) = extract_url_host_port(&url, Some(443))?;
    let params = parse_query_map(&url);
    let name = extract_remarks(&url, &server, port);

    let user_auth = if !url.username().is_empty() {
        Some(
            urlencoding::decode(url.username())
                .map(|s| s.into_owned())
                .unwrap_or_else(|_| url.username().to_string()),
        )
    } else {
        None
    };

    let auth_str = params
        .get("auth")
        .or_else(|| params.get("auth_str"))
        .cloned()
        .or(user_auth)
        .filter(|s| !s.is_empty());

    let protocol = params
        .get("protocol")
        .cloned()
        .filter(|s| !s.is_empty())
        .or_else(|| Some("udp".to_string()));

    let up_mbps = params
        .get("upmbps")
        .or_else(|| params.get("up"))
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(100);

    let down_mbps = params
        .get("downmbps")
        .or_else(|| params.get("down"))
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(100);

    let obfs = params
        .get("obfsParam")
        .or_else(|| params.get("obfs"))
        .cloned()
        .filter(|s| !s.is_empty() && s != "none");

    let sni = params
        .get("peer")
        .or_else(|| params.get("sni"))
        .cloned()
        .filter(|s| !s.is_empty());

    let insecure =
        parse_bool_flag(params.get("insecure")) || parse_bool_flag(params.get("allowInsecure"));

    let alpn = params
        .get("alpn")
        .map(|a| {
            a.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let tls = Some(TlsConfig {
        enabled: true,
        server_name: sni,
        insecure,
        alpn,
        fingerprint: params.get("fp").filter(|s| !s.is_empty()).cloned(),
        reality: None,
        ech_config: None,
        fragment: None,
    });

    Ok(ProxyNode {
        id: Uuid::new_v4().to_string(),
        name,
        server,
        port,
        protocol: ProtocolType::Hysteria,
        tls,
        transport: None,
        settings: ProtocolConfig::Hysteria(HysteriaConfig {
            auth_str,
            protocol,
            up_mbps,
            down_mbps,
            obfs,
            recv_window_conn: params
                .get("recv_window_conn")
                .and_then(|v| v.parse::<u64>().ok()),
            recv_window: params
                .get("recv_window")
                .and_then(|v| v.parse::<u64>().ok()),
            disable_mtu_discovery: None,
        }),
        subscription_id: None,
        group_id: None,
        latency: None,
        dialer_proxy: None,
        preferred_core: None,
        sort_order: 0,
    })
}

/// Serializes a `ProxyNode` of type `Hysteria` into a `hysteria://` URI
pub fn to_uri_v1(node: &ProxyNode) -> ParseResult<String> {
    let hy = match &node.settings {
        ProtocolConfig::Hysteria(c) => c,
        _ => {
            return Err(ParseError::InvalidUri(
                "node is not a Hysteria v1 node".to_string(),
            ))
        }
    };

    let mut pairs: Vec<(&'static str, String)> = Vec::new();
    if let Some(ref proto) = hy.protocol {
        if !proto.is_empty() {
            pairs.push(("protocol", proto.clone()));
        }
    }
    if let Some(ref auth) = hy.auth_str {
        if !auth.is_empty() {
            pairs.push(("auth", auth.clone()));
        }
    }
    if let Some(ref tls) = node.tls {
        if let Some(ref sni) = tls.server_name {
            if !sni.is_empty() {
                pairs.push(("peer", sni.clone()));
            }
        }
        if tls.insecure {
            pairs.push(("insecure", "1".to_string()));
        }
        if !tls.alpn.is_empty() {
            pairs.push(("alpn", tls.alpn.join(",")));
        }
    }
    pairs.push(("upmbps", hy.up_mbps.to_string()));
    pairs.push(("downmbps", hy.down_mbps.to_string()));
    if let Some(ref obfs) = hy.obfs {
        if !obfs.is_empty() {
            pairs.push(("obfs", "xplus".to_string()));
            pairs.push(("obfsParam", obfs.clone()));
        }
    }

    let query = encode_query_pairs(&pairs);
    let host = format_host_for_uri(&node.server);
    let remarks = encode_remarks(&node.name);
    Ok(format!("hysteria://{host}:{}?{query}#{remarks}", node.port))
}

/// Parses a Hysteria2 (`hysteria2://` or `hy2://`) share link into a `ProxyNode`
pub fn parse_v2(uri: &str) -> ParseResult<ProxyNode> {
    let trimmed = uri.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("hysteria2://") && !lower.starts_with("hy2://") {
        return Err(ParseError::UnsupportedScheme(trimmed.to_string()));
    }

    let url = Url::parse(trimmed).map_err(|e| ParseError::InvalidUri(e.to_string()))?;
    let (server, port) = extract_url_host_port(&url, Some(443))?;
    let params = parse_query_map(&url);
    let name = extract_remarks(&url, &server, port);

    let decoded_user = urlencoding::decode(url.username())
        .map(|s| s.into_owned())
        .unwrap_or_else(|_| url.username().to_string());
    let decoded_pass = url.password().map(|p| {
        urlencoding::decode(p)
            .map(|s| s.into_owned())
            .unwrap_or_else(|_| p.to_string())
    });

    let password = match (decoded_user.is_empty(), decoded_pass) {
        (false, Some(p)) => format!("{decoded_user}:{p}"),
        (false, None) => decoded_user,
        (true, Some(p)) => p,
        (true, None) => params
            .get("password")
            .or_else(|| params.get("auth"))
            .cloned()
            .unwrap_or_default(),
    };

    if password.is_empty() {
        return Err(ParseError::MissingField("password"));
    }

    let obfs_type = params
        .get("obfs")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s != "none");

    let obfs_password = params
        .get("obfs-password")
        .or_else(|| params.get("obfs_password"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let ports = params
        .get("mport")
        .or_else(|| params.get("ports"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let hop_interval = params
        .get("hop_interval")
        .or_else(|| params.get("hop-interval"))
        .and_then(|v| v.parse::<u32>().ok());

    let up_mbps = params
        .get("upmbps")
        .or_else(|| params.get("up"))
        .and_then(|v| v.parse::<u32>().ok());
    let down_mbps = params
        .get("downmbps")
        .or_else(|| params.get("down"))
        .and_then(|v| v.parse::<u32>().ok());

    let sni = params
        .get("sni")
        .or_else(|| params.get("peer"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let insecure =
        parse_bool_flag(params.get("insecure")) || parse_bool_flag(params.get("allowInsecure"));

    let alpn = params
        .get("alpn")
        .map(|a| {
            a.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let tls = Some(TlsConfig {
        enabled: true,
        server_name: sni,
        insecure,
        alpn,
        fingerprint: params
            .get("pinSHA256")
            .or_else(|| params.get("fp"))
            .filter(|s| !s.is_empty())
            .cloned(),
        reality: None,
        ech_config: None,
        fragment: None,
    });

    Ok(ProxyNode {
        id: Uuid::new_v4().to_string(),
        name,
        server,
        port,
        protocol: ProtocolType::Hysteria2,
        tls,
        transport: None,
        settings: ProtocolConfig::Hysteria2(Hysteria2Config {
            password,
            up_mbps,
            down_mbps,
            obfs_type,
            obfs_password,
            ports,
            hop_interval,
        }),
        subscription_id: None,
        group_id: None,
        latency: None,
        dialer_proxy: None,
        preferred_core: None,
        sort_order: 0,
    })
}

/// Serializes a `ProxyNode` of type `Hysteria2` into a `hysteria2://` URI
pub fn to_uri_v2(node: &ProxyNode) -> ParseResult<String> {
    let hy2 = match &node.settings {
        ProtocolConfig::Hysteria2(c) => c,
        _ => {
            return Err(ParseError::InvalidUri(
                "node is not a Hysteria2 node".to_string(),
            ))
        }
    };

    let mut pairs: Vec<(&'static str, String)> = Vec::new();
    if let Some(ref tls) = node.tls {
        if let Some(ref sni) = tls.server_name {
            if !sni.is_empty() {
                pairs.push(("sni", sni.clone()));
            }
        }
        if tls.insecure {
            pairs.push(("insecure", "1".to_string()));
        }
        if !tls.alpn.is_empty() {
            pairs.push(("alpn", tls.alpn.join(",")));
        }
        if let Some(ref fp) = tls.fingerprint {
            if !fp.is_empty() {
                pairs.push(("pinSHA256", fp.clone()));
            }
        }
    }
    if let Some(ref obfs) = hy2.obfs_type {
        if !obfs.is_empty() {
            pairs.push(("obfs", obfs.clone()));
        }
    }
    if let Some(ref obfs_pw) = hy2.obfs_password {
        if !obfs_pw.is_empty() {
            pairs.push(("obfs-password", obfs_pw.clone()));
        }
    }
    if let Some(ref ports) = hy2.ports {
        if !ports.is_empty() {
            pairs.push(("mport", ports.clone()));
        }
    }
    if let Some(hop) = hy2.hop_interval {
        pairs.push(("hop_interval", hop.to_string()));
    }
    if let Some(up) = hy2.up_mbps {
        pairs.push(("upmbps", up.to_string()));
    }
    if let Some(down) = hy2.down_mbps {
        pairs.push(("downmbps", down.to_string()));
    }

    let query = encode_query_pairs(&pairs);
    let user = urlencoding::encode(&hy2.password);
    let host = format_host_for_uri(&node.server);
    let remarks = encode_remarks(&node.name);

    if query.is_empty() {
        Ok(format!(
            "hysteria2://{user}@{host}:{}/#{remarks}",
            node.port
        ))
    } else {
        Ok(format!(
            "hysteria2://{user}@{host}:{}/?{query}#{remarks}",
            node.port
        ))
    }
}

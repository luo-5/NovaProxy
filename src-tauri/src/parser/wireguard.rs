use crate::model::{ProtocolConfig, ProtocolType, ProxyNode, WireGuardConfig};
use crate::parser::error::{ParseError, ParseResult};
use crate::parser::uri_common::{
    encode_query_pairs, encode_remarks, extract_remarks, extract_url_host_port,
    format_host_for_uri, parse_host_port, parse_query_map,
};
use url::Url;
use uuid::Uuid;

fn parse_reserved_bytes(raw: &str) -> Option<[u8; 3]> {
    let nums: Vec<u8> = raw
        .split(',')
        .filter_map(|s| s.trim().parse::<u8>().ok())
        .collect();
    if nums.len() == 3 {
        Some([nums[0], nums[1], nums[2]])
    } else {
        None
    }
}

/// Parses a `wireguard://` or `wg://` share link (or a WireGuard `.conf` INI block) into a `ProxyNode`
pub fn parse(uri: &str) -> ParseResult<ProxyNode> {
    let trimmed = uri.trim();
    if trimmed.contains("[Interface]") && trimmed.contains("[Peer]") {
        return parse_wg_conf(trimmed);
    }

    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("wireguard://") && !lower.starts_with("wg://") {
        return Err(ParseError::UnsupportedScheme(trimmed.to_string()));
    }

    let url = Url::parse(trimmed).map_err(|e| ParseError::InvalidUri(e.to_string()))?;
    let (server, port) = extract_url_host_port(&url, Some(51820))?;
    let params = parse_query_map(&url);
    let name = extract_remarks(&url, &server, port);

    let private_key = if !url.username().is_empty() {
        urlencoding::decode(url.username())
            .map(|s| s.into_owned())
            .unwrap_or_else(|_| url.username().to_string())
    } else {
        params
            .get("privatekey")
            .or_else(|| params.get("private_key"))
            .cloned()
            .unwrap_or_default()
    };

    if private_key.trim().is_empty() {
        return Err(ParseError::MissingField("private_key"));
    }

    let public_key = params
        .get("publickey")
        .or_else(|| params.get("public_key"))
        .or_else(|| params.get("peer_public_key"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or(ParseError::MissingField("publickey"))?;

    let preshared_key = params
        .get("presharedkey")
        .or_else(|| params.get("preshared_key"))
        .or_else(|| params.get("psk"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let local_address: Vec<String> = params
        .get("address")
        .or_else(|| params.get("ip"))
        .map(|a| {
            a.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .filter(|v: &Vec<String>| !v.is_empty())
        .unwrap_or_else(|| vec!["10.0.0.2/32".to_string()]);

    let reserved = params.get("reserved").and_then(|r| parse_reserved_bytes(r));

    let mtu = params.get("mtu").and_then(|m| m.parse::<u16>().ok());
    let workers = params.get("workers").and_then(|w| w.parse::<u8>().ok());

    Ok(ProxyNode {
        id: Uuid::new_v4().to_string(),
        name,
        server,
        port,
        protocol: ProtocolType::WireGuard,
        tls: None,
        transport: None,
        settings: ProtocolConfig::WireGuard(WireGuardConfig {
            private_key: private_key.trim().to_string(),
            public_key,
            preshared_key,
            local_address,
            reserved,
            mtu,
            workers,
        }),
        subscription_id: None,
        group_id: None,
        latency: None,
        dialer_proxy: None,
        preferred_core: None,
        sort_order: 0,
    })
}

/// Parses a standard WireGuard `.conf` INI configuration text
pub fn parse_wg_conf(conf: &str) -> ParseResult<ProxyNode> {
    let mut private_key = String::new();
    let mut public_key = String::new();
    let mut preshared_key = None;
    let mut local_address = Vec::new();
    let mut mtu = None;
    let mut reserved = None;
    let mut endpoint = String::new();
    let mut name = None;

    for raw_line in conf.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('[') {
            continue;
        }
        if let Some(comment) = line.strip_prefix('#') {
            let trimmed_comment = comment.trim();
            if name.is_none() && !trimmed_comment.is_empty() {
                name = Some(trimmed_comment.to_string());
            }
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let key = k.trim().to_ascii_lowercase();
            let val = v.trim();
            match key.as_str() {
                "privatekey" => private_key = val.to_string(),
                "publickey" => public_key = val.to_string(),
                "presharedkey" => {
                    if !val.is_empty() {
                        preshared_key = Some(val.to_string());
                    }
                }
                "address" => {
                    local_address = val
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                "mtu" => mtu = val.parse::<u16>().ok(),
                "reserved" => reserved = parse_reserved_bytes(val),
                "endpoint" => endpoint = val.to_string(),
                _ => {}
            }
        }
    }

    if private_key.is_empty() {
        return Err(ParseError::MissingField("PrivateKey"));
    }
    if public_key.is_empty() {
        return Err(ParseError::MissingField("PublicKey"));
    }
    if endpoint.is_empty() {
        return Err(ParseError::MissingField("Endpoint"));
    }

    let (server, port) = parse_host_port(&endpoint)?;
    if local_address.is_empty() {
        local_address.push("10.0.0.2/32".to_string());
    }

    let node_name = name.unwrap_or_else(|| format!("{}:{}", format_host_for_uri(&server), port));

    Ok(ProxyNode {
        id: Uuid::new_v4().to_string(),
        name: node_name,
        server,
        port,
        protocol: ProtocolType::WireGuard,
        tls: None,
        transport: None,
        settings: ProtocolConfig::WireGuard(WireGuardConfig {
            private_key,
            public_key,
            preshared_key,
            local_address,
            reserved,
            mtu,
            workers: None,
        }),
        subscription_id: None,
        group_id: None,
        latency: None,
        dialer_proxy: None,
        preferred_core: None,
        sort_order: 0,
    })
}

/// Serializes a `ProxyNode` of type `WireGuard` into a `wireguard://` URI
pub fn to_uri(node: &ProxyNode) -> ParseResult<String> {
    let wg = match &node.settings {
        ProtocolConfig::WireGuard(c) => c,
        _ => {
            return Err(ParseError::InvalidUri(
                "node is not a WireGuard node".to_string(),
            ))
        }
    };

    let mut pairs: Vec<(&'static str, String)> = Vec::new();
    pairs.push(("publickey", wg.public_key.clone()));
    if let Some(ref psk) = wg.preshared_key {
        if !psk.is_empty() {
            pairs.push(("presharedkey", psk.clone()));
        }
    }
    if !wg.local_address.is_empty() {
        pairs.push(("address", wg.local_address.join(",")));
    }
    if let Some(res) = wg.reserved {
        pairs.push(("reserved", format!("{},{},{}", res[0], res[1], res[2])));
    }
    if let Some(mtu) = wg.mtu {
        pairs.push(("mtu", mtu.to_string()));
    }

    let query = encode_query_pairs(&pairs);
    let user = urlencoding::encode(&wg.private_key);
    let host = format_host_for_uri(&node.server);
    let remarks = encode_remarks(&node.name);

    Ok(format!(
        "wireguard://{user}@{host}:{}?{query}#{remarks}",
        node.port
    ))
}

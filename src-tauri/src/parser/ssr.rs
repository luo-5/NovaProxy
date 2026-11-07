use crate::model::{ProtocolConfig, ProtocolType, ProxyNode, ShadowsocksRConfig};
use crate::parser::base64::{decode_base64_to_string, encode_base64_url_nopad};
use crate::parser::error::{ParseError, ParseResult};
use crate::parser::uri_common::{format_host_for_uri, normalize_host, parse_raw_query_map};
use uuid::Uuid;

/// Parses a `ssr://` share link into a `ProxyNode`
pub fn parse(uri: &str) -> ParseResult<ProxyNode> {
    let trimmed = uri.trim();
    let b64_payload = trimmed
        .strip_prefix("ssr://")
        .or_else(|| trimmed.strip_prefix("SSR://"))
        .ok_or_else(|| ParseError::UnsupportedScheme(trimmed.to_string()))?;

    let decoded = decode_base64_to_string(b64_payload)?;
    let (main_part, query_opt) = match decoded.split_once("/?") {
        Some((m, q)) => (m, Some(q)),
        None => match decoded.split_once('?') {
            Some((m, q)) => (m.trim_end_matches('/'), Some(q)),
            None => (decoded.trim_end_matches('/'), None),
        },
    };

    // Split from right so IPv6 addresses with colons in `server` are preserved
    let parts: Vec<&str> = main_part.rsplitn(6, ':').collect();
    if parts.len() != 6 {
        return Err(ParseError::InvalidUri(
            "SSR link must have 6 colon-separated fields".to_string(),
        ));
    }

    let password_b64 = parts[0];
    let obfs = parts[1].trim().to_string();
    let method = parts[2].trim().to_string();
    let protocol = parts[3].trim().to_string();
    let port_str = parts[4].trim();
    let server = normalize_host(parts[5]);

    if server.is_empty() {
        return Err(ParseError::MissingField("server"));
    }
    let port = port_str
        .parse::<u16>()
        .ok()
        .filter(|&p| p > 0)
        .ok_or_else(|| ParseError::InvalidPort(port_str.to_string()))?;

    let password = decode_base64_to_string(password_b64)?;
    if method.is_empty() {
        return Err(ParseError::MissingField("method"));
    }
    if protocol.is_empty() {
        return Err(ParseError::MissingField("protocol"));
    }
    if obfs.is_empty() {
        return Err(ParseError::MissingField("obfs"));
    }

    let params = query_opt.map(parse_raw_query_map).unwrap_or_default();

    let obfs_param = params
        .get("obfsparam")
        .filter(|s| !s.is_empty())
        .and_then(|s| decode_base64_to_string(s).ok())
        .filter(|s| !s.is_empty());

    let protocol_param = params
        .get("protoparam")
        .filter(|s| !s.is_empty())
        .and_then(|s| decode_base64_to_string(s).ok())
        .filter(|s| !s.is_empty());

    let remarks = params
        .get("remarks")
        .filter(|s| !s.is_empty())
        .and_then(|s| decode_base64_to_string(s).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{}:{}", format_host_for_uri(&server), port));

    Ok(ProxyNode {
        id: Uuid::new_v4().to_string(),
        name: remarks,
        server,
        port,
        protocol: ProtocolType::ShadowsocksR,
        tls: None,
        transport: None,
        settings: ProtocolConfig::ShadowsocksR(ShadowsocksRConfig {
            method,
            password,
            protocol,
            protocol_param,
            obfs,
            obfs_param,
        }),
        subscription_id: None,
        group_id: None,
        latency: None,
        dialer_proxy: None,
        preferred_core: None,
        sort_order: 0,
    })
}

/// Serializes a `ProxyNode` of type `ShadowsocksR` into a `ssr://` URI
pub fn to_uri(node: &ProxyNode) -> ParseResult<String> {
    let ssr = match &node.settings {
        ProtocolConfig::ShadowsocksR(c) => c,
        _ => {
            return Err(ParseError::InvalidUri(
                "node is not a ShadowsocksR node".to_string(),
            ))
        }
    };

    let pass_b64 = encode_base64_url_nopad(ssr.password.as_bytes());
    let remarks_b64 = encode_base64_url_nopad(node.name.as_bytes());
    let obfsparam_b64 = ssr
        .obfs_param
        .as_deref()
        .map(|s| encode_base64_url_nopad(s.as_bytes()))
        .unwrap_or_default();
    let protoparam_b64 = ssr
        .protocol_param
        .as_deref()
        .map(|s| encode_base64_url_nopad(s.as_bytes()))
        .unwrap_or_default();

    let server_str = normalize_host(&node.server);
    let raw = format!(
        "{}:{}:{}:{}:{}:{}/?obfsparam={}&protoparam={}&remarks={}",
        server_str,
        node.port,
        ssr.protocol,
        ssr.method,
        ssr.obfs,
        pass_b64,
        obfsparam_b64,
        protoparam_b64,
        remarks_b64
    );

    Ok(format!("ssr://{}", encode_base64_url_nopad(raw.as_bytes())))
}

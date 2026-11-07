use crate::model::{ProtocolConfig, ProtocolType, ProxyNode, TrojanConfig};
use crate::parser::error::{ParseError, ParseResult};
use crate::parser::uri_common::{
    append_v2ray_transport_and_tls_params, encode_query_pairs, encode_remarks, extract_remarks,
    extract_url_host_port, format_host_for_uri, parse_query_map, parse_v2ray_transport_and_tls,
};
use url::Url;
use uuid::Uuid;

/// Parses a `trojan://` share link into a `ProxyNode`
pub fn parse(uri: &str) -> ParseResult<ProxyNode> {
    let trimmed = uri.trim();
    if !trimmed.to_ascii_lowercase().starts_with("trojan://") {
        return Err(ParseError::UnsupportedScheme(trimmed.to_string()));
    }

    let url = Url::parse(trimmed).map_err(|e| ParseError::InvalidUri(e.to_string()))?;
    let password = urlencoding::decode(url.username())
        .map(|s| s.into_owned())
        .unwrap_or_else(|_| url.username().to_string());
    if password.is_empty() {
        return Err(ParseError::MissingField("password"));
    }

    let (server, port) = extract_url_host_port(&url, Some(443))?;
    let params = parse_query_map(&url);
    // Trojan defaults to TLS enabled unless security=none
    let (transport, tls) = parse_v2ray_transport_and_tls(&params, true);
    let name = extract_remarks(&url, &server, port);

    Ok(ProxyNode {
        id: Uuid::new_v4().to_string(),
        name,
        server,
        port,
        protocol: ProtocolType::Trojan,
        tls,
        transport,
        settings: ProtocolConfig::Trojan(TrojanConfig { password }),
        subscription_id: None,
        group_id: None,
        latency: None,
        dialer_proxy: None,
        preferred_core: None,
        sort_order: 0,
    })
}

/// Serializes a `ProxyNode` of type `Trojan` into a `trojan://` URI
pub fn to_uri(node: &ProxyNode) -> ParseResult<String> {
    let trojan = match &node.settings {
        ProtocolConfig::Trojan(c) => c,
        _ => {
            return Err(ParseError::InvalidUri(
                "node is not a Trojan node".to_string(),
            ))
        }
    };

    let mut pairs: Vec<(&'static str, String)> = Vec::new();
    append_v2ray_transport_and_tls_params(&mut pairs, &node.transport, &node.tls, false);

    let query = encode_query_pairs(&pairs);
    let user = urlencoding::encode(&trojan.password);
    let host = format_host_for_uri(&node.server);
    let remarks = encode_remarks(&node.name);

    if query.is_empty() {
        Ok(format!("trojan://{user}@{host}:{}#{remarks}", node.port))
    } else {
        Ok(format!(
            "trojan://{user}@{host}:{}?{query}#{remarks}",
            node.port
        ))
    }
}

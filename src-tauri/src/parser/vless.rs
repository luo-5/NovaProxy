use crate::model::{ProtocolConfig, ProtocolType, ProxyNode, VlessConfig};
use crate::parser::error::{ParseError, ParseResult};
use crate::parser::uri_common::{
    append_v2ray_transport_and_tls_params, encode_query_pairs, encode_remarks, extract_remarks,
    extract_url_host_port, format_host_for_uri, parse_query_map, parse_v2ray_transport_and_tls,
};
use url::Url;
use uuid::Uuid;

/// Parses a `vless://` share link into a `ProxyNode`
pub fn parse(uri: &str) -> ParseResult<ProxyNode> {
    let trimmed = uri.trim();
    if !trimmed.to_ascii_lowercase().starts_with("vless://") {
        return Err(ParseError::UnsupportedScheme(trimmed.to_string()));
    }

    let url = Url::parse(trimmed).map_err(|e| ParseError::InvalidUri(e.to_string()))?;
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

    let flow = params
        .get("flow")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let encryption = params
        .get("encryption")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "none".to_string());

    let packet_encoding = params
        .get("packetEncoding")
        .or_else(|| params.get("packet_encoding"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    Ok(ProxyNode {
        id: Uuid::new_v4().to_string(),
        name,
        server,
        port,
        protocol: ProtocolType::Vless,
        tls,
        transport,
        settings: ProtocolConfig::Vless(VlessConfig {
            uuid: uuid.trim().to_string(),
            flow,
            encryption,
            packet_encoding,
        }),
        subscription_id: None,
        group_id: None,
        latency: None,
        dialer_proxy: None,
        preferred_core: None,
        sort_order: 0,
    })
}

/// Serializes a `ProxyNode` of type `Vless` into a `vless://` URI
pub fn to_uri(node: &ProxyNode) -> ParseResult<String> {
    let vless = match &node.settings {
        ProtocolConfig::Vless(c) => c,
        _ => {
            return Err(ParseError::InvalidUri(
                "node is not a VLESS node".to_string(),
            ))
        }
    };

    let mut pairs: Vec<(&'static str, String)> = Vec::new();
    let enc = if vless.encryption.trim().is_empty() {
        "none".to_string()
    } else {
        vless.encryption.trim().to_string()
    };
    pairs.push(("encryption", enc));

    if let Some(ref flow) = vless.flow {
        if !flow.is_empty() {
            pairs.push(("flow", flow.clone()));
        }
    }
    if let Some(ref pe) = vless.packet_encoding {
        if !pe.is_empty() {
            pairs.push(("packetEncoding", pe.clone()));
        }
    }

    append_v2ray_transport_and_tls_params(&mut pairs, &node.transport, &node.tls, false);

    let query = encode_query_pairs(&pairs);
    let user = urlencoding::encode(&vless.uuid);
    let host = format_host_for_uri(&node.server);
    let remarks = encode_remarks(&node.name);

    if query.is_empty() {
        Ok(format!("vless://{user}@{host}:{}#{remarks}", node.port))
    } else {
        Ok(format!(
            "vless://{user}@{host}:{}?{query}#{remarks}",
            node.port
        ))
    }
}

use crate::model::{ProtocolConfig, ProtocolType, ProxyNode, TlsConfig, TuicConfig};
use crate::parser::error::{ParseError, ParseResult};
use crate::parser::uri_common::{
    encode_query_pairs, encode_remarks, extract_remarks, extract_url_host_port,
    format_host_for_uri, parse_bool_flag, parse_query_map,
};
use url::Url;
use uuid::Uuid;

/// Parses a `tuic://` share link into a `ProxyNode`
pub fn parse(uri: &str) -> ParseResult<ProxyNode> {
    let trimmed = uri.trim();
    if !trimmed.to_ascii_lowercase().starts_with("tuic://") {
        return Err(ParseError::UnsupportedScheme(trimmed.to_string()));
    }

    let url = Url::parse(trimmed).map_err(|e| ParseError::InvalidUri(e.to_string()))?;
    let (server, port) = extract_url_host_port(&url, Some(443))?;
    let params = parse_query_map(&url);
    let name = extract_remarks(&url, &server, port);

    let decoded_user = urlencoding::decode(url.username())
        .map(|s| s.into_owned())
        .unwrap_or_else(|_| url.username().to_string());

    let (uuid, password) = match url.password() {
        Some(raw_pass) => {
            let pass = urlencoding::decode(raw_pass)
                .map(|s| s.into_owned())
                .unwrap_or_else(|_| raw_pass.to_string());
            (decoded_user, pass)
        }
        None => match decoded_user.split_once(':') {
            Some((u, p)) => (u.to_string(), p.to_string()),
            None => (
                decoded_user,
                params.get("password").cloned().unwrap_or_default(),
            ),
        },
    };

    if uuid.trim().is_empty() {
        return Err(ParseError::MissingField("uuid"));
    }

    let congestion_control = params
        .get("congestion_control")
        .or_else(|| params.get("congestion-control"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let udp_relay_mode = params
        .get("udp_relay_mode")
        .or_else(|| params.get("udp-relay-mode"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let zero_rtt_handshake =
        if params.contains_key("reduce_rtt") || params.contains_key("zero_rtt_handshake") {
            Some(
                parse_bool_flag(params.get("reduce_rtt"))
                    || parse_bool_flag(params.get("zero_rtt_handshake")),
            )
        } else {
            None
        };

    let heartbeat = params
        .get("heartbeat")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let sni = params
        .get("sni")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let insecure = parse_bool_flag(params.get("allow_insecure"))
        || parse_bool_flag(params.get("allowInsecure"))
        || parse_bool_flag(params.get("insecure"));

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
        protocol: ProtocolType::Tuic,
        tls,
        transport: None,
        settings: ProtocolConfig::Tuic(TuicConfig {
            uuid: uuid.trim().to_string(),
            password,
            congestion_control,
            udp_relay_mode,
            zero_rtt_handshake,
            heartbeat,
        }),
        subscription_id: None,
        group_id: None,
        latency: None,
        dialer_proxy: None,
        preferred_core: None,
        sort_order: 0,
    })
}

/// Serializes a `ProxyNode` of type `Tuic` into a `tuic://` URI
pub fn to_uri(node: &ProxyNode) -> ParseResult<String> {
    let tuic = match &node.settings {
        ProtocolConfig::Tuic(c) => c,
        _ => {
            return Err(ParseError::InvalidUri(
                "node is not a TUIC node".to_string(),
            ))
        }
    };

    let mut pairs: Vec<(&'static str, String)> = Vec::new();
    if let Some(ref cc) = tuic.congestion_control {
        if !cc.is_empty() {
            pairs.push(("congestion_control", cc.clone()));
        }
    }
    if let Some(ref urm) = tuic.udp_relay_mode {
        if !urm.is_empty() {
            pairs.push(("udp_relay_mode", urm.clone()));
        }
    }
    if let Some(true) = tuic.zero_rtt_handshake {
        pairs.push(("reduce_rtt", "1".to_string()));
    }
    if let Some(ref hb) = tuic.heartbeat {
        if !hb.is_empty() {
            pairs.push(("heartbeat", hb.clone()));
        }
    }
    if let Some(ref tls) = node.tls {
        if let Some(ref sni) = tls.server_name {
            if !sni.is_empty() {
                pairs.push(("sni", sni.clone()));
            }
        }
        if !tls.alpn.is_empty() {
            pairs.push(("alpn", tls.alpn.join(",")));
        }
        if tls.insecure {
            pairs.push(("allow_insecure", "1".to_string()));
        }
    }

    let query = encode_query_pairs(&pairs);
    let user = urlencoding::encode(&tuic.uuid);
    let pass = urlencoding::encode(&tuic.password);
    let host = format_host_for_uri(&node.server);
    let remarks = encode_remarks(&node.name);

    if query.is_empty() {
        Ok(format!(
            "tuic://{user}:{pass}@{host}:{}#{remarks}",
            node.port
        ))
    } else {
        Ok(format!(
            "tuic://{user}:{pass}@{host}:{}?{query}#{remarks}",
            node.port
        ))
    }
}

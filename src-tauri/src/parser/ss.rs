use crate::model::{ProtocolConfig, ProtocolType, ProxyNode, ShadowsocksConfig};
use crate::parser::base64::{decode_base64_to_string, encode_base64_url_nopad};
use crate::parser::error::{ParseError, ParseResult};
use crate::parser::uri_common::{
    encode_query_pairs, encode_remarks, format_host_for_uri, parse_bool_flag, parse_host_port,
    parse_raw_query_map,
};
use uuid::Uuid;

/// Parses a `ss://` share link (SIP002 or Legacy Base64) into a `ProxyNode`
pub fn parse(uri: &str) -> ParseResult<ProxyNode> {
    let trimmed = uri.trim();
    let rest = trimmed
        .strip_prefix("ss://")
        .or_else(|| trimmed.strip_prefix("SS://"))
        .ok_or_else(|| ParseError::UnsupportedScheme(trimmed.to_string()))?;

    if rest.is_empty() {
        return Err(ParseError::EmptyInput);
    }

    // Extract fragment (#remarks)
    let (without_frag, frag_opt) = match rest.split_once('#') {
        Some((before, frag)) => (before, Some(frag)),
        None => (rest, None),
    };

    // Extract query (?plugin=...)
    let (main_part, query_opt) = match without_frag.split_once('?') {
        Some((before, q)) => (before.trim_end_matches('/'), Some(q)),
        None => (without_frag.trim_end_matches('/'), None),
    };

    let (method, password, server, port) =
        if let Some((user_part, host_port_part)) = main_part.rsplit_once('@') {
            // SIP002 format: user_part@host:port
            let (server, port) = parse_host_port(host_port_part)?;
            let decoded_user = if user_part.contains(':') {
                let u = urlencoding::decode(user_part)
                    .map(|s| s.into_owned())
                    .unwrap_or_else(|_| user_part.to_string());
                u
            } else {
                let pct_decoded = urlencoding::decode(user_part)
                    .map(|s| s.into_owned())
                    .unwrap_or_else(|_| user_part.to_string());
                decode_base64_to_string(&pct_decoded)?
            };
            let (m, p) = decoded_user.split_once(':').ok_or_else(|| {
                ParseError::InvalidUri("missing ':' in shadowsocks userinfo".to_string())
            })?;
            (m.trim().to_string(), p.to_string(), server, port)
        } else {
            // Legacy Base64 format: base64(method:password@host:port)
            let decoded = decode_base64_to_string(main_part)?;
            let (user_part, host_port_part) = decoded.rsplit_once('@').ok_or_else(|| {
                ParseError::InvalidUri("missing '@' in legacy shadowsocks payload".to_string())
            })?;
            let (server, port) = parse_host_port(host_port_part)?;
            let (m, p) = user_part.split_once(':').ok_or_else(|| {
                ParseError::InvalidUri("missing ':' in shadowsocks userinfo".to_string())
            })?;
            (m.trim().to_string(), p.to_string(), server, port)
        };

    if method.is_empty() {
        return Err(ParseError::MissingField("method"));
    }
    if password.is_empty() {
        return Err(ParseError::MissingField("password"));
    }

    let params = query_opt.map(parse_raw_query_map).unwrap_or_default();
    let (plugin, plugin_opts) = match params.get("plugin") {
        Some(raw_plugin) if !raw_plugin.trim().is_empty() => match raw_plugin.split_once(';') {
            Some((p_name, p_opts)) => (
                Some(p_name.trim().to_string()),
                Some(p_opts.trim().to_string()).filter(|s| !s.is_empty()),
            ),
            None => (Some(raw_plugin.trim().to_string()), None),
        },
        _ => (None, None),
    };

    let uot = if params.contains_key("uot") {
        Some(parse_bool_flag(params.get("uot")))
    } else {
        None
    };

    let name = match frag_opt {
        Some(frag) => {
            let decoded = urlencoding::decode(frag)
                .map(|s| s.into_owned())
                .unwrap_or_else(|_| frag.to_string());
            let t = decoded.trim();
            if t.is_empty() {
                format!("{}:{}", format_host_for_uri(&server), port)
            } else {
                t.to_string()
            }
        }
        None => format!("{}:{}", format_host_for_uri(&server), port),
    };

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
            uot,
        }),
        subscription_id: None,
        group_id: None,
        latency: None,
        dialer_proxy: None,
        preferred_core: None,
        sort_order: 0,
    })
}

/// Serializes a `ProxyNode` of type `Shadowsocks` into a SIP002 `ss://` URI
pub fn to_uri(node: &ProxyNode) -> ParseResult<String> {
    let ss = match &node.settings {
        ProtocolConfig::Shadowsocks(c) => c,
        _ => {
            return Err(ParseError::InvalidUri(
                "node is not a Shadowsocks node".to_string(),
            ))
        }
    };

    let userinfo = format!("{}:{}", ss.method, ss.password);
    let encoded_user = encode_base64_url_nopad(userinfo.as_bytes());
    let host = format_host_for_uri(&node.server);
    let remarks = encode_remarks(&node.name);

    let mut pairs: Vec<(&'static str, String)> = Vec::new();
    if let Some(ref plugin) = ss.plugin {
        if !plugin.is_empty() {
            let full_plugin = match &ss.plugin_opts {
                Some(opts) if !opts.is_empty() => format!("{plugin};{opts}"),
                _ => plugin.clone(),
            };
            pairs.push(("plugin", full_plugin));
        }
    }
    if let Some(true) = ss.uot {
        pairs.push(("uot", "1".to_string()));
    }

    let query = encode_query_pairs(&pairs);
    if query.is_empty() {
        Ok(format!(
            "ss://{encoded_user}@{host}:{}#{remarks}",
            node.port
        ))
    } else {
        Ok(format!(
            "ss://{encoded_user}@{host}:{}/?{query}#{remarks}",
            node.port
        ))
    }
}

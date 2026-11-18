use crate::model::{
    AnyTlsConfig, HttpConfig, MieruConfig, NaiveConfig, ProtocolConfig, ProtocolType, ProxyNode,
    ShadowTlsConfig, SnellConfig, Socks5Config, SshConfig, TlsConfig,
};
use crate::parser::base64::decode_base64_to_string;
use crate::parser::error::{ParseError, ParseResult};
use crate::parser::uri_common::{
    append_v2ray_transport_and_tls_params, encode_query_pairs, encode_remarks, extract_remarks,
    extract_url_host_port, format_host_for_uri, parse_bool_flag, parse_query_map,
    parse_v2ray_transport_and_tls,
};
use std::collections::BTreeMap;
use url::Url;
use uuid::Uuid;

fn extract_user_pass(url: &Url) -> (Option<String>, Option<String>) {
    let raw_user = url.username();
    let raw_pass = url.password();

    if raw_user.is_empty() && raw_pass.is_none() {
        return (None, None);
    }

    let decoded_user = urlencoding::decode(raw_user)
        .map(|s| s.into_owned())
        .unwrap_or_else(|_| raw_user.to_string());

    if let Some(p) = raw_pass {
        let decoded_pass = urlencoding::decode(p)
            .map(|s| s.into_owned())
            .unwrap_or_else(|_| p.to_string());
        return (
            Some(decoded_user).filter(|s| !s.is_empty()),
            Some(decoded_pass),
        );
    }

    // Check if username is base64(user:pass) (common in v2rayN socks:// links)
    if !decoded_user.contains(':') {
        if let Ok(b64_str) = decode_base64_to_string(&decoded_user) {
            if let Some((u, p)) = b64_str.split_once(':') {
                return (Some(u.to_string()), Some(p.to_string()));
            }
        }
    } else if let Some((u, p)) = decoded_user.split_once(':') {
        return (Some(u.to_string()), Some(p.to_string()));
    }

    (Some(decoded_user).filter(|s| !s.is_empty()), None)
}

/// Parses `anytls://` share links
pub fn parse_anytls(uri: &str) -> ParseResult<ProxyNode> {
    let url = Url::parse(uri.trim()).map_err(|e| ParseError::InvalidUri(e.to_string()))?;
    let (server, port) = extract_url_host_port(&url, Some(443))?;
    let params = parse_query_map(&url);
    let name = extract_remarks(&url, &server, port);

    let password = url
        .password()
        .and_then(|p| {
            let decoded = urlencoding::decode(p)
                .map(|s| s.into_owned())
                .unwrap_or_else(|_| p.to_string());
            (!decoded.is_empty()).then_some(decoded)
        })
        .or_else(|| {
            let raw_user = url.username();
            if raw_user.is_empty() {
                return None;
            }
            let decoded_user = urlencoding::decode(raw_user)
                .map(|s| s.into_owned())
                .unwrap_or_else(|_| raw_user.to_string());
            if let Some((_, pass)) = decoded_user.split_once(':') {
                (!pass.is_empty()).then_some(pass.to_string())
            } else {
                (!decoded_user.is_empty()).then_some(decoded_user)
            }
        })
        .or_else(|| params.get("password").filter(|s| !s.is_empty()).cloned())
        .ok_or(ParseError::MissingField("password"))?;

    let (transport, tls) = parse_v2ray_transport_and_tls(&params, true);

    Ok(ProxyNode {
        id: Uuid::new_v4().to_string(),
        name,
        server,
        port,
        protocol: ProtocolType::AnyTls,
        tls,
        transport,
        settings: ProtocolConfig::AnyTls(AnyTlsConfig {
            password,
            idle_session_check_interval: params.get("idle_session_check_interval").cloned(),
            idle_session_timeout: params.get("idle_session_timeout").cloned(),
            min_idle_session: params
                .get("min_idle_session")
                .and_then(|v| v.parse::<u32>().ok()),
        }),
        subscription_id: None,
        group_id: None,
        latency: None,
        dialer_proxy: None,
        preferred_core: None,
        sort_order: 0,
    })
}

pub fn to_uri_anytls(node: &ProxyNode) -> ParseResult<String> {
    let cfg = match &node.settings {
        ProtocolConfig::AnyTls(c) => c,
        _ => return Err(ParseError::InvalidUri("not an AnyTLS node".to_string())),
    };
    let mut pairs: Vec<(&'static str, String)> = Vec::new();
    append_v2ray_transport_and_tls_params(&mut pairs, &node.transport, &node.tls, true);
    if let Some(ref v) = cfg.idle_session_check_interval {
        pairs.push(("idle_session_check_interval", v.clone()));
    }
    if let Some(ref v) = cfg.idle_session_timeout {
        pairs.push(("idle_session_timeout", v.clone()));
    }
    if let Some(v) = cfg.min_idle_session {
        pairs.push(("min_idle_session", v.to_string()));
    }
    let query = encode_query_pairs(&pairs);
    let user = urlencoding::encode(&cfg.password);
    let host = format_host_for_uri(&node.server);
    let remarks = encode_remarks(&node.name);
    if query.is_empty() {
        Ok(format!("anytls://{user}@{host}:{}#{remarks}", node.port))
    } else {
        Ok(format!(
            "anytls://{user}@{host}:{}?{query}#{remarks}",
            node.port
        ))
    }
}

/// Parses `shadowtls://` share links
pub fn parse_shadowtls(uri: &str) -> ParseResult<ProxyNode> {
    let url = Url::parse(uri.trim()).map_err(|e| ParseError::InvalidUri(e.to_string()))?;
    let (server, port) = extract_url_host_port(&url, Some(443))?;
    let params = parse_query_map(&url);
    let name = extract_remarks(&url, &server, port);

    let password = url
        .password()
        .and_then(|p| {
            let decoded = urlencoding::decode(p)
                .map(|s| s.into_owned())
                .unwrap_or_else(|_| p.to_string());
            (!decoded.is_empty()).then_some(decoded)
        })
        .or_else(|| {
            let raw_user = url.username();
            if raw_user.is_empty() {
                return None;
            }
            let decoded_user = urlencoding::decode(raw_user)
                .map(|s| s.into_owned())
                .unwrap_or_else(|_| raw_user.to_string());
            if let Some((_, pass)) = decoded_user.split_once(':') {
                (!pass.is_empty()).then_some(pass.to_string())
            } else {
                (!decoded_user.is_empty()).then_some(decoded_user)
            }
        })
        .or_else(|| params.get("password").filter(|s| !s.is_empty()).cloned())
        .ok_or(ParseError::MissingField("password"))?;

    let version = params
        .get("version")
        .or_else(|| params.get("v"))
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(3);

    let (_, tls) = parse_v2ray_transport_and_tls(&params, true);

    Ok(ProxyNode {
        id: Uuid::new_v4().to_string(),
        name,
        server,
        port,
        protocol: ProtocolType::ShadowTls,
        tls,
        transport: None,
        settings: ProtocolConfig::ShadowTls(ShadowTlsConfig {
            version,
            password,
            inner_protocol: params.get("inner_protocol").cloned(),
            inner_method: params.get("inner_method").cloned(),
            inner_password: params.get("inner_password").cloned(),
        }),
        subscription_id: None,
        group_id: None,
        latency: None,
        dialer_proxy: None,
        preferred_core: None,
        sort_order: 0,
    })
}

pub fn to_uri_shadowtls(node: &ProxyNode) -> ParseResult<String> {
    let cfg = match &node.settings {
        ProtocolConfig::ShadowTls(c) => c,
        _ => return Err(ParseError::InvalidUri("not a ShadowTLS node".to_string())),
    };
    let mut pairs: Vec<(&'static str, String)> = vec![("version", cfg.version.to_string())];
    if let Some(ref p) = cfg.inner_protocol {
        pairs.push(("inner_protocol", p.clone()));
    }
    if let Some(ref m) = cfg.inner_method {
        pairs.push(("inner_method", m.clone()));
    }
    if let Some(ref pw) = cfg.inner_password {
        pairs.push(("inner_password", pw.clone()));
    }
    append_v2ray_transport_and_tls_params(&mut pairs, &None, &node.tls, true);
    let query = encode_query_pairs(&pairs);
    let user = urlencoding::encode(&cfg.password);
    let host = format_host_for_uri(&node.server);
    let remarks = encode_remarks(&node.name);
    Ok(format!(
        "shadowtls://{user}@{host}:{}?{query}#{remarks}",
        node.port
    ))
}

/// Parses `naive+https://`, `naive+quic://`, or `naive://` share links
pub fn parse_naive(uri: &str) -> ParseResult<ProxyNode> {
    let trimmed = uri.trim();
    let lower = trimmed.to_ascii_lowercase();
    let quic_scheme = lower.starts_with("naive+quic://");

    let url = Url::parse(trimmed).map_err(|e| ParseError::InvalidUri(e.to_string()))?;
    let (server, port) = extract_url_host_port(&url, Some(443))?;
    let params = parse_query_map(&url);
    let name = extract_remarks(&url, &server, port);

    let (user_opt, pass_opt) = extract_user_pass(&url);
    let username = user_opt.ok_or(ParseError::MissingField("username"))?;
    let password = pass_opt.unwrap_or_default();

    let sni = params.get("sni").filter(|s| !s.is_empty()).cloned();
    let insecure = parse_bool_flag(params.get("insecure"));

    Ok(ProxyNode {
        id: Uuid::new_v4().to_string(),
        name,
        server,
        port,
        protocol: ProtocolType::Naive,
        tls: Some(TlsConfig {
            enabled: true,
            server_name: sni,
            insecure,
            alpn: Vec::new(),
            fingerprint: None,
            reality: None,
            ech_config: None,
            fragment: None,
        }),
        transport: None,
        settings: ProtocolConfig::Naive(NaiveConfig {
            username,
            password,
            extra_headers: BTreeMap::new(),
            insecure_concurrency: params
                .get("insecure-concurrency")
                .or_else(|| params.get("insecure_concurrency"))
                .and_then(|v| v.parse::<u8>().ok()),
            quic: quic_scheme || parse_bool_flag(params.get("quic")),
        }),
        subscription_id: None,
        group_id: None,
        latency: None,
        dialer_proxy: None,
        preferred_core: None,
        sort_order: 0,
    })
}

pub fn to_uri_naive(node: &ProxyNode) -> ParseResult<String> {
    let cfg = match &node.settings {
        ProtocolConfig::Naive(c) => c,
        _ => return Err(ParseError::InvalidUri("not a NaiveProxy node".to_string())),
    };
    let scheme = if cfg.quic {
        "naive+quic"
    } else {
        "naive+https"
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
    }
    if let Some(ic) = cfg.insecure_concurrency {
        pairs.push(("insecure-concurrency", ic.to_string()));
    }
    let query = encode_query_pairs(&pairs);
    let u = urlencoding::encode(&cfg.username);
    let p = urlencoding::encode(&cfg.password);
    let host = format_host_for_uri(&node.server);
    let remarks = encode_remarks(&node.name);
    if query.is_empty() {
        Ok(format!("{scheme}://{u}:{p}@{host}:{}#{remarks}", node.port))
    } else {
        Ok(format!(
            "{scheme}://{u}:{p}@{host}:{}?{query}#{remarks}",
            node.port
        ))
    }
}

/// Parses `snell://` share links
pub fn parse_snell(uri: &str) -> ParseResult<ProxyNode> {
    let url = Url::parse(uri.trim()).map_err(|e| ParseError::InvalidUri(e.to_string()))?;
    let (server, port) = extract_url_host_port(&url, None)?;
    let params = parse_query_map(&url);
    let name = extract_remarks(&url, &server, port);

    let psk = urlencoding::decode(url.username())
        .map(|s| s.into_owned())
        .unwrap_or_else(|_| url.username().to_string());
    if psk.is_empty() {
        return Err(ParseError::MissingField("psk"));
    }

    let version = params
        .get("version")
        .or_else(|| params.get("v"))
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(4);

    Ok(ProxyNode {
        id: Uuid::new_v4().to_string(),
        name,
        server,
        port,
        protocol: ProtocolType::Snell,
        tls: None,
        transport: None,
        settings: ProtocolConfig::Snell(SnellConfig {
            psk,
            version,
            obfs_mode: params
                .get("obfs")
                .or_else(|| params.get("obfs_mode"))
                .filter(|s| !s.is_empty())
                .cloned(),
            obfs_host: params
                .get("obfs-host")
                .or_else(|| params.get("obfs_host"))
                .filter(|s| !s.is_empty())
                .cloned(),
        }),
        subscription_id: None,
        group_id: None,
        latency: None,
        dialer_proxy: None,
        preferred_core: None,
        sort_order: 0,
    })
}

pub fn to_uri_snell(node: &ProxyNode) -> ParseResult<String> {
    let cfg = match &node.settings {
        ProtocolConfig::Snell(c) => c,
        _ => return Err(ParseError::InvalidUri("not a Snell node".to_string())),
    };
    let mut pairs: Vec<(&'static str, String)> = vec![("version", cfg.version.to_string())];
    if let Some(ref m) = cfg.obfs_mode {
        pairs.push(("obfs", m.clone()));
    }
    if let Some(ref h) = cfg.obfs_host {
        pairs.push(("obfs-host", h.clone()));
    }
    let query = encode_query_pairs(&pairs);
    let psk = urlencoding::encode(&cfg.psk);
    let host = format_host_for_uri(&node.server);
    let remarks = encode_remarks(&node.name);
    Ok(format!(
        "snell://{psk}@{host}:{}?{query}#{remarks}",
        node.port
    ))
}

/// Parses `mieru://` share links
pub fn parse_mieru(uri: &str) -> ParseResult<ProxyNode> {
    let url = Url::parse(uri.trim()).map_err(|e| ParseError::InvalidUri(e.to_string()))?;
    let (server, port) = extract_url_host_port(&url, None)?;
    let params = parse_query_map(&url);
    let name = extract_remarks(&url, &server, port);

    let (user_opt, pass_opt) = extract_user_pass(&url);
    let username = user_opt.ok_or(ParseError::MissingField("username"))?;
    let password = pass_opt.ok_or(ParseError::MissingField("password"))?;

    let transport = params
        .get("transport")
        .or_else(|| params.get("protocol"))
        .map(|s| s.to_ascii_uppercase())
        .unwrap_or_else(|| "TCP".to_string());

    Ok(ProxyNode {
        id: Uuid::new_v4().to_string(),
        name,
        server,
        port,
        protocol: ProtocolType::Mieru,
        tls: None,
        transport: None,
        settings: ProtocolConfig::Mieru(MieruConfig {
            username,
            password,
            transport,
            port_range: params
                .get("port_range")
                .or_else(|| params.get("port-range"))
                .filter(|s| !s.trim().is_empty())
                .cloned(),
            multiplexing: params.get("multiplexing").cloned(),
        }),
        subscription_id: None,
        group_id: None,
        latency: None,
        dialer_proxy: None,
        preferred_core: None,
        sort_order: 0,
    })
}

pub fn to_uri_mieru(node: &ProxyNode) -> ParseResult<String> {
    let cfg = match &node.settings {
        ProtocolConfig::Mieru(c) => c,
        _ => return Err(ParseError::InvalidUri("not a Mieru node".to_string())),
    };
    let mut pairs: Vec<(&'static str, String)> = vec![("transport", cfg.transport.clone())];
    if let Some(ref pr) = cfg.port_range {
        pairs.push(("port_range", pr.clone()));
    }
    if let Some(ref mux) = cfg.multiplexing {
        pairs.push(("multiplexing", mux.clone()));
    }
    let query = encode_query_pairs(&pairs);
    let u = urlencoding::encode(&cfg.username);
    let p = urlencoding::encode(&cfg.password);
    let host = format_host_for_uri(&node.server);
    let remarks = encode_remarks(&node.name);
    Ok(format!(
        "mieru://{u}:{p}@{host}:{}?{query}#{remarks}",
        node.port
    ))
}

/// Parses `ssh://` share links
pub fn parse_ssh(uri: &str) -> ParseResult<ProxyNode> {
    let url = Url::parse(uri.trim()).map_err(|e| ParseError::InvalidUri(e.to_string()))?;
    let (server, port) = extract_url_host_port(&url, Some(22))?;
    let params = parse_query_map(&url);
    let name = extract_remarks(&url, &server, port);

    let (user_opt, pass_opt) = extract_user_pass(&url);
    let user = user_opt.unwrap_or_else(|| "root".to_string());

    Ok(ProxyNode {
        id: Uuid::new_v4().to_string(),
        name,
        server,
        port,
        protocol: ProtocolType::Ssh,
        tls: None,
        transport: None,
        settings: ProtocolConfig::Ssh(SshConfig {
            user,
            password: pass_opt,
            private_key: params.get("private_key").cloned(),
            private_key_passphrase: params.get("private_key_passphrase").cloned(),
            host_key: Vec::new(),
            host_key_algorithms: Vec::new(),
            client_version: params.get("client_version").cloned(),
        }),
        subscription_id: None,
        group_id: None,
        latency: None,
        dialer_proxy: None,
        preferred_core: None,
        sort_order: 0,
    })
}

pub fn to_uri_ssh(node: &ProxyNode) -> ParseResult<String> {
    let cfg = match &node.settings {
        ProtocolConfig::Ssh(c) => c,
        _ => return Err(ParseError::InvalidUri("not an SSH node".to_string())),
    };
    let u = urlencoding::encode(&cfg.user);
    let auth = match &cfg.password {
        Some(p) if !p.is_empty() => format!("{u}:{}", urlencoding::encode(p)),
        _ => u.into_owned(),
    };
    let host = format_host_for_uri(&node.server);
    let remarks = encode_remarks(&node.name);
    Ok(format!("ssh://{auth}@{host}:{}#{remarks}", node.port))
}

/// Parses `socks5://`, `socks5h://`, or `socks://` share links
pub fn parse_socks5(uri: &str) -> ParseResult<ProxyNode> {
    let url = Url::parse(uri.trim()).map_err(|e| ParseError::InvalidUri(e.to_string()))?;
    let (server, port) = extract_url_host_port(&url, Some(1080))?;
    let params = parse_query_map(&url);
    let name = extract_remarks(&url, &server, port);
    let (username, password) = extract_user_pass(&url);
    let (_, tls) = parse_v2ray_transport_and_tls(&params, false);

    let udp = params
        .get("udp")
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(true);

    Ok(ProxyNode {
        id: Uuid::new_v4().to_string(),
        name,
        server,
        port,
        protocol: ProtocolType::Socks5,
        tls,
        transport: None,
        settings: ProtocolConfig::Socks5(Socks5Config {
            username,
            password,
            udp,
        }),
        subscription_id: None,
        group_id: None,
        latency: None,
        dialer_proxy: None,
        preferred_core: None,
        sort_order: 0,
    })
}

pub fn to_uri_socks5(node: &ProxyNode) -> ParseResult<String> {
    let cfg = match &node.settings {
        ProtocolConfig::Socks5(c) => c,
        _ => return Err(ParseError::InvalidUri("not a SOCKS5 node".to_string())),
    };
    let auth = match (&cfg.username, &cfg.password) {
        (Some(u), Some(p)) if !u.is_empty() => {
            format!("{}:{}@", urlencoding::encode(u), urlencoding::encode(p))
        }
        (Some(u), None) if !u.is_empty() => format!("{}@", urlencoding::encode(u)),
        _ => String::new(),
    };
    let mut pairs: Vec<(&'static str, String)> = Vec::new();
    if !cfg.udp {
        pairs.push(("udp", "0".to_string()));
    }
    append_v2ray_transport_and_tls_params(&mut pairs, &None, &node.tls, false);
    let query = encode_query_pairs(&pairs);
    let host = format_host_for_uri(&node.server);
    let remarks = encode_remarks(&node.name);
    if query.is_empty() {
        Ok(format!("socks5://{auth}{host}:{}#{remarks}", node.port))
    } else {
        Ok(format!(
            "socks5://{auth}{host}:{}?{query}#{remarks}",
            node.port
        ))
    }
}

/// Parses `http://` or `https://` proxy share links
pub fn parse_http(uri: &str) -> ParseResult<ProxyNode> {
    let trimmed = uri.trim();
    let lower = trimmed.to_ascii_lowercase();
    let is_https = lower.starts_with("https://");

    let url = Url::parse(trimmed).map_err(|e| ParseError::InvalidUri(e.to_string()))?;
    let (server, port) = extract_url_host_port(&url, Some(if is_https { 443 } else { 8080 }))?;
    let params = parse_query_map(&url);
    let name = extract_remarks(&url, &server, port);
    let (username, password) = extract_user_pass(&url);
    let (_, tls) = parse_v2ray_transport_and_tls(&params, is_https);

    Ok(ProxyNode {
        id: Uuid::new_v4().to_string(),
        name,
        server,
        port,
        protocol: ProtocolType::Http,
        tls,
        transport: None,
        settings: ProtocolConfig::Http(HttpConfig {
            username,
            password,
            headers: BTreeMap::new(),
        }),
        subscription_id: None,
        group_id: None,
        latency: None,
        dialer_proxy: None,
        preferred_core: None,
        sort_order: 0,
    })
}

pub fn to_uri_http(node: &ProxyNode) -> ParseResult<String> {
    let cfg = match &node.settings {
        ProtocolConfig::Http(c) => c,
        _ => return Err(ParseError::InvalidUri("not an HTTP node".to_string())),
    };
    let is_https = matches!(&node.tls, Some(t) if t.enabled);
    let scheme = if is_https { "https" } else { "http" };
    let auth = match (&cfg.username, &cfg.password) {
        (Some(u), Some(p)) if !u.is_empty() => {
            format!("{}:{}@", urlencoding::encode(u), urlencoding::encode(p))
        }
        (Some(u), None) if !u.is_empty() => format!("{}@", urlencoding::encode(u)),
        _ => String::new(),
    };
    let mut pairs: Vec<(&'static str, String)> = Vec::new();
    append_v2ray_transport_and_tls_params(&mut pairs, &None, &node.tls, is_https);
    let query = encode_query_pairs(&pairs);
    let host = format_host_for_uri(&node.server);
    let remarks = encode_remarks(&node.name);
    if query.is_empty() {
        Ok(format!("{scheme}://{auth}{host}:{}#{remarks}", node.port))
    } else {
        Ok(format!(
            "{scheme}://{auth}{host}:{}?{query}#{remarks}",
            node.port
        ))
    }
}

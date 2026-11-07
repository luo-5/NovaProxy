use crate::model::{ProxyNode, Subscription};
use crate::parser::base64::{decode_base64_to_string, strip_bom_and_trim};
use crate::parser::clash_yaml::parse_clash_yaml;
use crate::parser::error::{ParseError, ParseResult};
use crate::parser::parse_share_link;
use crate::parser::singbox_json::parse_json_subscription;
use crate::parser::wireguard::parse_wg_conf;
use chrono::Utc;
use reqwest::header::USER_AGENT;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Duration;
use ts_rs::TS;

/// Detected subscription document format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionFormat {
    PlainLinks,
    Base64Links,
    ClashYaml,
    SingBoxOrSip008Json,
    WireGuardConf,
}

/// Result of parsing subscription content or batch links
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SubscriptionParseReport {
    pub format: SubscriptionFormat,
    pub nodes: Vec<ProxyNode>,
    pub warnings: Vec<String>,
}

/// Parsed `subscription-userinfo` HTTP header fields
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SubscriptionUserInfo {
    #[ts(type = "number | null")]
    pub upload: Option<u64>,
    #[ts(type = "number | null")]
    pub download: Option<u64>,
    #[ts(type = "number | null")]
    pub total: Option<u64>,
    #[ts(type = "number | null")]
    pub expire: Option<i64>,
    pub update_interval_mins: Option<u32>,
    pub home_url: Option<String>,
}

/// Parses the `subscription-userinfo` header value:
/// e.g. `upload=1234; download=5678; total=10737418240; expire=1798761600`
pub fn parse_subscription_userinfo(header_val: &str) -> SubscriptionUserInfo {
    let mut info = SubscriptionUserInfo::default();

    for part in header_val.split(';') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once('=') {
            let key = k.trim().to_ascii_lowercase();
            let val = v.trim();
            match key.as_str() {
                "upload" => {
                    info.upload = val
                        .parse::<u64>()
                        .ok()
                        .or_else(|| val.parse::<f64>().ok().map(|f| f.max(0.0) as u64));
                }
                "download" => {
                    info.download = val
                        .parse::<u64>()
                        .ok()
                        .or_else(|| val.parse::<f64>().ok().map(|f| f.max(0.0) as u64));
                }
                "total" => {
                    info.total = val
                        .parse::<u64>()
                        .ok()
                        .or_else(|| val.parse::<f64>().ok().map(|f| f.max(0.0) as u64));
                }
                "expire" => {
                    info.expire = val
                        .parse::<i64>()
                        .ok()
                        .or_else(|| val.parse::<f64>().ok().map(|f| f as i64));
                }
                _ => {}
            }
        }
    }

    info
}

fn looks_like_clash_yaml(text: &str) -> bool {
    text.lines().any(|line| {
        let t = line.trim();
        t.starts_with("proxies:") || t.starts_with("Proxy:")
    })
}

fn looks_like_share_link_line(line: &str) -> bool {
    let lower = line.trim().to_ascii_lowercase();
    lower.starts_with("vmess://")
        || lower.starts_with("vless://")
        || lower.starts_with("trojan://")
        || lower.starts_with("ss://")
        || lower.starts_with("ssr://")
        || lower.starts_with("hysteria://")
        || lower.starts_with("hy://")
        || lower.starts_with("hysteria2://")
        || lower.starts_with("hy2://")
        || lower.starts_with("tuic://")
        || lower.starts_with("wireguard://")
        || lower.starts_with("wg://")
        || lower.starts_with("anytls://")
        || lower.starts_with("shadowtls://")
        || lower.starts_with("naive+https://")
        || lower.starts_with("naive+quic://")
        || lower.starts_with("naive://")
        || lower.starts_with("snell://")
        || lower.starts_with("mieru://")
        || lower.starts_with("ssh://")
        || lower.starts_with("socks5://")
        || lower.starts_with("socks5h://")
        || lower.starts_with("socks://")
        || lower.starts_with("http://")
        || lower.starts_with("https://")
}

/// Parses multi-line plain share links with fault tolerance
pub fn parse_multiline_links(text: &str) -> (Vec<ProxyNode>, Vec<String>) {
    let mut nodes = Vec::new();
    let mut warnings = Vec::new();

    for (idx, raw_line) in text.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with("//")
            || line.starts_with("STATUS=")
            || line.starts_with("REMARKS=")
        {
            continue;
        }

        match parse_share_link(line) {
            Ok(mut node) => {
                node.sort_order = nodes.len() as i32;
                nodes.push(node);
            }
            Err(err) => {
                warnings.push(format!("Line {}: {}", idx + 1, err));
            }
        }
    }

    (nodes, warnings)
}

/// Auto-detects the subscription format and parses all proxy nodes with fault tolerance
pub fn parse_subscription_content(raw_input: &str) -> ParseResult<SubscriptionParseReport> {
    let cleaned = strip_bom_and_trim(raw_input);
    if cleaned.is_empty() {
        return Err(ParseError::EmptyInput);
    }

    // 1. JSON (sing-box outbounds or SIP008 servers)
    if cleaned.starts_with('{') || cleaned.starts_with('[') {
        let (nodes, warnings) = parse_json_subscription(cleaned)?;
        return Ok(SubscriptionParseReport {
            format: SubscriptionFormat::SingBoxOrSip008Json,
            nodes: deduplicate_nodes(nodes),
            warnings,
        });
    }

    // 2. WireGuard .conf INI
    if cleaned.contains("[Interface]") && cleaned.contains("[Peer]") {
        let node = parse_wg_conf(cleaned)?;
        return Ok(SubscriptionParseReport {
            format: SubscriptionFormat::WireGuardConf,
            nodes: vec![node],
            warnings: Vec::new(),
        });
    }

    // 3. Clash / Mihomo YAML
    if looks_like_clash_yaml(cleaned) {
        let (nodes, warnings) = parse_clash_yaml(cleaned)?;
        return Ok(SubscriptionParseReport {
            format: SubscriptionFormat::ClashYaml,
            nodes: deduplicate_nodes(nodes),
            warnings,
        });
    }

    // 4. Plain-text share links (if at least one line starts with a known URI scheme)
    if cleaned.lines().any(looks_like_share_link_line) {
        let (nodes, warnings) = parse_multiline_links(cleaned);
        if nodes.is_empty() && !warnings.is_empty() {
            return Err(ParseError::InvalidUri(warnings.join("; ")));
        }
        return Ok(SubscriptionParseReport {
            format: SubscriptionFormat::PlainLinks,
            nodes: deduplicate_nodes(nodes),
            warnings,
        });
    }

    // 5. Try Base64 decoding (may decode into links, Clash YAML, JSON, or WireGuard conf)
    if let Ok(decoded) = decode_base64_to_string(cleaned) {
        let decoded_clean = strip_bom_and_trim(&decoded);
        if decoded_clean.starts_with('{') || decoded_clean.starts_with('[') {
            if let Ok((nodes, warnings)) = parse_json_subscription(decoded_clean) {
                return Ok(SubscriptionParseReport {
                    format: SubscriptionFormat::SingBoxOrSip008Json,
                    nodes: deduplicate_nodes(nodes),
                    warnings,
                });
            }
        }
        if looks_like_clash_yaml(decoded_clean) {
            if let Ok((nodes, warnings)) = parse_clash_yaml(decoded_clean) {
                return Ok(SubscriptionParseReport {
                    format: SubscriptionFormat::ClashYaml,
                    nodes: deduplicate_nodes(nodes),
                    warnings,
                });
            }
        }
        if decoded_clean.contains("[Interface]") && decoded_clean.contains("[Peer]") {
            if let Ok(node) = parse_wg_conf(decoded_clean) {
                return Ok(SubscriptionParseReport {
                    format: SubscriptionFormat::WireGuardConf,
                    nodes: vec![node],
                    warnings: Vec::new(),
                });
            }
        }
        let (nodes, warnings) = parse_multiline_links(decoded_clean);
        if !nodes.is_empty() {
            return Ok(SubscriptionParseReport {
                format: SubscriptionFormat::Base64Links,
                nodes: deduplicate_nodes(nodes),
                warnings,
            });
        }
    }

    Err(ParseError::InvalidUri(
        "Unrecognized subscription or share link format".to_string(),
    ))
}

/// Deduplicates nodes by their deterministic SHA-256 fingerprint (`compute_fingerprint()`),
/// preserving the first occurrence's order.
pub fn deduplicate_nodes(nodes: Vec<ProxyNode>) -> Vec<ProxyNode> {
    let mut seen = HashSet::new();
    let mut unique = Vec::with_capacity(nodes.len());
    for mut node in nodes {
        let fp = node.compute_fingerprint();
        if seen.insert(fp) {
            node.sort_order = unique.len() as i32;
            unique.push(node);
        }
    }
    unique
}

/// Incrementally merges newly parsed subscription nodes with the subscription's existing nodes:
/// - Matches by `compute_fingerprint()`
/// - Preserves existing `id`, user-customized `name`, `sort_order`, `latency`, `group_id`,
///   `dialer_proxy`, and `preferred_core`
/// - Assigns `subscription_id` to all resulting nodes
pub fn merge_subscription_nodes(
    subscription_id: &str,
    existing_nodes: &[ProxyNode],
    incoming_nodes: Vec<ProxyNode>,
) -> Vec<ProxyNode> {
    let deduped_incoming = deduplicate_nodes(incoming_nodes);

    // Map fingerprint -> existing node
    let mut existing_by_fp = std::collections::HashMap::with_capacity(existing_nodes.len());
    let mut max_sort_order = existing_nodes
        .iter()
        .map(|n| n.sort_order)
        .max()
        .unwrap_or(-1);

    for existing in existing_nodes {
        existing_by_fp.insert(existing.compute_fingerprint(), existing);
    }

    let mut merged = Vec::with_capacity(deduped_incoming.len());

    for mut incoming in deduped_incoming {
        let fp = incoming.compute_fingerprint();
        incoming.subscription_id = Some(subscription_id.to_string());

        if let Some(prev) = existing_by_fp.get(&fp) {
            incoming.id = prev.id.clone();
            if !prev.name.trim().is_empty() {
                incoming.name = prev.name.clone();
            }
            incoming.sort_order = prev.sort_order;
            incoming.latency = prev.latency;
            incoming.group_id = prev.group_id.clone();
            if prev.dialer_proxy.is_some() {
                incoming.dialer_proxy = prev.dialer_proxy.clone();
            }
            if prev.preferred_core.is_some() {
                incoming.preferred_core = prev.preferred_core;
            }
        } else {
            max_sort_order += 1;
            incoming.sort_order = max_sort_order;
        }

        merged.push(incoming);
    }

    merged.sort_by_key(|n| n.sort_order);
    merged
}

/// Downloads a subscription URL with custom User-Agent, timeout, retry, and optional local proxy,
/// returning the parsed report and extracted `SubscriptionUserInfo`.
pub async fn fetch_and_parse_subscription(
    sub: &Subscription,
    local_proxy_port: Option<u16>,
    timeout_secs: u64,
    max_retries: usize,
) -> ParseResult<(SubscriptionParseReport, SubscriptionUserInfo)> {
    let ua = sub
        .user_agent
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("clash.meta/v1.19.33 NovaProxy/0.1.0");

    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs.max(3)))
        .default_headers({
            let mut h = reqwest::header::HeaderMap::new();
            if let Ok(val) = reqwest::header::HeaderValue::from_str(ua) {
                h.insert(USER_AGENT, val);
            }
            h
        });

    if sub.use_proxy {
        if let Some(port) = local_proxy_port {
            let proxy_url = format!("http://127.0.0.1:{port}");
            let proxy = reqwest::Proxy::all(&proxy_url)
                .map_err(|e| ParseError::Network(format!("invalid proxy '{proxy_url}': {e}")))?;
            builder = builder.proxy(proxy);
        }
    }

    let client = builder
        .build()
        .map_err(|e| ParseError::Network(e.to_string()))?;

    let attempts = max_retries.max(1);
    let mut last_err = None;

    for attempt in 1..=attempts {
        match client.get(&sub.url).send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    last_err = Some(ParseError::Network(format!(
                        "HTTP status {} from {}",
                        resp.status(),
                        sub.url
                    )));
                } else {
                    let headers = resp.headers().clone();
                    let mut user_info = headers
                        .get("subscription-userinfo")
                        .and_then(|v| v.to_str().ok())
                        .map(parse_subscription_userinfo)
                        .unwrap_or_default();

                    if let Some(interval_hours) = headers
                        .get("profile-update-interval")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.trim().parse::<u32>().ok())
                    {
                        user_info.update_interval_mins = Some(interval_hours.saturating_mul(60));
                    }

                    if let Some(web_url) = headers
                        .get("profile-web-page-url")
                        .and_then(|v| v.to_str().ok())
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                    {
                        user_info.home_url = Some(web_url);
                    }

                    let body = resp
                        .text()
                        .await
                        .map_err(|e| ParseError::Network(e.to_string()))?;
                    let report = parse_subscription_content(&body)?;
                    return Ok((report, user_info));
                }
            }
            Err(e) => {
                last_err = Some(ParseError::Network(e.to_string()));
            }
        }

        if attempt < attempts {
            tokio::time::sleep(Duration::from_millis(300 * attempt as u64)).await;
        }
    }

    Err(last_err.unwrap_or_else(|| ParseError::Network("unknown network failure".to_string())))
}

/// Applies `SubscriptionUserInfo` and timestamp onto a `Subscription` model
pub fn apply_userinfo_to_subscription(sub: &mut Subscription, info: &SubscriptionUserInfo) {
    sub.updated_at = Some(Utc::now().timestamp());
    if info.upload.is_some() {
        sub.upload = info.upload;
    }
    if info.download.is_some() {
        sub.download = info.download;
    }
    if info.total.is_some() {
        sub.total = info.total;
    }
    if info.expire.is_some() {
        sub.expire = info.expire;
    }
    if sub.update_interval_mins.is_none() && info.update_interval_mins.is_some() {
        sub.update_interval_mins = info.update_interval_mins;
    }
    if sub.home_url.is_none() && info.home_url.is_some() {
        sub.home_url = info.home_url.clone();
    }
}

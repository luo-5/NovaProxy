//! Protocol URI and Subscription parsing modules (Phase P2)

pub mod base64;
pub mod clash_yaml;
pub mod error;
pub mod extra_uris;
pub mod hysteria;
pub mod qrcode;
pub mod singbox_json;
pub mod ss;
pub mod ssr;
pub mod subscription;
pub mod trojan;
pub mod tuic;
pub mod uri_common;
pub mod vless;
pub mod vmess;
pub mod wireguard;

pub use error::{ParseError, ParseResult};
pub use subscription::{
    apply_userinfo_to_subscription, deduplicate_nodes, fetch_and_parse_subscription,
    merge_subscription_nodes, parse_multiline_links, parse_subscription_content,
    parse_subscription_userinfo, SubscriptionFormat, SubscriptionParseReport, SubscriptionUserInfo,
};

use crate::model::{ProtocolType, ProxyNode};

/// Parses a single proxy share link (`vmess://`, `vless://`, `trojan://`, `ss://`, `ssr://`,
/// `hysteria://`/`hy://`, `hysteria2://`/`hy2://`, `tuic://`, `wireguard://`/`wg://`, `anytls://`,
/// `shadowtls://`, `naive+https://`/`naive+quic://`, `snell://`, `mieru://`, `ssh://`,
/// `socks5://`/`socks://`, `http://`/`https://`) into a `ProxyNode`.
pub fn parse_share_link(uri: &str) -> ParseResult<ProxyNode> {
    let trimmed = base64::strip_bom_and_trim(uri);
    if trimmed.is_empty() {
        return Err(ParseError::EmptyInput);
    }

    if trimmed.contains("[Interface]") && trimmed.contains("[Peer]") {
        return wireguard::parse_wg_conf(trimmed);
    }

    let Some((scheme_raw, _)) = trimmed.split_once("://") else {
        return Err(ParseError::UnsupportedScheme(trimmed.to_string()));
    };

    let scheme = scheme_raw.trim().to_ascii_lowercase();
    match scheme.as_str() {
        "vmess" => vmess::parse(trimmed),
        "vless" => vless::parse(trimmed),
        "trojan" => trojan::parse(trimmed),
        "ss" => ss::parse(trimmed),
        "ssr" => ssr::parse(trimmed),
        "hysteria" | "hy" => hysteria::parse_v1(trimmed),
        "hysteria2" | "hy2" => hysteria::parse_v2(trimmed),
        "tuic" => tuic::parse(trimmed),
        "wireguard" | "wg" => wireguard::parse(trimmed),
        "anytls" => extra_uris::parse_anytls(trimmed),
        "shadowtls" => extra_uris::parse_shadowtls(trimmed),
        "naive+https" | "naive+quic" | "naive" => extra_uris::parse_naive(trimmed),
        "snell" => extra_uris::parse_snell(trimmed),
        "mieru" => extra_uris::parse_mieru(trimmed),
        "ssh" => extra_uris::parse_ssh(trimmed),
        "socks5" | "socks5h" | "socks" => extra_uris::parse_socks5(trimmed),
        "http" | "https" => extra_uris::parse_http(trimmed),
        other => Err(ParseError::UnsupportedScheme(format!("{other}://"))),
    }
}

/// Serializes any `ProxyNode` into its canonical share link (`to_uri`).
pub fn node_to_share_link(node: &ProxyNode) -> ParseResult<String> {
    match node.protocol {
        ProtocolType::Vmess => vmess::to_uri(node),
        ProtocolType::Vless => vless::to_uri(node),
        ProtocolType::Trojan => trojan::to_uri(node),
        ProtocolType::Shadowsocks => ss::to_uri(node),
        ProtocolType::ShadowsocksR => ssr::to_uri(node),
        ProtocolType::Hysteria => hysteria::to_uri_v1(node),
        ProtocolType::Hysteria2 => hysteria::to_uri_v2(node),
        ProtocolType::Tuic => tuic::to_uri(node),
        ProtocolType::WireGuard => wireguard::to_uri(node),
        ProtocolType::AnyTls => extra_uris::to_uri_anytls(node),
        ProtocolType::ShadowTls => extra_uris::to_uri_shadowtls(node),
        ProtocolType::Naive => extra_uris::to_uri_naive(node),
        ProtocolType::Snell => extra_uris::to_uri_snell(node),
        ProtocolType::Mieru => extra_uris::to_uri_mieru(node),
        ProtocolType::Ssh => extra_uris::to_uri_ssh(node),
        ProtocolType::Socks5 => extra_uris::to_uri_socks5(node),
        ProtocolType::Http => extra_uris::to_uri_http(node),
    }
}

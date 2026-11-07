use serde::{Deserialize, Serialize};

/// Supported proxy protocols
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProtocolType {
    Shadowsocks,
    ShadowsocksR,
    Vmess,
    Vless,
    Trojan,
    Hysteria,
    Hysteria2,
    Tuic,
    WireGuard,
    AnyTls,
    ShadowTls,
    Naive,
    Snell,
    Ssh,
    Socks5,
    Http,
}

/// Generic proxy node structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyNode {
    pub id: String,
    pub name: String,
    pub server: String,
    pub port: u16,
    pub protocol: ProtocolType,
    #[serde(default)]
    pub latency: Option<i64>,
}

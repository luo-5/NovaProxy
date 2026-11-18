use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use ts_rs::TS;

/// Supported proxy core engines
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
pub enum CoreType {
    #[serde(rename = "sing-box")]
    SingBox,
    #[serde(rename = "mihomo")]
    Mihomo,
    #[serde(rename = "xray")]
    Xray,
}

impl CoreType {
    pub fn as_str(&self) -> &'static str {
        match self {
            CoreType::SingBox => "sing-box",
            CoreType::Mihomo => "mihomo",
            CoreType::Xray => "xray",
        }
    }

    pub fn binary_name(&self) -> &'static str {
        match self {
            #[cfg(target_os = "windows")]
            CoreType::SingBox => "sing-box.exe",
            #[cfg(target_os = "android")]
            CoreType::SingBox => "libsingbox.so",
            #[cfg(all(not(target_os = "windows"), not(target_os = "android")))]
            CoreType::SingBox => "sing-box",

            #[cfg(target_os = "windows")]
            CoreType::Mihomo => "mihomo.exe",
            #[cfg(target_os = "android")]
            CoreType::Mihomo => "libmihomo.so",
            #[cfg(all(not(target_os = "windows"), not(target_os = "android")))]
            CoreType::Mihomo => "mihomo",

            #[cfg(target_os = "windows")]
            CoreType::Xray => "xray.exe",
            #[cfg(target_os = "android")]
            CoreType::Xray => "libxray.so",
            #[cfg(all(not(target_os = "windows"), not(target_os = "android")))]
            CoreType::Xray => "xray",
        }
    }
}

impl std::fmt::Display for CoreType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Supported proxy protocols
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
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
    Mieru,
    Ssh,
    Socks5,
    Http,
}

impl ProtocolType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProtocolType::Shadowsocks => "shadowsocks",
            ProtocolType::ShadowsocksR => "shadowsocksr",
            ProtocolType::Vmess => "vmess",
            ProtocolType::Vless => "vless",
            ProtocolType::Trojan => "trojan",
            ProtocolType::Hysteria => "hysteria",
            ProtocolType::Hysteria2 => "hysteria2",
            ProtocolType::Tuic => "tuic",
            ProtocolType::WireGuard => "wireguard",
            ProtocolType::AnyTls => "anytls",
            ProtocolType::ShadowTls => "shadowtls",
            ProtocolType::Naive => "naive",
            ProtocolType::Snell => "snell",
            ProtocolType::Mieru => "mieru",
            ProtocolType::Ssh => "ssh",
            ProtocolType::Socks5 => "socks5",
            ProtocolType::Http => "http",
        }
    }
}

impl std::fmt::Display for ProtocolType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Transport layer kind for capability matching
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum TransportKind {
    Tcp,
    Ws,
    Grpc,
    Http,
    HttpUpgrade,
    Xhttp,
    Mkcp,
    Quic,
}

/// Transport layer configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TransportConfig {
    Tcp {
        #[serde(default)]
        header_type: Option<String>,
        #[serde(default)]
        http_host: Vec<String>,
        #[serde(default)]
        http_path: Option<String>,
    },
    Ws {
        path: String,
        #[serde(default)]
        host: Option<String>,
        #[serde(default)]
        headers: BTreeMap<String, String>,
        #[serde(default)]
        max_early_data: Option<u32>,
        #[serde(default)]
        early_data_header_name: Option<String>,
    },
    Grpc {
        service_name: String,
        #[serde(default)]
        multi_mode: bool,
        #[serde(default)]
        authority: Option<String>,
    },
    Http {
        #[serde(default)]
        host: Vec<String>,
        path: String,
        #[serde(default)]
        method: Option<String>,
        #[serde(default)]
        headers: BTreeMap<String, String>,
    },
    HttpUpgrade {
        #[serde(default)]
        host: Option<String>,
        path: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
    },
    Xhttp {
        #[serde(default)]
        host: Option<String>,
        path: String,
        #[serde(default)]
        mode: Option<String>,
        #[serde(default)]
        extra: Option<String>,
    },
    Mkcp {
        #[serde(default)]
        seed: Option<String>,
        #[serde(default)]
        header_type: Option<String>,
        #[serde(default)]
        mtu: Option<u16>,
        #[serde(default)]
        tti: Option<u16>,
        #[serde(default)]
        uplink_capacity: Option<u16>,
        #[serde(default)]
        downlink_capacity: Option<u16>,
        #[serde(default)]
        congestion: bool,
    },
    Quic {
        #[serde(default)]
        security: Option<String>,
        #[serde(default)]
        key: Option<String>,
        #[serde(default)]
        header_type: Option<String>,
    },
}

impl TransportConfig {
    pub fn kind(&self) -> TransportKind {
        match self {
            TransportConfig::Tcp { .. } => TransportKind::Tcp,
            TransportConfig::Ws { .. } => TransportKind::Ws,
            TransportConfig::Grpc { .. } => TransportKind::Grpc,
            TransportConfig::Http { .. } => TransportKind::Http,
            TransportConfig::HttpUpgrade { .. } => TransportKind::HttpUpgrade,
            TransportConfig::Xhttp { .. } => TransportKind::Xhttp,
            TransportConfig::Mkcp { .. } => TransportKind::Mkcp,
            TransportConfig::Quic { .. } => TransportKind::Quic,
        }
    }
}

/// REALITY TLS configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RealityConfig {
    pub public_key: String,
    #[serde(default)]
    pub short_id: String,
    #[serde(default)]
    pub spider_x: Option<String>,
}

/// TLS / REALITY security configuration
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TlsConfig {
    pub enabled: bool,
    #[serde(default)]
    pub server_name: Option<String>,
    #[serde(default)]
    pub insecure: bool,
    #[serde(default)]
    pub alpn: Vec<String>,
    #[serde(default)]
    pub fingerprint: Option<String>,
    #[serde(default)]
    pub reality: Option<RealityConfig>,
    #[serde(default)]
    pub ech_config: Option<String>,
    #[serde(default)]
    pub fragment: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ShadowsocksConfig {
    pub method: String,
    pub password: String,
    #[serde(default)]
    pub plugin: Option<String>,
    #[serde(default)]
    pub plugin_opts: Option<String>,
    #[serde(default)]
    pub uot: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ShadowsocksRConfig {
    pub method: String,
    pub password: String,
    pub protocol: String,
    #[serde(default)]
    pub protocol_param: Option<String>,
    pub obfs: String,
    #[serde(default)]
    pub obfs_param: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct VmessConfig {
    pub uuid: String,
    #[serde(default)]
    pub alter_id: u16,
    #[serde(default = "default_vmess_security")]
    pub security: String,
    #[serde(default)]
    pub global_padding: Option<bool>,
    #[serde(default)]
    pub authenticated_length: Option<bool>,
    #[serde(default)]
    pub packet_encoding: Option<String>,
}

fn default_vmess_security() -> String {
    "auto".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct VlessConfig {
    pub uuid: String,
    #[serde(default)]
    pub flow: Option<String>,
    #[serde(default = "default_vless_encryption")]
    pub encryption: String,
    #[serde(default)]
    pub packet_encoding: Option<String>,
}

fn default_vless_encryption() -> String {
    "none".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TrojanConfig {
    pub password: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct HysteriaConfig {
    #[serde(default)]
    pub auth_str: Option<String>,
    #[serde(default)]
    pub protocol: Option<String>,
    pub up_mbps: u32,
    pub down_mbps: u32,
    #[serde(default)]
    pub obfs: Option<String>,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub recv_window_conn: Option<u64>,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub recv_window: Option<u64>,
    #[serde(default)]
    pub disable_mtu_discovery: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Hysteria2Config {
    pub password: String,
    #[serde(default)]
    pub up_mbps: Option<u32>,
    #[serde(default)]
    pub down_mbps: Option<u32>,
    #[serde(default)]
    pub obfs_type: Option<String>,
    #[serde(default)]
    pub obfs_password: Option<String>,
    #[serde(default)]
    pub ports: Option<String>,
    #[serde(default)]
    pub hop_interval: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TuicConfig {
    pub uuid: String,
    pub password: String,
    #[serde(default)]
    pub congestion_control: Option<String>,
    #[serde(default)]
    pub udp_relay_mode: Option<String>,
    #[serde(default)]
    pub zero_rtt_handshake: Option<bool>,
    #[serde(default)]
    pub heartbeat: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WireGuardConfig {
    pub private_key: String,
    pub public_key: String,
    #[serde(default)]
    pub preshared_key: Option<String>,
    pub local_address: Vec<String>,
    #[serde(default)]
    pub reserved: Option<[u8; 3]>,
    #[serde(default)]
    pub mtu: Option<u16>,
    #[serde(default)]
    pub workers: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AnyTlsConfig {
    pub password: String,
    #[serde(default)]
    pub idle_session_check_interval: Option<String>,
    #[serde(default)]
    pub idle_session_timeout: Option<String>,
    #[serde(default)]
    pub min_idle_session: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ShadowTlsConfig {
    #[serde(default = "default_shadowtls_version")]
    pub version: u8,
    pub password: String,
    #[serde(default)]
    pub inner_protocol: Option<String>,
    #[serde(default)]
    pub inner_method: Option<String>,
    #[serde(default)]
    pub inner_password: Option<String>,
}

fn default_shadowtls_version() -> u8 {
    3
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct NaiveConfig {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub extra_headers: BTreeMap<String, String>,
    #[serde(default)]
    pub insecure_concurrency: Option<u8>,
    #[serde(default)]
    pub quic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SnellConfig {
    pub psk: String,
    #[serde(default = "default_snell_version")]
    pub version: u8,
    #[serde(default)]
    pub obfs_mode: Option<String>,
    #[serde(default)]
    pub obfs_host: Option<String>,
}

fn default_snell_version() -> u8 {
    4
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MieruConfig {
    pub username: String,
    pub password: String,
    #[serde(default = "default_mieru_transport")]
    pub transport: String,
    #[serde(default)]
    pub port_range: Option<String>,
    #[serde(default)]
    pub multiplexing: Option<String>,
}

fn default_mieru_transport() -> String {
    "TCP".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SshConfig {
    pub user: String,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub private_key: Option<String>,
    #[serde(default)]
    pub private_key_passphrase: Option<String>,
    #[serde(default)]
    pub host_key: Vec<String>,
    #[serde(default)]
    pub host_key_algorithms: Vec<String>,
    #[serde(default)]
    pub client_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Socks5Config {
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default = "default_true")]
    pub udp: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct HttpConfig {
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

/// Protocol-specific settings payload
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ProtocolConfig {
    Shadowsocks(ShadowsocksConfig),
    ShadowsocksR(ShadowsocksRConfig),
    Vmess(VmessConfig),
    Vless(VlessConfig),
    Trojan(TrojanConfig),
    Hysteria(HysteriaConfig),
    Hysteria2(Hysteria2Config),
    Tuic(TuicConfig),
    WireGuard(WireGuardConfig),
    AnyTls(AnyTlsConfig),
    ShadowTls(ShadowTlsConfig),
    Naive(NaiveConfig),
    Snell(SnellConfig),
    Mieru(MieruConfig),
    Ssh(SshConfig),
    Socks5(Socks5Config),
    Http(HttpConfig),
}

impl ProtocolConfig {
    pub fn credential_key(&self) -> String {
        match self {
            ProtocolConfig::Shadowsocks(c) => format!("{}:{}", c.method, c.password),
            ProtocolConfig::ShadowsocksR(c) => {
                format!("{}:{}:{}:{}", c.method, c.password, c.protocol, c.obfs)
            }
            ProtocolConfig::Vmess(c) => format!("{}:{}", c.uuid, c.alter_id),
            ProtocolConfig::Vless(c) => {
                format!("{}:{}", c.uuid, c.flow.as_deref().unwrap_or(""))
            }
            ProtocolConfig::Trojan(c) => c.password.clone(),
            ProtocolConfig::Hysteria(c) => c.auth_str.clone().unwrap_or_default(),
            ProtocolConfig::Hysteria2(c) => c.password.clone(),
            ProtocolConfig::Tuic(c) => format!("{}:{}", c.uuid, c.password),
            ProtocolConfig::WireGuard(c) => format!("{}:{}", c.private_key, c.public_key),
            ProtocolConfig::AnyTls(c) => c.password.clone(),
            ProtocolConfig::ShadowTls(c) => format!("{}:{}", c.version, c.password),
            ProtocolConfig::Naive(c) => format!("{}:{}", c.username, c.password),
            ProtocolConfig::Snell(c) => format!("{}:{}", c.version, c.psk),
            ProtocolConfig::Mieru(c) => format!("{}:{}", c.username, c.password),
            ProtocolConfig::Ssh(c) => format!(
                "{}:{}",
                c.user,
                c.password
                    .as_deref()
                    .or(c.private_key.as_deref())
                    .unwrap_or("")
            ),
            ProtocolConfig::Socks5(c) => format!(
                "{}:{}",
                c.username.as_deref().unwrap_or(""),
                c.password.as_deref().unwrap_or("")
            ),
            ProtocolConfig::Http(c) => format!(
                "{}:{}",
                c.username.as_deref().unwrap_or(""),
                c.password.as_deref().unwrap_or("")
            ),
        }
    }
}

/// Unified intermediate proxy node representation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ProxyNode {
    pub id: String,
    pub name: String,
    pub server: String,
    pub port: u16,
    pub protocol: ProtocolType,
    #[serde(default)]
    pub tls: Option<TlsConfig>,
    #[serde(default)]
    pub transport: Option<TransportConfig>,
    pub settings: ProtocolConfig,
    #[serde(default)]
    pub subscription_id: Option<String>,
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub latency: Option<i64>,
    #[serde(default)]
    pub dialer_proxy: Option<String>,
    #[serde(default)]
    pub preferred_core: Option<CoreType>,
    #[serde(default)]
    pub sort_order: i32,
}

impl ProxyNode {
    /// Returns the effective transport kind (defaults to TCP when None)
    pub fn transport_kind(&self) -> TransportKind {
        self.transport
            .as_ref()
            .map(TransportConfig::kind)
            .unwrap_or(TransportKind::Tcp)
    }

    /// Computes a deterministic SHA-256 fingerprint from server + port + protocol + transport + TLS + key credentials
    pub fn compute_fingerprint(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.server.trim().to_ascii_lowercase().as_bytes());
        hasher.update(b":");
        hasher.update(self.port.to_string().as_bytes());
        hasher.update(b":");
        hasher.update(self.protocol.as_str().as_bytes());
        hasher.update(b":");
        hasher.update(format!("{:?}", self.transport_kind()).as_bytes());
        hasher.update(b":");
        hasher.update(self.settings.credential_key().as_bytes());

        if let ProtocolConfig::Hysteria2(hy2) = &self.settings {
            if let Some(ports) = &hy2.ports {
                hasher.update(b":ports=");
                hasher.update(ports.trim().as_bytes());
            }
        }

        if let Some(tr) = &self.transport {
            match tr {
                TransportConfig::Ws { path, host, .. } => {
                    hasher.update(b":ws=");
                    hasher.update(path.trim().as_bytes());
                    if let Some(h) = host {
                        hasher.update(b"@");
                        hasher.update(h.trim().to_ascii_lowercase().as_bytes());
                    }
                }
                TransportConfig::Grpc {
                    service_name,
                    authority,
                    ..
                } => {
                    hasher.update(b":grpc=");
                    hasher.update(service_name.trim().as_bytes());
                    if let Some(a) = authority {
                        hasher.update(b"@");
                        hasher.update(a.trim().to_ascii_lowercase().as_bytes());
                    }
                }
                TransportConfig::Http { host, path, .. } => {
                    hasher.update(b":http=");
                    hasher.update(path.trim().as_bytes());
                    for h in host {
                        hasher.update(b"@");
                        hasher.update(h.trim().to_ascii_lowercase().as_bytes());
                    }
                }
                TransportConfig::HttpUpgrade { host, path, .. } => {
                    hasher.update(b":hu=");
                    hasher.update(path.trim().as_bytes());
                    if let Some(h) = host {
                        hasher.update(b"@");
                        hasher.update(h.trim().to_ascii_lowercase().as_bytes());
                    }
                }
                TransportConfig::Xhttp {
                    host, path, mode, ..
                } => {
                    hasher.update(b":xh=");
                    hasher.update(path.trim().as_bytes());
                    if let Some(h) = host {
                        hasher.update(b"@");
                        hasher.update(h.trim().to_ascii_lowercase().as_bytes());
                    }
                    if let Some(m) = mode {
                        hasher.update(b"#");
                        hasher.update(m.trim().as_bytes());
                    }
                }
                TransportConfig::Tcp {
                    header_type,
                    http_path,
                    ..
                } => {
                    if let Some(ht) = header_type {
                        hasher.update(b":tcp=");
                        hasher.update(ht.trim().as_bytes());
                    }
                    if let Some(p) = http_path {
                        hasher.update(b":path=");
                        hasher.update(p.trim().as_bytes());
                    }
                }
                TransportConfig::Mkcp {
                    seed, header_type, ..
                } => {
                    if let Some(s) = seed {
                        hasher.update(b":seed=");
                        hasher.update(s.trim().as_bytes());
                    }
                    if let Some(ht) = header_type {
                        hasher.update(b":ht=");
                        hasher.update(ht.trim().as_bytes());
                    }
                }
                TransportConfig::Quic { security, key, .. } => {
                    if let Some(sec) = security {
                        hasher.update(b":qsec=");
                        hasher.update(sec.trim().as_bytes());
                    }
                    if let Some(k) = key {
                        hasher.update(b":qkey=");
                        hasher.update(k.trim().as_bytes());
                    }
                }
            }
        }

        if let Some(tls) = &self.tls {
            if tls.enabled {
                hasher.update(b":tls");
                if let Some(sni) = &tls.server_name {
                    hasher.update(b":sni=");
                    hasher.update(sni.trim().to_ascii_lowercase().as_bytes());
                }
                if let Some(reality) = &tls.reality {
                    hasher.update(b":pbk=");
                    hasher.update(reality.public_key.trim().as_bytes());
                }
            }
        }

        hex::encode(hasher.finalize())
    }
}

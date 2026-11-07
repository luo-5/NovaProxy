use crate::model::{CoreType, ProtocolType, ProxyNode, TransportKind};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use ts_rs::TS;

/// Default data-driven capability matrix JSON (can be hot-reloaded at runtime)
pub const DEFAULT_CAPABILITIES_JSON: &str = r#"{
  "version": "2026.11.1",
  "rules": [
    {
      "id": "vless-xray-exclusive-transports",
      "protocol": "vless",
      "transports": ["xhttp", "mkcp"],
      "default_core": "xray",
      "supported_cores": ["xray"],
      "notes": "VLESS over XHTTP or mKCP requires Xray-core"
    },
    {
      "id": "vmess-xray-exclusive-transports",
      "protocol": "vmess",
      "transports": ["xhttp", "mkcp"],
      "default_core": "xray",
      "supported_cores": ["xray"],
      "notes": "VMess over XHTTP or mKCP requires Xray-core"
    },
    {
      "id": "shadowsocks-general",
      "protocol": "shadowsocks",
      "transports": [],
      "default_core": "sing-box",
      "supported_cores": ["sing-box", "mihomo", "xray"],
      "notes": "Shadowsocks including 2022 ciphers and obfs/v2ray-plugin"
    },
    {
      "id": "shadowsocksr-general",
      "protocol": "shadowsocksr",
      "transports": [],
      "default_core": "mihomo",
      "supported_cores": ["mihomo"],
      "notes": "ShadowsocksR is supported by mihomo"
    },
    {
      "id": "vmess-general",
      "protocol": "vmess",
      "transports": [],
      "default_core": "sing-box",
      "supported_cores": ["sing-box", "mihomo", "xray"],
      "notes": "VMess over TCP/WS/gRPC/HTTP/H2/QUIC"
    },
    {
      "id": "vless-general",
      "protocol": "vless",
      "transports": [],
      "default_core": "sing-box",
      "supported_cores": ["sing-box", "mihomo", "xray"],
      "notes": "VLESS with TLS, REALITY, or XTLS-Vision"
    },
    {
      "id": "trojan-general",
      "protocol": "trojan",
      "transports": [],
      "default_core": "sing-box",
      "supported_cores": ["sing-box", "mihomo", "xray"],
      "notes": "Trojan over TCP/WS/gRPC"
    },
    {
      "id": "hysteria-v1",
      "protocol": "hysteria",
      "transports": [],
      "default_core": "sing-box",
      "supported_cores": ["sing-box", "mihomo"],
      "notes": "Hysteria v1 QUIC protocol"
    },
    {
      "id": "hysteria2-general",
      "protocol": "hysteria2",
      "transports": [],
      "default_core": "sing-box",
      "supported_cores": ["sing-box", "mihomo", "xray"],
      "notes": "Hysteria2 QUIC protocol"
    },
    {
      "id": "tuic-v5",
      "protocol": "tuic",
      "transports": [],
      "default_core": "sing-box",
      "supported_cores": ["sing-box", "mihomo"],
      "notes": "TUIC v5 protocol"
    },
    {
      "id": "wireguard-general",
      "protocol": "wireguard",
      "transports": [],
      "default_core": "sing-box",
      "supported_cores": ["sing-box", "mihomo", "xray"],
      "notes": "WireGuard tunnel outbound"
    },
    {
      "id": "anytls-general",
      "protocol": "anytls",
      "transports": [],
      "default_core": "sing-box",
      "supported_cores": ["sing-box", "mihomo"],
      "notes": "AnyTLS protocol"
    },
    {
      "id": "shadowtls-v3",
      "protocol": "shadowtls",
      "transports": [],
      "default_core": "sing-box",
      "supported_cores": ["sing-box", "mihomo"],
      "notes": "ShadowTLS v3"
    },
    {
      "id": "naiveproxy-general",
      "protocol": "naive",
      "transports": [],
      "default_core": "sing-box",
      "supported_cores": ["sing-box"],
      "notes": "NaiveProxy outbound (built with with_naive_outbound tag)"
    },
    {
      "id": "snell-general",
      "protocol": "snell",
      "transports": [],
      "default_core": "mihomo",
      "supported_cores": ["mihomo"],
      "notes": "Snell v3/v4/v5 supported by mihomo"
    },
    {
      "id": "mieru-general",
      "protocol": "mieru",
      "transports": [],
      "default_core": "mihomo",
      "supported_cores": ["mihomo"],
      "notes": "mieru supported by mihomo"
    },
    {
      "id": "ssh-general",
      "protocol": "ssh",
      "transports": [],
      "default_core": "sing-box",
      "supported_cores": ["sing-box", "mihomo"],
      "notes": "SSH tunnel outbound"
    },
    {
      "id": "socks5-general",
      "protocol": "socks5",
      "transports": [],
      "default_core": "sing-box",
      "supported_cores": ["sing-box", "mihomo", "xray"],
      "notes": "SOCKS5 outbound"
    },
    {
      "id": "http-general",
      "protocol": "http",
      "transports": [],
      "default_core": "sing-box",
      "supported_cores": ["sing-box", "mihomo", "xray"],
      "notes": "HTTP/HTTPS proxy outbound"
    }
  ]
}"#;

#[derive(Debug, Error)]
pub enum CapabilityError {
    #[error("Failed to parse capabilities JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("Failed to read capabilities file: {0}")]
    Io(#[from] std::io::Error),
    #[error("Capability rule '{0}' has an empty supported_cores list")]
    EmptySupportedCores(String),
    #[error("No capability rule found for protocol '{protocol}' and transport '{transport:?}'")]
    NoMatchingRule {
        protocol: ProtocolType,
        transport: TransportKind,
    },
    #[error("None of the supported cores ({supported:?}) for rule '{rule_id}' are currently installed/available")]
    NoAvailableCore {
        rule_id: String,
        supported: Vec<CoreType>,
    },
}

/// Single rule in the data-driven capability table
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CapabilityRule {
    pub id: String,
    pub protocol: ProtocolType,
    #[serde(default)]
    pub transports: Vec<TransportKind>,
    pub default_core: CoreType,
    pub supported_cores: Vec<CoreType>,
    #[serde(default)]
    pub notes: Option<String>,
}

/// Complete capability table schema
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CoreCapabilities {
    pub version: String,
    pub rules: Vec<CapabilityRule>,
}

/// Reason why a particular core was chosen
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SelectionReason {
    DefaultForProtocol,
    UserPreferred,
    FallbackFromUnsupportedPreferred { requested: CoreType },
    FallbackToAvailableCore { primary_unavailable: CoreType },
}

/// Result of automatic core selection for a proxy node
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CoreSelection {
    pub core: CoreType,
    pub reason: SelectionReason,
    pub matched_rule_id: String,
    pub supported_cores: Vec<CoreType>,
}

impl CoreCapabilities {
    /// Parses and validates a `CoreCapabilities` table from a JSON string
    pub fn from_json_str(json_str: &str) -> Result<Self, CapabilityError> {
        let trimmed = json_str.trim_start_matches('\u{feff}');
        let caps: CoreCapabilities = serde_json::from_str(trimmed)?;
        caps.validate()?;
        Ok(caps)
    }

    /// Loads default built-in capability matrix
    pub fn default_table() -> Self {
        Self::from_json_str(DEFAULT_CAPABILITIES_JSON)
            .expect("Built-in DEFAULT_CAPABILITIES_JSON must be valid")
    }

    pub fn validate(&self) -> Result<(), CapabilityError> {
        for rule in &self.rules {
            if rule.supported_cores.is_empty() {
                return Err(CapabilityError::EmptySupportedCores(rule.id.clone()));
            }
        }
        Ok(())
    }

    /// Finds the most specific matching rule for `(protocol, transport)`
    pub fn find_rule(
        &self,
        protocol: ProtocolType,
        transport: TransportKind,
    ) -> Result<&CapabilityRule, CapabilityError> {
        // 1. Check rules that explicitly list the transport (higher specificity)
        if let Some(specific) = self
            .rules
            .iter()
            .find(|r| r.protocol == protocol && r.transports.contains(&transport))
        {
            return Ok(specific);
        }

        // 2. Fallback to general protocol rule (where transports filter is empty)
        if let Some(general) = self
            .rules
            .iter()
            .find(|r| r.protocol == protocol && r.transports.is_empty())
        {
            return Ok(general);
        }

        Err(CapabilityError::NoMatchingRule {
            protocol,
            transport,
        })
    }

    /// Selects the optimal core for a `ProxyNode` given optional user preference and installed cores.
    /// If `available_cores` is empty, all cores in the rule are assumed available.
    pub fn select_core_for_node(
        &self,
        node: &ProxyNode,
        global_preferred: Option<CoreType>,
        available_cores: &[CoreType],
    ) -> Result<CoreSelection, CapabilityError> {
        let transport = node.transport_kind();
        let rule = self.find_rule(node.protocol, transport)?;

        let is_available =
            |core: CoreType| available_cores.is_empty() || available_cores.contains(&core);

        let effective_preferred = node.preferred_core.or(global_preferred);

        // If user or node specified a preferred core:
        if let Some(pref) = effective_preferred {
            if rule.supported_cores.contains(&pref) && is_available(pref) {
                return Ok(CoreSelection {
                    core: pref,
                    reason: SelectionReason::UserPreferred,
                    matched_rule_id: rule.id.clone(),
                    supported_cores: rule.supported_cores.clone(),
                });
            }

            // Preferred core is either unsupported for this protocol/transport or not installed
            if is_available(rule.default_core) {
                return Ok(CoreSelection {
                    core: rule.default_core,
                    reason: SelectionReason::FallbackFromUnsupportedPreferred { requested: pref },
                    matched_rule_id: rule.id.clone(),
                    supported_cores: rule.supported_cores.clone(),
                });
            }

            if let Some(&fallback) = rule.supported_cores.iter().find(|&&c| is_available(c)) {
                return Ok(CoreSelection {
                    core: fallback,
                    reason: SelectionReason::FallbackFromUnsupportedPreferred { requested: pref },
                    matched_rule_id: rule.id.clone(),
                    supported_cores: rule.supported_cores.clone(),
                });
            }

            return Err(CapabilityError::NoAvailableCore {
                rule_id: rule.id.clone(),
                supported: rule.supported_cores.clone(),
            });
        }

        // Standard automatic selection: use rule.default_core if available
        if is_available(rule.default_core) {
            return Ok(CoreSelection {
                core: rule.default_core,
                reason: SelectionReason::DefaultForProtocol,
                matched_rule_id: rule.id.clone(),
                supported_cores: rule.supported_cores.clone(),
            });
        }

        // Otherwise pick first available supported core
        if let Some(&fallback) = rule.supported_cores.iter().find(|&&c| is_available(c)) {
            return Ok(CoreSelection {
                core: fallback,
                reason: SelectionReason::FallbackToAvailableCore {
                    primary_unavailable: rule.default_core,
                },
                matched_rule_id: rule.id.clone(),
                supported_cores: rule.supported_cores.clone(),
            });
        }

        Err(CapabilityError::NoAvailableCore {
            rule_id: rule.id.clone(),
            supported: rule.supported_cores.clone(),
        })
    }
}

/// Thread-safe, hot-reloadable registry holding the active `CoreCapabilities` table
#[derive(Clone)]
pub struct CoreCapabilitiesRegistry {
    inner: Arc<RwLock<CoreCapabilities>>,
}

impl Default for CoreCapabilitiesRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CoreCapabilitiesRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(CoreCapabilities::default_table())),
        }
    }

    pub fn snapshot(&self) -> CoreCapabilities {
        self.inner.read().clone()
    }

    /// Hot-reloads the capability table from a JSON string at runtime
    pub fn reload_from_json(&self, json_str: &str) -> Result<String, CapabilityError> {
        let new_table = CoreCapabilities::from_json_str(json_str)?;
        let version = new_table.version.clone();
        *self.inner.write() = new_table;
        Ok(version)
    }

    /// Hot-reloads the capability table from a JSON file on disk
    pub fn reload_from_file(&self, path: impl AsRef<Path>) -> Result<String, CapabilityError> {
        let content = std::fs::read_to_string(path)?;
        self.reload_from_json(&content)
    }

    pub fn select_core_for_node(
        &self,
        node: &ProxyNode,
        global_preferred: Option<CoreType>,
        available_cores: &[CoreType],
    ) -> Result<CoreSelection, CapabilityError> {
        self.inner
            .read()
            .select_core_for_node(node, global_preferred, available_cores)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        ProtocolConfig, ShadowsocksRConfig, SnellConfig, TransportConfig, VlessConfig,
    };

    fn make_vless_node(transport: Option<TransportConfig>) -> ProxyNode {
        ProxyNode {
            id: "n1".to_string(),
            name: "VLESS Node".to_string(),
            server: "example.invalid".to_string(),
            port: 443,
            protocol: ProtocolType::Vless,
            tls: None,
            transport,
            settings: ProtocolConfig::Vless(VlessConfig {
                uuid: "00000000-0000-0000-0000-000000000000".to_string(),
                flow: None,
                encryption: "none".to_string(),
                packet_encoding: None,
            }),
            subscription_id: None,
            group_id: None,
            latency: None,
            dialer_proxy: None,
            preferred_core: None,
            sort_order: 0,
        }
    }

    #[test]
    fn test_protocol_matrix_routing() {
        let reg = CoreCapabilitiesRegistry::new();
        let all_cores = vec![CoreType::SingBox, CoreType::Mihomo, CoreType::Xray];

        // 1. Standard VLESS over TCP -> sing-box
        let vless_tcp = make_vless_node(None);
        let sel = reg
            .select_core_for_node(&vless_tcp, None, &all_cores)
            .unwrap();
        assert_eq!(sel.core, CoreType::SingBox);
        assert_eq!(sel.reason, SelectionReason::DefaultForProtocol);

        // 2. VLESS over XHTTP -> Xray (specific transport rule overrides general VLESS rule)
        let vless_xhttp = make_vless_node(Some(TransportConfig::Xhttp {
            host: None,
            path: "/xhttp".to_string(),
            mode: Some("auto".to_string()),
            extra: None,
        }));
        let sel_xhttp = reg
            .select_core_for_node(&vless_xhttp, None, &all_cores)
            .unwrap();
        assert_eq!(sel_xhttp.core, CoreType::Xray);
        assert_eq!(sel_xhttp.matched_rule_id, "vless-xray-exclusive-transports");

        // 3. VLESS over mKCP -> Xray
        let vless_mkcp = make_vless_node(Some(TransportConfig::Mkcp {
            seed: None,
            header_type: None,
            mtu: None,
            tti: None,
            uplink_capacity: None,
            downlink_capacity: None,
            congestion: false,
        }));
        let sel_mkcp = reg
            .select_core_for_node(&vless_mkcp, None, &all_cores)
            .unwrap();
        assert_eq!(sel_mkcp.core, CoreType::Xray);

        // 4. SSR -> mihomo
        let mut ssr_node = make_vless_node(None);
        ssr_node.protocol = ProtocolType::ShadowsocksR;
        ssr_node.settings = ProtocolConfig::ShadowsocksR(ShadowsocksRConfig {
            method: "aes-256-cfb".to_string(),
            password: "dummy".to_string(),
            protocol: "origin".to_string(),
            protocol_param: None,
            obfs: "plain".to_string(),
            obfs_param: None,
        });
        let sel_ssr = reg
            .select_core_for_node(&ssr_node, None, &all_cores)
            .unwrap();
        assert_eq!(sel_ssr.core, CoreType::Mihomo);

        // 5. Snell -> mihomo
        let mut snell_node = make_vless_node(None);
        snell_node.protocol = ProtocolType::Snell;
        snell_node.settings = ProtocolConfig::Snell(SnellConfig {
            psk: "dummy".to_string(),
            version: 4,
            obfs_mode: None,
            obfs_host: None,
        });
        let sel_snell = reg
            .select_core_for_node(&snell_node, None, &all_cores)
            .unwrap();
        assert_eq!(sel_snell.core, CoreType::Mihomo);
    }

    #[test]
    fn test_preferred_core_and_fallbacks() {
        let reg = CoreCapabilitiesRegistry::new();
        let all_cores = vec![CoreType::SingBox, CoreType::Mihomo, CoreType::Xray];

        let vless_tcp = make_vless_node(None);
        // User prefers mihomo, and VLESS TCP supports mihomo -> chooses mihomo
        let sel = reg
            .select_core_for_node(&vless_tcp, Some(CoreType::Mihomo), &all_cores)
            .unwrap();
        assert_eq!(sel.core, CoreType::Mihomo);
        assert_eq!(sel.reason, SelectionReason::UserPreferred);

        // VLESS XHTTP when user prefers sing-box -> falls back to Xray because sing-box doesn't support XHTTP
        let vless_xhttp = make_vless_node(Some(TransportConfig::Xhttp {
            host: None,
            path: "/xhttp".to_string(),
            mode: None,
            extra: None,
        }));
        let sel_fallback = reg
            .select_core_for_node(&vless_xhttp, Some(CoreType::SingBox), &all_cores)
            .unwrap();
        assert_eq!(sel_fallback.core, CoreType::Xray);
        assert_eq!(
            sel_fallback.reason,
            SelectionReason::FallbackFromUnsupportedPreferred {
                requested: CoreType::SingBox
            }
        );

        // When primary core (sing-box) is not in available_cores, falls back to mihomo
        let only_mihomo = vec![CoreType::Mihomo];
        let sel_avail = reg
            .select_core_for_node(&vless_tcp, None, &only_mihomo)
            .unwrap();
        assert_eq!(sel_avail.core, CoreType::Mihomo);
        assert_eq!(
            sel_avail.reason,
            SelectionReason::FallbackToAvailableCore {
                primary_unavailable: CoreType::SingBox
            }
        );
    }

    #[test]
    fn test_hot_reload_capabilities() {
        let reg = CoreCapabilitiesRegistry::new();
        let custom_json = r#"{
            "version": "2026.12.0-custom",
            "rules": [
                {
                    "id": "vless-custom",
                    "protocol": "vless",
                    "transports": [],
                    "default_core": "xray",
                    "supported_cores": ["xray", "sing-box"]
                }
            ]
        }"#;
        let ver = reg.reload_from_json(custom_json).unwrap();
        assert_eq!(ver, "2026.12.0-custom");

        let vless = make_vless_node(None);
        let sel = reg.select_core_for_node(&vless, None, &[]).unwrap();
        assert_eq!(sel.core, CoreType::Xray);
    }
}

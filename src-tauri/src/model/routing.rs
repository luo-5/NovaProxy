use crate::model::settings::RoutingMode;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Rule matching condition type
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum RuleType {
    Domain,
    DomainSuffix,
    DomainKeyword,
    DomainRegex,
    IpCidr,
    ProcessName,
    Port,
    GeoSite,
    GeoIp,
}

impl RuleType {
    pub fn as_str(&self) -> &'static str {
        match self {
            RuleType::Domain => "domain",
            RuleType::DomainSuffix => "domain_suffix",
            RuleType::DomainKeyword => "domain_keyword",
            RuleType::DomainRegex => "domain_regex",
            RuleType::IpCidr => "ip_cidr",
            RuleType::ProcessName => "process_name",
            RuleType::Port => "port",
            RuleType::GeoSite => "geosite",
            RuleType::GeoIp => "geoip",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "domain" | "full" => RuleType::Domain,
            "domain_suffix" | "suffix" => RuleType::DomainSuffix,
            "domain_keyword" | "keyword" => RuleType::DomainKeyword,
            "domain_regex" | "regex" => RuleType::DomainRegex,
            "ip_cidr" | "ip" | "cidr" => RuleType::IpCidr,
            "process_name" | "process" => RuleType::ProcessName,
            "port" => RuleType::Port,
            "geosite" => RuleType::GeoSite,
            "geoip" => RuleType::GeoIp,
            _ => RuleType::DomainSuffix,
        }
    }
}

/// Action to execute when a rule matches
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "target", rename_all = "lowercase")]
pub enum RuleAction {
    Proxy,
    Direct,
    Block,
    Tag(String),
}

impl RuleAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            RuleAction::Proxy => "proxy",
            RuleAction::Direct => "direct",
            RuleAction::Block => "block",
            RuleAction::Tag(_) => "tag",
        }
    }

    pub fn target_tag<'a>(&'a self, default_proxy_tag: &'a str) -> &'a str {
        match self {
            RuleAction::Proxy => default_proxy_tag,
            RuleAction::Direct => "direct",
            RuleAction::Block => "block",
            RuleAction::Tag(t) => t.as_str(),
        }
    }
}

/// Individual customizable routing rule
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RuleItem {
    pub id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub rule_type: RuleType,
    pub pattern: String,
    pub action: RuleAction,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub sort_order: i32,
}

fn default_true() -> bool {
    true
}

/// Application-wide routing settings
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RoutingSettings {
    #[serde(default)]
    pub mode: RoutingMode,
    #[serde(default = "default_true")]
    pub cn_direct: bool,
    #[serde(default = "default_true")]
    pub private_direct: bool,
    #[serde(default = "default_true")]
    pub ads_block: bool,
    #[serde(default)]
    pub custom_rules: Vec<RuleItem>,
    #[serde(default = "default_outbound_proxy")]
    pub default_outbound: String,
}

fn default_outbound_proxy() -> String {
    "proxy".to_string()
}

impl Default for RoutingSettings {
    fn default() -> Self {
        Self {
            mode: RoutingMode::Rule,
            cn_direct: true,
            private_direct: true,
            ads_block: true,
            custom_rules: Vec::new(),
            default_outbound: "proxy".to_string(),
        }
    }
}

/// DNS domain resolution IP preference strategy
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum DnsStrategy {
    #[default]
    PreferIpv4,
    PreferIpv6,
    Ipv4Only,
    Ipv6Only,
}

impl DnsStrategy {
    pub fn as_singbox_str(&self) -> &'static str {
        match self {
            DnsStrategy::PreferIpv4 => "prefer_ipv4",
            DnsStrategy::PreferIpv6 => "prefer_ipv6",
            DnsStrategy::Ipv4Only => "ipv4_only",
            DnsStrategy::Ipv6Only => "ipv6_only",
        }
    }
}

/// Global DNS configuration settings
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DnsSettings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub fake_ip: bool,
    #[serde(default = "default_fake_ip_range")]
    pub fake_ip_range: String,
    #[serde(default = "default_domestic_dns")]
    pub domestic_dns: Vec<String>,
    #[serde(default = "default_remote_dns")]
    pub remote_dns: Vec<String>,
    #[serde(default = "default_fallback_dns")]
    pub fallback_dns: Vec<String>,
    #[serde(default)]
    pub strategy: DnsStrategy,
    #[serde(default)]
    pub disable_ipv6: bool,
}

fn default_fake_ip_range() -> String {
    "198.18.0.0/15".to_string()
}

fn default_domestic_dns() -> Vec<String> {
    vec![
        "https://223.5.5.5/dns-query".to_string(),
        "223.5.5.5".to_string(),
    ]
}

fn default_remote_dns() -> Vec<String> {
    vec![
        "https://8.8.8.8/dns-query".to_string(),
        "https://1.1.1.1/dns-query".to_string(),
    ]
}

fn default_fallback_dns() -> Vec<String> {
    vec!["1.1.1.1".to_string(), "8.8.8.8".to_string()]
}

impl Default for DnsSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            fake_ip: true,
            fake_ip_range: default_fake_ip_range(),
            domestic_dns: default_domestic_dns(),
            remote_dns: default_remote_dns(),
            fallback_dns: default_fallback_dns(),
            strategy: DnsStrategy::default(),
            disable_ipv6: false,
        }
    }
}

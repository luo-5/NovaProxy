use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Subscription source metadata and quota tracking
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Subscription {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub user_agent: Option<String>,
    #[serde(default)]
    pub update_interval_mins: Option<u32>,
    #[serde(default)]
    pub use_proxy: bool,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub updated_at: Option<i64>,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub upload: Option<u64>,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub download: Option<u64>,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub total: Option<u64>,
    #[serde(default)]
    #[ts(type = "number | null")]
    pub expire: Option<i64>,
    #[serde(default)]
    pub home_url: Option<String>,
    #[serde(default)]
    pub sort_order: i32,
}

fn default_enabled() -> bool {
    true
}

/// Outbound proxy group strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum GroupType {
    Selector,
    UrlTest,
    Fallback,
    LoadBalance,
}

impl GroupType {
    pub fn as_str(&self) -> &'static str {
        match self {
            GroupType::Selector => "selector",
            GroupType::UrlTest => "urltest",
            GroupType::Fallback => "fallback",
            GroupType::LoadBalance => "loadbalance",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "urltest" | "url-test" => GroupType::UrlTest,
            "fallback" => GroupType::Fallback,
            "loadbalance" | "load-balance" => GroupType::LoadBalance,
            _ => GroupType::Selector,
        }
    }
}

/// User-defined or subscription-generated proxy group
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct NodeGroup {
    pub id: String,
    pub name: String,
    pub group_type: GroupType,
    #[serde(default)]
    pub node_ids: Vec<String>,
    #[serde(default)]
    pub test_url: Option<String>,
    #[serde(default)]
    pub interval_secs: Option<u32>,
    #[serde(default)]
    pub sort_order: i32,
}

/// Historical latency measurement record for a node
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LatencyRecord {
    #[ts(type = "number")]
    pub id: i64,
    pub node_id: String,
    #[ts(type = "number")]
    pub tested_at: i64,
    #[ts(type = "number | null")]
    pub latency_ms: Option<i64>,
    pub test_type: String,
    #[serde(default)]
    pub error_msg: Option<String>,
}

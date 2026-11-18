use crate::model::CoreType;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Traffic routing mode
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum RoutingMode {
    #[default]
    Rule,
    Global,
    Direct,
}

/// Application and core log verbosity level
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }
}

/// Global application settings
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AppSettings {
    pub mixed_port: u16,
    #[serde(default)]
    pub socks_port: Option<u16>,
    #[serde(default)]
    pub http_port: Option<u16>,
    pub clash_api_port: u16,
    #[serde(default)]
    pub clash_api_secret: Option<String>,
    pub allow_lan: bool,
    pub ipv6: bool,
    pub log_level: LogLevel,
    pub routing_mode: RoutingMode,
    #[serde(default)]
    pub preferred_core: Option<CoreType>,
    pub tun_mode: bool,
    pub system_proxy: bool,
    #[serde(default)]
    pub active_node_id: Option<String>,
    pub auto_port_fallback: bool,
    #[serde(default)]
    pub register_clash_scheme: bool,
    pub language: String,
    pub theme: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            mixed_port: 7890,
            socks_port: None,
            http_port: None,
            clash_api_port: 9090,
            clash_api_secret: None,
            allow_lan: false,
            ipv6: true,
            log_level: LogLevel::Info,
            routing_mode: RoutingMode::Rule,
            preferred_core: None,
            tun_mode: false,
            system_proxy: false,
            active_node_id: None,
            auto_port_fallback: true,
            register_clash_scheme: false,
            language: "zh-CN".to_string(),
            theme: "dark".to_string(),
        }
    }
}

/// Backward-compatible alias for `AppSettings`
pub type Settings = AppSettings;

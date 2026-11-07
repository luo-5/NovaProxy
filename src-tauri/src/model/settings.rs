use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub mixed_port: u16,
    pub allow_lan: bool,
    pub ipv6: bool,
    pub log_level: String,
    pub tun_mode: bool,
    pub system_proxy: bool,
    pub active_node_id: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            mixed_port: 7890,
            allow_lan: false,
            ipv6: true,
            log_level: "info".to_string(),
            tun_mode: false,
            system_proxy: false,
            active_node_id: None,
        }
    }
}

//! TUN mode configuration and interface utilities

use super::elevation::is_elevated;
use super::sysproxy::SystemError;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Tun device and routing parameters
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TunSettings {
    pub enabled: bool,
    pub interface_name: String,
    pub address: String,
    pub auto_route: bool,
    pub strict_route: bool,
    pub stack: String,
    pub mtu: u16,
}

impl Default for TunSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            interface_name: "NovaProxy-TUN".to_string(),
            address: "172.19.0.1/30".to_string(),
            auto_route: true,
            strict_route: true,
            stack: "system".to_string(),
            mtu: 1500,
        }
    }
}

/// Verifies that the host process or runner has Administrator privileges required for TUN virtual adapters
pub fn verify_tun_prerequisites() -> Result<(), SystemError> {
    if !is_elevated() {
        return Err(SystemError::General(
            "TUN mode requires Administrator privileges on Windows. Please relaunch NovaProxy as Administrator.".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tun_settings_default() {
        let tun = TunSettings::default();
        assert_eq!(tun.interface_name, "NovaProxy-TUN");
        assert!(tun.auto_route);
    }
}

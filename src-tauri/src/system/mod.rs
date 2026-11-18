//! System integration (System proxy, TUN mode, Service, Autostart)

pub mod autostart;
pub mod deeplink;
pub mod elevation;
pub mod network;
pub mod service;
pub mod sysproxy;
pub mod tun;

pub use autostart::*;
pub use deeplink::*;
pub use elevation::*;
pub use network::*;
pub use service::*;
pub use sysproxy::*;
pub use tun::*;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Runtime platform & capability summary reported to the frontend for adaptive UI and diagnostics
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PlatformInfo {
    pub os: String,
    pub arch: String,
    pub is_mobile: bool,
    pub supports_system_proxy: bool,
    pub supports_autostart: bool,
    pub supports_tray: bool,
    pub supports_screen_qr: bool,
    pub supports_core_update: bool,
    pub supports_tun: bool,
}

pub fn detect_platform_info() -> PlatformInfo {
    PlatformInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        is_mobile: cfg!(any(target_os = "android", target_os = "ios")),
        supports_system_proxy: cfg!(any(
            target_os = "windows",
            target_os = "macos",
            target_os = "linux"
        )),
        supports_autostart: cfg!(any(
            target_os = "windows",
            target_os = "macos",
            target_os = "linux"
        )),
        supports_tray: cfg!(not(any(target_os = "android", target_os = "ios"))),
        supports_screen_qr: cfg!(not(any(target_os = "android", target_os = "ios"))),
        supports_core_update: cfg!(not(any(target_os = "android", target_os = "ios"))),
        supports_tun: true,
    }
}

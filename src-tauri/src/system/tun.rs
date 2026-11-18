//! Cross-platform TUN mode readiness inspection and settings

use super::elevation::is_elevated;
use super::service::{query_service_state, ServiceState};
use super::sysproxy::SystemError;
use serde::{Deserialize, Serialize};
use std::path::Path;
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
            interface_name: default_tun_interface_name(),
            address: "172.19.0.1/30".to_string(),
            auto_route: true,
            strict_route: true,
            stack: "system".to_string(),
            mtu: 1500,
        }
    }
}

fn default_tun_interface_name() -> String {
    #[cfg(target_os = "macos")]
    {
        "utun9".to_string()
    }
    #[cfg(target_os = "linux")]
    {
        "novaproxy0".to_string()
    }
    #[cfg(target_os = "android")]
    {
        "tun0".to_string()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "android")))]
    {
        "NovaProxy-TUN".to_string()
    }
}

/// Runtime readiness status for TUN mode across Windows, macOS, Linux, and Android
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WintunStatus {
    pub elevated: bool,
    pub wintun_present: bool,
    pub wintun_path: Option<String>,
    pub service_state: ServiceState,
    pub ready: bool,
    pub message: String,
}

/// Inspects OS privilege / TUN driver readiness for the active platform.
pub fn check_wintun_status(cores_dir: &Path) -> WintunStatus {
    let elevated = is_elevated();
    let service_state = query_service_state().unwrap_or(ServiceState::NotInstalled);

    #[cfg(target_os = "windows")]
    {
        let can_privilege = elevated || service_state == ServiceState::Running;
        let mut candidates = vec![cores_dir.join("wintun.dll")];
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                candidates.push(dir.join("wintun.dll"));
            }
        }
        if let Some(sys_root) = std::env::var_os("SystemRoot") {
            candidates.push(Path::new(&sys_root).join("System32").join("wintun.dll"));
        }

        let found_path = candidates.into_iter().find(|p| p.exists());
        let wintun_present = found_path.is_some();
        let wintun_path = found_path.map(|p| p.to_string_lossy().to_string());

        let message = match (can_privilege, wintun_present) {
            (true, true) => "Windows TUN prerequisites satisfied (Wintun ready)".to_string(),
            (true, false) => {
                "Elevated privileges ready; sing-box/mihomo will load or unpack Wintun on TUN start"
                    .to_string()
            }
            (false, _) => {
                "Administrator privileges required for TUN adapter creation on Windows".to_string()
            }
        };

        WintunStatus {
            elevated,
            wintun_present,
            wintun_path,
            service_state,
            ready: can_privilege,
            message,
        }
    }

    #[cfg(target_os = "linux")]
    {
        let _ = cores_dir;
        let tun_dev = Path::new("/dev/net/tun");
        let wintun_present = tun_dev.exists();
        let can_privilege = elevated || service_state == ServiceState::Running;
        let ready = wintun_present && can_privilege;
        let message = if !wintun_present {
            "/dev/net/tun kernel device not found (modprobe tun)".to_string()
        } else if can_privilege {
            "Linux TUN ready (/dev/net/tun + CAP_NET_ADMIN/root)".to_string()
        } else {
            "Linux TUN requires root or CAP_NET_ADMIN capability (click Relaunch Elevated or grant setcap cap_net_admin=eip)".to_string()
        };

        WintunStatus {
            elevated,
            wintun_present,
            wintun_path: wintun_present.then(|| "/dev/net/tun".to_string()),
            service_state,
            ready,
            message,
        }
    }

    #[cfg(target_os = "macos")]
    {
        let _ = cores_dir;
        let can_privilege = elevated || service_state == ServiceState::Running;
        let message = if can_privilege {
            "macOS utun prerequisites satisfied".to_string()
        } else {
            "Administrator privileges required to open utun interfaces on macOS".to_string()
        };
        WintunStatus {
            elevated,
            wintun_present: true,
            wintun_path: Some("utun".to_string()),
            service_state,
            ready: can_privilege,
            message,
        }
    }

    #[cfg(target_os = "android")]
    {
        let _ = (cores_dir, elevated);
        WintunStatus {
            elevated: true,
            wintun_present: true,
            wintun_path: Some("android.net.VpnService".to_string()),
            service_state,
            ready: true,
            message:
                "Android VpnService + hev-socks5-tunnel ready (OS consent prompted on connect)"
                    .to_string(),
        }
    }

    #[cfg(not(any(
        target_os = "windows",
        target_os = "linux",
        target_os = "macos",
        target_os = "android"
    )))]
    {
        let _ = cores_dir;
        WintunStatus {
            elevated,
            wintun_present: false,
            wintun_path: None,
            service_state,
            ready: false,
            message: "Unsupported platform for TUN inspection".to_string(),
        }
    }
}

/// Verifies that the host process has privileges required for TUN virtual adapters
pub fn verify_tun_prerequisites() -> Result<(), SystemError> {
    if !is_elevated() {
        #[cfg(target_os = "windows")]
        return Err(SystemError::General(
            "TUN mode requires Administrator privileges on Windows. Please relaunch NovaProxy as Administrator.".to_string(),
        ));
        #[cfg(target_os = "macos")]
        return Err(SystemError::General(
            "TUN mode requires Administrator privileges on macOS to open utun devices.".to_string(),
        ));
        #[cfg(target_os = "linux")]
        return Err(SystemError::General(
            "TUN mode requires root or CAP_NET_ADMIN on Linux (relaunch elevated or run setcap cap_net_admin=eip).".to_string(),
        ));
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        return Err(SystemError::General(
            "TUN mode requires elevated privileges on this OS.".to_string(),
        ));
    }
    #[cfg(target_os = "linux")]
    if !Path::new("/dev/net/tun").exists() {
        return Err(SystemError::General(
            "/dev/net/tun device missing on Linux host".to_string(),
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
        assert!(!tun.interface_name.is_empty());
        assert!(tun.auto_route);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_check_wintun_status_detects_bundled_dll() {
        let dir = std::env::temp_dir().join("novaproxy_test_wintun_probe");
        let _ = std::fs::create_dir_all(&dir);
        let dll = dir.join("wintun.dll");
        std::fs::write(&dll, b"dummy").unwrap();

        let status = check_wintun_status(&dir);
        assert!(status.wintun_present);
        assert!(status
            .wintun_path
            .as_deref()
            .unwrap_or("")
            .ends_with("wintun.dll"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

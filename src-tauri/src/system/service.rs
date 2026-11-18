//! Cross-platform helper service status inspection

use super::sysproxy::SystemError;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const SERVICE_NAME: &str = "NovaProxyHelper";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ServiceState {
    NotInstalled,
    Stopped,
    Running,
    Paused,
}

/// Parses English/localized `sc.exe query` output into `ServiceState`.
pub fn parse_sc_query_output(stdout: &str, stderr: &str, exit_code: Option<i32>) -> ServiceState {
    let combined = format!("{stdout}\n{stderr}");
    let upper = combined.to_ascii_uppercase();

    if exit_code == Some(1060)
        || upper.contains("1060")
        || upper.contains("DOES NOT EXIST AS AN INSTALLED SERVICE")
        || combined.contains("指定的服务未安装")
    {
        return ServiceState::NotInstalled;
    }

    for line in upper.lines() {
        if line.contains("STATE") {
            if line.contains("RUNNING") || line.contains(": 4") {
                return ServiceState::Running;
            }
            if line.contains("PAUSED") || line.contains(": 7") {
                return ServiceState::Paused;
            }
            if line.contains("STOPPED")
                || line.contains("STOP_PENDING")
                || line.contains("START_PENDING")
                || line.contains(": 1")
            {
                return ServiceState::Stopped;
            }
        }
    }

    if exit_code == Some(0) {
        ServiceState::Stopped
    } else {
        ServiceState::NotInstalled
    }
}

/// Checks the live state of `NovaProxyHelper` across OS service managers.
pub fn query_service_state() -> Result<ServiceState, SystemError> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        let output = std::process::Command::new("sc.exe")
            .args(["query", SERVICE_NAME])
            .creation_flags(CREATE_NO_WINDOW)
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        Ok(parse_sc_query_output(
            &stdout,
            &stderr,
            output.status.code(),
        ))
    }
    #[cfg(target_os = "linux")]
    {
        let Ok(out) = std::process::Command::new("systemctl")
            .args(["is-active", "novaproxy-helper"])
            .output()
        else {
            return Ok(ServiceState::NotInstalled);
        };
        let state_str = String::from_utf8_lossy(&out.stdout)
            .trim()
            .to_ascii_lowercase();
        match state_str.as_str() {
            "active" => Ok(ServiceState::Running),
            "inactive" | "failed" => Ok(ServiceState::Stopped),
            _ => Ok(ServiceState::NotInstalled),
        }
    }
    #[cfg(target_os = "macos")]
    {
        let plist = std::path::Path::new("/Library/LaunchDaemons/com.novaproxy.helper.plist");
        if !plist.exists() {
            return Ok(ServiceState::NotInstalled);
        }
        let Ok(out) = std::process::Command::new("launchctl")
            .args(["list", "com.novaproxy.helper"])
            .output()
        else {
            return Ok(ServiceState::Stopped);
        };
        if out.status.success() {
            Ok(ServiceState::Running)
        } else {
            Ok(ServiceState::Stopped)
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        Ok(ServiceState::NotInstalled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sc_query_output_variants() {
        let running_out = r#"
SERVICE_NAME: NovaProxyHelper
        TYPE               : 10  WIN32_OWN_PROCESS
        STATE              : 4  RUNNING
                                (STOPPABLE, NOT_PAUSABLE, IGNORES_SHUTDOWN)
"#;
        assert_eq!(
            parse_sc_query_output(running_out, "", Some(0)),
            ServiceState::Running
        );

        let stopped_out = "        STATE              : 1  STOPPED\n";
        assert_eq!(
            parse_sc_query_output(stopped_out, "", Some(0)),
            ServiceState::Stopped
        );

        let paused_out = "        STATE              : 7  PAUSED\n";
        assert_eq!(
            parse_sc_query_output(paused_out, "", Some(0)),
            ServiceState::Paused
        );

        let missing_out = "[SC] EnumQueryServicesStatus:OpenService FAILED 1060:\nThe specified service does not exist as an installed service.";
        assert_eq!(
            parse_sc_query_output(missing_out, "", Some(1060)),
            ServiceState::NotInstalled
        );
    }

    #[test]
    fn test_query_service_state_live_no_panic() {
        let state = query_service_state().expect("service query should succeed or return state");
        assert!(matches!(
            state,
            ServiceState::NotInstalled
                | ServiceState::Stopped
                | ServiceState::Running
                | ServiceState::Paused
        ));
    }
}

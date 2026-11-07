//! Windows service status inspection and control helpers

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

/// Checks whether the NovaProxy privileged helper service is installed
pub fn query_service_state() -> Result<ServiceState, SystemError> {
    // Currently default to NotInstalled until optional external service companion is deployed
    Ok(ServiceState::NotInstalled)
}

//! Core process management and capabilities
//! Implemented in Phase P1.

pub mod capabilities;
pub mod hot_reload;
pub mod manager;
pub mod process;
pub mod updater;

pub use capabilities::{
    CapabilityError, CapabilityRule, CoreCapabilities, CoreCapabilitiesRegistry, CoreSelection,
    SelectionReason,
};
pub use hot_reload::{try_hot_reload_config, try_hot_switch_proxy, try_smart_hot_switch};
pub use manager::{
    CoreEventSink, CoreLifecycleState, CoreManager, CoreStartRequest, CoreStatusInfo, ManagerError,
};
pub use process::{
    check_core_config, default_minimal_singbox_config, find_available_port, is_port_available,
    resolve_and_patch_config_port, JobObject, PortResolution, ProcessError,
};
pub use updater::{
    apply_binary_update_with_rollback, compute_file_sha256, CoreLockEntry, CoreUpdateCheckResult,
    CoreUpdateResult, CoreUpdater, UpdaterError,
};

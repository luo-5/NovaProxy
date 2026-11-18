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
pub use hot_reload::{
    try_hot_reload_config, try_hot_switch_proxy, try_smart_hot_switch,
    try_smart_hot_switch_candidates,
};
pub use manager::{
    CoreEventSink, CoreLifecycleState, CoreManager, CoreStartRequest, CoreStatusInfo, ManagerError,
};
pub use process::{
    check_core_config, default_minimal_singbox_config, extract_clash_api_port, find_available_port,
    is_port_available, patch_relay_outbound_port, resolve_and_patch_config_port,
    resolve_and_patch_core_ports, JobObject, PortResolution, ProcessError,
};
pub use updater::{
    apply_binary_update_with_rollback, compute_file_sha256, CoreBinaryStatus, CoreLockEntry,
    CoreUpdateCheckResult, CoreUpdateResult, CoreUpdater, UpdaterError,
};

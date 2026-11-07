//! Windows System Proxy management via WinINet and Registry
//! Provides atomic activation, bypass list management, snapshot recording,
//! and clean recovery on exit or abnormal termination.

use serde::{Deserialize, Serialize};
use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use thiserror::Error;
use ts_rs::TS;

const DEFAULT_BYPASS: &str = "<local>;localhost;127.*;10.*;172.16.*;172.17.*;172.18.*;172.19.*;172.20.*;172.21.*;172.22.*;172.23.*;172.24.*;172.25.*;172.26.*;172.27.*;172.28.*;172.29.*;172.30.*;172.31.*;192.168.*";

#[derive(Debug, Error)]
pub enum SystemError {
    #[error("Windows Registry error ({0}): {1}")]
    Registry(String, u32),
    #[error("WinINet InternetSetOptionW failed: {0}")]
    WinINet(u32),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("System integration error: {0}")]
    General(String),
}

/// Snapshot of the system proxy settings
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SysProxySnapshot {
    pub enabled: bool,
    #[serde(default)]
    pub server: Option<String>,
    #[serde(default)]
    pub bypass: Option<String>,
    #[serde(default)]
    pub auto_config_url: Option<String>,
}

#[cfg(target_os = "windows")]
pub(crate) mod win32 {
    use super::*;

    pub type Hkey = *mut c_void;
    pub type Long = i32;
    pub type Dword = u32;
    pub type Bool = i32;

    pub const HKEY_CURRENT_USER: Hkey = 0x8000_0001_usize as Hkey;
    pub const KEY_READ: Dword = 0x20019;
    pub const KEY_SET_VALUE: Dword = 0x0002;
    pub const REG_SZ: Dword = 1;
    pub const REG_DWORD: Dword = 4;

    pub const INTERNET_OPTION_REFRESH: Dword = 37;
    pub const INTERNET_OPTION_SETTINGS_CHANGED: Dword = 39;

    #[link(name = "advapi32")]
    extern "system" {
        pub fn RegOpenKeyExW(
            h_key: Hkey,
            lp_sub_key: *const u16,
            ul_options: Dword,
            sam_desired: Dword,
            phk_result: *mut Hkey,
        ) -> Long;

        pub fn RegQueryValueExW(
            h_key: Hkey,
            lp_value_name: *const u16,
            lp_reserved: *mut Dword,
            lp_type: *mut Dword,
            lp_data: *mut u8,
            lpcb_data: *mut Dword,
        ) -> Long;

        pub fn RegSetValueExW(
            h_key: Hkey,
            lp_value_name: *const u16,
            reserved: Dword,
            dw_type: Dword,
            lp_data: *const u8,
            cb_data: Dword,
        ) -> Long;

        #[allow(dead_code)]
        pub fn RegDeleteValueW(h_key: Hkey, lp_value_name: *const u16) -> Long;

        pub fn RegCloseKey(h_key: Hkey) -> Long;
    }

    #[link(name = "wininet")]
    extern "system" {
        pub fn InternetSetOptionW(
            h_internet: *mut c_void,
            dw_option: Dword,
            lp_buffer: *mut c_void,
            dw_buffer_length: Dword,
        ) -> Bool;
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub fn GetLastError() -> Dword;
    }

    pub fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

/// Global system proxy manager holding recorded baseline settings
#[derive(Clone)]
pub struct SystemProxyManager {
    baseline: Arc<parking_lot::Mutex<Option<SysProxySnapshot>>>,
    is_active: Arc<AtomicBool>,
}

impl Default for SystemProxyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemProxyManager {
    pub fn new() -> Self {
        Self {
            baseline: Arc::new(parking_lot::Mutex::new(None)),
            is_active: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Reads the current system proxy configuration
    pub fn query_current() -> Result<SysProxySnapshot, SystemError> {
        #[cfg(target_os = "windows")]
        {
            use win32::*;
            let subkey = to_wide(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings");
            let mut hkey: Hkey = null_mut();

            unsafe {
                let status =
                    RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut hkey);
                if status != 0 {
                    return Err(SystemError::Registry("RegOpenKeyExW".into(), status as u32));
                }

                let read_dword = |name: &str| -> Option<u32> {
                    let wname = to_wide(name);
                    let mut val_type: Dword = 0;
                    let mut data: u32 = 0;
                    let mut data_len: Dword = std::mem::size_of::<u32>() as Dword;
                    let ret = RegQueryValueExW(
                        hkey,
                        wname.as_ptr(),
                        null_mut(),
                        &mut val_type,
                        &mut data as *mut _ as *mut u8,
                        &mut data_len,
                    );
                    if ret == 0 && val_type == REG_DWORD {
                        Some(data)
                    } else {
                        None
                    }
                };

                let read_string = |name: &str| -> Option<String> {
                    let wname = to_wide(name);
                    let mut val_type: Dword = 0;
                    let mut data_len: Dword = 0;
                    let ret = RegQueryValueExW(
                        hkey,
                        wname.as_ptr(),
                        null_mut(),
                        &mut val_type,
                        null_mut(),
                        &mut data_len,
                    );
                    if ret != 0 || data_len == 0 {
                        return None;
                    }

                    let mut buf: Vec<u8> = vec![0; data_len as usize];
                    let ret = RegQueryValueExW(
                        hkey,
                        wname.as_ptr(),
                        null_mut(),
                        &mut val_type,
                        buf.as_mut_ptr(),
                        &mut data_len,
                    );
                    if ret == 0 && val_type == REG_SZ {
                        let u16_slice: &[u16] = std::slice::from_raw_parts(
                            buf.as_ptr() as *const u16,
                            (data_len as usize) / 2,
                        );
                        let s = String::from_utf16_lossy(u16_slice);
                        Some(s.trim_matches('\0').to_string())
                    } else {
                        None
                    }
                };

                let enabled = read_dword("ProxyEnable").unwrap_or(0) != 0;
                let server = read_string("ProxyServer");
                let bypass = read_string("ProxyOverride");
                let auto_config_url = read_string("AutoConfigURL");

                RegCloseKey(hkey);

                Ok(SysProxySnapshot {
                    enabled,
                    server,
                    bypass,
                    auto_config_url,
                })
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            Ok(SysProxySnapshot::default())
        }
    }

    /// Enables system proxy targeting the specified server address (e.g. `127.0.0.1:7890`)
    pub fn set_proxy(
        &self,
        server: &str,
        custom_bypass: Option<&str>,
    ) -> Result<SysProxySnapshot, SystemError> {
        // Record baseline before modifying if not already recorded
        {
            let mut base_guard = self.baseline.lock();
            if base_guard.is_none() {
                if let Ok(cur) = Self::query_current() {
                    *base_guard = Some(cur);
                }
            }
        }

        let bypass_str = custom_bypass.unwrap_or(DEFAULT_BYPASS);

        #[cfg(target_os = "windows")]
        {
            use win32::*;
            let subkey = to_wide(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings");
            let mut hkey: Hkey = null_mut();

            unsafe {
                let status = RegOpenKeyExW(
                    HKEY_CURRENT_USER,
                    subkey.as_ptr(),
                    0,
                    KEY_SET_VALUE | KEY_READ,
                    &mut hkey,
                );
                if status != 0 {
                    return Err(SystemError::Registry(
                        "RegOpenKeyExW write".into(),
                        status as u32,
                    ));
                }

                // 1. Set ProxyEnable = 1
                let enable_val: u32 = 1;
                let w_enable = to_wide("ProxyEnable");
                RegSetValueExW(
                    hkey,
                    w_enable.as_ptr(),
                    0,
                    REG_DWORD,
                    &enable_val as *const _ as *const u8,
                    std::mem::size_of::<u32>() as Dword,
                );

                // 2. Set ProxyServer = server
                let w_server_key = to_wide("ProxyServer");
                let w_server_val = to_wide(server);
                RegSetValueExW(
                    hkey,
                    w_server_key.as_ptr(),
                    0,
                    REG_SZ,
                    w_server_val.as_ptr() as *const u8,
                    (w_server_val.len() * 2) as Dword,
                );

                // 3. Set ProxyOverride = bypass
                let w_bypass_key = to_wide("ProxyOverride");
                let w_bypass_val = to_wide(bypass_str);
                RegSetValueExW(
                    hkey,
                    w_bypass_key.as_ptr(),
                    0,
                    REG_SZ,
                    w_bypass_val.as_ptr() as *const u8,
                    (w_bypass_val.len() * 2) as Dword,
                );

                RegCloseKey(hkey);

                // 4. Notify WinINet and refresh active application proxy settings
                let ok1 =
                    InternetSetOptionW(null_mut(), INTERNET_OPTION_SETTINGS_CHANGED, null_mut(), 0);
                let ok2 = InternetSetOptionW(null_mut(), INTERNET_OPTION_REFRESH, null_mut(), 0);
                if ok1 == 0 || ok2 == 0 {
                    return Err(SystemError::WinINet(GetLastError()));
                }
            }
        }

        self.is_active.store(true, Ordering::SeqCst);
        Self::query_current()
    }

    /// Disables system proxy
    pub fn clear_proxy(&self) -> Result<SysProxySnapshot, SystemError> {
        #[cfg(target_os = "windows")]
        {
            use win32::*;
            let subkey = to_wide(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings");
            let mut hkey: Hkey = null_mut();

            unsafe {
                let status = RegOpenKeyExW(
                    HKEY_CURRENT_USER,
                    subkey.as_ptr(),
                    0,
                    KEY_SET_VALUE | KEY_READ,
                    &mut hkey,
                );
                if status == 0 {
                    // Set ProxyEnable = 0
                    let enable_val: u32 = 0;
                    let w_enable = to_wide("ProxyEnable");
                    RegSetValueExW(
                        hkey,
                        w_enable.as_ptr(),
                        0,
                        REG_DWORD,
                        &enable_val as *const _ as *const u8,
                        std::mem::size_of::<u32>() as Dword,
                    );
                    RegCloseKey(hkey);
                }

                // Notify WinINet and refresh active applications
                let _ =
                    InternetSetOptionW(null_mut(), INTERNET_OPTION_SETTINGS_CHANGED, null_mut(), 0);
                let _ = InternetSetOptionW(null_mut(), INTERNET_OPTION_REFRESH, null_mut(), 0);
            }
        }

        self.is_active.store(false, Ordering::SeqCst);
        Self::query_current()
    }

    /// Restores the original baseline proxy settings saved before NovaProxy activated
    pub fn restore_original(&self) -> Result<(), SystemError> {
        let baseline_opt = self.baseline.lock().take();
        let Some(snap) = baseline_opt else {
            let _ = self.clear_proxy();
            return Ok(());
        };

        #[cfg(target_os = "windows")]
        {
            use win32::*;
            let subkey = to_wide(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings");
            let mut hkey: Hkey = null_mut();

            unsafe {
                let status = RegOpenKeyExW(
                    HKEY_CURRENT_USER,
                    subkey.as_ptr(),
                    0,
                    KEY_SET_VALUE | KEY_READ,
                    &mut hkey,
                );
                if status == 0 {
                    let enable_val: u32 = if snap.enabled { 1 } else { 0 };
                    let w_enable = to_wide("ProxyEnable");
                    RegSetValueExW(
                        hkey,
                        w_enable.as_ptr(),
                        0,
                        REG_DWORD,
                        &enable_val as *const _ as *const u8,
                        std::mem::size_of::<u32>() as Dword,
                    );

                    if let Some(srv) = snap.server {
                        let w_key = to_wide("ProxyServer");
                        let w_val = to_wide(&srv);
                        RegSetValueExW(
                            hkey,
                            w_key.as_ptr(),
                            0,
                            REG_SZ,
                            w_val.as_ptr() as *const u8,
                            (w_val.len() * 2) as Dword,
                        );
                    }

                    if let Some(byp) = snap.bypass {
                        let w_key = to_wide("ProxyOverride");
                        let w_val = to_wide(&byp);
                        RegSetValueExW(
                            hkey,
                            w_key.as_ptr(),
                            0,
                            REG_SZ,
                            w_val.as_ptr() as *const u8,
                            (w_val.len() * 2) as Dword,
                        );
                    }

                    RegCloseKey(hkey);
                }

                let _ =
                    InternetSetOptionW(null_mut(), INTERNET_OPTION_SETTINGS_CHANGED, null_mut(), 0);
                let _ = InternetSetOptionW(null_mut(), INTERNET_OPTION_REFRESH, null_mut(), 0);
            }
        }

        self.is_active.store(snap.enabled, Ordering::SeqCst);
        Ok(())
    }
}

impl Drop for SystemProxyManager {
    fn drop(&mut self) {
        if self.is_active.load(Ordering::SeqCst) {
            let _ = self.restore_original();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_proxy_query_and_cycle() {
        let initial = SystemProxyManager::query_current().expect("query initial system proxy");

        let manager = SystemProxyManager::new();
        let set_res = manager
            .set_proxy("127.0.0.1:17890", Some("<local>;localhost;127.*"))
            .expect("set proxy");
        assert!(set_res.enabled);
        assert_eq!(set_res.server, Some("127.0.0.1:17890".to_string()));

        let cleared = manager.clear_proxy().expect("clear proxy");
        assert!(!cleared.enabled);

        manager
            .restore_original()
            .expect("restore original baseline");
        let restored = SystemProxyManager::query_current().expect("query restored");
        assert_eq!(restored.enabled, initial.enabled);
    }
}

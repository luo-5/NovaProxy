//! Cross-platform System Proxy management:
//! - Windows: WinINet & HKCU Registry + baseline snapshot recovery
//! - macOS: `networksetup` Web / SecureWeb / SOCKS firewall proxy across active services
//! - Linux: GNOME `gsettings` (`org.gnome.system.proxy`) + KDE `kwriteconfig6`/`kwriteconfig5` (`kioslaverc`)
//! - Android: Managed via Android `VpnService` (`NovaProxyVpnService`)

use serde::{Deserialize, Serialize};
#[cfg(target_os = "windows")]
use std::ffi::c_void;
#[cfg(target_os = "windows")]
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

/// Splits `"host:port"` cleanly, supporting IPv6 bracket notation `"[::1]:7890"`.
#[cfg_attr(any(target_os = "windows", target_os = "android"), allow(dead_code))]
fn split_host_port(server: &str) -> (String, u16) {
    let trimmed = server.trim();
    if let Some(rest) = trimmed.strip_prefix('[') {
        if let Some((host, port_part)) = rest.split_once("]:") {
            let port = port_part.trim().parse::<u16>().unwrap_or(7890);
            return (host.to_string(), port);
        }
    }
    if let Some((host, port_part)) = trimmed.rsplit_once(':') {
        if let Ok(port) = port_part.trim().parse::<u16>() {
            return (host.trim().to_string(), port);
        }
    }
    (trimmed.to_string(), 7890)
}

#[cfg_attr(any(target_os = "windows", target_os = "android"), allow(dead_code))]
fn normalize_bypass_domains(bypass: &str) -> Vec<String> {
    bypass
        .split([';', ','])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("<local>"))
        .map(|s| match s {
            "127.*" => "127.0.0.0/8".to_string(),
            "10.*" => "10.0.0.0/8".to_string(),
            "192.168.*" => "192.168.0.0/16".to_string(),
            other => other.to_string(),
        })
        .collect()
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

#[cfg(target_os = "macos")]
mod macos_proxy {
    use super::*;
    use std::process::Command;

    pub fn list_active_services() -> Vec<String> {
        let Ok(out) = Command::new("networksetup")
            .arg("-listallnetworkservices")
            .output()
        else {
            return Vec::new();
        };
        if !out.status.success() {
            return Vec::new();
        }
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('*') && !l.starts_with("An asterisk"))
            .map(|l| l.to_string())
            .collect()
    }

    fn parse_networksetup_kv(raw: &str) -> (bool, Option<String>, Option<u16>) {
        let mut enabled = false;
        let mut server = None;
        let mut port = None;
        for line in raw.lines() {
            if let Some((k, v)) = line.split_once(':') {
                let key = k.trim().to_ascii_lowercase();
                let val = v.trim();
                match key.as_str() {
                    "enabled" => enabled = val.eq_ignore_ascii_case("yes"),
                    "server" if !val.is_empty() => server = Some(val.to_string()),
                    "port" => {
                        if let Ok(p) = val.parse::<u16>() {
                            if p > 0 {
                                port = Some(p);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        (enabled, server, port)
    }

    pub fn query() -> Result<SysProxySnapshot, SystemError> {
        let services = list_active_services();
        let Some(svc) = services.first() else {
            return Ok(SysProxySnapshot::default());
        };
        let web_out = Command::new("networksetup")
            .args(["-getwebproxy", svc])
            .output()?;
        let (web_en, web_srv, web_port) =
            parse_networksetup_kv(&String::from_utf8_lossy(&web_out.stdout));

        let socks_out = Command::new("networksetup")
            .args(["-getsocksfirewallproxy", svc])
            .output()?;
        let (socks_en, socks_srv, socks_port) =
            parse_networksetup_kv(&String::from_utf8_lossy(&socks_out.stdout));

        let enabled = web_en || socks_en;
        let server = match (web_srv, web_port, socks_srv, socks_port) {
            (Some(h), Some(p), _, _) if web_en => Some(format!("{h}:{p}")),
            (_, _, Some(h), Some(p)) if socks_en => Some(format!("{h}:{p}")),
            _ => None,
        };

        let bypass_out = Command::new("networksetup")
            .args(["-getproxybypassdomains", svc])
            .output()?;
        let bypass_raw = String::from_utf8_lossy(&bypass_out.stdout);
        let bypass_domains: Vec<&str> = bypass_raw
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.contains("There aren't any bypass domains"))
            .collect();
        let bypass = (!bypass_domains.is_empty()).then(|| bypass_domains.join(";"));

        Ok(SysProxySnapshot {
            enabled,
            server,
            bypass,
            auto_config_url: None,
        })
    }

    pub fn set(server: &str, bypass_str: &str) -> Result<(), SystemError> {
        let services = list_active_services();
        if services.is_empty() {
            return Err(SystemError::General(
                "No active macOS network services returned by networksetup".into(),
            ));
        }
        let (host, port) = split_host_port(server);
        let port_str = port.to_string();
        let bypass_list = normalize_bypass_domains(bypass_str);

        for svc in &services {
            let _ = Command::new("networksetup")
                .args(["-setwebproxy", svc, &host, &port_str])
                .status();
            let _ = Command::new("networksetup")
                .args(["-setsecurewebproxy", svc, &host, &port_str])
                .status();
            let _ = Command::new("networksetup")
                .args(["-setsocksfirewallproxy", svc, &host, &port_str])
                .status();
            let _ = Command::new("networksetup")
                .args(["-setwebproxystate", svc, "on"])
                .status();
            let _ = Command::new("networksetup")
                .args(["-setsecurewebproxystate", svc, "on"])
                .status();
            let _ = Command::new("networksetup")
                .args(["-setsocksfirewallproxystate", svc, "on"])
                .status();
            if !bypass_list.is_empty() {
                let mut byp_cmd = Command::new("networksetup");
                byp_cmd.arg("-setproxybypassdomains").arg(svc);
                for domain in &bypass_list {
                    byp_cmd.arg(domain);
                }
                let _ = byp_cmd.status();
            }
        }
        Ok(())
    }

    pub fn clear() -> Result<(), SystemError> {
        for svc in &list_active_services() {
            let _ = Command::new("networksetup")
                .args(["-setwebproxystate", svc, "off"])
                .status();
            let _ = Command::new("networksetup")
                .args(["-setsecurewebproxystate", svc, "off"])
                .status();
            let _ = Command::new("networksetup")
                .args(["-setsocksfirewallproxystate", svc, "off"])
                .status();
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
mod linux_proxy {
    use super::*;
    use std::process::Command;

    fn gsettings_get(schema: &str, key: &str) -> Option<String> {
        let out = Command::new("gsettings")
            .args(["get", schema, key])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        Some(
            String::from_utf8_lossy(&out.stdout)
                .trim()
                .trim_matches('\'')
                .to_string(),
        )
    }

    fn has_dbus_session() -> bool {
        std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some()
    }

    fn find_kwriteconfig() -> Option<&'static str> {
        ["kwriteconfig6", "kwriteconfig5"]
            .into_iter()
            .find(|bin| Command::new(bin).arg("--version").output().is_ok())
    }

    pub fn query() -> Result<SysProxySnapshot, SystemError> {
        if has_dbus_session() {
            if let Some(mode) = gsettings_get("org.gnome.system.proxy", "mode") {
                let enabled = mode.eq_ignore_ascii_case("manual");
                let server = if enabled {
                    let h = gsettings_get("org.gnome.system.proxy.http", "host")
                        .or_else(|| gsettings_get("org.gnome.system.proxy.socks", "host"));
                    let p = gsettings_get("org.gnome.system.proxy.http", "port")
                        .or_else(|| gsettings_get("org.gnome.system.proxy.socks", "port"));
                    match (h, p) {
                        (Some(host), Some(port)) if !host.is_empty() && port != "0" => {
                            Some(format!("{host}:{port}"))
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                return Ok(SysProxySnapshot {
                    enabled,
                    server,
                    bypass: None,
                    auto_config_url: None,
                });
            }
        }
        Ok(SysProxySnapshot::default())
    }

    pub fn set(server: &str, bypass_str: &str) -> Result<(), SystemError> {
        let (host, port) = split_host_port(server);
        let port_str = port.to_string();
        let bypass = normalize_bypass_domains(bypass_str);

        if has_dbus_session() {
            let formatted_ignore = format!(
                "[{}]",
                bypass
                    .iter()
                    .map(|s| format!("'{}'", s.replace('\'', "")))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let _ = Command::new("gsettings")
                .args(["set", "org.gnome.system.proxy", "mode", "manual"])
                .status();
            for proto in ["http", "https", "socks"] {
                let schema = format!("org.gnome.system.proxy.{proto}");
                let _ = Command::new("gsettings")
                    .args(["set", &schema, "host", &host])
                    .status();
                let _ = Command::new("gsettings")
                    .args(["set", &schema, "port", &port_str])
                    .status();
            }
            let _ = Command::new("gsettings")
                .args([
                    "set",
                    "org.gnome.system.proxy",
                    "ignore-hosts",
                    &formatted_ignore,
                ])
                .status();
        }

        if let Some(kw) = find_kwriteconfig() {
            let proxy_url = format!("http://{host}:{port}");
            let socks_url = format!("socks://{host}:{port}");
            let no_proxy = bypass.join(",");
            for args in [
                vec![
                    "--file",
                    "kioslaverc",
                    "--group",
                    "Proxy Settings",
                    "--key",
                    "ProxyType",
                    "1",
                ],
                vec![
                    "--file",
                    "kioslaverc",
                    "--group",
                    "Proxy Settings",
                    "--key",
                    "httpProxy",
                    &proxy_url,
                ],
                vec![
                    "--file",
                    "kioslaverc",
                    "--group",
                    "Proxy Settings",
                    "--key",
                    "httpsProxy",
                    &proxy_url,
                ],
                vec![
                    "--file",
                    "kioslaverc",
                    "--group",
                    "Proxy Settings",
                    "--key",
                    "socksProxy",
                    &socks_url,
                ],
                vec![
                    "--file",
                    "kioslaverc",
                    "--group",
                    "Proxy Settings",
                    "--key",
                    "NoProxyFor",
                    &no_proxy,
                ],
            ] {
                let _ = Command::new(kw).args(&args).status();
            }
            let _ = Command::new("dbus-send")
                .args([
                    "--type=signal",
                    "/KIO/Scheduler",
                    "org.kde.KIO.Scheduler.reparseSlaveConfiguration",
                    "string:",
                ])
                .status();
        }

        Ok(())
    }

    pub fn clear() -> Result<(), SystemError> {
        if has_dbus_session() {
            let _ = Command::new("gsettings")
                .args(["set", "org.gnome.system.proxy", "mode", "none"])
                .status();
        }
        if let Some(kw) = find_kwriteconfig() {
            let _ = Command::new(kw)
                .args([
                    "--file",
                    "kioslaverc",
                    "--group",
                    "Proxy Settings",
                    "--key",
                    "ProxyType",
                    "0",
                ])
                .status();
        }
        Ok(())
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

    pub fn is_active(&self) -> bool {
        self.is_active.load(Ordering::SeqCst)
    }

    /// Triggers OS proxy cache/notification refresh (WinINet on Windows, no-op elsewhere)
    pub fn refresh_wininet() -> Result<(), SystemError> {
        #[cfg(target_os = "windows")]
        {
            use win32::*;
            unsafe {
                let ok1 =
                    InternetSetOptionW(null_mut(), INTERNET_OPTION_SETTINGS_CHANGED, null_mut(), 0);
                let ok2 = InternetSetOptionW(null_mut(), INTERNET_OPTION_REFRESH, null_mut(), 0);
                if ok1 == 0 || ok2 == 0 {
                    return Err(SystemError::WinINet(GetLastError()));
                }
            }
        }
        Ok(())
    }

    /// Queries the current OS system proxy configuration
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
        #[cfg(target_os = "macos")]
        {
            macos_proxy::query()
        }
        #[cfg(target_os = "linux")]
        {
            linux_proxy::query()
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
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

                let ok1 =
                    InternetSetOptionW(null_mut(), INTERNET_OPTION_SETTINGS_CHANGED, null_mut(), 0);
                let ok2 = InternetSetOptionW(null_mut(), INTERNET_OPTION_REFRESH, null_mut(), 0);
                if ok1 == 0 || ok2 == 0 {
                    return Err(SystemError::WinINet(GetLastError()));
                }
            }
        }
        #[cfg(target_os = "macos")]
        {
            macos_proxy::set(server, bypass_str)?;
        }
        #[cfg(target_os = "linux")]
        {
            linux_proxy::set(server, bypass_str)?;
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            let _ = (server, bypass_str);
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

                let _ =
                    InternetSetOptionW(null_mut(), INTERNET_OPTION_SETTINGS_CHANGED, null_mut(), 0);
                let _ = InternetSetOptionW(null_mut(), INTERNET_OPTION_REFRESH, null_mut(), 0);
            }
        }
        #[cfg(target_os = "macos")]
        {
            macos_proxy::clear()?;
        }
        #[cfg(target_os = "linux")]
        {
            linux_proxy::clear()?;
        }

        self.is_active.store(false, Ordering::SeqCst);
        Self::query_current()
    }

    /// Restores the original baseline proxy settings saved before NovaProxy activated.
    /// If NovaProxy never modified the system proxy during this session, returns `Ok(())`
    /// without clobbering any pre-existing external proxy configuration.
    pub fn restore_original(&self) -> Result<(), SystemError> {
        let baseline_opt = self.baseline.lock().take();
        let Some(snap) = baseline_opt else {
            if self.is_active.load(Ordering::SeqCst) {
                let _ = self.clear_proxy();
            }
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

                    if let Some(ref srv) = snap.server {
                        let w_key = to_wide("ProxyServer");
                        let w_val = to_wide(srv);
                        RegSetValueExW(
                            hkey,
                            w_key.as_ptr(),
                            0,
                            REG_SZ,
                            w_val.as_ptr() as *const u8,
                            (w_val.len() * 2) as Dword,
                        );
                    }

                    if let Some(ref byp) = snap.bypass {
                        let w_key = to_wide("ProxyOverride");
                        let w_val = to_wide(byp);
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
        #[cfg(target_os = "macos")]
        {
            if snap.enabled {
                if let Some(ref srv) = snap.server {
                    let _ = macos_proxy::set(srv, snap.bypass.as_deref().unwrap_or(DEFAULT_BYPASS));
                }
            } else {
                let _ = macos_proxy::clear();
            }
        }
        #[cfg(target_os = "linux")]
        {
            if snap.enabled {
                if let Some(ref srv) = snap.server {
                    let _ = linux_proxy::set(srv, snap.bypass.as_deref().unwrap_or(DEFAULT_BYPASS));
                }
            } else {
                let _ = linux_proxy::clear();
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
    use std::sync::Mutex;

    static SYSPROXY_TEST_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_host_port_and_bypass_normalization() {
        assert_eq!(
            split_host_port("127.0.0.1:7890"),
            ("127.0.0.1".into(), 7890)
        );
        assert_eq!(split_host_port("[::1]:17890"), ("::1".into(), 17890));
        let domains = normalize_bypass_domains("<local>;localhost;127.*;192.168.*");
        assert!(domains.contains(&"localhost".to_string()));
        assert!(domains.contains(&"127.0.0.0/8".to_string()));
        assert!(!domains.contains(&"<local>".to_string()));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_system_proxy_query_and_cycle() {
        let _guard = SYSPROXY_TEST_MUTEX.lock().unwrap();
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

    #[cfg(target_os = "windows")]
    #[test]
    fn test_restore_original_noop_when_inactive_without_baseline() {
        let _guard = SYSPROXY_TEST_MUTEX.lock().unwrap();
        let initial = SystemProxyManager::query_current().expect("query initial system proxy");

        let external = SystemProxyManager::new();
        let _ = external
            .set_proxy("127.0.0.1:17891", None)
            .expect("set external");

        let passive_mgr = SystemProxyManager::new();
        assert!(!passive_mgr.is_active());
        passive_mgr
            .restore_original()
            .expect("passive restore_original should succeed");

        let after = SystemProxyManager::query_current().expect("query after passive restore");
        assert!(after.enabled);
        assert_eq!(after.server.as_deref(), Some("127.0.0.1:17891"));

        let _ = external.restore_original();
        let final_snap = SystemProxyManager::query_current().expect("query final");
        assert_eq!(final_snap.enabled, initial.enabled);
        assert_eq!(final_snap.server, initial.server);
    }
}

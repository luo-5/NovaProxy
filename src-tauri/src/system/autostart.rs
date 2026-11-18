//! Cross-platform autostart configuration:
//! - Windows: HKCU\Software\Microsoft\Windows\CurrentVersion\Run
//! - macOS: ~/Library/LaunchAgents/com.novaproxy.app.plist
//! - Linux: ~/.config/autostart/novaproxy.desktop
//! - Android: Managed by OS (returns false / unsupported)

use super::sysproxy::SystemError;
#[cfg(target_os = "windows")]
use std::ffi::c_void;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::path::PathBuf;
#[cfg(target_os = "windows")]
use std::ptr::null_mut;

#[cfg(target_os = "windows")]
const RUN_KEY_NAME: &str = "NovaProxy";

#[cfg(target_os = "windows")]
mod win32 {
    use super::*;

    pub type Hkey = *mut c_void;
    pub type Long = i32;
    pub type Dword = u32;

    pub const HKEY_CURRENT_USER: Hkey = 0x8000_0001_usize as Hkey;
    pub const KEY_READ: Dword = 0x20019;
    pub const KEY_SET_VALUE: Dword = 0x0002;
    pub const REG_SZ: Dword = 1;

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

        pub fn RegDeleteValueW(h_key: Hkey, lp_value_name: *const u16) -> Long;

        pub fn RegCloseKey(h_key: Hkey) -> Long;
    }

    pub fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

#[cfg(target_os = "macos")]
fn macos_plist_path() -> Result<PathBuf, SystemError> {
    let home = dirs::home_dir()
        .ok_or_else(|| SystemError::General("Cannot determine user home directory".into()))?;
    Ok(home
        .join("Library")
        .join("LaunchAgents")
        .join("com.novaproxy.app.plist"))
}

#[cfg(target_os = "linux")]
fn linux_desktop_entry_path() -> Result<PathBuf, SystemError> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| SystemError::General("Cannot determine XDG config directory".into()))?;
    Ok(config_dir.join("autostart").join("novaproxy.desktop"))
}

/// Checks whether autostart is currently enabled for NovaProxy
pub fn is_autostart_enabled() -> Result<bool, SystemError> {
    #[cfg(target_os = "windows")]
    {
        use win32::*;
        let subkey = to_wide(r"Software\Microsoft\Windows\CurrentVersion\Run");
        let mut hkey: Hkey = null_mut();

        unsafe {
            let status = RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut hkey);
            if status != 0 {
                return Ok(false);
            }

            let name_wide = to_wide(RUN_KEY_NAME);
            let mut val_type: Dword = 0;
            let mut data_len: Dword = 0;

            let ret = RegQueryValueExW(
                hkey,
                name_wide.as_ptr(),
                null_mut(),
                &mut val_type,
                null_mut(),
                &mut data_len,
            );

            RegCloseKey(hkey);

            Ok(ret == 0 && val_type == REG_SZ && data_len > 0)
        }
    }
    #[cfg(target_os = "macos")]
    {
        Ok(macos_plist_path()?.exists())
    }
    #[cfg(target_os = "linux")]
    {
        let path = linux_desktop_entry_path()?;
        if !path.exists() {
            return Ok(false);
        }
        let raw = std::fs::read_to_string(&path).unwrap_or_default();
        Ok(!raw.contains("X-GNOME-Autostart-enabled=false") && !raw.contains("Hidden=true"))
    }
    #[cfg(target_os = "android")]
    {
        Ok(false)
    }
    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "linux",
        target_os = "android"
    )))]
    {
        Ok(false)
    }
}

/// Enables or disables autostart on user login
pub fn set_autostart(enabled: bool) -> Result<bool, SystemError> {
    #[cfg(target_os = "windows")]
    {
        use win32::*;
        let subkey = to_wide(r"Software\Microsoft\Windows\CurrentVersion\Run");
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
                return Err(SystemError::Registry("RegOpenKeyExW".into(), status as u32));
            }

            let name_wide = to_wide(RUN_KEY_NAME);

            if enabled {
                let current_exe = std::env::current_exe()?;
                let command_line = format!("\"{}\" --minimized", current_exe.to_string_lossy());
                let val_wide = to_wide(&command_line);

                let set_res = RegSetValueExW(
                    hkey,
                    name_wide.as_ptr(),
                    0,
                    REG_SZ,
                    val_wide.as_ptr() as *const u8,
                    (val_wide.len() * 2) as Dword,
                );
                RegCloseKey(hkey);

                if set_res != 0 {
                    return Err(SystemError::Registry(
                        "RegSetValueExW".into(),
                        set_res as u32,
                    ));
                }
            } else {
                let _ = RegDeleteValueW(hkey, name_wide.as_ptr());
                RegCloseKey(hkey);
            }
        }
        Ok(enabled)
    }
    #[cfg(target_os = "macos")]
    {
        let plist_path = macos_plist_path()?;
        if enabled {
            if let Some(parent) = plist_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let current_exe = std::env::current_exe()?;
            let exe_escaped = current_exe
                .to_string_lossy()
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            let plist = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.novaproxy.app</string>
  <key>ProgramArguments</key>
  <array>
    <string>{exe_escaped}</string>
    <string>--minimized</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
</dict>
</plist>
"#
            );
            std::fs::write(&plist_path, plist)?;
        } else if plist_path.exists() {
            std::fs::remove_file(&plist_path)?;
        }
        Ok(enabled)
    }
    #[cfg(target_os = "linux")]
    {
        let entry_path = linux_desktop_entry_path()?;
        if enabled {
            if let Some(parent) = entry_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let current_exe = std::env::current_exe()?;
            let desktop = format!(
                "[Desktop Entry]\n\
                 Type=Application\n\
                 Version=1.0\n\
                 Name=NovaProxy\n\
                 Comment=High-performance multi-core VPN client\n\
                 Exec=\"{}\" --minimized\n\
                 Terminal=false\n\
                 StartupNotify=false\n\
                 X-GNOME-Autostart-enabled=true\n",
                current_exe.to_string_lossy()
            );
            std::fs::write(&entry_path, desktop)?;
        } else if entry_path.exists() {
            std::fs::remove_file(&entry_path)?;
        }
        Ok(enabled)
    }
    #[cfg(target_os = "android")]
    {
        if enabled {
            Err(SystemError::General(
                "Autostart on Android is managed by system VPN Always-On settings".into(),
            ))
        } else {
            Ok(false)
        }
    }
    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "linux",
        target_os = "android"
    )))]
    {
        Ok(enabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_autostart_toggle() {
        let initial = is_autostart_enabled().unwrap();

        set_autostart(true).unwrap();
        assert!(is_autostart_enabled().unwrap());

        set_autostart(false).unwrap();
        assert!(!is_autostart_enabled().unwrap());

        set_autostart(initial).unwrap();
    }
}

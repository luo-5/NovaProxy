//! Windows autostart configuration via Registry Run key

use super::sysproxy::SystemError;
use std::ffi::c_void;
use std::ptr::null_mut;

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
    #[cfg(not(target_os = "windows"))]
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
    #[cfg(not(target_os = "windows"))]
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

        // Enable
        set_autostart(true).unwrap();
        assert!(is_autostart_enabled().unwrap());

        // Disable
        set_autostart(false).unwrap();
        assert!(!is_autostart_enabled().unwrap());

        // Restore initial state
        set_autostart(initial).unwrap();
    }
}

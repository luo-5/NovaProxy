//! Cross-platform process privilege inspection & admin elevation relaunch:
//! - Windows: TokenElevation + ShellExecuteExW("runas")
//! - macOS: geteuid() == 0 + osascript administrator prompt
//! - Linux: geteuid() == 0 or CAP_NET_ADMIN + pkexec GUI relaunch
//! - Android: No root required (VpnService uses OS VPN consent dialog)

use super::sysproxy::SystemError;
#[cfg(target_os = "windows")]
use std::ffi::c_void;
#[cfg(target_os = "windows")]
use std::ptr::{null, null_mut};

#[cfg(target_os = "windows")]
mod win32 {
    use super::*;

    pub type Handle = *mut c_void;
    pub type Dword = u32;
    pub type Bool = i32;

    pub const TOKEN_QUERY: Dword = 0x0008;
    pub const TOKEN_ELEVATION: i32 = 20;
    pub const SEE_MASK_NOCLOSEPROCESS: Dword = 0x00000040;

    #[repr(C)]
    #[derive(Default)]
    pub struct TokenElevation {
        pub token_is_elevated: Dword,
    }

    #[repr(C)]
    pub struct ShellExecuteInfoW {
        pub cb_size: Dword,
        pub f_mask: Dword,
        pub hwnd: Handle,
        pub lp_verb: *const u16,
        pub lp_file: *const u16,
        pub lp_parameters: *const u16,
        pub lp_directory: *const u16,
        pub n_show: i32,
        pub h_inst_app: Handle,
        pub lp_id_list: *mut c_void,
        pub lp_class: *const u16,
        pub hkey_class: Handle,
        pub dw_hot_key: Dword,
        pub h_icon_or_monitor: Handle,
        pub h_process: Handle,
    }

    #[link(name = "advapi32")]
    extern "system" {
        pub fn OpenProcessToken(
            process_handle: Handle,
            desired_access: Dword,
            token_handle: *mut Handle,
        ) -> Bool;

        pub fn GetTokenInformation(
            token_handle: Handle,
            token_information_class: i32,
            token_information: *mut c_void,
            token_information_length: Dword,
            return_length: *mut Dword,
        ) -> Bool;
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub fn GetCurrentProcess() -> Handle;
        pub fn CloseHandle(handle: Handle) -> Bool;
        pub fn GetLastError() -> Dword;
    }

    #[link(name = "shell32")]
    extern "system" {
        pub fn ShellExecuteExW(p_exec_info: *mut ShellExecuteInfoW) -> Bool;
    }

    pub fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

#[cfg(target_os = "linux")]
pub fn linux_has_cap_net_admin() -> bool {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return false;
    };
    for line in status.lines() {
        if let Some(hex_val) = line.strip_prefix("CapEff:") {
            if let Ok(caps) = u64::from_str_radix(hex_val.trim(), 16) {
                // CAP_NET_ADMIN is bit 12
                return (caps & (1u64 << 12)) != 0;
            }
        }
    }
    false
}

/// Checks whether the current process has OS privileges needed for TUN virtual adapters
pub fn is_elevated() -> bool {
    #[cfg(target_os = "windows")]
    {
        use win32::*;
        unsafe {
            let mut token: *mut c_void = null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return false;
            }

            let mut elevation = TokenElevation::default();
            let mut ret_len: u32 = 0;
            let success = GetTokenInformation(
                token,
                TOKEN_ELEVATION,
                &mut elevation as *mut _ as *mut c_void,
                std::mem::size_of::<TokenElevation>() as u32,
                &mut ret_len,
            );

            CloseHandle(token);

            success != 0 && elevation.token_is_elevated != 0
        }
    }
    #[cfg(target_os = "macos")]
    {
        unsafe { libc::geteuid() == 0 }
    }
    #[cfg(target_os = "linux")]
    {
        unsafe { libc::geteuid() == 0 }
        || linux_has_cap_net_admin()
    }
    #[cfg(target_os = "android")]
    {
        // On Android, VpnService grants user-consented TUN access without root
        true
    }
    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "linux",
        target_os = "android"
    )))]
    {
        false
    }
}

/// Relaunches the current executable with Administrator / root elevation prompt
pub fn relaunch_as_admin(args: &[&str]) -> Result<(), SystemError> {
    #[cfg(target_os = "windows")]
    {
        use win32::*;

        let current_exe = std::env::current_exe()?;
        let exe_wide = to_wide(&current_exe.to_string_lossy());
        let verb_wide = to_wide("runas");

        let args_combined = args.join(" ");
        let params_wide = to_wide(&args_combined);

        unsafe {
            let mut info = ShellExecuteInfoW {
                cb_size: std::mem::size_of::<ShellExecuteInfoW>() as u32,
                f_mask: SEE_MASK_NOCLOSEPROCESS,
                hwnd: null_mut(),
                lp_verb: verb_wide.as_ptr(),
                lp_file: exe_wide.as_ptr(),
                lp_parameters: if args.is_empty() {
                    null()
                } else {
                    params_wide.as_ptr()
                },
                lp_directory: null(),
                n_show: 1, // SW_SHOWNORMAL
                h_inst_app: null_mut(),
                lp_id_list: null_mut(),
                lp_class: null(),
                hkey_class: null_mut(),
                dw_hot_key: 0,
                h_icon_or_monitor: null_mut(),
                h_process: null_mut(),
            };

            let ok = ShellExecuteExW(&mut info);
            if ok == 0 {
                return Err(SystemError::General(format!(
                    "ShellExecuteExW(runas) failed with code {}",
                    GetLastError()
                )));
            }

            if !info.h_process.is_null() {
                CloseHandle(info.h_process);
            }
        }
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        let exe = std::env::current_exe()?;
        let joined_args = args
            .iter()
            .map(|a| format!("'{}'", a.replace('\'', "'\\''")))
            .collect::<Vec<_>>()
            .join(" ");
        let shell_cmd = format!(
            "'{}' {} >/dev/null 2>&1 &",
            exe.to_string_lossy().replace('\'', "'\\''"),
            joined_args
        );
        let script = format!(
            "do shell script \"{}\" with administrator privileges",
            shell_cmd.replace('\\', "\\\\").replace('"', "\\\"")
        );
        let status = std::process::Command::new("osascript")
            .args(["-e", &script])
            .status()?;
        if !status.success() {
            return Err(SystemError::General(
                "macOS administrator authorization cancelled or failed".into(),
            ));
        }
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        let exe = std::env::current_exe()?;
        let mut cmd = std::process::Command::new("pkexec");
        cmd.arg("env");
        for var in [
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "XAUTHORITY",
            "XDG_RUNTIME_DIR",
            "DBUS_SESSION_BUS_ADDRESS",
        ] {
            if let Ok(val) = std::env::var(var) {
                cmd.arg(format!("{var}={val}"));
            }
        }
        cmd.arg(exe);
        cmd.args(args);
        cmd.spawn()?;
        Ok(())
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = args;
        Err(SystemError::General(
            "Privilege relaunch is not applicable on this OS".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_elevated_does_not_panic() {
        let elevated = is_elevated();
        println!("Running with TUN/admin privileges: {}", elevated);
    }
}

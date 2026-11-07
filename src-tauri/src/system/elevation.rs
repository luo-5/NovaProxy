//! Windows process elevation checks and UAC relaunch

use super::sysproxy::SystemError;
use std::ffi::c_void;
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

/// Checks whether the current process is running with Administrator / elevated token
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
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// Relaunches the current executable with Administrator elevation prompt (UAC)
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
    #[cfg(not(target_os = "windows"))]
    {
        Err(SystemError::General(
            "Elevation not supported on this OS".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_elevated_does_not_panic() {
        let elevated = is_elevated();
        println!("Running as elevated/admin: {}", elevated);
    }
}

//! Deep link protocol registration and parsing (`novaproxy://` and `clash://`)

use super::sysproxy::SystemError;
use std::ptr::null_mut;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepLinkAction {
    pub subscription_url: String,
    pub name: Option<String>,
}

/// Parses a deep link URL (e.g. `novaproxy://install-config?url=...` or `clash://install-config?url=...`)
pub fn parse_deep_link(uri: &str) -> Option<DeepLinkAction> {
    let trimmed = uri.trim();
    if !trimmed.starts_with("novaproxy://") && !trimmed.starts_with("clash://") {
        return None;
    }

    let parsed = Url::parse(trimmed).ok()?;
    let mut sub_url = None;
    let mut name = None;

    for (k, v) in parsed.query_pairs() {
        if k == "url" {
            sub_url = Some(v.into_owned());
        } else if k == "name" {
            name = Some(v.into_owned());
        }
    }

    sub_url.map(|u| DeepLinkAction {
        subscription_url: u,
        name,
    })
}

/// Registers `novaproxy://` and `clash://` custom URL protocols in the Windows registry for the current user
pub fn register_url_schemes() -> Result<(), SystemError> {
    #[cfg(target_os = "windows")]
    {
        let current_exe = std::env::current_exe()?;
        let cmd = format!("\"{}\" \"%1\"", current_exe.to_string_lossy());

        register_single_scheme("novaproxy", "NovaProxy Protocol", &cmd)?;
        register_single_scheme("clash", "Clash Protocol", &cmd)?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn register_single_scheme(
    scheme: &str,
    description: &str,
    open_cmd: &str,
) -> Result<(), SystemError> {
    use super::sysproxy::win32::*;

    unsafe {
        let base_path = format!(r"Software\Classes\{}", scheme);
        let base_wide = to_wide(&base_path);
        let mut hkey: Hkey = null_mut();

        let status = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            base_wide.as_ptr(),
            0,
            KEY_SET_VALUE | KEY_READ,
            &mut hkey,
        );

        let key_to_use = if status != 0 {
            // Create subkey
            let mut created: Hkey = null_mut();
            #[link(name = "advapi32")]
            extern "system" {
                fn RegCreateKeyW(hKey: Hkey, lpSubKey: *const u16, phkResult: *mut Hkey) -> i32;
            }
            if RegCreateKeyW(HKEY_CURRENT_USER, base_wide.as_ptr(), &mut created) != 0 {
                return Ok(());
            }
            created
        } else {
            hkey
        };

        // Set default value = description
        let desc_wide = to_wide(description);
        RegSetValueExW(
            key_to_use,
            null_mut(),
            0,
            REG_SZ,
            desc_wide.as_ptr() as *const u8,
            (desc_wide.len() * 2) as Dword,
        );

        // Set "URL Protocol" = ""
        let proto_wide = to_wide("URL Protocol");
        let empty_wide = to_wide("");
        RegSetValueExW(
            key_to_use,
            proto_wide.as_ptr(),
            0,
            REG_SZ,
            empty_wide.as_ptr() as *const u8,
            (empty_wide.len() * 2) as Dword,
        );

        RegCloseKey(key_to_use);

        // Now set shell\open\command
        let cmd_path = format!(r"Software\Classes\{}\shell\open\command", scheme);
        let cmd_wide = to_wide(&cmd_path);
        let mut cmd_key: Hkey = null_mut();
        #[link(name = "advapi32")]
        extern "system" {
            fn RegCreateKeyW(hKey: Hkey, lpSubKey: *const u16, phkResult: *mut Hkey) -> i32;
        }
        if RegCreateKeyW(HKEY_CURRENT_USER, cmd_wide.as_ptr(), &mut cmd_key) == 0 {
            let cmd_val_wide = to_wide(open_cmd);
            RegSetValueExW(
                cmd_key,
                null_mut(),
                0,
                REG_SZ,
                cmd_val_wide.as_ptr() as *const u8,
                (cmd_val_wide.len() * 2) as Dword,
            );
            RegCloseKey(cmd_key);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_deep_link() {
        let uri1 = "novaproxy://install-config?url=https%3A%2F%2Fexample.com%2Fsub&name=MySub";
        let action1 = parse_deep_link(uri1).expect("parse novaproxy uri");
        assert_eq!(action1.subscription_url, "https://example.com/sub");
        assert_eq!(action1.name, Some("MySub".to_string()));

        let uri2 = "clash://install-config?url=https%3A%2F%2Fexample.com%2Fclash";
        let action2 = parse_deep_link(uri2).expect("parse clash uri");
        assert_eq!(action2.subscription_url, "https://example.com/clash");
        assert_eq!(action2.name, None);

        assert!(parse_deep_link("https://google.com").is_none());
    }
}

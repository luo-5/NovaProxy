//! Cross-platform deep link URL scheme registration and single-instance IPC forwarding:
//! - Windows: HKCU\Software\Classes + Win32 Named Mutex + localhost IPC
//! - Linux: ~/.local/share/applications/novaproxy.desktop (`xdg-mime`) + Unix `flock` + localhost IPC
//! - macOS: Info.plist `CFBundleURLTypes` / `RunEvent::Opened` + Unix `flock` + localhost IPC
//! - Android: AndroidManifest.xml `<intent-filter>` + MainActivity intent forwarding

use super::sysproxy::SystemError;
use serde::{Deserialize, Serialize};
#[cfg(all(unix, not(target_os = "android")))]
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
#[cfg(all(unix, not(target_os = "android")))]
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use ts_rs::TS;
use url::Url;

const FOCUS_COMMAND: &str = "__NOVAPROXY_FOCUS__";
pub const INSTANCE_IPC_FILENAME: &str = "instance_ipc.json";
#[cfg(target_os = "windows")]
const INSTANCE_MUTEX_NAME: &str = r"Local\NovaProxy_SingleInstance_Mutex_v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeepLinkAction {
    pub subscription_url: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UrlSchemeStatus {
    pub novaproxy_registered: bool,
    pub clash_registered_to_us: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InstanceIpcEndpoint {
    pid: u32,
    port: u16,
    token: String,
}

pub enum SingleInstanceOutcome {
    Primary(SingleInstanceGuard),
    ForwardedToExisting,
}

/// RAII guard for the primary single-instance lock and local IPC listener.
pub struct SingleInstanceGuard {
    #[cfg(target_os = "windows")]
    mutex_handle: isize,
    #[cfg(all(unix, not(target_os = "android")))]
    lock_file: Option<File>,
    #[cfg(all(unix, not(target_os = "android")))]
    lock_path: PathBuf,
    endpoint_file: PathBuf,
    running: Arc<AtomicBool>,
    listen_port: u16,
}

impl SingleInstanceGuard {
    pub fn listen_port(&self) -> u16 {
        self.listen_port
    }

    pub fn shutdown(&self) {
        self.running.store(false, Ordering::SeqCst);
        let _ = TcpStream::connect_timeout(
            &SocketAddr::from(([127, 0, 0, 1], self.listen_port)),
            Duration::from_millis(80),
        );
        let _ = std::fs::remove_file(&self.endpoint_file);
    }
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        self.shutdown();

        #[cfg(target_os = "windows")]
        if self.mutex_handle != 0 {
            unsafe {
                CloseHandle(self.mutex_handle as *mut std::ffi::c_void);
            }
        }

        #[cfg(all(unix, not(target_os = "android")))]
        {
            if let Some(f) = self.lock_file.take() {
                unsafe {
                    libc::flock(f.as_raw_fd(), libc::LOCK_UN);
                }
            }
            let _ = std::fs::remove_file(&self.lock_path);
        }
    }
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
extern "system" {
    fn CreateMutexW(
        lp_mutex_attributes: *mut std::ffi::c_void,
        b_initial_owner: i32,
        lp_name: *const u16,
    ) -> *mut std::ffi::c_void;
    fn GetLastError() -> u32;
    fn CloseHandle(h_object: *mut std::ffi::c_void) -> i32;
}

/// Parses a deep link URL (e.g. `novaproxy://install-config?url=...` or `clash://install-config?url=...`).
pub fn parse_deep_link(uri: &str) -> Option<DeepLinkAction> {
    let trimmed = uri.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("novaproxy://") && !lower.starts_with("clash://") {
        return None;
    }

    let parsed = Url::parse(trimmed).ok()?;
    let mut sub_url = None;
    let mut name = None;

    for (k, v) in parsed.query_pairs() {
        if k == "url" {
            let val = v.into_owned();
            if !val.trim().is_empty() {
                sub_url = Some(val);
            }
        } else if k == "name" {
            let val = v.into_owned();
            if !val.trim().is_empty() {
                name = Some(val);
            }
        }
    }

    sub_url.map(|u| DeepLinkAction {
        subscription_url: u,
        name,
    })
}

/// Extracts the first deep-link CLI argument (`novaproxy://...` or `clash://...`).
pub fn extract_deep_link_from_args<I, S>(args: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    for arg in args.into_iter().skip(1) {
        let trimmed = arg.as_ref().trim();
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("novaproxy://") || lower.starts_with("clash://") {
            return Some(trimmed.to_string());
        }
    }
    None
}

/// Starts a token-authenticated localhost IPC listener for single-instance focus & deep-link delivery.
pub fn start_ipc_listener<F>(
    endpoint_file: &Path,
    running: Arc<AtomicBool>,
    handler: F,
) -> Result<u16, SystemError>
where
    F: Fn(Option<String>) + Send + Sync + 'static,
{
    if let Some(parent) = endpoint_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let listener = TcpListener::bind("127.0.0.1:0").map_err(SystemError::Io)?;
    let port = listener.local_addr().map_err(SystemError::Io)?.port();
    let token = uuid::Uuid::new_v4().to_string();

    let endpoint = InstanceIpcEndpoint {
        pid: std::process::id(),
        port,
        token: token.clone(),
    };
    let json = serde_json::to_string(&endpoint)
        .map_err(|e| SystemError::General(format!("IPC serialize failed: {e}")))?;
    std::fs::write(endpoint_file, json)?;

    std::thread::Builder::new()
        .name("novaproxy-instance-ipc".to_string())
        .spawn(move || {
            for stream in listener.incoming() {
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(mut stream) = stream else {
                    continue;
                };
                let _ = stream.set_read_timeout(Some(Duration::from_millis(800)));
                let mut line = String::new();
                let read_ok = {
                    let mut reader = BufReader::new(&stream);
                    reader.read_line(&mut line).is_ok()
                };
                if read_ok {
                    let trimmed = line.trim_end_matches(['\r', '\n']);
                    if let Some((recv_token, payload)) = trimmed.split_once('\t') {
                        if recv_token == token {
                            let _ = stream.write_all(b"OK\n");
                            let msg = if payload == FOCUS_COMMAND || payload.is_empty() {
                                None
                            } else {
                                Some(payload.to_string())
                            };
                            handler(msg);
                        }
                    }
                }
            }
        })
        .map_err(SystemError::Io)?;

    Ok(port)
}

/// Attempts to forward a deep-link URI (or window focus signal) to an existing IPC endpoint.
pub fn forward_to_ipc_endpoint(endpoint_file: &Path, deep_link: Option<&str>) -> bool {
    let Ok(bytes) = std::fs::read(endpoint_file) else {
        return false;
    };
    let Ok(info) = serde_json::from_slice::<InstanceIpcEndpoint>(&bytes) else {
        return false;
    };
    let addr = SocketAddr::from(([127, 0, 0, 1], info.port));
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(600)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(800)));
    let body = deep_link.unwrap_or(FOCUS_COMMAND);
    let msg = format!("{}\t{}\n", info.token, body);
    if stream.write_all(msg.as_bytes()).is_err() {
        return false;
    }
    let mut reader = BufReader::new(stream);
    let mut ack = String::new();
    reader.read_line(&mut ack).is_ok() && ack.trim() == "OK"
}

/// Acquires a desktop single-instance lock or forwards the invocation to the live primary instance.
pub fn acquire_single_instance_or_forward<F>(
    app_data_dir: &Path,
    deep_link: Option<&str>,
    on_message: F,
) -> Result<SingleInstanceOutcome, SystemError>
where
    F: Fn(Option<String>) + Send + Sync + 'static,
{
    let endpoint_file = app_data_dir.join(INSTANCE_IPC_FILENAME);
    let _ = deep_link;

    #[cfg(target_os = "windows")]
    let mutex_handle: isize = {
        use super::sysproxy::win32::to_wide;
        const ERROR_ALREADY_EXISTS: u32 = 183;

        let wide_name = to_wide(INSTANCE_MUTEX_NAME);
        let handle = unsafe { CreateMutexW(null_mut(), 0, wide_name.as_ptr()) };
        if !handle.is_null() {
            let err = unsafe { GetLastError() };
            if err == ERROR_ALREADY_EXISTS {
                let forwarded = forward_to_ipc_endpoint(&endpoint_file, deep_link);
                unsafe {
                    CloseHandle(handle);
                }
                if forwarded {
                    return Ok(SingleInstanceOutcome::ForwardedToExisting);
                }
                return Ok(SingleInstanceOutcome::ForwardedToExisting);
            }
        }
        handle as isize
    };

    #[cfg(all(unix, not(target_os = "android")))]
    let (lock_file, lock_path) = {
        std::fs::create_dir_all(app_data_dir)?;
        let lp = app_data_dir.join("novaproxy.lock");
        let f = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lp)?;
        let rc = unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if rc != 0 {
            let _ = forward_to_ipc_endpoint(&endpoint_file, deep_link);
            return Ok(SingleInstanceOutcome::ForwardedToExisting);
        }
        (Some(f), lp)
    };

    let running = Arc::new(AtomicBool::new(true));
    let listen_port = start_ipc_listener(&endpoint_file, running.clone(), on_message)?;

    Ok(SingleInstanceOutcome::Primary(SingleInstanceGuard {
        #[cfg(target_os = "windows")]
        mutex_handle,
        #[cfg(all(unix, not(target_os = "android")))]
        lock_file,
        #[cfg(all(unix, not(target_os = "android")))]
        lock_path,
        endpoint_file,
        running,
        listen_port,
    }))
}

#[cfg(target_os = "linux")]
fn linux_desktop_scheme_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("applications").join("novaproxy.desktop"))
}

#[cfg(target_os = "linux")]
fn sync_linux_desktop_entry(include_clash: bool) -> Result<(), SystemError> {
    let Some(path) = linux_desktop_scheme_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let exe = std::env::current_exe()?;
    let mimes = if include_clash {
        "x-scheme-handler/novaproxy;x-scheme-handler/clash;"
    } else {
        "x-scheme-handler/novaproxy;"
    };
    let content = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Version=1.0\n\
         Name=NovaProxy\n\
         Comment=High-performance multi-core VPN client\n\
         Exec=\"{}\" %u\n\
         Terminal=false\n\
         Categories=Network;\n\
         MimeType={}\n",
        exe.to_string_lossy(),
        mimes
    );
    std::fs::write(&path, content)?;
    let _ = std::process::Command::new("xdg-mime")
        .args(["default", "novaproxy.desktop", "x-scheme-handler/novaproxy"])
        .status();
    if include_clash {
        let _ = std::process::Command::new("xdg-mime")
            .args(["default", "novaproxy.desktop", "x-scheme-handler/clash"])
            .status();
    }
    Ok(())
}

/// Registers `novaproxy://` always and optionally registers opt-in `clash://`.
pub fn register_url_schemes_with_options(register_clash: bool) -> Result<(), SystemError> {
    #[cfg(target_os = "windows")]
    {
        let current_exe = std::env::current_exe()?;
        let cmd = format!("\"{}\" \"%1\"", current_exe.to_string_lossy());

        register_single_scheme("novaproxy", "NovaProxy Protocol", &cmd)?;
        if register_clash {
            register_single_scheme("clash", "Clash Protocol (NovaProxy)", &cmd)?;
        }
    }
    #[cfg(target_os = "linux")]
    {
        sync_linux_desktop_entry(register_clash)?;
    }
    let _ = register_clash;
    Ok(())
}

/// Registers both `novaproxy://` and `clash://` custom URL protocols for current user.
pub fn register_url_schemes() -> Result<(), SystemError> {
    register_url_schemes_with_options(true)
}

/// Enables or disables `clash://` protocol registration for NovaProxy cleanly without clobbering
/// third-party Clash clients when disabling if not owned by NovaProxy.
pub fn set_clash_url_scheme(enabled: bool) -> Result<UrlSchemeStatus, SystemError> {
    #[cfg(target_os = "windows")]
    {
        let current_exe = std::env::current_exe()?;
        let cmd = format!("\"{}\" \"%1\"", current_exe.to_string_lossy());
        if enabled {
            register_single_scheme("clash", "Clash Protocol (NovaProxy)", &cmd)?;
        } else if is_scheme_owned_by_us("clash") {
            unregister_single_scheme("clash")?;
        }
    }
    #[cfg(target_os = "linux")]
    {
        sync_linux_desktop_entry(enabled)?;
    }
    let _ = enabled;
    get_url_scheme_status()
}

/// Inspects current custom protocol registration status.
pub fn get_url_scheme_status() -> Result<UrlSchemeStatus, SystemError> {
    #[cfg(target_os = "windows")]
    {
        Ok(UrlSchemeStatus {
            novaproxy_registered: is_scheme_owned_by_us("novaproxy"),
            clash_registered_to_us: is_scheme_owned_by_us("clash"),
        })
    }
    #[cfg(target_os = "linux")]
    {
        let content = linux_desktop_scheme_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        Ok(UrlSchemeStatus {
            novaproxy_registered: content.contains("x-scheme-handler/novaproxy"),
            clash_registered_to_us: content.contains("x-scheme-handler/clash"),
        })
    }
    #[cfg(any(target_os = "macos", target_os = "android"))]
    {
        Ok(UrlSchemeStatus {
            novaproxy_registered: true,
            clash_registered_to_us: true,
        })
    }
    #[cfg(not(any(
        target_os = "windows",
        target_os = "linux",
        target_os = "macos",
        target_os = "android"
    )))]
    {
        Ok(UrlSchemeStatus::default())
    }
}

#[cfg(target_os = "windows")]
fn read_scheme_command(scheme: &str) -> Option<String> {
    use super::sysproxy::win32::*;

    let subkey = format!(r"Software\Classes\{}\shell\open\command", scheme);
    let subkey_wide = to_wide(&subkey);
    let mut hkey: Hkey = null_mut();

    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            subkey_wide.as_ptr(),
            0,
            KEY_READ,
            &mut hkey,
        )
    };
    if status != 0 || hkey.is_null() {
        return None;
    }

    let empty = [0u16];
    let mut val_type: Dword = 0;
    let mut data_len: Dword = 0;
    let q1 = unsafe {
        RegQueryValueExW(
            hkey,
            empty.as_ptr(),
            null_mut(),
            &mut val_type,
            null_mut(),
            &mut data_len,
        )
    };
    if q1 != 0 || data_len < 2 {
        unsafe {
            RegCloseKey(hkey);
        }
        return None;
    }

    let mut buf: Vec<u16> = vec![0u16; (data_len as usize).div_ceil(2)];
    let q2 = unsafe {
        RegQueryValueExW(
            hkey,
            empty.as_ptr(),
            null_mut(),
            &mut val_type,
            buf.as_mut_ptr() as *mut u8,
            &mut data_len,
        )
    };
    unsafe {
        RegCloseKey(hkey);
    }
    if q2 != 0 {
        return None;
    }

    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..end]))
}

#[cfg(target_os = "windows")]
fn is_scheme_owned_by_us(scheme: &str) -> bool {
    let Some(cmd) = read_scheme_command(scheme) else {
        return false;
    };
    let lower_cmd = cmd.to_ascii_lowercase();
    if let Ok(exe) = std::env::current_exe() {
        let exe_str = exe.to_string_lossy().to_ascii_lowercase();
        if lower_cmd.contains(&exe_str) {
            return true;
        }
    }
    lower_cmd.contains("novaproxy")
}

#[cfg(target_os = "windows")]
fn unregister_single_scheme(scheme: &str) -> Result<(), SystemError> {
    use super::sysproxy::win32::*;
    #[link(name = "advapi32")]
    extern "system" {
        fn RegDeleteTreeW(hKey: Hkey, lpSubKey: *const u16) -> Long;
    }
    let classes_wide = to_wide(r"Software\Classes");
    let scheme_wide = to_wide(scheme);
    let mut classes_key: Hkey = null_mut();
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            classes_wide.as_ptr(),
            0,
            KEY_SET_VALUE | KEY_READ,
            &mut classes_key,
        )
    };
    if status == 0 && !classes_key.is_null() {
        unsafe {
            let _ = RegDeleteTreeW(classes_key, scheme_wide.as_ptr());
            RegCloseKey(classes_key);
        }
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

        let desc_wide = to_wide(description);
        RegSetValueExW(
            key_to_use,
            null_mut(),
            0,
            REG_SZ,
            desc_wide.as_ptr() as *const u8,
            (desc_wide.len() * 2) as Dword,
        );

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

        let cmd_path = format!(r"Software\Classes\{}\shell\open\command", scheme);
        let cmd_wide = to_wide(&cmd_path);
        let mut cmd_key: Hkey = null_mut();

        #[link(name = "advapi32")]
        extern "system" {
            fn RegCreateKeyW(hKey: Hkey, lpSubKey: *const u16, phkResult: *mut Hkey) -> i32;
        }
        if RegCreateKeyW(HKEY_CURRENT_USER, cmd_wide.as_ptr(), &mut cmd_key) == 0 {
            let val_wide = to_wide(open_cmd);
            RegSetValueExW(
                cmd_key,
                null_mut(),
                0,
                REG_SZ,
                val_wide.as_ptr() as *const u8,
                (val_wide.len() * 2) as Dword,
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
        let a = parse_deep_link(
            "novaproxy://install-config?url=https%3A%2F%2Fexample.com%2Fsub&name=TestSub",
        )
        .expect("parse novaproxy link");
        assert_eq!(a.subscription_url, "https://example.com/sub");
        assert_eq!(a.name.as_deref(), Some("TestSub"));

        let b = parse_deep_link("clash://install-config?url=https://a.invalid/c.yaml")
            .expect("parse clash link");
        assert_eq!(b.subscription_url, "https://a.invalid/c.yaml");
        assert!(b.name.is_none());

        assert!(parse_deep_link("https://example.com").is_none());
    }

    #[test]
    fn test_extract_deep_link_from_args() {
        let args = vec![
            "novaproxy".to_string(),
            "--minimized".to_string(),
            "clash://install-config?url=https://example.com/sub".to_string(),
        ];
        assert_eq!(
            extract_deep_link_from_args(args),
            Some("clash://install-config?url=https://example.com/sub".to_string())
        );
    }

    #[test]
    fn test_single_instance_ipc_forwarding() {
        let dir = std::env::temp_dir().join(format!("novaproxy_ipc_test_{}", uuid::Uuid::new_v4()));
        let endpoint = dir.join(INSTANCE_IPC_FILENAME);
        let received = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let recv_clone = received.clone();
        let running = Arc::new(AtomicBool::new(true));

        let _port = start_ipc_listener(&endpoint, running.clone(), move |msg| {
            recv_clone.lock().push(msg);
        })
        .expect("start loopback ipc listener");

        let ok = forward_to_ipc_endpoint(
            &endpoint,
            Some("novaproxy://install-config?url=https://example.com/sub"),
        );
        assert!(ok);

        std::thread::sleep(Duration::from_millis(80));
        let items = received.lock();
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].as_deref(),
            Some("novaproxy://install-config?url=https://example.com/sub")
        );

        running.store(false, Ordering::SeqCst);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

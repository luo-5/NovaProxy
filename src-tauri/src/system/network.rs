//! Windows Network Change Listener
//! Listens for network interface, adapter, and IP routing table changes
//! using Win32 `NotifyAddrChange` and ensures WinINet system proxy state
//! remains consistent across WiFi changes, sleep/wake, and VPN tunnel events.

use crate::system::sysproxy::SystemProxyManager;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
#[cfg(not(target_os = "windows"))]
use std::time::Duration;

#[cfg(target_os = "windows")]
mod win32 {
    use std::ffi::c_void;

    pub type Handle = *mut c_void;
    pub type Dword = u32;

    #[repr(C)]
    #[derive(Default)]
    pub struct Overlapped {
        pub internal: usize,
        pub internal_high: usize,
        pub offset: u32,
        pub offset_high: u32,
        pub h_event: Handle,
    }

    pub const WAIT_OBJECT_0: u32 = 0;
    pub const WAIT_TIMEOUT: u32 = 258;
    #[allow(dead_code)]
    pub const INFINITE: u32 = 0xFFFF_FFFF;

    #[link(name = "iphlpapi")]
    extern "system" {
        pub fn NotifyAddrChange(handle: *mut Handle, overlapped: *mut Overlapped) -> Dword;
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub fn CreateEventW(
            lp_event_attributes: *mut c_void,
            b_manual_reset: i32,
            b_initial_state: i32,
            lp_name: *const u16,
        ) -> Handle;

        pub fn SetEvent(h_event: Handle) -> i32;
        pub fn ResetEvent(h_event: Handle) -> i32;
        pub fn WaitForMultipleObjects(
            n_count: u32,
            lp_handles: *const Handle,
            b_wait_all: i32,
            dw_milliseconds: u32,
        ) -> u32;
        pub fn CloseHandle(h_object: Handle) -> i32;
    }
}

pub struct NetworkChangeListener {
    sysproxy: SystemProxyManager,
    running: Arc<AtomicBool>,
    #[cfg(target_os = "windows")]
    stop_event: Arc<AtomicUsize>,
}

impl NetworkChangeListener {
    pub fn new(sysproxy: SystemProxyManager) -> Self {
        Self {
            sysproxy,
            running: Arc::new(AtomicBool::new(false)),
            #[cfg(target_os = "windows")]
            stop_event: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        #[cfg(target_os = "windows")]
        {
            let raw_h = self.stop_event.load(Ordering::SeqCst);
            if raw_h != 0 {
                unsafe {
                    win32::SetEvent(raw_h as win32::Handle);
                }
            }
        }
    }

    pub fn start(self: Arc<Self>) -> std::thread::JoinHandle<()> {
        self.running.store(true, Ordering::SeqCst);
        let listener = self.clone();

        std::thread::Builder::new()
            .name("novaproxy-netchange".to_string())
            .spawn(move || {
                tracing::info!("Windows network change listener thread started");

                #[cfg(target_os = "windows")]
                {
                    use win32::*;
                    let stop_h =
                        unsafe { CreateEventW(std::ptr::null_mut(), 1, 0, std::ptr::null()) };
                    let change_h =
                        unsafe { CreateEventW(std::ptr::null_mut(), 1, 0, std::ptr::null()) };

                    listener.stop_event.store(stop_h as usize, Ordering::SeqCst);

                    let mut overlapped = Overlapped {
                        h_event: change_h,
                        ..Default::default()
                    };

                    let handles = [stop_h, change_h];

                    while listener.running.load(Ordering::SeqCst) {
                        let mut handle_out: Handle = std::ptr::null_mut();
                        unsafe {
                            ResetEvent(change_h);
                            let _ = NotifyAddrChange(&mut handle_out, &mut overlapped);
                            let wait_res = WaitForMultipleObjects(2, handles.as_ptr(), 0, 3000);

                            if wait_res == WAIT_OBJECT_0 {
                                // Stop event signaled
                                break;
                            } else if wait_res == WAIT_OBJECT_0 + 1 {
                                // Network address changed
                                tracing::info!("Network interface or address change detected");
                                if listener.sysproxy.is_active() {
                                    // Refresh WinINet options to keep browsers & OS aligned
                                    let _ = SystemProxyManager::refresh_wininet();
                                    tracing::debug!(
                                        "Refreshed WinINet proxy settings after network change"
                                    );
                                }
                            } else if wait_res == WAIT_TIMEOUT {
                                // Periodic loop check
                                continue;
                            }
                        }
                    }

                    unsafe {
                        CloseHandle(stop_h);
                        CloseHandle(change_h);
                    }
                    listener.stop_event.store(0, Ordering::SeqCst);
                }

                #[cfg(not(target_os = "windows"))]
                {
                    while listener.running.load(Ordering::SeqCst) {
                        std::thread::sleep(Duration::from_millis(500));
                    }
                }

                tracing::info!("Windows network change listener thread stopped");
            })
            .expect("Failed to spawn network change listener thread")
    }
}

impl Drop for NetworkChangeListener {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_listener_start_and_stop() {
        let sysproxy = SystemProxyManager::new();
        let listener = Arc::new(NetworkChangeListener::new(sysproxy));
        assert!(!listener.is_running());

        let handle = listener.clone().start();
        assert!(listener.is_running());

        // Let it run for a brief moment
        std::thread::sleep(std::time::Duration::from_millis(50));
        listener.stop();
        let _ = handle.join();
        assert!(!listener.is_running());
    }
}

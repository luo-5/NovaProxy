use serde::{Deserialize, Serialize};
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Manager, Runtime,
};
#[cfg(target_os = "android")]
use tauri::plugin::PluginHandle;

#[cfg(target_os = "android")]
const PLUGIN_IDENTIFIER: &str = "com.novaproxy.androidvpn";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartVpnOptions {
    pub mixed_port: u16,
    pub ipv6: bool,
    pub mtu: u16,
    pub dns_server: Option<String>,
    pub log_file: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidVpnStatus {
    pub running: bool,
    pub prepared: bool,
    pub native_lib_dir: Option<String>,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
struct PrepareVpnResponse {
    granted: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
struct NativeLibDirResponse {
    path: Option<String>,
}

pub struct AndroidVpn<R: Runtime> {
    #[allow(dead_code)]
    app: AppHandle<R>,
    #[cfg(target_os = "android")]
    handle: PluginHandle<R>,
}

impl<R: Runtime> AndroidVpn<R> {
    pub fn prepare_vpn(&self) -> Result<bool, String> {
        #[cfg(target_os = "android")]
        {
            let res: PrepareVpnResponse = self
                .handle
                .run_mobile_plugin("prepareVpn", serde_json::json!({}))
                .map_err(|e| e.to_string())?;
            Ok(res.granted)
        }
        #[cfg(not(target_os = "android"))]
        {
            Ok(false)
        }
    }

    pub fn start_vpn(&self, opts: StartVpnOptions) -> Result<(), String> {
        #[cfg(target_os = "android")]
        {
            let _: serde_json::Value = self
                .handle
                .run_mobile_plugin("startVpn", opts)
                .map_err(|e| e.to_string())?;
            Ok(())
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = opts;
            Err("Android VpnService is only available on Android".to_string())
        }
    }

    pub fn stop_vpn(&self) -> Result<(), String> {
        #[cfg(target_os = "android")]
        {
            let _: serde_json::Value = self
                .handle
                .run_mobile_plugin("stopVpn", serde_json::json!({}))
                .map_err(|e| e.to_string())?;
            Ok(())
        }
        #[cfg(not(target_os = "android"))]
        {
            Ok(())
        }
    }

    pub fn get_vpn_status(&self) -> Result<AndroidVpnStatus, String> {
        #[cfg(target_os = "android")]
        {
            self.handle
                .run_mobile_plugin("getVpnStatus", serde_json::json!({}))
                .map_err(|e| e.to_string())
        }
        #[cfg(not(target_os = "android"))]
        {
            Ok(AndroidVpnStatus::default())
        }
    }

    pub fn get_native_lib_dir(&self) -> Result<Option<String>, String> {
        #[cfg(target_os = "android")]
        {
            let res: NativeLibDirResponse = self
                .handle
                .run_mobile_plugin("getNativeLibDir", serde_json::json!({}))
                .map_err(|e| e.to_string())?;
            Ok(res.path)
        }
        #[cfg(not(target_os = "android"))]
        {
            Ok(None)
        }
    }
}

pub trait AndroidVpnExt<R: Runtime> {
    fn android_vpn(&self) -> &AndroidVpn<R>;
}

impl<R: Runtime, T: Manager<R>> AndroidVpnExt<R> for T {
    fn android_vpn(&self) -> &AndroidVpn<R> {
        self.state::<AndroidVpn<R>>().inner()
    }
}

#[tauri::command]
fn prepare_vpn<R: Runtime>(app: AppHandle<R>) -> Result<bool, String> {
    app.android_vpn().prepare_vpn()
}

#[tauri::command]
fn start_vpn<R: Runtime>(app: AppHandle<R>, options: StartVpnOptions) -> Result<(), String> {
    app.android_vpn().start_vpn(options)
}

#[tauri::command]
fn stop_vpn<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    app.android_vpn().stop_vpn()
}

#[tauri::command]
fn get_vpn_status<R: Runtime>(app: AppHandle<R>) -> Result<AndroidVpnStatus, String> {
    app.android_vpn().get_vpn_status()
}

#[tauri::command]
fn get_native_lib_dir<R: Runtime>(app: AppHandle<R>) -> Result<Option<String>, String> {
    app.android_vpn().get_native_lib_dir()
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("android-vpn")
        .invoke_handler(tauri::generate_handler![
            prepare_vpn,
            start_vpn,
            stop_vpn,
            get_vpn_status,
            get_native_lib_dir,
        ])
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            let handle = api.register_android_plugin(PLUGIN_IDENTIFIER, "AndroidVpnPlugin")?;
            #[cfg(not(target_os = "android"))]
            let _ = api;

            app.manage(AndroidVpn {
                app: app.clone(),
                #[cfg(target_os = "android")]
                handle,
            });
            Ok(())
        })
        .build()
}

import { invoke } from "@tauri-apps/api/core";

/**
 * Type-safe Tauri IPC wrapper
 */
export async function getAppVersion(): Promise<string> {
  return await invoke<string>("get_app_version");
}

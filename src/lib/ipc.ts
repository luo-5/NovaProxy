import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  CoreLogEntry,
  CoreStartRequest,
  CoreStatusInfo,
} from "../types/bindings";

export type {
  AppSettings,
  CapabilityRule,
  CoreCapabilities,
  CoreLifecycleState,
  CoreLogEntry,
  CoreSelection,
  CoreStartRequest,
  CoreStatusInfo,
  CoreType,
  GroupType,
  LatencyRecord,
  LogLevel,
  LogStream,
  NodeGroup,
  ProtocolConfig,
  ProtocolType,
  ProxyNode,
  RoutingMode,
  Subscription,
  TlsConfig,
  TransportConfig,
  TransportKind,
} from "../types/bindings";

/**
 * Type-safe Tauri IPC wrapper
 */
export async function getAppVersion(): Promise<string> {
  return await invoke<string>("get_app_version");
}

export async function getCoreDefaultConfig(mixedPort?: number): Promise<string> {
  return await invoke<string>("core_default_config", { mixedPort });
}

export async function coreStart(
  request?: Partial<CoreStartRequest>
): Promise<CoreStatusInfo> {
  return await invoke<CoreStatusInfo>("core_start", { request });
}

export async function coreStop(): Promise<CoreStatusInfo> {
  return await invoke<CoreStatusInfo>("core_stop");
}

export async function coreRestart(
  request?: Partial<CoreStartRequest>
): Promise<CoreStatusInfo> {
  return await invoke<CoreStatusInfo>("core_restart", { request });
}

export async function coreStatus(): Promise<CoreStatusInfo> {
  return await invoke<CoreStatusInfo>("core_status");
}

export async function coreLogs(
  sinceSeq?: number,
  limit?: number
): Promise<CoreLogEntry[]> {
  return await invoke<CoreLogEntry[]>("core_logs", { sinceSeq, limit });
}

export async function coreClearLogs(): Promise<void> {
  return await invoke<void>("core_clear_logs");
}

export async function onCoreLog(
  handler: (entry: CoreLogEntry) => void
): Promise<UnlistenFn> {
  return await listen<CoreLogEntry>("core://log", (event) => {
    handler(event.payload);
  });
}

export async function onCoreStatusChange(
  handler: (status: CoreStatusInfo) => void
): Promise<UnlistenFn> {
  return await listen<CoreStatusInfo>("core://status", (event) => {
    handler(event.payload);
  });
}

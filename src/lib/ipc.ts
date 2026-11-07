import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  CoreLogEntry,
  CoreStartRequest,
  CoreStatusInfo,
  CoreType,
  DnsSettings,
  ProxyNode,
  RoutingSettings,
  RuleItem,
  Subscription,
  SubscriptionParseReport,
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
  DnsSettings,
  DnsStrategy,
  GroupType,
  LatencyRecord,
  LogLevel,
  LogStream,
  NodeGroup,
  ProtocolConfig,
  ProtocolType,
  ProxyNode,
  RoutingMode,
  RoutingSettings,
  RuleAction,
  RuleItem,
  RuleType,
  Subscription,
  SubscriptionFormat,
  SubscriptionParseReport,
  SubscriptionUserInfo,
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

// ==================== Phase P2: Share Links, Subscriptions & QR Code ====================

export async function parseLink(uri: string): Promise<ProxyNode> {
  return await invoke<ProxyNode>("parse_link", { uri });
}

export async function exportNodeLink(node: ProxyNode): Promise<string> {
  return await invoke<string>("export_node_link", { node });
}

export async function parseSubscriptionText(
  content: string
): Promise<SubscriptionParseReport> {
  return await invoke<SubscriptionParseReport>("parse_subscription_text", {
    content,
  });
}

export async function importClipboardNodes(
  content: string
): Promise<SubscriptionParseReport> {
  return await invoke<SubscriptionParseReport>("import_clipboard_nodes", {
    content,
  });
}

export async function listNodes(subscriptionId?: string): Promise<ProxyNode[]> {
  return await invoke<ProxyNode[]>("list_nodes", { subscriptionId });
}

export async function upsertNode(node: ProxyNode): Promise<ProxyNode> {
  return await invoke<ProxyNode>("upsert_node", { node });
}

export async function deleteNode(id: string): Promise<boolean> {
  return await invoke<boolean>("delete_node", { id });
}

export async function listSubscriptions(): Promise<Subscription[]> {
  return await invoke<Subscription[]>("list_subscriptions");
}

export async function upsertSubscription(
  subscription: Subscription
): Promise<Subscription> {
  return await invoke<Subscription>("upsert_subscription", { subscription });
}

export async function deleteSubscription(id: string): Promise<boolean> {
  return await invoke<boolean>("delete_subscription", { id });
}

export async function refreshSubscription(
  id: string
): Promise<SubscriptionParseReport> {
  return await invoke<SubscriptionParseReport>("refresh_subscription", { id });
}

export async function encodeNodeQr(node: ProxyNode): Promise<string> {
  return await invoke<string>("encode_node_qr", { node });
}

export async function decodeQrImageBase64(
  imageBase64: string
): Promise<string[]> {
  return await invoke<string[]>("decode_qr_image_base64", { imageBase64 });
}

export async function scanScreenQr(): Promise<string[]> {
  return await invoke<string[]>("scan_screen_qr");
}

// ==================== Phase P3: Routing, DNS & Config Generation ====================

export async function getRoutingSettings(): Promise<RoutingSettings> {
  return await invoke<RoutingSettings>("get_routing_settings");
}

export async function saveRoutingSettings(
  settings: RoutingSettings
): Promise<RoutingSettings> {
  return await invoke<RoutingSettings>("save_routing_settings", { settings });
}

export async function listRoutingRules(): Promise<RuleItem[]> {
  return await invoke<RuleItem[]>("list_routing_rules");
}

export async function upsertRoutingRule(rule: RuleItem): Promise<RuleItem> {
  return await invoke<RuleItem>("upsert_routing_rule", { rule });
}

export async function deleteRoutingRule(id: string): Promise<boolean> {
  return await invoke<boolean>("delete_routing_rule", { id });
}

export async function reorderRoutingRules(ids: string[]): Promise<void> {
  return await invoke<void>("reorder_routing_rules", { ids });
}

export async function getDnsSettings(): Promise<DnsSettings> {
  return await invoke<DnsSettings>("get_dns_settings");
}

export async function saveDnsSettings(
  settings: DnsSettings
): Promise<DnsSettings> {
  return await invoke<DnsSettings>("save_dns_settings", { settings });
}

export async function generateConfig(
  coreType?: CoreType,
  activeNodeId?: string
): Promise<string> {
  return await invoke<string>("generate_config", { coreType, activeNodeId });
}

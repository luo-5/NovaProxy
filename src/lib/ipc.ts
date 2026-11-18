import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppSettings,
  CoreBinaryStatus,
  CoreLogEntry,
  CoreStartRequest,
  CoreStatusInfo,
  CoreType,
  CoreUpdateCheckResult,
  CoreUpdateResult,
  DiagnosticBundleResult,
  DnsSettings,
  LatencyTestMode,
  LogFileInfo,
  LogLevel,
  NodeGroup,
  PlatformInfo,
  ProxyNode,
  RoutingSettings,
  RuleItem,
  ServiceState,
  Subscription,
  SubscriptionParseReport,
  SysProxySnapshot,
  TelemetrySummary,
  UrlSchemeStatus,
  WintunStatus,
} from "../types/bindings";

export type {
  AppSettings,
  CapabilityRule,
  CoreBinaryStatus,
  CoreCapabilities,
  CoreLifecycleState,
  CoreLogEntry,
  CoreSelection,
  CoreStartRequest,
  CoreStartSample,
  CoreStatusInfo,
  CoreType,
  CoreUpdateCheckResult,
  CoreUpdateResult,
  CrashSample,
  DiagnosticBundleResult,
  DnsSettings,
  DnsStrategy,
  FrontendErrorSample,
  GroupType,
  InsightSeverity,
  LatencyRecord,
  LatencyTestMode,
  LogFileInfo,
  LogLevel,
  LogStream,
  NodeGroup,
  OptimizationInsight,
  PlatformInfo,
  ProtocolConfig,
  ProtocolLatencyStats,
  ProtocolType,
  ProxyNode,
  RoutingMode,
  RoutingSettings,
  RuleAction,
  RuleItem,
  RuleType,
  ServiceState,
  Subscription,
  SubscriptionFormat,
  SubscriptionParseReport,
  SubscriptionUserInfo,
  SysProxySnapshot,
  TelemetrySummary,
  TlsConfig,
  TrafficSnapshot,
  TransportConfig,
  TransportKind,
  TunSettings,
  UrlSchemeStatus,
  WintunStatus,
} from "../types/bindings";

export interface AndroidVpnStatus {
  running: boolean;
  prepared: boolean;
  nativeLibDir: string | null;
  rxBytes: number;
  txBytes: number;
}

/**
 * Type-safe Tauri IPC wrapper
 */
export async function getAppVersion(): Promise<string> {
  return await invoke<string>("get_app_version");
}

export async function getPlatformInfo(): Promise<PlatformInfo> {
  return await invoke<PlatformInfo>("get_platform_info");
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

// ==================== Phase P4: System Proxy, TUN & Deep Links ====================

export async function getSysproxyStatus(): Promise<SysProxySnapshot> {
  return await invoke<SysProxySnapshot>("get_sysproxy_status");
}

export async function setSysproxy(
  enabled: boolean
): Promise<SysProxySnapshot> {
  return await invoke<SysProxySnapshot>("set_sysproxy", { enabled });
}

export async function isProcessElevated(): Promise<boolean> {
  return await invoke<boolean>("is_process_elevated");
}

export async function requestAdminRelaunch(): Promise<void> {
  return await invoke<void>("request_admin_relaunch");
}

export async function getAutostartStatus(): Promise<boolean> {
  return await invoke<boolean>("get_autostart_status");
}

export async function setAutostartStatus(enabled: boolean): Promise<boolean> {
  return await invoke<boolean>("set_autostart_status", { enabled });
}

export async function handleDeepLinkUrl(
  uri: string
): Promise<Subscription | null> {
  return await invoke<Subscription | null>("handle_deep_link_url", { uri });
}

// ==================== Phase P5: App Settings, Latency & Traffic ====================

export interface CoreTraffic {
  up: number;
  down: number;
}

export interface ConnectionMetadata {
  network?: string;
  type?: string;
  sourceIP?: string;
  destinationIP?: string;
  sourcePort?: string;
  destinationPort?: string;
  host?: string;
  dnsMode?: string;
  process?: string;
  processPath?: string;
}

export interface ConnectionItem {
  id: string;
  metadata: ConnectionMetadata;
  upload: number;
  download: number;
  start: string;
  chains: string[];
  rule: string;
  rulePayload?: string;
}

export interface CoreConnectionsReport {
  downloadTotal: number;
  uploadTotal: number;
  connections: ConnectionItem[];
}

export async function getAppSettings(): Promise<AppSettings> {
  return await invoke<AppSettings>("get_app_settings");
}

export async function saveAppSettings(
  settings: AppSettings
): Promise<AppSettings> {
  return await invoke<AppSettings>("save_app_settings", { settings });
}

export async function testNodeLatency(
  id: string,
  mode?: LatencyTestMode,
  testUrl?: string
): Promise<number | null> {
  return await invoke<number | null>("test_node_latency", {
    id,
    mode,
    testUrl,
  });
}

export async function testAllNodesLatency(
  mode?: LatencyTestMode,
  testUrl?: string
): Promise<[string, number | null][]> {
  return await invoke<[string, number | null][]>("test_all_nodes_latency", {
    mode,
    testUrl,
  });
}

export async function listGroups(): Promise<NodeGroup[]> {
  return await invoke<NodeGroup[]>("list_groups");
}

export async function selectActiveNode(id: string): Promise<void> {
  return await invoke<void>("select_active_node", { id });
}

export async function getCoreTraffic(): Promise<CoreTraffic> {
  return await invoke<CoreTraffic>("get_core_traffic");
}

export async function getCoreConnections(): Promise<CoreConnectionsReport> {
  return await invoke<CoreConnectionsReport>("get_core_connections");
}

export async function closeCoreConnection(id?: string): Promise<void> {
  return await invoke<void>("close_core_connection", { id });
}

export async function checkDatabaseIntegrity(): Promise<boolean> {
  return await invoke<boolean>("check_database_integrity");
}

export async function backupDatabase(targetPath: string): Promise<void> {
  return await invoke<void>("backup_database", { targetPath });
}

export async function exportDiagnosticsReport(): Promise<Record<string, unknown>> {
  return await invoke<Record<string, unknown>>("export_diagnostics_report");
}

export async function getTelemetrySummary(): Promise<TelemetrySummary> {
  return await invoke<TelemetrySummary>("get_telemetry_summary");
}

export async function getLogFiles(): Promise<LogFileInfo[]> {
  return await invoke<LogFileInfo[]>("get_log_files");
}

export async function readLogFile(
  fileName: string,
  tailLines?: number
): Promise<string> {
  return await invoke<string>("read_log_file", { fileName, tailLines });
}

export async function logFrontendEvent(
  level: LogLevel,
  component: string,
  message: string
): Promise<void> {
  await invoke<void>("log_frontend_event", {
    level,
    component,
    message,
  }).catch(() => {});
}

export async function exportDiagnosticBundle(): Promise<DiagnosticBundleResult> {
  return await invoke<DiagnosticBundleResult>("export_diagnostic_bundle");
}

export async function prepareAndroidVpn(): Promise<boolean> {
  return await invoke<boolean>("prepare_android_vpn");
}

export async function getAndroidVpnStatus(): Promise<AndroidVpnStatus> {
  return await invoke<AndroidVpnStatus>("get_android_vpn_status");
}

export async function getServiceStatus(): Promise<ServiceState> {
  return await invoke<ServiceState>("get_service_status");
}

export async function checkWintunStatus(): Promise<WintunStatus> {
  return await invoke<WintunStatus>("check_wintun_status");
}

export async function getUrlSchemeStatus(): Promise<UrlSchemeStatus> {
  return await invoke<UrlSchemeStatus>("get_url_scheme_status");
}

export async function setClashUrlScheme(enabled: boolean): Promise<UrlSchemeStatus> {
  return await invoke<UrlSchemeStatus>("set_clash_url_scheme", { enabled });
}

export async function getCoreVersions(): Promise<CoreBinaryStatus[]> {
  return await invoke<CoreBinaryStatus[]>("get_core_versions");
}

export async function checkCoreUpdate(
  core: CoreType,
  useProxy?: boolean
): Promise<CoreUpdateCheckResult> {
  return await invoke<CoreUpdateCheckResult>("check_core_update", {
    core,
    useProxy,
  });
}

export async function installCoreUpdate(
  core: CoreType,
  useProxy?: boolean,
  expectedSha256?: string
): Promise<CoreUpdateResult> {
  return await invoke<CoreUpdateResult>("install_core_update", {
    core,
    useProxy,
    expectedSha256,
  });
}

export function onDeepLinkImport(
  handler: (sub: Subscription) => void
): Promise<UnlistenFn> {
  return listen<Subscription>("app://deep-link", (event) => {
    handler(event.payload);
  });
}

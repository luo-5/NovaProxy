import React, { useEffect, useState, useMemo } from "react";
import { useTranslation } from "react-i18next";
import {
  Power,
  ArrowUp,
  ArrowDown,
  Globe,
  Shield,
  Zap,
  Radio,
  Server,
  AlertCircle,
  ExternalLink,
  Cpu,
} from "lucide-react";
import { useAppStore } from "../stores/appStore";
import {
  coreStart,
  coreStatus,
  coreStop,
  getAppSettings,
  saveAppSettings,
  getSysproxyStatus,
  setSysproxy,
  isProcessElevated,
  requestAdminRelaunch,
  listNodes,
  getCoreTraffic,
  onCoreStatusChange,
  type CoreStatusInfo,
} from "../lib/ipc";
import { formatSpeed, formatLatency, getProtocolBadge } from "../lib/utils";
import { SpeedChart } from "../components/SpeedChart";

export const Dashboard: React.FC = () => {
  const { t } = useTranslation();
  const {
    isConnected,
    setIsConnected,
    systemProxy,
    setSystemProxy,
    tunMode,
    setTunMode,
    routingMode,
    setRoutingMode,
    activeNode,
    setActiveNode,
    appSettings,
    setAppSettings,
    trafficHistory,
    addTrafficPoint,
    setCurrentTab,
  } = useAppStore();

  const [status, setStatus] = useState<CoreStatusInfo | null>(null);
  const [busy, setBusy] = useState<boolean>(false);
  const [isElevated, setIsElevated] = useState<boolean>(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [currentSpeed, setCurrentSpeed] = useState<{ up: number; down: number }>({
    up: 0,
    down: 0,
  });

  // Initial load
  useEffect(() => {
    // 1. Fetch app settings
    getAppSettings()
      .then((s) => {
        setAppSettings(s);
        setSystemProxy(s.system_proxy);
        setTunMode(s.tun_mode);
        setRoutingMode(s.routing_mode);
      })
      .catch(() => {});

    // 2. Fetch elevation
    isProcessElevated().then(setIsElevated).catch(() => {});

    // 3. Fetch sysproxy status
    getSysproxyStatus()
      .then((snap) => {
        setSystemProxy(snap.enabled);
      })
      .catch(() => {});

    // 4. Fetch core status
    coreStatus()
      .then((s) => {
        setStatus(s);
        setIsConnected(s.lifecycle.state === "running");
      })
      .catch(() => {});

    // 5. Fetch active node
    listNodes()
      .then((nodes) => {
        if (nodes.length > 0) {
          getAppSettings().then((s) => {
            const found = nodes.find((n) => n.id === s.active_node_id);
            setActiveNode(found || nodes[0]);
          });
        }
      })
      .catch(() => {});

    // Listen for core status transitions
    let unlisten: (() => void) | undefined;
    onCoreStatusChange((newStatus) => {
      setStatus(newStatus);
      setIsConnected(newStatus.lifecycle.state === "running");
    })
      .then((fn) => {
        unlisten = fn;
      })
      .catch(() => {});

    return () => {
      if (unlisten) unlisten();
    };
  }, [setAppSettings, setIsConnected, setRoutingMode, setActiveNode, setSystemProxy, setTunMode]);

  // Periodic traffic sampling
  useEffect(() => {
    if (!isConnected) {
      setCurrentSpeed({ up: 0, down: 0 });
      return;
    }

    const timer = setInterval(() => {
      getCoreTraffic()
        .then((data) => {
          const up = data.up || 0;
          const down = data.down || 0;
          setCurrentSpeed({ up, down });
          addTrafficPoint(up, down);
        })
        .catch(() => {});
    }, 1000);

    return () => clearInterval(timer);
  }, [isConnected, addTrafficPoint]);

  // Handle hero connect toggle
  const handleToggleConnect = async () => {
    if (busy) return;
    setBusy(true);
    setErrorMsg(null);
    try {
      if (isConnected) {
        const s = await coreStop();
        setStatus(s);
        setIsConnected(false);
      } else {
        const s = await coreStart();
        setStatus(s);
        setIsConnected(s.lifecycle.state === "running");
      }
    } catch (err) {
      setErrorMsg(String(err));
    } finally {
      setBusy(false);
    }
  };

  // Toggle system proxy
  const handleToggleSysproxy = async () => {
    try {
      const target = !systemProxy;
      const snap = await setSysproxy(target);
      setSystemProxy(snap.enabled);
    } catch (err) {
      setErrorMsg(String(err));
    }
  };

  // Toggle TUN mode
  const handleToggleTun = async () => {
    if (!isElevated && !tunMode) {
      setErrorMsg("TUN mode requires administrator privileges. Please relaunch as admin.");
      return;
    }
    try {
      const target = !tunMode;
      if (appSettings) {
        const updated = { ...appSettings, tun_mode: target };
        await saveAppSettings(updated);
        setAppSettings(updated);
        setTunMode(target);
        // If core is running, restart to apply TUN config
        if (isConnected) {
          const s = await coreStart();
          setStatus(s);
        }
      }
    } catch (err) {
      setErrorMsg(String(err));
    }
  };

  // Change routing mode
  const handleRoutingModeChange = async (mode: "rule" | "global" | "direct") => {
    try {
      setRoutingMode(mode);
      if (appSettings) {
        const updated = { ...appSettings, routing_mode: mode };
        await saveAppSettings(updated);
        setAppSettings(updated);
        if (isConnected) {
          const s = await coreStart();
          setStatus(s);
        }
      }
    } catch (err) {
      setErrorMsg(String(err));
    }
  };

  // Active protocol badge
  const protocolBadge = useMemo(() => {
    return activeNode ? getProtocolBadge(activeNode.protocol) : null;
  }, [activeNode]);

  // Latency badge
  const latencyBadge = useMemo(() => {
    return activeNode ? formatLatency(activeNode.latency) : null;
  }, [activeNode]);

  const activePort = status?.actual_mixed_port ?? appSettings?.mixed_port ?? 7890;

  return (
    <div className="p-8 max-w-5xl mx-auto space-y-6">
      {/* Top Alert if error exists */}
      {errorMsg && (
        <div className="p-4 rounded-2xl bg-destructive/10 border border-destructive/20 text-destructive text-xs flex items-center justify-between">
          <div className="flex items-center gap-2">
            <AlertCircle className="w-4 h-4 shrink-0" />
            <span>{errorMsg}</span>
          </div>
          <button
            onClick={() => setErrorMsg(null)}
            className="text-xs font-semibold hover:underline"
          >
            {t("common.close")}
          </button>
        </div>
      )}

      {/* Hero Connect Switch & Speed Curves */}
      <div className="bg-card border border-border rounded-2xl p-6 shadow-sm relative overflow-hidden">
        <div className="grid grid-cols-1 md:grid-cols-3 gap-6 items-center">
          {/* Left: Big Connect Button */}
          <div className="flex flex-col items-center justify-center text-center space-y-3">
            <button
              disabled={busy}
              onClick={handleToggleConnect}
              className={`w-28 h-28 rounded-full flex flex-col items-center justify-center transition-all duration-300 shadow-xl disabled:opacity-50 relative group ${
                isConnected
                  ? "bg-emerald-500 text-white shadow-emerald-500/25 ring-4 ring-emerald-500/20 scale-105"
                  : "bg-secondary text-muted-foreground hover:text-foreground hover:bg-secondary/80 ring-4 ring-transparent"
              }`}
            >
              <Power className={`w-9 h-9 transition-transform group-hover:scale-110 ${busy ? "animate-spin" : ""}`} />
              <span className="text-[11px] font-semibold mt-1">
                {isConnected ? t("dashboard.connected") : t("dashboard.clickToConnect")}
              </span>
            </button>
            <div className="text-center">
              <span className="text-xs text-muted-foreground">
                {isConnected ? `Listening on port :${activePort}` : "Ready to connect"}
              </span>
            </div>
          </div>

          {/* Center: Live Speed & Traffic Stats */}
          <div className="md:col-span-2 space-y-4">
            <div className="grid grid-cols-2 gap-4">
              <div className="p-4 rounded-xl bg-secondary/40 border border-border/50 flex items-center gap-3">
                <div className="w-10 h-10 rounded-xl bg-sky-500/10 text-sky-400 flex items-center justify-center">
                  <ArrowDown className="w-5 h-5" />
                </div>
                <div>
                  <span className="text-[11px] text-muted-foreground block">
                    {t("dashboard.downloadSpeed")}
                  </span>
                  <span className="text-lg font-bold text-foreground">
                    {formatSpeed(currentSpeed.down)}
                  </span>
                </div>
              </div>

              <div className="p-4 rounded-xl bg-secondary/40 border border-border/50 flex items-center gap-3">
                <div className="w-10 h-10 rounded-xl bg-emerald-500/10 text-emerald-400 flex items-center justify-center">
                  <ArrowUp className="w-5 h-5" />
                </div>
                <div>
                  <span className="text-[11px] text-muted-foreground block">
                    {t("dashboard.uploadSpeed")}
                  </span>
                  <span className="text-lg font-bold text-foreground">
                    {formatSpeed(currentSpeed.up)}
                  </span>
                </div>
              </div>
            </div>

            {/* Speed Curve Chart */}
            <div className="bg-secondary/20 border border-border/40 rounded-xl p-2">
              <SpeedChart data={trafficHistory} />
            </div>
          </div>
        </div>
      </div>

      {/* Control Strip: Routing Mode + System Proxy + TUN Mode */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        {/* 1. Routing Mode */}
        <div className="bg-card border border-border rounded-2xl p-5 shadow-sm space-y-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <Globe className="w-4 h-4 text-primary" />
              <span className="text-xs font-semibold">{t("dashboard.routingMode")}</span>
            </div>
            <span className="text-[11px] text-muted-foreground capitalize">{routingMode}</span>
          </div>

          <div className="grid grid-cols-3 gap-1.5 p-1 bg-secondary/50 rounded-xl border border-border/50">
            <button
              onClick={() => handleRoutingModeChange("rule")}
              className={`py-1.5 text-xs font-medium rounded-lg transition-all ${
                routingMode === "rule"
                  ? "bg-primary text-primary-foreground shadow-sm"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              Rule
            </button>
            <button
              onClick={() => handleRoutingModeChange("global")}
              className={`py-1.5 text-xs font-medium rounded-lg transition-all ${
                routingMode === "global"
                  ? "bg-primary text-primary-foreground shadow-sm"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              Global
            </button>
            <button
              onClick={() => handleRoutingModeChange("direct")}
              className={`py-1.5 text-xs font-medium rounded-lg transition-all ${
                routingMode === "direct"
                  ? "bg-primary text-primary-foreground shadow-sm"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              Direct
            </button>
          </div>
        </div>

        {/* 2. System Proxy */}
        <div className="bg-card border border-border rounded-2xl p-5 shadow-sm flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="w-9 h-9 rounded-xl bg-primary/10 text-primary flex items-center justify-center">
              <Zap className="w-4 h-4" />
            </div>
            <div>
              <span className="text-xs font-semibold block">{t("dashboard.systemProxy")}</span>
              <span className="text-[11px] text-muted-foreground">WinINet Registry</span>
            </div>
          </div>
          <button
            onClick={handleToggleSysproxy}
            className={`w-12 h-6 rounded-full transition-colors relative flex items-center p-0.5 ${
              systemProxy ? "bg-primary" : "bg-secondary"
            }`}
          >
            <span
              className={`w-5 h-5 rounded-full bg-white transition-transform ${
                systemProxy ? "translate-x-6" : "translate-x-0"
              }`}
            />
          </button>
        </div>

        {/* 3. TUN Mode */}
        <div className="bg-card border border-border rounded-2xl p-5 shadow-sm flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="w-9 h-9 rounded-xl bg-primary/10 text-primary flex items-center justify-center">
              <Shield className="w-4 h-4" />
            </div>
            <div>
              <div className="flex items-center gap-1.5">
                <span className="text-xs font-semibold block">{t("dashboard.tunMode")}</span>
                {!isElevated && (
                  <span className="text-[10px] bg-amber-500/10 text-amber-500 px-1.5 py-0.2 rounded font-medium">
                    UAC
                  </span>
                )}
              </div>
              <span className="text-[11px] text-muted-foreground">Virtual NIC</span>
            </div>
          </div>

          <div className="flex items-center gap-2">
            {!isElevated && (
              <button
                onClick={() => requestAdminRelaunch().catch(() => {})}
                title="Relaunch as administrator to enable TUN"
                className="p-1 rounded-lg bg-amber-500/10 text-amber-500 hover:bg-amber-500/20 text-xs"
              >
                <ExternalLink className="w-3.5 h-3.5" />
              </button>
            )}
            <button
              onClick={handleToggleTun}
              className={`w-12 h-6 rounded-full transition-colors relative flex items-center p-0.5 ${
                tunMode ? "bg-primary" : "bg-secondary"
              }`}
            >
              <span
                className={`w-5 h-5 rounded-full bg-white transition-transform ${
                  tunMode ? "translate-x-6" : "translate-x-0"
                }`}
              />
            </button>
          </div>
        </div>
      </div>

      {/* Active Node Quick Card */}
      <div className="bg-card border border-border rounded-2xl p-5 shadow-sm flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-xl bg-secondary flex items-center justify-center text-foreground">
            <Server className="w-5 h-5" />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <span className="text-sm font-semibold">
                {activeNode?.name ?? t("dashboard.noProxySelected")}
              </span>
              {protocolBadge && (
                <span
                  className={`text-[10px] px-2 py-0.5 rounded-full font-medium ${protocolBadge.bg} ${protocolBadge.text}`}
                >
                  {protocolBadge.label}
                </span>
              )}
            </div>
            <p className="text-xs text-muted-foreground font-mono mt-0.5">
              {activeNode ? `${activeNode.server}:${activeNode.port}` : "Select a node to route traffic"}
            </p>
          </div>
        </div>

        <div className="flex items-center gap-3">
          {latencyBadge && (
            <span
              className={`text-xs px-2.5 py-1 rounded-xl font-mono border ${latencyBadge.badgeClass}`}
            >
              {latencyBadge.text}
            </span>
          )}
          <button
            onClick={() => setCurrentTab("proxies")}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-secondary hover:bg-secondary/80 text-xs font-medium transition-colors"
          >
            <Radio className="w-3.5 h-3.5 text-primary" />
            <span>{t("dashboard.selectProxy")}</span>
          </button>
        </div>
      </div>

      {/* Core Runtime Status Pill */}
      {status && (
        <div className="bg-secondary/30 border border-border/50 rounded-2xl p-4 flex flex-wrap items-center justify-between gap-3 text-xs text-muted-foreground">
          <div className="flex items-center gap-2">
            <Cpu className="w-4 h-4 text-primary" />
            <span>
              {t("dashboard.activeCore")}:{" "}
              <strong className="text-foreground capitalize font-semibold">
                {status.active_core ?? appSettings?.preferred_core ?? "sing-box"}
              </strong>
            </span>
            {status.pid && (
              <span className="font-mono text-[11px] text-muted-foreground/70">
                (PID: {status.pid})
              </span>
            )}
          </div>

          <div className="flex items-center gap-4 font-mono text-[11px]">
            <span>Mixed Port: {activePort}</span>
            {status.port_reassigned && (
              <span className="text-amber-500 font-sans font-medium">
                {t("dashboard.portReassigned")}
              </span>
            )}
          </div>
        </div>
      )}
    </div>
  );
};

import React, { useEffect, useRef, useState, useMemo, useCallback } from "react";
import { useTranslation } from "react-i18next";
import {
  Terminal,
  Play,
  Square,
  RotateCcw,
  Trash2,
  Code2,
  Copy,
  Check,
  Search,
  Pause,
  ArrowDownCircle,
  Activity,
  Download,
  RefreshCw,
  AlertTriangle,
  FileText,
  Sparkles,
  CheckCircle2,
} from "lucide-react";
import {
  coreClearLogs,
  coreLogs,
  coreRestart,
  coreStart,
  coreStatus,
  coreStop,
  exportDiagnosticBundle,
  getAppSettings,
  getCoreDefaultConfig,
  getLogFiles,
  getTelemetrySummary,
  logFrontendEvent,
  onCoreLog,
  onCoreStatusChange,
  readLogFile,
  type CoreLogEntry,
  type CoreStatusInfo,
  type CoreType,
  type LogFileInfo,
  type TelemetrySummary,
} from "../lib/ipc";
import { formatBytes } from "../lib/utils";
import { useAppStore } from "../stores/appStore";

const DEFAULT_MINIMAL_SINGBOX_CONFIG = JSON.stringify(
  {
    log: {
      level: "info",
      timestamp: true,
    },
    inbounds: [
      {
        type: "mixed",
        tag: "mixed-in",
        listen: "127.0.0.1",
        listen_port: 7890,
      },
    ],
    outbounds: [
      {
        type: "direct",
        tag: "direct",
      },
    ],
  },
  null,
  2
);

export const Logs: React.FC = () => {
  const { t, i18n } = useTranslation();
  const { setIsConnected } = useAppStore();

  const [status, setStatus] = useState<CoreStatusInfo | null>(null);
  const [logs, setLogs] = useState<CoreLogEntry[]>([]);
  const [selectedCore, setSelectedCore] = useState<CoreType>("sing-box");
  const [mixedPort, setMixedPort] = useState<number>(7890);
  const [configText, setConfigText] = useState<string>(DEFAULT_MINIMAL_SINGBOX_CONFIG);
  const [configEdited, setConfigEdited] = useState<boolean>(false);
  const [showConfigEditor, setShowConfigEditor] = useState<boolean>(false);
  const [busy, setBusy] = useState<boolean>(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [toastMsg, setToastMsg] = useState<string | null>(null);

  // Filters & Controls
  const [viewMode, setViewMode] = useState<"live" | "files">("live");
  const [searchQuery, setSearchQuery] = useState<string>("");
  const [levelFilter, setLevelFilter] = useState<string>("all");
  const [sourceFilter, setSourceFilter] = useState<string>("all");
  const [autoScroll, setAutoScroll] = useState<boolean>(true);
  const [copied, setCopied] = useState<boolean>(false);

  // Telemetry & Disk Logs
  const [telemetry, setTelemetry] = useState<TelemetrySummary | null>(null);
  const [logFiles, setLogFiles] = useState<LogFileInfo[]>([]);
  const [selectedLogFile, setSelectedLogFile] = useState<string>("");
  const [diskFileContent, setDiskFileContent] = useState<string>("");
  const [loadingDiagnostics, setLoadingDiagnostics] = useState<boolean>(false);
  const [exportingBundle, setExportingBundle] = useState<boolean>(false);

  const logBottomRef = useRef<HTMLDivElement | null>(null);
  const toastTimerRef = useRef<number | null>(null);

  const showToast = useCallback((msg: string) => {
    setToastMsg(msg);
    if (toastTimerRef.current !== null) {
      window.clearTimeout(toastTimerRef.current);
    }
    toastTimerRef.current = window.setTimeout(() => {
      setToastMsg(null);
      toastTimerRef.current = null;
    }, 3500);
  }, []);

  const refreshDiagnostics = useCallback(async () => {
    setLoadingDiagnostics(true);
    try {
      const [telRes, filesRes] = await Promise.allSettled([
        getTelemetrySummary(),
        getLogFiles(),
      ]);
      if (telRes.status === "fulfilled") {
        setTelemetry(telRes.value);
      }
      if (filesRes.status === "fulfilled") {
        setLogFiles(filesRes.value);
        setSelectedLogFile((prev) =>
          prev && filesRes.value.some((f) => f.name === prev)
            ? prev
            : filesRes.value[0]?.name ?? ""
        );
      }
    } finally {
      setLoadingDiagnostics(false);
    }
  }, []);

  useEffect(() => {
    Promise.allSettled([coreStatus(), getAppSettings()])
      .then(([statusRes, settingsRes]) => {
        const s = statusRes.status === "fulfilled" ? statusRes.value : null;
        const settings = settingsRes.status === "fulfilled" ? settingsRes.value : null;
        if (s) {
          setStatus(s);
          setIsConnected(s.lifecycle.state === "running");
        }
        const preferred = s?.active_core ?? settings?.preferred_core;
        if (preferred) setSelectedCore(preferred);
        const port = s?.actual_mixed_port ?? settings?.mixed_port;
        if (port) setMixedPort(port);
      })
      .catch(() => {});

    coreLogs(undefined, 500)
      .then((entries) => setLogs(entries))
      .catch(() => {});

    getCoreDefaultConfig(7890)
      .then((cfg) => {
        if (cfg) setConfigText(cfg);
      })
      .catch(() => {});

    void refreshDiagnostics();

    let unlistenLog: (() => void) | undefined;
    let unlistenStatus: (() => void) | undefined;

    onCoreLog((entry) => {
      setLogs((prev) => {
        if (prev.some((existing) => existing.seq === entry.seq)) {
          return prev;
        }
        const next = [...prev, entry];
        return next.length > 1000 ? next.slice(next.length - 1000) : next;
      });
    })
      .then((fn) => {
        unlistenLog = fn;
      })
      .catch(() => {});

    onCoreStatusChange((newStatus) => {
      setStatus(newStatus);
      setIsConnected(newStatus.lifecycle.state === "running");
      if (newStatus.active_core) setSelectedCore(newStatus.active_core);
      if (newStatus.actual_mixed_port) setMixedPort(newStatus.actual_mixed_port);
      void refreshDiagnostics();
    })
      .then((fn) => {
        unlistenStatus = fn;
      })
      .catch(() => {});

    return () => {
      if (unlistenLog) unlistenLog();
      if (unlistenStatus) unlistenStatus();
      if (toastTimerRef.current !== null) {
        window.clearTimeout(toastTimerRef.current);
      }
    };
  }, [refreshDiagnostics, setIsConnected]);

  useEffect(() => {
    if (viewMode === "files" && selectedLogFile) {
      readLogFile(selectedLogFile, 500)
        .then(setDiskFileContent)
        .catch((err) => setDiskFileContent(`Failed to read log file: ${String(err)}`));
    }
  }, [viewMode, selectedLogFile]);

  useEffect(() => {
    if (autoScroll && viewMode === "live") {
      logBottomRef.current?.scrollIntoView({ behavior: "smooth" });
    }
  }, [logs, autoScroll, viewMode]);

  const handleStart = async () => {
    setBusy(true);
    setErrorMsg(null);
    try {
      const s = await coreStart({
        core_type: selectedCore,
        config_json: configEdited ? configText : null,
        mixed_port: mixedPort,
        allow_lan: false,
        auto_port_fallback: true,
      });
      setStatus(s);
      setIsConnected(s.lifecycle.state === "running");
      void refreshDiagnostics();
    } catch (err) {
      setErrorMsg(String(err));
      void logFrontendEvent("error", "LogsPage", `Manual core start failed: ${String(err)}`);
    } finally {
      setBusy(false);
    }
  };

  const handleStop = async () => {
    setBusy(true);
    setErrorMsg(null);
    try {
      const s = await coreStop();
      setStatus(s);
      setIsConnected(false);
      void refreshDiagnostics();
    } catch (err) {
      setErrorMsg(String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleRestart = async () => {
    setBusy(true);
    setErrorMsg(null);
    try {
      const s = await coreRestart({
        core_type: selectedCore,
        config_json: configEdited ? configText : null,
        mixed_port: mixedPort,
        allow_lan: false,
        auto_port_fallback: true,
      });
      setStatus(s);
      setIsConnected(s.lifecycle.state === "running");
      void refreshDiagnostics();
    } catch (err) {
      setErrorMsg(String(err));
      void logFrontendEvent("error", "LogsPage", `Core restart failed: ${String(err)}`);
    } finally {
      setBusy(false);
    }
  };

  const handleClearLogs = async () => {
    try {
      await coreClearLogs();
      setLogs([]);
    } catch {
      setLogs([]);
    }
  };

  const handleExportBundle = async () => {
    if (exportingBundle) return;
    setExportingBundle(true);
    try {
      const res = await exportDiagnosticBundle();
      showToast(
        t("logs.bundleExported", {
          path: res.path,
          defaultValue: `Diagnostic bundle saved to ${res.path}`,
        })
      );
      void refreshDiagnostics();
    } catch (err) {
      showToast(String(err));
    } finally {
      setExportingBundle(false);
    }
  };

  const entrySourceCategory = (entry: CoreLogEntry): string => {
    if (entry.stream === "frontend") return "frontend";
    if (entry.core) return entry.core;
    if (entry.stream === "app") return "app";
    return "system";
  };

  const filteredLogs = useMemo(() => {
    return logs.filter((entry) => {
      if (levelFilter !== "all" && entry.level.toLowerCase() !== levelFilter) {
        return false;
      }
      if (sourceFilter !== "all") {
        const cat = entrySourceCategory(entry);
        if (sourceFilter === "cores") {
          if (!entry.core) return false;
        } else if (cat !== sourceFilter) {
          return false;
        }
      }
      if (searchQuery.trim()) {
        const q = searchQuery.toLowerCase();
        const src = (entry.source ?? entry.core ?? entry.stream).toLowerCase();
        if (!entry.message.toLowerCase().includes(q) && !src.includes(q)) {
          return false;
        }
      }
      return true;
    });
  }, [logs, levelFilter, sourceFilter, searchQuery]);

  const handleCopyLogs = () => {
    const text =
      viewMode === "files"
        ? diskFileContent
        : filteredLogs
            .map((l) => {
              const tag = l.source ?? l.core ?? l.stream;
              const dur = l.duration_ms != null ? ` (${l.duration_ms}ms)` : "";
              return `[${new Date(l.timestamp).toLocaleTimeString()}] [${l.level.toUpperCase()}] [${tag}]${dur} ${l.message}`;
            })
            .join("\n");
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  };

  const lifecycleState = status?.lifecycle.state ?? "stopped";
  const isRunning = lifecycleState === "running";
  const isZh = i18n.language.startsWith("zh");

  const levelBadgeClass = (level: string) => {
    switch (level) {
      case "error":
        return "text-red-400 bg-red-500/10 border-red-500/30";
      case "warn":
        return "text-amber-400 bg-amber-500/10 border-amber-500/30";
      case "debug":
      case "trace":
        return "text-purple-400 bg-purple-500/10 border-purple-500/30";
      default:
        return "text-emerald-400 bg-emerald-500/10 border-emerald-500/30";
    }
  };

  const insightSeverityClass = (sev: string) => {
    switch (sev) {
      case "critical":
        return "border-red-500/30 bg-red-500/10 text-red-400";
      case "warn":
        return "border-amber-500/30 bg-amber-500/10 text-amber-400";
      default:
        return "border-sky-500/30 bg-sky-500/10 text-sky-400";
    }
  };

  return (
    <div className="p-4 md:p-8 max-w-5xl mx-auto space-y-5">
      {toastMsg && (
        <div className="fixed bottom-16 md:bottom-6 right-4 md:right-6 z-50 max-w-md px-4 py-2.5 rounded-xl bg-card border border-border text-foreground shadow-2xl text-xs font-medium break-all">
          {toastMsg}
        </div>
      )}

      {/* Header */}
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div>
          <h2 className="text-xl font-bold">{t("logs.title")}</h2>
          <p className="text-xs text-muted-foreground">{t("logs.subtitle")}</p>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <span
            className={`px-2.5 py-1 rounded-full text-xs font-semibold uppercase tracking-wide ${
              isRunning
                ? "bg-emerald-500/20 text-emerald-400 border border-emerald-500/30"
                : lifecycleState === "crashed"
                ? "bg-red-500/20 text-red-400 border border-red-500/30"
                : "bg-secondary text-muted-foreground"
            }`}
          >
            {lifecycleState}
          </span>
          {status?.pid && (
            <span className="text-xs font-mono px-2 py-1 rounded-xl bg-secondary text-foreground">
              PID: {status.pid}
            </span>
          )}
          {status?.actual_mixed_port && (
            <span className="text-xs font-mono px-2 py-1 rounded-xl bg-primary/15 text-primary">
              Port: {status.actual_mixed_port}
              {status.port_reassigned ? " (auto-switched)" : ""}
            </span>
          )}
          <button
            type="button"
            disabled={exportingBundle}
            onClick={handleExportBundle}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-medium bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50 transition-colors shadow-sm"
          >
            <Download className={`w-3.5 h-3.5 ${exportingBundle ? "animate-bounce" : ""}`} />
            <span>{t("logs.exportBundle")}</span>
          </button>
        </div>
      </div>

      {/* Telemetry & Optimization Insights Panel */}
      <div className="bg-card border border-border rounded-2xl p-4 md:p-5 shadow-sm space-y-4">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            <Activity className="w-4 h-4 text-primary" />
            <h3 className="text-xs font-semibold uppercase tracking-wider text-foreground">
              {t("logs.telemetryTitle")}
            </h3>
            {telemetry && (
              <span className="text-[11px] font-mono text-muted-foreground">
                ({telemetry.os}/{telemetry.arch})
              </span>
            )}
          </div>
          <button
            type="button"
            onClick={() => void refreshDiagnostics()}
            className="flex items-center gap-1.5 px-2.5 py-1 rounded-xl bg-secondary hover:bg-secondary/80 text-xs text-muted-foreground hover:text-foreground transition-colors"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${loadingDiagnostics ? "animate-spin" : ""}`} />
            <span>{t("common.refresh")}</span>
          </button>
        </div>

        {/* Metrics Strip */}
        <div className="grid grid-cols-2 md:grid-cols-6 gap-2.5 text-xs">
          <div className="p-3 rounded-xl bg-secondary/40 border border-border/50">
            <span className="text-[11px] text-muted-foreground block">{t("logs.avgStartMs")}</span>
            <span className="text-sm font-bold font-mono text-foreground">
              {telemetry?.avg_core_start_ms ?? 0} ms
            </span>
          </div>
          <div className="p-3 rounded-xl bg-secondary/40 border border-border/50">
            <span className="text-[11px] text-muted-foreground block">{t("logs.crashCount")}</span>
            <span
              className={`text-sm font-bold font-mono ${
                (telemetry?.crash_count ?? 0) > 0 ? "text-red-400" : "text-foreground"
              }`}
            >
              {telemetry?.crash_count ?? 0}
            </span>
          </div>
          <div className="p-3 rounded-xl bg-secondary/40 border border-border/50">
            <span className="text-[11px] text-muted-foreground block">{t("logs.failoverCount")}</span>
            <span className="text-sm font-bold font-mono text-foreground">
              {telemetry?.failover_count ?? 0}
            </span>
          </div>
          <div className="p-3 rounded-xl bg-secondary/40 border border-border/50">
            <span className="text-[11px] text-muted-foreground block">{t("logs.netChanges")}</span>
            <span className="text-sm font-bold font-mono text-foreground">
              {telemetry?.network_change_count ?? 0}
            </span>
          </div>
          <div className="p-3 rounded-xl bg-secondary/40 border border-border/50">
            <span className="text-[11px] text-muted-foreground block">{t("logs.subRefreshStats")}</span>
            <span className="text-sm font-bold font-mono text-foreground">
              {telemetry?.subscription_refresh_count ?? 0} /{" "}
              <span className={(telemetry?.subscription_fail_count ?? 0) > 0 ? "text-amber-400" : ""}>
                {telemetry?.subscription_fail_count ?? 0} err
              </span>
            </span>
          </div>
          <div className="p-3 rounded-xl bg-secondary/40 border border-border/50">
            <span className="text-[11px] text-muted-foreground block">{t("logs.logStorage")}</span>
            <span className="text-sm font-bold font-mono text-foreground">
              {formatBytes(telemetry?.total_log_bytes ?? 0)}
            </span>
          </div>
        </div>

        {/* Optimization Insights */}
        {telemetry && telemetry.insights.length > 0 ? (
          <div className="space-y-2">
            {telemetry.insights.map((ins) => (
              <div
                key={ins.code}
                className={`p-3 rounded-xl border text-xs flex flex-col gap-1 ${insightSeverityClass(ins.severity)}`}
              >
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div className="flex items-center gap-2 font-semibold">
                    {ins.severity === "info" ? (
                      <Sparkles className="w-3.5 h-3.5 shrink-0" />
                    ) : (
                      <AlertTriangle className="w-3.5 h-3.5 shrink-0" />
                    )}
                    <span>{isZh ? ins.title_zh : ins.title_en}</span>
                  </div>
                  <span className="font-mono text-[11px] opacity-80">{ins.metric}</span>
                </div>
                <p className="text-[11px] opacity-90 leading-relaxed">
                  {isZh ? ins.suggestion_zh : ins.suggestion_en}
                </p>
              </div>
            ))}
          </div>
        ) : (
          <div className="flex items-center gap-2 px-3 py-2 rounded-xl bg-emerald-500/5 border border-emerald-500/20 text-xs text-emerald-400">
            <CheckCircle2 className="w-4 h-4 shrink-0" />
            <span>{t("logs.noInsights")}</span>
          </div>
        )}

        {/* Protocol Latency Breakdown */}
        {telemetry && telemetry.latency_by_protocol.length > 0 && (
          <div className="space-y-1.5 pt-1">
            <span className="text-[11px] font-medium text-muted-foreground">
              {t("logs.protocolStatsTitle")}
            </span>
            <div className="flex flex-wrap gap-2">
              {telemetry.latency_by_protocol.map((p) => (
                <div
                  key={p.protocol}
                  className="px-2.5 py-1.5 rounded-xl bg-secondary/40 border border-border/50 text-[11px] font-mono flex items-center gap-2"
                >
                  <span className="font-semibold uppercase text-foreground">{p.protocol}</span>
                  <span className="text-emerald-400">
                    {p.successes} {t("logs.protoSuccess")}
                  </span>
                  {p.timeouts > 0 && (
                    <span className="text-amber-400">
                      {p.timeouts} {t("logs.protoTimeouts")}
                    </span>
                  )}
                  <span className="text-muted-foreground">avg {p.avg_ms}ms</span>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* Control Bar */}
      <div className="bg-card border border-border rounded-2xl p-4 shadow-sm flex flex-wrap items-center justify-between gap-3">
        <div className="flex flex-wrap items-center gap-3">
          <label className="text-xs text-muted-foreground flex items-center gap-1.5">
            <span>Core:</span>
            <select
              value={selectedCore}
              onChange={(e) => {
                setSelectedCore(e.target.value as CoreType);
                setConfigEdited(false);
              }}
              className="bg-secondary/70 text-foreground text-xs rounded-xl px-2.5 py-1.5 border border-border focus:outline-none"
            >
              <option value="sing-box">sing-box</option>
              <option value="mihomo">mihomo</option>
              <option value="xray">xray</option>
            </select>
          </label>

          <label className="text-xs text-muted-foreground flex items-center gap-1.5">
            <span>Port:</span>
            <input
              type="number"
              min={1}
              max={65535}
              value={mixedPort}
              onChange={(e) => {
                const v = Number(e.target.value);
                if (Number.isInteger(v) && v >= 1 && v <= 65535) {
                  setMixedPort(v);
                }
              }}
              className="w-20 bg-secondary/70 text-foreground text-xs font-mono rounded-xl px-2 py-1.5 border border-border focus:outline-none"
            />
          </label>

          <button
            onClick={() => setShowConfigEditor(!showConfigEditor)}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs bg-secondary hover:bg-secondary/80 text-foreground transition-colors"
          >
            <Code2 className="w-3.5 h-3.5" />
            <span>{showConfigEditor ? "Hide Config" : "Edit Config"}</span>
          </button>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <button
            disabled={busy || isRunning}
            onClick={handleStart}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-medium bg-emerald-600 hover:bg-emerald-500 disabled:opacity-40 text-white transition-colors"
          >
            <Play className="w-3.5 h-3.5" />
            <span>Start</span>
          </button>

          <button
            disabled={busy || !isRunning}
            onClick={handleStop}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-medium bg-red-600 hover:bg-red-500 disabled:opacity-40 text-white transition-colors"
          >
            <Square className="w-3.5 h-3.5" />
            <span>Stop</span>
          </button>

          <button
            disabled={busy}
            onClick={handleRestart}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-medium bg-primary hover:bg-primary/90 disabled:opacity-40 text-primary-foreground transition-colors"
          >
            <RotateCcw className="w-3.5 h-3.5" />
            <span>Restart</span>
          </button>

          <button
            onClick={handleClearLogs}
            className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-xl text-xs font-medium bg-secondary hover:bg-secondary/80 text-muted-foreground hover:text-foreground transition-colors"
          >
            <Trash2 className="w-3.5 h-3.5" />
            <span>Clear</span>
          </button>
        </div>
      </div>

      {errorMsg && (
        <div className="bg-destructive/10 border border-destructive/20 rounded-2xl p-3 text-xs text-destructive font-mono whitespace-pre-wrap">
          {errorMsg}
        </div>
      )}

      {/* Minimal Config Editor */}
      {showConfigEditor && (
        <div className="bg-card border border-border rounded-2xl p-4 shadow-sm space-y-2">
          <div className="flex items-center justify-between">
            <span className="text-xs font-medium text-muted-foreground">
              Direct JSON / YAML Config Editor
            </span>
            <button
              onClick={() => {
                setConfigText(DEFAULT_MINIMAL_SINGBOX_CONFIG);
                setConfigEdited(false);
              }}
              className="text-[11px] text-primary hover:underline"
            >
              Reset to Minimal Default
            </button>
          </div>
          <textarea
            value={configText}
            onChange={(e) => {
              setConfigText(e.target.value);
              setConfigEdited(true);
            }}
            rows={8}
            spellCheck={false}
            className="w-full bg-background border border-border rounded-xl p-3 font-mono text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-primary"
          />
        </div>
      )}

      {/* Filter and Stream Toolbar */}
      <div className="bg-card border border-border rounded-2xl p-3 shadow-sm flex flex-wrap items-center justify-between gap-3 text-xs">
        <div className="flex flex-wrap items-center gap-2 flex-1 min-w-[240px]">
          {/* Mode Switch: Live vs Saved Files */}
          <div className="flex items-center gap-1 p-1 rounded-xl bg-secondary/60 border border-border/50">
            <button
              type="button"
              onClick={() => setViewMode("live")}
              className={`px-2.5 py-1 rounded-lg font-medium transition-all ${
                viewMode === "live"
                  ? "bg-primary text-primary-foreground shadow-sm"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              {t("logs.viewLiveStream")}
            </button>
            <button
              type="button"
              onClick={() => setViewMode("files")}
              className={`px-2.5 py-1 rounded-lg font-medium transition-all ${
                viewMode === "files"
                  ? "bg-primary text-primary-foreground shadow-sm"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              {t("logs.viewSavedFiles")} ({logFiles.length})
            </button>
          </div>

          {viewMode === "live" ? (
            <>
              <div className="relative flex-1 min-w-[160px]">
                <Search className="w-3.5 h-3.5 absolute left-3 top-2.5 text-muted-foreground" />
                <input
                  type="text"
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  placeholder={t("logs.searchPlaceholder")}
                  className="w-full pl-8 pr-3 py-1.5 rounded-xl bg-secondary/50 border border-border text-xs focus:outline-none focus:ring-1 focus:ring-primary"
                />
              </div>

              <select
                value={sourceFilter}
                onChange={(e) => setSourceFilter(e.target.value)}
                className="bg-secondary/70 text-foreground text-xs rounded-xl px-2.5 py-1.5 border border-border focus:outline-none"
              >
                <option value="all">{t("logs.allSources")}</option>
                <option value="cores">All Cores</option>
                <option value="sing-box">sing-box</option>
                <option value="mihomo">mihomo</option>
                <option value="xray">xray</option>
                <option value="app">Rust App</option>
                <option value="system">System</option>
                <option value="frontend">Frontend UI</option>
              </select>

              <select
                value={levelFilter}
                onChange={(e) => setLevelFilter(e.target.value)}
                className="bg-secondary/70 text-foreground text-xs rounded-xl px-2.5 py-1.5 border border-border focus:outline-none"
              >
                <option value="all">All Levels</option>
                <option value="info">Info</option>
                <option value="warn">Warn</option>
                <option value="error">Error</option>
                <option value="debug">Debug</option>
              </select>
            </>
          ) : (
            <div className="flex flex-wrap items-center gap-2 flex-1">
              <FileText className="w-3.5 h-3.5 text-primary" />
              <select
                value={selectedLogFile}
                onChange={(e) => setSelectedLogFile(e.target.value)}
                className="bg-secondary/70 text-foreground text-xs font-mono rounded-xl px-2.5 py-1.5 border border-border focus:outline-none"
              >
                {logFiles.length === 0 ? (
                  <option value="">{t("logs.emptyFile")}</option>
                ) : (
                  logFiles.map((f) => (
                    <option key={f.name} value={f.name}>
                      {f.name} ({formatBytes(f.size_bytes)})
                    </option>
                  ))
                )}
              </select>
              <button
                type="button"
                onClick={() => {
                  if (selectedLogFile) {
                    void readLogFile(selectedLogFile, 500).then(setDiskFileContent);
                  }
                }}
                className="px-2.5 py-1.5 rounded-xl bg-secondary hover:bg-secondary/80 text-xs font-medium"
              >
                {t("common.refresh")}
              </button>
            </div>
          )}
        </div>

        <div className="flex items-center gap-2">
          {viewMode === "live" && (
            <button
              onClick={() => setAutoScroll(!autoScroll)}
              className={`flex items-center gap-1.5 px-3 py-1.5 rounded-xl font-medium transition-colors ${
                autoScroll
                  ? "bg-primary/10 text-primary"
                  : "bg-secondary text-muted-foreground hover:text-foreground"
              }`}
            >
              {autoScroll ? (
                <ArrowDownCircle className="w-3.5 h-3.5" />
              ) : (
                <Pause className="w-3.5 h-3.5" />
              )}
              <span>{autoScroll ? t("logs.autoScroll") : t("logs.pause")}</span>
            </button>
          )}

          <button
            onClick={handleCopyLogs}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-secondary hover:bg-secondary/80 text-muted-foreground hover:text-foreground font-medium transition-colors"
          >
            {copied ? (
              <Check className="w-3.5 h-3.5 text-emerald-500" />
            ) : (
              <Copy className="w-3.5 h-3.5" />
            )}
            <span>{copied ? t("common.copied") : t("logs.copyAll")}</span>
          </button>
        </div>
      </div>

      {/* Console Display */}
      {viewMode === "live" ? (
        <div className="bg-card border border-border rounded-2xl p-4 md:p-5 font-mono text-xs h-96 overflow-y-auto space-y-1.5 select-text shadow-sm">
          <div className="flex items-center justify-between text-primary font-sans font-medium pb-2 mb-2 border-b border-border/60">
            <div className="flex items-center gap-2">
              <Terminal className="w-4 h-4" />
              <span>Unified Log Console ({filteredLogs.length} events)</span>
            </div>
            {telemetry?.log_dir && (
              <span className="text-[10px] font-mono text-muted-foreground truncate max-w-xs">
                {telemetry.log_dir}
              </span>
            )}
          </div>

          {filteredLogs.length === 0 ? (
            <p className="text-muted-foreground/60">
              [System] Console ready. Core stdout/stderr, Rust subsystem traces, and UI errors stream here in real time.
            </p>
          ) : (
            filteredLogs.map((entry) => {
              const sourceTag = entry.source ?? entry.core ?? entry.stream;
              return (
                <div
                  key={entry.seq}
                  className="flex items-start gap-2 leading-relaxed break-all"
                >
                  <span className="text-muted-foreground/60 shrink-0 font-mono text-[11px]">
                    {new Date(entry.timestamp).toLocaleTimeString()}
                  </span>
                  <span
                    className={`px-1.5 py-0.2 rounded text-[10px] uppercase border shrink-0 font-sans font-semibold ${levelBadgeClass(
                      entry.level
                    )}`}
                  >
                    {entry.level}
                  </span>
                  <span className="px-1.5 py-0.2 rounded text-[10px] bg-secondary text-muted-foreground shrink-0 font-mono">
                    {sourceTag}
                  </span>
                  {entry.duration_ms != null && (
                    <span className="text-[10px] text-sky-400 shrink-0 font-mono">
                      {entry.duration_ms}ms
                    </span>
                  )}
                  <span className="text-foreground/90 font-mono text-xs">{entry.message}</span>
                </div>
              );
            })
          )}
          <div ref={logBottomRef} />
        </div>
      ) : (
        <div className="bg-card border border-border rounded-2xl p-4 md:p-5 font-mono text-xs h-96 overflow-y-auto select-text shadow-sm">
          <pre className="whitespace-pre-wrap break-all text-foreground/90 leading-relaxed">
            {diskFileContent || t("logs.emptyFile")}
          </pre>
        </div>
      )}
    </div>
  );
};

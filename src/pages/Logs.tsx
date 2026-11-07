import React, { useEffect, useRef, useState, useMemo } from "react";
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
} from "lucide-react";
import {
  coreClearLogs,
  coreLogs,
  coreRestart,
  coreStart,
  coreStatus,
  coreStop,
  getCoreDefaultConfig,
  onCoreLog,
  onCoreStatusChange,
  type CoreLogEntry,
  type CoreStatusInfo,
  type CoreType,
} from "../lib/ipc";
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
  const { t } = useTranslation();
  const { setIsConnected } = useAppStore();

  const [status, setStatus] = useState<CoreStatusInfo | null>(null);
  const [logs, setLogs] = useState<CoreLogEntry[]>([]);
  const [selectedCore, setSelectedCore] = useState<CoreType>("sing-box");
  const [mixedPort, setMixedPort] = useState<number>(7890);
  const [configText, setConfigText] = useState<string>(
    DEFAULT_MINIMAL_SINGBOX_CONFIG
  );
  const [showConfigEditor, setShowConfigEditor] = useState<boolean>(false);
  const [busy, setBusy] = useState<boolean>(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  // Filters & Controls
  const [searchQuery, setSearchQuery] = useState<string>("");
  const [levelFilter, setLevelFilter] = useState<string>("all");
  const [autoScroll, setAutoScroll] = useState<boolean>(true);
  const [copied, setCopied] = useState<boolean>(false);

  const logBottomRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    coreStatus()
      .then((s) => {
        setStatus(s);
        setIsConnected(s.lifecycle.state === "running");
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

    let unlistenLog: (() => void) | undefined;
    let unlistenStatus: (() => void) | undefined;

    onCoreLog((entry) => {
      setLogs((prev) => {
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
    })
      .then((fn) => {
        unlistenStatus = fn;
      })
      .catch(() => {});

    return () => {
      if (unlistenLog) unlistenLog();
      if (unlistenStatus) unlistenStatus();
    };
  }, [setIsConnected]);

  useEffect(() => {
    if (autoScroll) {
      logBottomRef.current?.scrollIntoView({ behavior: "smooth" });
    }
  }, [logs, autoScroll]);

  const handleStart = async () => {
    setBusy(true);
    setErrorMsg(null);
    try {
      const s = await coreStart({
        core_type: selectedCore,
        config_json: configText,
        mixed_port: mixedPort,
        allow_lan: false,
        auto_port_fallback: true,
      });
      setStatus(s);
      setIsConnected(s.lifecycle.state === "running");
    } catch (err) {
      setErrorMsg(String(err));
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
        config_json: configText,
        mixed_port: mixedPort,
        allow_lan: false,
        auto_port_fallback: true,
      });
      setStatus(s);
      setIsConnected(s.lifecycle.state === "running");
    } catch (err) {
      setErrorMsg(String(err));
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

  const handleCopyLogs = () => {
    const text = filteredLogs
      .map(
        (l) =>
          `[${new Date(l.timestamp).toLocaleTimeString()}] [${l.level.toUpperCase()}] ${l.message}`
      )
      .join("\n");
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  };

  const filteredLogs = useMemo(() => {
    return logs.filter((entry) => {
      if (levelFilter !== "all" && entry.level.toLowerCase() !== levelFilter) {
        return false;
      }
      if (searchQuery.trim()) {
        const q = searchQuery.toLowerCase();
        if (!entry.message.toLowerCase().includes(q)) {
          return false;
        }
      }
      return true;
    });
  }, [logs, levelFilter, searchQuery]);

  const lifecycleState = status?.lifecycle.state ?? "stopped";
  const isRunning = lifecycleState === "running";

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

  return (
    <div className="p-8 max-w-5xl mx-auto space-y-5">
      {/* Header */}
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div>
          <h2 className="text-xl font-bold">{t("logs.title")}</h2>
          <p className="text-xs text-muted-foreground">{t("logs.subtitle")}</p>
        </div>

        <div className="flex items-center gap-2">
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
        </div>
      </div>

      {/* Control Bar */}
      <div className="bg-card border border-border rounded-2xl p-4 shadow-sm flex flex-wrap items-center justify-between gap-3">
        <div className="flex items-center gap-3">
          <label className="text-xs text-muted-foreground flex items-center gap-1.5">
            <span>Core:</span>
            <select
              value={selectedCore}
              onChange={(e) => setSelectedCore(e.target.value as CoreType)}
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
              value={mixedPort}
              onChange={(e) => setMixedPort(Number(e.target.value) || 7890)}
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

        <div className="flex items-center gap-2">
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
              onClick={() => setConfigText(DEFAULT_MINIMAL_SINGBOX_CONFIG)}
              className="text-[11px] text-primary hover:underline"
            >
              Reset to Minimal Default
            </button>
          </div>
          <textarea
            value={configText}
            onChange={(e) => setConfigText(e.target.value)}
            rows={8}
            spellCheck={false}
            className="w-full bg-background border border-border rounded-xl p-3 font-mono text-xs text-foreground focus:outline-none focus:ring-1 focus:ring-primary"
          />
        </div>
      )}

      {/* Filter and Stream Toolbar */}
      <div className="bg-card border border-border rounded-2xl p-3 shadow-sm flex flex-wrap items-center justify-between gap-3 text-xs">
        <div className="flex items-center gap-3 flex-1 min-w-[200px]">
          <div className="relative flex-1">
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
        </div>

        <div className="flex items-center gap-2">
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

      {/* Real-time Log Stream Console */}
      <div className="bg-card border border-border rounded-2xl p-5 font-mono text-xs h-96 overflow-y-auto space-y-1.5 select-text shadow-sm">
        <div className="flex items-center justify-between text-primary font-sans font-medium pb-2 mb-2 border-b border-border/60">
          <div className="flex items-center gap-2">
            <Terminal className="w-4 h-4" />
            <span>Core Log Console ({filteredLogs.length} events)</span>
          </div>
        </div>

        {filteredLogs.length === 0 ? (
          <p className="text-muted-foreground/60">
            [System] Console ready. Logs will appear here in real-time as the core operates.
          </p>
        ) : (
          filteredLogs.map((entry) => (
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
              <span className="text-foreground/90 font-mono text-xs">{entry.message}</span>
            </div>
          ))
        )}
        <div ref={logBottomRef} />
      </div>
    </div>
  );
};

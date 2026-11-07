import React, { useEffect, useState, useMemo } from "react";
import { useTranslation } from "react-i18next";
import {
  Activity,
  Search,
  Trash2,
  X,
  ArrowUp,
  ArrowDown,
  Radio,
} from "lucide-react";
import { useAppStore } from "../stores/appStore";
import {
  getCoreConnections,
  closeCoreConnection,
  type ConnectionItem,
} from "../lib/ipc";
import { formatBytes } from "../lib/utils";

export const Connections: React.FC = () => {
  const { t } = useTranslation();
  const { isConnected } = useAppStore();

  const [connections, setConnections] = useState<ConnectionItem[]>([]);
  const [downloadTotal, setDownloadTotal] = useState<number>(0);
  const [uploadTotal, setUploadTotal] = useState<number>(0);
  const [searchQuery, setSearchQuery] = useState<string>("");
  const [selectedNetwork, setSelectedNetwork] = useState<string>("all");
  const [toastMsg, setToastMsg] = useState<string | null>(null);

  useEffect(() => {
    if (!isConnected) {
      setConnections([]);
      return;
    }

    const fetchConns = () => {
      getCoreConnections()
        .then((report) => {
          setConnections(report.connections || []);
          setDownloadTotal(report.downloadTotal || 0);
          setUploadTotal(report.uploadTotal || 0);
        })
        .catch(() => {});
    };

    fetchConns();
    const interval = setInterval(fetchConns, 1500);
    return () => clearInterval(interval);
  }, [isConnected]);

  const showToast = (msg: string) => {
    setToastMsg(msg);
    setTimeout(() => setToastMsg(null), 2500);
  };

  const handleCloseOne = async (id: string) => {
    try {
      await closeCoreConnection(id);
      setConnections((prev) => prev.filter((c) => c.id !== id));
      showToast("Connection closed");
    } catch {
      // ignore
    }
  };

  const handleCloseAll = async () => {
    try {
      await closeCoreConnection();
      setConnections([]);
      showToast("All connections closed");
    } catch {
      // ignore
    }
  };

  const filtered = useMemo(() => {
    return connections.filter((c) => {
      if (selectedNetwork !== "all") {
        if (c.metadata.network?.toLowerCase() !== selectedNetwork.toLowerCase()) {
          return false;
        }
      }
      if (searchQuery.trim()) {
        const q = searchQuery.toLowerCase();
        const host = (c.metadata.host || c.metadata.destinationIP || "").toLowerCase();
        const rule = (c.rule || "").toLowerCase();
        const chain = (c.chains || []).join(" ").toLowerCase();
        if (!host.includes(q) && !rule.includes(q) && !chain.includes(q)) {
          return false;
        }
      }
      return true;
    });
  }, [connections, selectedNetwork, searchQuery]);

  return (
    <div className="p-8 max-w-5xl mx-auto space-y-6">
      {/* Toast Notification */}
      {toastMsg && (
        <div className="fixed bottom-6 right-6 z-50 px-4 py-2.5 rounded-xl bg-card border border-border text-foreground shadow-2xl text-xs font-medium animate-in fade-in slide-in-from-bottom-2">
          {toastMsg}
        </div>
      )}

      {/* Header */}
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div>
          <h2 className="text-xl font-bold">{t("connections.title")}</h2>
          <p className="text-xs text-muted-foreground">{t("connections.subtitle")}</p>
        </div>

        <div className="flex items-center gap-3">
          <div className="flex items-center gap-4 text-xs font-mono bg-secondary/50 px-4 py-1.5 rounded-xl border border-border/50">
            <span className="flex items-center gap-1 text-sky-400">
              <ArrowDown className="w-3.5 h-3.5" />
              <span>{formatBytes(downloadTotal)}</span>
            </span>
            <span className="flex items-center gap-1 text-emerald-400">
              <ArrowUp className="w-3.5 h-3.5" />
              <span>{formatBytes(uploadTotal)}</span>
            </span>
          </div>

          <button
            disabled={connections.length === 0}
            onClick={handleCloseAll}
            className="flex items-center gap-1.5 px-3 py-1.5 bg-destructive/10 text-destructive text-xs font-medium rounded-xl hover:bg-destructive/20 transition-colors disabled:opacity-50"
          >
            <Trash2 className="w-3.5 h-3.5" />
            <span>{t("connections.closeAll")}</span>
          </button>
        </div>
      </div>

      {/* Filter toolbar */}
      <div className="bg-card border border-border rounded-2xl p-4 shadow-sm flex flex-wrap items-center justify-between gap-3">
        <div className="relative flex-1 min-w-[240px]">
          <Search className="w-4 h-4 absolute left-3 top-2.5 text-muted-foreground" />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder={t("connections.searchPlaceholder")}
            className="w-full pl-9 pr-3 py-1.5 rounded-xl bg-secondary/50 border border-border text-xs focus:outline-none focus:ring-1 focus:ring-primary"
          />
        </div>

        <div className="flex items-center gap-1.5 p-1 bg-secondary/60 rounded-xl border border-border/50">
          <button
            onClick={() => setSelectedNetwork("all")}
            className={`px-3 py-1 text-xs font-medium rounded-lg transition-all ${
              selectedNetwork === "all"
                ? "bg-primary text-primary-foreground shadow-sm"
                : "text-muted-foreground hover:text-foreground"
            }`}
          >
            All ({connections.length})
          </button>
          <button
            onClick={() => setSelectedNetwork("tcp")}
            className={`px-3 py-1 text-xs font-medium rounded-lg transition-all ${
              selectedNetwork === "tcp"
                ? "bg-primary text-primary-foreground shadow-sm"
                : "text-muted-foreground hover:text-foreground"
            }`}
          >
            TCP
          </button>
          <button
            onClick={() => setSelectedNetwork("udp")}
            className={`px-3 py-1 text-xs font-medium rounded-lg transition-all ${
              selectedNetwork === "udp"
                ? "bg-primary text-primary-foreground shadow-sm"
                : "text-muted-foreground hover:text-foreground"
            }`}
          >
            UDP
          </button>
        </div>
      </div>

      {/* Connections List / Table */}
      {!isConnected ? (
        <div className="bg-card border border-border rounded-2xl p-12 text-center text-muted-foreground flex flex-col items-center justify-center">
          <Radio className="w-12 h-12 stroke-[1.5] mb-3 text-muted-foreground/60" />
          <p className="text-sm font-medium">Service Not Connected</p>
          <p className="text-xs text-muted-foreground/80 mt-1">
            Start the core service on Dashboard to track real-time connections.
          </p>
        </div>
      ) : filtered.length === 0 ? (
        <div className="bg-card border border-border rounded-2xl p-12 text-center text-muted-foreground flex flex-col items-center justify-center">
          <Activity className="w-12 h-12 stroke-[1.5] mb-3 text-muted-foreground/60" />
          <p className="text-sm font-medium">No Active Connections</p>
          <p className="text-xs text-muted-foreground/80 mt-1">
            Connections will appear here as network traffic routes through the client.
          </p>
        </div>
      ) : (
        <div className="bg-card border border-border rounded-2xl overflow-hidden shadow-sm">
          <div className="overflow-x-auto">
            <table className="w-full text-left text-xs">
              <thead className="bg-secondary/40 border-b border-border text-muted-foreground uppercase text-[10px] tracking-wider">
                <tr>
                  <th className="py-3 px-4">{t("connections.host")}</th>
                  <th className="py-3 px-3">{t("connections.network")}</th>
                  <th className="py-3 px-3">{t("connections.chains")}</th>
                  <th className="py-3 px-3">{t("connections.rule")}</th>
                  <th className="py-3 px-3">{t("connections.traffic")}</th>
                  <th className="py-3 px-4 text-right">Action</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border/60">
                {filtered.map((c) => {
                  const targetHost = c.metadata.host || c.metadata.destinationIP || "unknown";
                  const targetPort = c.metadata.destinationPort || "";
                  const net = (c.metadata.network || "tcp").toUpperCase();
                  const chain = c.chains && c.chains.length > 0 ? c.chains.join(" → ") : "DIRECT";

                  return (
                    <tr
                      key={c.id}
                      className="hover:bg-secondary/20 transition-colors group"
                    >
                      <td className="py-3 px-4 font-mono font-medium max-w-[260px] truncate" title={targetHost}>
                        <span>{targetHost}</span>
                        {targetPort && (
                          <span className="text-muted-foreground font-normal">:{targetPort}</span>
                        )}
                      </td>
                      <td className="py-3 px-3">
                        <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-secondary text-foreground">
                          {net}
                        </span>
                      </td>
                      <td className="py-3 px-3 text-muted-foreground truncate max-w-[180px]" title={chain}>
                        {chain}
                      </td>
                      <td className="py-3 px-3">
                        <span className="px-2 py-0.5 rounded-full text-[10px] font-semibold bg-primary/10 text-primary">
                          {c.rule || "Match"}
                        </span>
                      </td>
                      <td className="py-3 px-3 font-mono text-[11px] text-muted-foreground">
                        ↓ {formatBytes(c.download)} / ↑ {formatBytes(c.upload)}
                      </td>
                      <td className="py-3 px-4 text-right">
                        <button
                          onClick={() => handleCloseOne(c.id)}
                          title={t("connections.closeConn")}
                          className="p-1 rounded-lg text-muted-foreground hover:text-destructive hover:bg-destructive/10 transition-colors"
                        >
                          <X className="w-3.5 h-3.5" />
                        </button>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </div>
  );
};

import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { X, Sparkles, Plus, AlertCircle } from "lucide-react";
import { parseLink, upsertNode, type ProxyNode } from "../lib/ipc";

interface AddNodeModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSuccess: (node: ProxyNode) => void;
}

export const AddNodeModal: React.FC<AddNodeModalProps> = ({
  isOpen,
  onClose,
  onSuccess,
}) => {
  const { t } = useTranslation();
  const [tab, setTab] = useState<"quick" | "manual">("quick");
  const [shareUri, setShareUri] = useState<string>("");
  const [loading, setLoading] = useState<boolean>(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  // Manual Form States
  const [name, setName] = useState<string>("");
  const [protocol, setProtocol] = useState<string>("shadowsocks");
  const [server, setServer] = useState<string>("");
  const [port, setPort] = useState<number>(8388);
  const [password, setPassword] = useState<string>("");

  if (!isOpen) return null;

  const handleQuickImport = async () => {
    const trimmed = shareUri.trim();
    if (!trimmed) return;
    setLoading(true);
    setErrorMsg(null);
    try {
      const parsed = await parseLink(trimmed);
      const saved = await upsertNode(parsed);
      onSuccess(saved);
      onClose();
    } catch (err) {
      setErrorMsg(String(err));
    } finally {
      setLoading(false);
    }
  };

  const handleManualSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!server.trim() || !port) {
      setErrorMsg("Server and port are required");
      return;
    }
    setLoading(true);
    setErrorMsg(null);
    try {
      const node: ProxyNode = {
        id: `node-${Date.now()}`,
        name: name.trim() || `${protocol.toUpperCase()}-${server}`,
        server: server.trim(),
        port,
        protocol: protocol as any, // eslint-disable-line @typescript-eslint/no-explicit-any
        settings: {
          method: "chacha20-ietf-poly1305",
          password: password || "password",
          plugin: null,
          plugin_opts: null,
          uot: true,
        } as any, // eslint-disable-line @typescript-eslint/no-explicit-any
        tls: null,
        transport: null,
        subscription_id: null,
        group_id: null,
        latency: null,
        dialer_proxy: null,
        preferred_core: null,
        sort_order: 0,
      };
      const saved = await upsertNode(node);
      onSuccess(saved);
      onClose();
    } catch (err) {
      setErrorMsg(String(err));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
      <div className="bg-card border border-border rounded-2xl w-full max-w-md shadow-2xl overflow-hidden animate-in fade-in zoom-in-95 duration-150">
        <div className="flex items-center justify-between px-6 py-4 border-b border-border">
          <h3 className="font-semibold text-sm">{t("proxies.addNode")}</h3>
          <button
            onClick={onClose}
            className="p-1 rounded-lg text-muted-foreground hover:text-foreground hover:bg-secondary/80 transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Tab Switcher */}
        <div className="flex border-b border-border px-6 pt-2">
          <button
            onClick={() => {
              setTab("quick");
              setErrorMsg(null);
            }}
            className={`pb-2.5 px-3 text-xs font-medium border-b-2 transition-all flex items-center gap-1.5 ${
              tab === "quick"
                ? "border-primary text-foreground"
                : "border-transparent text-muted-foreground hover:text-foreground"
            }`}
          >
            <Sparkles className="w-3.5 h-3.5 text-primary" />
            <span>Quick URI Import</span>
          </button>
          <button
            onClick={() => {
              setTab("manual");
              setErrorMsg(null);
            }}
            className={`pb-2.5 px-3 text-xs font-medium border-b-2 transition-all flex items-center gap-1.5 ${
              tab === "manual"
                ? "border-primary text-foreground"
                : "border-transparent text-muted-foreground hover:text-foreground"
            }`}
          >
            <Plus className="w-3.5 h-3.5" />
            <span>Manual Entry</span>
          </button>
        </div>

        <div className="p-6">
          {errorMsg && (
            <div className="mb-4 p-3 rounded-xl bg-destructive/10 border border-destructive/20 text-destructive text-xs flex items-start gap-2">
              <AlertCircle className="w-4 h-4 shrink-0 mt-0.5" />
              <span className="break-all">{errorMsg}</span>
            </div>
          )}

          {tab === "quick" ? (
            <div className="space-y-4">
              <div>
                <label className="block text-xs font-medium text-muted-foreground mb-1.5">
                  {t("proxies.manualImportTitle")}
                </label>
                <textarea
                  rows={4}
                  value={shareUri}
                  onChange={(e) => setShareUri(e.target.value)}
                  placeholder="vless://... or vmess://... or ss://..."
                  className="w-full rounded-xl bg-secondary/50 border border-border p-3 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-primary"
                />
                <p className="text-[11px] text-muted-foreground mt-1">
                  {t("proxies.manualImportHint")}
                </p>
              </div>

              <div className="flex justify-end gap-2 pt-2">
                <button
                  type="button"
                  onClick={onClose}
                  className="px-4 py-2 rounded-xl text-xs font-medium text-muted-foreground hover:bg-secondary transition-colors"
                >
                  {t("common.cancel")}
                </button>
                <button
                  type="button"
                  disabled={loading || !shareUri.trim()}
                  onClick={handleQuickImport}
                  className="px-4 py-2 rounded-xl text-xs font-medium bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50 transition-colors"
                >
                  {loading ? t("common.loading") : t("common.add")}
                </button>
              </div>
            </div>
          ) : (
            <form onSubmit={handleManualSubmit} className="space-y-3.5">
              <div>
                <label className="block text-xs font-medium text-muted-foreground mb-1">
                  Node Name
                </label>
                <input
                  type="text"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder="e.g. Hong Kong 01"
                  className="w-full rounded-xl bg-secondary/50 border border-border px-3 py-2 text-xs focus:outline-none focus:ring-1 focus:ring-primary"
                />
              </div>

              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="block text-xs font-medium text-muted-foreground mb-1">
                    {t("proxies.protocol")}
                  </label>
                  <select
                    value={protocol}
                    onChange={(e) => setProtocol(e.target.value)}
                    className="w-full rounded-xl bg-secondary/50 border border-border px-3 py-2 text-xs focus:outline-none focus:ring-1 focus:ring-primary"
                  >
                    <option value="shadowsocks">Shadowsocks</option>
                    <option value="vless">VLESS</option>
                    <option value="vmess">VMess</option>
                    <option value="trojan">Trojan</option>
                    <option value="hysteria2">Hysteria 2</option>
                    <option value="tuic">TUIC</option>
                    <option value="wireguard">WireGuard</option>
                    <option value="socks5">SOCKS5</option>
                    <option value="http">HTTP</option>
                  </select>
                </div>
                <div>
                  <label className="block text-xs font-medium text-muted-foreground mb-1">
                    {t("proxies.port")}
                  </label>
                  <input
                    type="number"
                    value={port}
                    onChange={(e) => setPort(Number(e.target.value))}
                    className="w-full rounded-xl bg-secondary/50 border border-border px-3 py-2 text-xs focus:outline-none focus:ring-1 focus:ring-primary"
                  />
                </div>
              </div>

              <div>
                <label className="block text-xs font-medium text-muted-foreground mb-1">
                  {t("proxies.server")}
                </label>
                <input
                  type="text"
                  required
                  value={server}
                  onChange={(e) => setServer(e.target.value)}
                  placeholder="domain or IP address"
                  className="w-full rounded-xl bg-secondary/50 border border-border px-3 py-2 text-xs focus:outline-none focus:ring-1 focus:ring-primary"
                />
              </div>

              <div>
                <label className="block text-xs font-medium text-muted-foreground mb-1">
                  Password / UUID
                </label>
                <input
                  type="text"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  placeholder="Enter secret or key"
                  className="w-full rounded-xl bg-secondary/50 border border-border px-3 py-2 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-primary"
                />
              </div>

              <div className="flex justify-end gap-2 pt-3">
                <button
                  type="button"
                  onClick={onClose}
                  className="px-4 py-2 rounded-xl text-xs font-medium text-muted-foreground hover:bg-secondary transition-colors"
                >
                  {t("common.cancel")}
                </button>
                <button
                  type="submit"
                  disabled={loading}
                  className="px-4 py-2 rounded-xl text-xs font-medium bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50 transition-colors"
                >
                  {loading ? t("common.loading") : t("common.add")}
                </button>
              </div>
            </form>
          )}
        </div>
      </div>
    </div>
  );
};

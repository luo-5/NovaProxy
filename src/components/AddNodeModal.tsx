import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { X, Sparkles, Plus, AlertCircle } from "lucide-react";
import {
  parseLink,
  upsertNode,
  type ProtocolConfig,
  type ProtocolType,
  type ProxyNode,
  type TlsConfig,
} from "../lib/ipc";

interface AddNodeModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSuccess: (node: ProxyNode) => void;
}

const DEFAULT_PORTS: Record<string, number> = {
  shadowsocks: 8388,
  vless: 443,
  vmess: 443,
  trojan: 443,
  hysteria2: 443,
  tuic: 443,
  wireguard: 51820,
  socks5: 1080,
  http: 8080,
};

const PROTOCOLS_WITH_DEFAULT_TLS = new Set(["vless", "trojan", "hysteria2", "tuic"]);

function buildDefaultTls(server: string, sni: string, enabled: boolean): TlsConfig | null {
  if (!enabled) return null;
  return {
    enabled: true,
    server_name: sni.trim() || server.trim() || null,
    insecure: false,
    alpn: [],
    fingerprint: "chrome",
    reality: null,
    ech_config: null,
    fragment: null,
  };
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
  const [protocol, setProtocol] = useState<ProtocolType>("shadowsocks");
  const [server, setServer] = useState<string>("");
  const [port, setPort] = useState<string>("8388");
  const [password, setPassword] = useState<string>("");
  const [username, setUsername] = useState<string>("");
  const [ssMethod, setSsMethod] = useState<string>("chacha20-ietf-poly1305");
  const [wgPublicKey, setWgPublicKey] = useState<string>("");
  const [enableTls, setEnableTls] = useState<boolean>(false);
  const [sni, setSni] = useState<string>("");

  if (!isOpen) return null;

  const handleProtocolChange = (nextProto: ProtocolType) => {
    setProtocol(nextProto);
    setPort(String(DEFAULT_PORTS[nextProto] ?? 443));
    setEnableTls(PROTOCOLS_WITH_DEFAULT_TLS.has(nextProto));
    setErrorMsg(null);
  };

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

  const buildProtocolSettings = (secret: string): ProtocolConfig | null => {
    switch (protocol) {
      case "shadowsocks":
        if (!secret) {
          setErrorMsg(t("proxies.secretRequired", { defaultValue: "请输入密码" }));
          return null;
        }
        return {
          kind: "shadowsocks",
          method: ssMethod,
          password: secret,
          plugin: null,
          plugin_opts: null,
          uot: true,
        };
      case "vless":
        if (!secret) {
          setErrorMsg(t("proxies.uuidRequired", { defaultValue: "请输入 UUID" }));
          return null;
        }
        return {
          kind: "vless",
          uuid: secret,
          flow: null,
          encryption: "none",
          packet_encoding: null,
        };
      case "vmess":
        if (!secret) {
          setErrorMsg(t("proxies.uuidRequired", { defaultValue: "请输入 UUID" }));
          return null;
        }
        return {
          kind: "vmess",
          uuid: secret,
          alter_id: 0,
          security: "auto",
          global_padding: null,
          authenticated_length: null,
          packet_encoding: null,
        };
      case "trojan":
        if (!secret) {
          setErrorMsg(t("proxies.secretRequired", { defaultValue: "请输入密码" }));
          return null;
        }
        return { kind: "trojan", password: secret };
      case "hysteria2":
        if (!secret) {
          setErrorMsg(t("proxies.secretRequired", { defaultValue: "请输入密码" }));
          return null;
        }
        return {
          kind: "hysteria2",
          password: secret,
          up_mbps: null,
          down_mbps: null,
          obfs_type: null,
          obfs_password: null,
          ports: null,
          hop_interval: null,
        };
      case "tuic":
        if (!username.trim() || !secret) {
          setErrorMsg(
            t("proxies.tuicAuthRequired", { defaultValue: "TUIC 需要同时填写 UUID 与密码" })
          );
          return null;
        }
        return {
          kind: "tuic",
          uuid: username.trim(),
          password: secret,
          congestion_control: "bbr",
          udp_relay_mode: "native",
          zero_rtt_handshake: false,
          heartbeat: null,
        };
      case "wireguard":
        if (!secret || !wgPublicKey.trim()) {
          setErrorMsg(
            t("proxies.wgKeyRequired", { defaultValue: "WireGuard 需要私钥与对端公钥" })
          );
          return null;
        }
        return {
          kind: "wireguard",
          private_key: secret,
          public_key: wgPublicKey.trim(),
          preshared_key: null,
          local_address: ["10.0.0.2/32"],
          reserved: null,
          mtu: 1420,
          workers: null,
        };
      case "socks5":
        return {
          kind: "socks5",
          username: username.trim() || null,
          password: secret || null,
          udp: true,
        };
      case "http":
        return {
          kind: "http",
          username: username.trim() || null,
          password: secret || null,
          headers: {},
        };
      default:
        setErrorMsg(`Unsupported manual protocol: ${protocol}`);
        return null;
    }
  };

  const handleManualSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const cleanServer = server.trim();
    const parsedPort = Number(port);
    if (
      !cleanServer ||
      !Number.isInteger(parsedPort) ||
      parsedPort < 1 ||
      parsedPort > 65535
    ) {
      setErrorMsg(
        t("proxies.invalidServerPort", { defaultValue: "请填写有效的服务器地址与端口 (1-65535)" })
      );
      return;
    }
    const settings = buildProtocolSettings(password.trim());
    if (!settings) return;

    const tlsForced = protocol === "trojan" || protocol === "hysteria2" || protocol === "tuic";
    const tlsUnsupported =
      protocol === "shadowsocks" || protocol === "wireguard";
    const resolvedTls = tlsUnsupported
      ? null
      : buildDefaultTls(cleanServer, sni, tlsForced || enableTls);

    setLoading(true);
    setErrorMsg(null);
    try {
      const node: ProxyNode = {
        id: `node-${Date.now()}`,
        name: name.trim() || `${protocol.toUpperCase()}-${cleanServer}`,
        server: cleanServer,
        port: parsedPort,
        protocol,
        settings,
        tls: resolvedTls,
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
                    onChange={(e) => handleProtocolChange(e.target.value as ProtocolType)}
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
                    min={1}
                    max={65535}
                    value={port}
                    onChange={(e) => setPort(e.target.value)}
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

              {protocol === "shadowsocks" && (
                <div>
                  <label className="block text-xs font-medium text-muted-foreground mb-1">
                    {t("proxies.cipherMethod", { defaultValue: "加密方式" })}
                  </label>
                  <select
                    value={ssMethod}
                    onChange={(e) => setSsMethod(e.target.value)}
                    className="w-full rounded-xl bg-secondary/50 border border-border px-3 py-2 text-xs focus:outline-none focus:ring-1 focus:ring-primary"
                  >
                    <option value="chacha20-ietf-poly1305">chacha20-ietf-poly1305</option>
                    <option value="aes-256-gcm">aes-256-gcm</option>
                    <option value="aes-128-gcm">aes-128-gcm</option>
                    <option value="2022-blake3-aes-128-gcm">2022-blake3-aes-128-gcm</option>
                    <option value="2022-blake3-aes-256-gcm">2022-blake3-aes-256-gcm</option>
                  </select>
                </div>
              )}

              {(protocol === "tuic" || protocol === "socks5" || protocol === "http") && (
                <div>
                  <label className="block text-xs font-medium text-muted-foreground mb-1">
                    {protocol === "tuic"
                      ? "UUID"
                      : t("proxies.usernameOptional", { defaultValue: "用户名 (可选)" })}
                  </label>
                  <input
                    type="text"
                    value={username}
                    onChange={(e) => setUsername(e.target.value)}
                    placeholder={protocol === "tuic" ? "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx" : ""}
                    className="w-full rounded-xl bg-secondary/50 border border-border px-3 py-2 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-primary"
                  />
                </div>
              )}

              <div>
                <label className="block text-xs font-medium text-muted-foreground mb-1">
                  {protocol === "vless" || protocol === "vmess"
                    ? "UUID"
                    : protocol === "wireguard"
                    ? t("proxies.privateKey", { defaultValue: "私钥 (Private Key)" })
                    : protocol === "socks5" || protocol === "http"
                    ? t("proxies.passwordOptional", { defaultValue: "密码 (可选)" })
                    : t("proxies.passwordLabel", { defaultValue: "密码" })}
                </label>
                <input
                  type="text"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  placeholder={
                    protocol === "vless" || protocol === "vmess"
                      ? "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
                      : "Enter secret or key"
                  }
                  className="w-full rounded-xl bg-secondary/50 border border-border px-3 py-2 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-primary"
                />
              </div>

              {protocol === "wireguard" && (
                <div>
                  <label className="block text-xs font-medium text-muted-foreground mb-1">
                    {t("proxies.publicKey", { defaultValue: "对端公钥 (Peer Public Key)" })}
                  </label>
                  <input
                    type="text"
                    value={wgPublicKey}
                    onChange={(e) => setWgPublicKey(e.target.value)}
                    placeholder="Base64 public key"
                    className="w-full rounded-xl bg-secondary/50 border border-border px-3 py-2 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-primary"
                  />
                </div>
              )}

              {protocol !== "shadowsocks" && protocol !== "wireguard" && (
                <div className="grid grid-cols-2 gap-3 items-center">
                  <label className="flex items-center gap-2 text-xs text-muted-foreground cursor-pointer select-none">
                    <input
                      type="checkbox"
                      checked={
                        protocol === "trojan" || protocol === "hysteria2" || protocol === "tuic"
                          ? true
                          : enableTls
                      }
                      disabled={
                        protocol === "trojan" || protocol === "hysteria2" || protocol === "tuic"
                      }
                      onChange={(e) => setEnableTls(e.target.checked)}
                      className="rounded border-border text-primary focus:ring-0"
                    />
                    <span>TLS</span>
                  </label>
                  {(enableTls ||
                    protocol === "trojan" ||
                    protocol === "hysteria2" ||
                    protocol === "tuic") && (
                    <input
                      type="text"
                      value={sni}
                      onChange={(e) => setSni(e.target.value)}
                      placeholder="SNI (optional)"
                      className="w-full rounded-xl bg-secondary/50 border border-border px-3 py-1.5 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-primary"
                    />
                  )}
                </div>
              )}

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

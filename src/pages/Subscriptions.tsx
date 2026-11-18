import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Rss,
  Plus,
  RefreshCw,
  Clipboard,
  QrCode,
  Trash2,
  Calendar,
  HardDrive,
} from "lucide-react";
import {
  getAppSettings,
  listNodes,
  listSubscriptions,
  deleteSubscription,
  refreshSubscription,
  importClipboardNodes,
  scanScreenQr,
  upsertSubscription,
  type Subscription,
} from "../lib/ipc";
import { useAppStore } from "../stores/appStore";
import { formatBytes, formatRelativeTime } from "../lib/utils";
import { AddSubscriptionModal } from "../components/AddSubscriptionModal";

export const Subscriptions: React.FC = () => {
  const { t } = useTranslation();
  const { activeNode, setActiveNode, setAppSettings, platformInfo } = useAppStore();

  const [subscriptions, setSubscriptions] = useState<Subscription[]>([]);
  const [loading, setLoading] = useState<boolean>(true);
  const [updatingAll, setUpdatingAll] = useState<boolean>(false);
  const [updatingIds, setUpdatingIds] = useState<Set<string>>(new Set());
  const [isAddOpen, setIsAddOpen] = useState<boolean>(false);
  const [toastMsg, setToastMsg] = useState<string | null>(null);
  const toastTimerRef = useRef<number | null>(null);

  const fetchSubs = async () => {
    try {
      const list = await listSubscriptions();
      setSubscriptions(list);
    } catch {
      // ignore
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchSubs();
    const onUpdated = () => {
      fetchSubs();
    };
    window.addEventListener("novaproxy:subscriptions-updated", onUpdated);
    return () => {
      window.removeEventListener("novaproxy:subscriptions-updated", onUpdated);
      if (toastTimerRef.current !== null) {
        window.clearTimeout(toastTimerRef.current);
      }
    };
  }, []);

  const showToast = (msg: string) => {
    setToastMsg(msg);
    if (toastTimerRef.current !== null) {
      window.clearTimeout(toastTimerRef.current);
    }
    toastTimerRef.current = window.setTimeout(() => {
      setToastMsg(null);
      toastTimerRef.current = null;
    }, 3000);
  };

  // Update single subscription
  const handleRefreshSub = async (id: string) => {
    if (updatingIds.has(id)) return;
    setUpdatingIds((prev) => new Set(prev).add(id));
    try {
      const report = await refreshSubscription(id);
      await fetchSubs();
      showToast(t("subscriptions.subUpdatedCount", { count: report.nodes.length }));
    } catch (err) {
      showToast(t("subscriptions.subUpdateFailed", { err: String(err) }));
    } finally {
      setUpdatingIds((prev) => {
        const next = new Set(prev);
        next.delete(id);
        return next;
      });
    }
  };

  // Update all subscriptions
  const handleUpdateAll = async () => {
    if (updatingAll || subscriptions.length === 0) return;
    setUpdatingAll(true);
    let totalImported = 0;
    let failedCount = 0;
    try {
      for (const sub of subscriptions) {
        if (!sub.enabled) continue;
        try {
          const report = await refreshSubscription(sub.id);
          totalImported += report.nodes.length;
        } catch {
          failedCount += 1;
        }
      }
      await fetchSubs();
      showToast(
        t("subscriptions.allSubsUpdated", { count: totalImported, failed: failedCount })
      );
    } finally {
      setUpdatingAll(false);
    }
  };

  // Delete subscription
  const handleDeleteSub = async (id: string) => {
    if (!window.confirm(t("subscriptions.deleteSubConfirm"))) return;
    try {
      await deleteSubscription(id);
      setSubscriptions((prev) => prev.filter((s) => s.id !== id));
      if (activeNode?.subscription_id === id) {
        const [freshSettings, remainingNodes] = await Promise.all([
          getAppSettings().catch(() => null),
          listNodes().catch(() => []),
        ]);
        if (freshSettings) setAppSettings(freshSettings);
        const replacement =
          freshSettings?.active_node_id
            ? remainingNodes.find((n) => n.id === freshSettings.active_node_id) ?? null
            : null;
        setActiveNode(replacement);
      }
      showToast(t("subscriptions.subDeleted"));
    } catch (err) {
      showToast(String(err));
    }
  };

  // Import from clipboard
  const handleImportClipboard = async () => {
    try {
      const text = await navigator.clipboard.readText();
      if (!text.trim()) {
        showToast(t("common.clipboardEmpty"));
        return;
      }
      const report = await importClipboardNodes(text);
      showToast(t("subscriptions.importSuccess", { count: report.nodes.length }));
    } catch (err) {
      showToast(t("subscriptions.importFailed", { err: String(err) }));
    }
  };

  // Scan screen QR
  const handleScanScreenQr = async () => {
    try {
      const found = await scanScreenQr();
      if (found.length === 0) {
        showToast(t("subscriptions.noQrDetected"));
        return;
      }
      let count = 0;
      for (const text of found) {
        const rep = await importClipboardNodes(text).catch(() => null);
        if (rep) count += rep.nodes.length;
      }
      showToast(t("subscriptions.scanComplete", { count }));
    } catch (err) {
      showToast(t("subscriptions.scanFailed", { err: String(err) }));
    }
  };

  // Toggle subscription enabled
  const handleToggleSubEnabled = async (sub: Subscription) => {
    try {
      const updated = { ...sub, enabled: !sub.enabled };
      await upsertSubscription(updated);
      setSubscriptions((prev) =>
        prev.map((s) => (s.id === sub.id ? updated : s))
      );
    } catch (err) {
      showToast(String(err));
    }
  };

  return (
    <div className="p-4 md:p-8 max-w-5xl mx-auto space-y-6">
      {/* Toast Notification */}
      {toastMsg && (
        <div className="fixed bottom-16 md:bottom-6 right-4 md:right-6 z-50 px-4 py-2.5 rounded-xl bg-card border border-border text-foreground shadow-2xl text-xs font-medium animate-in fade-in slide-in-from-bottom-2">
          {toastMsg}
        </div>
      )}

      {/* Header */}
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div>
          <h2 className="text-xl font-bold">{t("subscriptions.title")}</h2>
          <p className="text-xs text-muted-foreground">{t("subscriptions.subtitle")}</p>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <button
            onClick={handleImportClipboard}
            className="flex items-center gap-1.5 px-3 py-1.5 bg-secondary text-secondary-foreground text-xs font-medium rounded-xl hover:bg-secondary/80 transition-colors"
          >
            <Clipboard className="w-3.5 h-3.5" />
            <span>{t("subscriptions.importClipboard")}</span>
          </button>

          {(platformInfo?.supports_screen_qr ?? true) && (
            <button
              onClick={handleScanScreenQr}
              className="flex items-center gap-1.5 px-3 py-1.5 bg-secondary text-secondary-foreground text-xs font-medium rounded-xl hover:bg-secondary/80 transition-colors"
            >
              <QrCode className="w-3.5 h-3.5" />
              <span>{t("subscriptions.scanQr")}</span>
            </button>
          )}

          <button
            disabled={updatingAll || subscriptions.length === 0}
            onClick={handleUpdateAll}
            className="flex items-center gap-1.5 px-3 py-1.5 bg-secondary text-secondary-foreground text-xs font-medium rounded-xl hover:bg-secondary/80 transition-colors disabled:opacity-50"
          >
            <RefreshCw className={`w-3.5 h-3.5 text-primary ${updatingAll ? "animate-spin" : ""}`} />
            <span>{updatingAll ? t("subscriptions.updating") : t("subscriptions.updateAll")}</span>
          </button>

          <button
            onClick={() => setIsAddOpen(true)}
            className="flex items-center gap-1.5 px-3.5 py-1.5 bg-primary text-primary-foreground text-xs font-medium rounded-xl hover:bg-primary/90 transition-colors shadow-sm"
          >
            <Plus className="w-4 h-4" />
            <span>{t("subscriptions.addSub")}</span>
          </button>
        </div>
      </div>

      {/* Subscription Cards */}
      {loading ? (
        <div className="p-12 text-center text-xs text-muted-foreground">
          {t("common.loading")}
        </div>
      ) : subscriptions.length === 0 ? (
        <div className="bg-card border border-border rounded-2xl p-12 text-center text-muted-foreground flex flex-col items-center justify-center">
          <Rss className="w-12 h-12 stroke-[1.5] mb-3 text-muted-foreground/60" />
          <p className="text-sm font-medium">{t("subscriptions.noSubs")}</p>
          <p className="text-xs text-muted-foreground/80 mt-1 max-w-sm">
            {t("subscriptions.noSubsHint")}
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {subscriptions.map((sub) => {
            const isUpdating = updatingIds.has(sub.id);
            const upload = sub.upload ?? 0;
            const download = sub.download ?? 0;
            const total = sub.total ?? 0;
            const used = upload + download;
            const percent = total > 0 ? Math.min(100, Math.round((used / total) * 100)) : 0;

            const expireDate = sub.expire
              ? new Date(sub.expire * 1000).toLocaleDateString()
              : null;

            return (
              <div
                key={sub.id}
                className="bg-card border border-border rounded-2xl p-5 shadow-sm space-y-4 hover:border-primary/40 transition-all"
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="flex items-center gap-3 min-w-0">
                    <div className="w-10 h-10 rounded-xl bg-primary/10 text-primary flex items-center justify-center shrink-0">
                      <Rss className="w-5 h-5" />
                    </div>
                    <div className="min-w-0">
                      <h4 className="font-semibold text-sm truncate" title={sub.name}>
                        {sub.name}
                      </h4>
                      <p className="text-[11px] text-muted-foreground font-mono truncate max-w-[220px]">
                        {sub.url}
                      </p>
                    </div>
                  </div>

                  {/* Toggle enable switch */}
                  <button
                    onClick={() => handleToggleSubEnabled(sub)}
                    className={`w-10 h-5 rounded-full transition-colors relative flex items-center p-0.5 shrink-0 ${
                      sub.enabled ? "bg-primary" : "bg-secondary"
                    }`}
                  >
                    <span
                      className={`w-4 h-4 rounded-full bg-white transition-transform ${
                        sub.enabled ? "translate-x-5" : "translate-x-0"
                      }`}
                    />
                  </button>
                </div>

                {/* Traffic Usage Progress Bar */}
                {total > 0 && (
                  <div className="space-y-1.5 bg-secondary/30 rounded-xl p-3 border border-border/40">
                    <div className="flex items-center justify-between text-[11px]">
                      <span className="text-muted-foreground flex items-center gap-1.5">
                        <HardDrive className="w-3.5 h-3.5" />
                        <span>Traffic Usage</span>
                      </span>
                      <span className="font-mono font-medium">
                        {formatBytes(used)} / {formatBytes(total)} ({percent}%)
                      </span>
                    </div>

                    <div className="w-full h-2 bg-secondary rounded-full overflow-hidden">
                      <div
                        className={`h-full rounded-full transition-all duration-500 ${
                          percent > 90
                            ? "bg-rose-500"
                            : percent > 75
                            ? "bg-amber-500"
                            : "bg-emerald-500"
                        }`}
                        style={{ width: `${percent}%` }}
                      />
                    </div>
                  </div>
                )}

                {/* Sub Metadata Strip */}
                <div className="flex flex-wrap items-center justify-between gap-2 text-[11px] text-muted-foreground pt-1 border-t border-border/40">
                  <div className="flex items-center gap-3">
                    <span>
                      {t("subscriptions.lastUpdated")}:{" "}
                      <strong className="text-foreground font-normal">
                        {formatRelativeTime(sub.updated_at)}
                      </strong>
                    </span>
                    {expireDate && (
                      <span className="flex items-center gap-1">
                        <Calendar className="w-3 h-3 text-muted-foreground/70" />
                        <span>{expireDate}</span>
                      </span>
                    )}
                  </div>

                  <div className="flex items-center gap-1">
                    <button
                      disabled={isUpdating}
                      onClick={() => handleRefreshSub(sub.id)}
                      title="Update Subscription"
                      className="p-1.5 rounded-lg bg-secondary/60 hover:bg-secondary text-muted-foreground hover:text-foreground transition-colors disabled:opacity-50"
                    >
                      <RefreshCw className={`w-3.5 h-3.5 ${isUpdating ? "animate-spin text-primary" : ""}`} />
                    </button>

                    <button
                      onClick={() => handleDeleteSub(sub.id)}
                      title="Delete Subscription"
                      className="p-1.5 rounded-lg bg-destructive/10 hover:bg-destructive/20 text-destructive transition-colors"
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                    </button>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}

      {/* Add Subscription Modal */}
      <AddSubscriptionModal
        isOpen={isAddOpen}
        onClose={() => setIsAddOpen(false)}
        onSuccess={() => {
          fetchSubs();
          showToast("Subscription added successfully");
        }}
      />
    </div>
  );
};

import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { X, Rss, AlertCircle } from "lucide-react";
import { upsertSubscription, refreshSubscription, type Subscription } from "../lib/ipc";

interface AddSubscriptionModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSuccess: (sub: Subscription) => void;
}

export const AddSubscriptionModal: React.FC<AddSubscriptionModalProps> = ({
  isOpen,
  onClose,
  onSuccess,
}) => {
  const { t } = useTranslation();
  const [name, setName] = useState<string>("");
  const [url, setUrl] = useState<string>("");
  const [userAgent, setUserAgent] = useState<string>("clash.meta");
  const [updateIntervalMins, setUpdateIntervalMins] = useState<number>(1440);
  const [loading, setLoading] = useState<boolean>(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  if (!isOpen) return null;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!url.trim()) {
      setErrorMsg("Subscription URL is required");
      return;
    }

    setLoading(true);
    setErrorMsg(null);
    try {
      const sub: Subscription = {
        id: `sub-${Date.now()}`,
        name: name.trim() || `Sub-${new Date().toLocaleDateString()}`,
        url: url.trim(),
        enabled: true,
        user_agent: userAgent.trim() ? userAgent.trim() : null,
        update_interval_mins: updateIntervalMins > 0 ? updateIntervalMins : 1440,
        use_proxy: false,
        updated_at: null,
        upload: null,
        download: null,
        total: null,
        expire: null,
        home_url: null,
        sort_order: 0,
      };

      const saved = await upsertSubscription(sub);
      // Immediately trigger initial refresh in the background
      refreshSubscription(saved.id).catch(() => {});
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
          <div className="flex items-center gap-2">
            <Rss className="w-4 h-4 text-primary" />
            <h3 className="font-semibold text-sm">{t("subscriptions.addSub")}</h3>
          </div>
          <button
            onClick={onClose}
            className="p-1 rounded-lg text-muted-foreground hover:text-foreground hover:bg-secondary/80 transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <form onSubmit={handleSubmit} className="p-6 space-y-4">
          {errorMsg && (
            <div className="p-3 rounded-xl bg-destructive/10 border border-destructive/20 text-destructive text-xs flex items-start gap-2">
              <AlertCircle className="w-4 h-4 shrink-0 mt-0.5" />
              <span className="break-all">{errorMsg}</span>
            </div>
          )}

          <div>
            <label className="block text-xs font-medium text-muted-foreground mb-1">
              {t("subscriptions.subName")}
            </label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. My Provider VIP"
              className="w-full rounded-xl bg-secondary/50 border border-border px-3 py-2 text-xs focus:outline-none focus:ring-1 focus:ring-primary"
            />
          </div>

          <div>
            <label className="block text-xs font-medium text-muted-foreground mb-1">
              {t("subscriptions.subUrl")} *
            </label>
            <input
              type="url"
              required
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="https://example.com/api/v1/client/subscribe?token=..."
              className="w-full rounded-xl bg-secondary/50 border border-border px-3 py-2 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-primary"
            />
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-xs font-medium text-muted-foreground mb-1">
                {t("subscriptions.userAgent")}
              </label>
              <input
                type="text"
                value={userAgent}
                onChange={(e) => setUserAgent(e.target.value)}
                placeholder="clash.meta / sing-box"
                className="w-full rounded-xl bg-secondary/50 border border-border px-3 py-2 text-xs focus:outline-none focus:ring-1 focus:ring-primary"
              />
            </div>
            <div>
              <label className="block text-xs font-medium text-muted-foreground mb-1">
                {t("subscriptions.updateInterval")}
              </label>
              <input
                type="number"
                min={10}
                value={updateIntervalMins}
                onChange={(e) => setUpdateIntervalMins(Number(e.target.value))}
                className="w-full rounded-xl bg-secondary/50 border border-border px-3 py-2 text-xs focus:outline-none focus:ring-1 focus:ring-primary"
              />
            </div>
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
      </div>
    </div>
  );
};

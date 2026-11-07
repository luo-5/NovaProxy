import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { X, Network, AlertCircle } from "lucide-react";
import { upsertRoutingRule, type RuleItem, type RuleType, type RuleAction } from "../lib/ipc";

interface AddRuleModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSuccess: (rule: RuleItem) => void;
}

export const AddRuleModal: React.FC<AddRuleModalProps> = ({
  isOpen,
  onClose,
  onSuccess,
}) => {
  const { t } = useTranslation();
  const [ruleType, setRuleType] = useState<RuleType>("domain_suffix");
  const [rulePattern, setRulePattern] = useState<string>("");
  const [action, setAction] = useState<"Proxy" | "Direct" | "Block">("Proxy");
  const [loading, setLoading] = useState<boolean>(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  if (!isOpen) return null;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!rulePattern.trim()) {
      setErrorMsg("Rule pattern is required");
      return;
    }

    setLoading(true);
    setErrorMsg(null);
    try {
      const ruleAction: RuleAction =
        action === "Proxy"
          ? { type: "proxy" }
          : action === "Direct"
          ? { type: "direct" }
          : { type: "block" };

      const item: RuleItem = {
        id: `rule-${Date.now()}`,
        rule_type: ruleType,
        pattern: rulePattern.trim(),
        action: ruleAction,
        enabled: true,
        description: null,
        sort_order: 0,
      };

      const saved = await upsertRoutingRule(item);
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
            <Network className="w-4 h-4 text-primary" />
            <h3 className="font-semibold text-sm">{t("rules.addRule")}</h3>
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
              {t("rules.ruleType")}
            </label>
            <select
              value={ruleType}
              onChange={(e) => setRuleType(e.target.value as RuleType)}
              className="w-full rounded-xl bg-secondary/50 border border-border px-3 py-2 text-xs focus:outline-none focus:ring-1 focus:ring-primary"
            >
              <option value="domain_suffix">Domain Suffix (e.g. google.com)</option>
              <option value="domain_keyword">Domain Keyword (e.g. netflix)</option>
              <option value="domain">Full Domain (e.g. api.github.com)</option>
              <option value="ip_cidr">IP CIDR (e.g. 1.1.1.1/32, 192.168.0.0/16)</option>
              <option value="geo_site">GeoSite (e.g. cn, geolocation-!cn, telegram)</option>
              <option value="geo_ip">GeoIP (e.g. cn, telegram)</option>
              <option value="port">Port (e.g. 80, 443, 22)</option>
              <option value="process_name">Process Name (e.g. chrome.exe, telegram.exe)</option>
            </select>
          </div>

          <div>
            <label className="block text-xs font-medium text-muted-foreground mb-1">
              {t("rules.ruleValue")} *
            </label>
            <input
              type="text"
              required
              value={rulePattern}
              onChange={(e) => setRulePattern(e.target.value)}
              placeholder="e.g. openai.com or 8.8.8.8/32"
              className="w-full rounded-xl bg-secondary/50 border border-border px-3 py-2 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-primary"
            />
          </div>

          <div>
            <label className="block text-xs font-medium text-muted-foreground mb-1">
              {t("rules.ruleAction")}
            </label>
            <div className="grid grid-cols-3 gap-2">
              <button
                type="button"
                onClick={() => setAction("Proxy")}
                className={`py-2 text-xs font-medium rounded-xl border transition-all ${
                  action === "Proxy"
                    ? "bg-primary text-primary-foreground border-primary"
                    : "bg-secondary/50 text-muted-foreground border-border hover:text-foreground"
                }`}
              >
                {t("rules.actionProxy")}
              </button>
              <button
                type="button"
                onClick={() => setAction("Direct")}
                className={`py-2 text-xs font-medium rounded-xl border transition-all ${
                  action === "Direct"
                    ? "bg-emerald-500 text-white border-emerald-500"
                    : "bg-secondary/50 text-muted-foreground border-border hover:text-foreground"
                }`}
              >
                {t("rules.actionDirect")}
              </button>
              <button
                type="button"
                onClick={() => setAction("Block")}
                className={`py-2 text-xs font-medium rounded-xl border transition-all ${
                  action === "Block"
                    ? "bg-destructive text-destructive-foreground border-destructive"
                    : "bg-secondary/50 text-muted-foreground border-border hover:text-foreground"
                }`}
              >
                {t("rules.actionBlock")}
              </button>
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

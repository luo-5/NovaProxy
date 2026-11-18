import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Plus,
  ArrowUp,
  ArrowDown,
  GripVertical,
  Trash2,
  Database,
  Sliders,
  Check,
} from "lucide-react";
import {
  getRoutingSettings,
  saveRoutingSettings,
  listRoutingRules,
  deleteRoutingRule,
  reorderRoutingRules,
  getDnsSettings,
  saveDnsSettings,
  type RoutingSettings,
  type RuleItem,
  type DnsSettings,
  type DnsStrategy,
} from "../lib/ipc";
import { reorderItemsById, reorderItemsByIndex } from "../lib/listHelpers";
import { AddRuleModal } from "../components/AddRuleModal";

export const Rules: React.FC = () => {
  const { t } = useTranslation();

  const [routingSettings, setRoutingSettings] = useState<RoutingSettings | null>(null);
  const [dnsSettings, setDnsSettings] = useState<DnsSettings | null>(null);
  const [rules, setRules] = useState<RuleItem[]>([]);
  const [activeTab, setActiveTab] = useState<"routing" | "dns">("routing");
  const [isAddOpen, setIsAddOpen] = useState<boolean>(false);
  const [toastMsg, setToastMsg] = useState<string | null>(null);
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [dragOverId, setDragOverId] = useState<string | null>(null);
  const [domesticDnsInput, setDomesticDnsInput] = useState<string>("");
  const [remoteDnsInput, setRemoteDnsInput] = useState<string>("");
  const toastTimerRef = useRef<number | null>(null);

  const fetchAll = async () => {
    try {
      const [rSettings, dSettings, rRules] = await Promise.all([
        getRoutingSettings(),
        getDnsSettings(),
        listRoutingRules(),
      ]);
      setRoutingSettings(rSettings);
      setDnsSettings(dSettings);
      setDomesticDnsInput(dSettings.domestic_dns.join(", "));
      setRemoteDnsInput(dSettings.remote_dns.join(", "));
      setRules(rRules);
    } catch {
      // ignore
    }
  };

  useEffect(() => {
    fetchAll();
    return () => {
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
    }, 2500);
  };

  // Toggle built-in rules
  const handleToggleBuiltin = async (key: "private_direct" | "cn_direct" | "ads_block") => {
    if (!routingSettings) return;
    const updated = { ...routingSettings, [key]: !routingSettings[key] };
    try {
      const saved = await saveRoutingSettings(updated);
      setRoutingSettings(saved);
      showToast(t("rules.routingSaved"));
    } catch (err) {
      showToast(String(err));
    }
  };

  // Persist reordered rules with optimistic update & rollback on failure
  const persistRuleOrder = async (nextRules: RuleItem[], previousRules: RuleItem[]) => {
    setRules(nextRules);
    try {
      await reorderRoutingRules(nextRules.map((r) => r.id));
    } catch (err) {
      setRules(previousRules);
      showToast(String(err));
    }
  };

  // Move rule up or down (keyboard / button accessible fallback)
  const handleMoveRule = async (index: number, direction: "up" | "down") => {
    const targetIdx = direction === "up" ? index - 1 : index + 1;
    if (targetIdx < 0 || targetIdx >= rules.length) return;
    const previous = rules;
    const next = reorderItemsByIndex(rules, index, targetIdx);
    await persistRuleOrder(next, previous);
  };

  // Drag-and-drop drop handler
  const handleDropOnRule = async (targetId: string) => {
    const sourceId = draggingId;
    setDraggingId(null);
    setDragOverId(null);
    if (!sourceId || sourceId === targetId) return;
    const previous = rules;
    const next = reorderItemsById(rules, sourceId, targetId);
    await persistRuleOrder(next, previous);
  };

  // Delete rule
  const handleDeleteRule = async (id: string) => {
    try {
      await deleteRoutingRule(id);
      setRules((prev) => prev.filter((r) => r.id !== id));
      showToast(t("rules.ruleDeleted"));
    } catch (err) {
      showToast(String(err));
    }
  };

  // Save DNS settings
  const handleSaveDns = async (partial: Partial<DnsSettings>) => {
    if (!dnsSettings) return;
    const updated = { ...dnsSettings, ...partial };
    try {
      const saved = await saveDnsSettings(updated);
      setDnsSettings(saved);
      setDomesticDnsInput(saved.domestic_dns.join(", "));
      setRemoteDnsInput(saved.remote_dns.join(", "));
      showToast(t("rules.dnsSaved"));
    } catch (err) {
      showToast(String(err));
    }
  };

  const commitDnsField = (field: "domestic_dns" | "remote_dns") => {
    if (!dnsSettings) return;
    const raw = field === "domestic_dns" ? domesticDnsInput : remoteDnsInput;
    const list = raw
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
    if (list.length === 0) {
      showToast(t("rules.dnsRequiredError"));
      setDomesticDnsInput(dnsSettings.domestic_dns.join(", "));
      setRemoteDnsInput(dnsSettings.remote_dns.join(", "));
      return;
    }
    const prevList = dnsSettings[field];
    if (
      prevList.length === list.length &&
      prevList.every((v, idx) => v === list[idx])
    ) {
      return;
    }
    void handleSaveDns({ [field]: list } as Partial<DnsSettings>);
  };

  return (
    <div className="p-4 md:p-8 max-w-5xl mx-auto space-y-6">
      {/* Toast Notification */}
      {toastMsg && (
        <div className="fixed bottom-6 right-6 z-50 px-4 py-2.5 rounded-xl bg-card border border-border text-foreground shadow-2xl text-xs font-medium animate-in fade-in slide-in-from-bottom-2">
          {toastMsg}
        </div>
      )}

      {/* Header & Tabs */}
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div>
          <h2 className="text-xl font-bold">{t("rules.title")}</h2>
          <p className="text-xs text-muted-foreground">{t("rules.subtitle")}</p>
        </div>

        <div className="flex items-center gap-1.5 p-1 bg-secondary/60 rounded-xl border border-border/50">
          <button
            onClick={() => setActiveTab("routing")}
            className={`px-3 py-1.5 text-xs font-medium rounded-lg transition-all ${
              activeTab === "routing"
                ? "bg-primary text-primary-foreground shadow-sm"
                : "text-muted-foreground hover:text-foreground"
            }`}
          >
            {t("rules.tabRouting")}
          </button>
          <button
            onClick={() => setActiveTab("dns")}
            className={`px-3 py-1.5 text-xs font-medium rounded-lg transition-all ${
              activeTab === "dns"
                ? "bg-primary text-primary-foreground shadow-sm"
                : "text-muted-foreground hover:text-foreground"
            }`}
          >
            {t("rules.tabDns")} & Anti-Leak
          </button>
        </div>
      </div>

      {activeTab === "routing" ? (
        <div className="space-y-6">
          {/* Built-in Presets Card */}
          {routingSettings && (
            <div className="bg-card border border-border rounded-2xl p-6 shadow-sm space-y-4">
              <h3 className="font-semibold text-xs text-foreground uppercase tracking-wider flex items-center gap-2">
                <Sliders className="w-4 h-4 text-primary" />
                <span>{t("rules.systemRules")}</span>
              </h3>

              <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                {/* 1. Bypass Private IPs */}
                <div
                  onClick={() => handleToggleBuiltin("private_direct")}
                  className={`p-4 rounded-xl border cursor-pointer select-none transition-all flex items-start justify-between ${
                    routingSettings.private_direct
                      ? "bg-primary/5 border-primary/40 ring-1 ring-primary/20"
                      : "bg-secondary/30 border-border opacity-70"
                  }`}
                >
                  <div className="space-y-1">
                    <span className="text-xs font-semibold block text-foreground">
                      {t("rules.privateIpBypass")}
                    </span>
                    <span className="text-[11px] text-muted-foreground block">
                      {t("rules.privateIpDesc")}
                    </span>
                  </div>
                  <div
                    className={`w-5 h-5 rounded-md flex items-center justify-center shrink-0 mt-0.5 ${
                      routingSettings.private_direct
                        ? "bg-primary text-primary-foreground"
                        : "bg-secondary border border-border"
                    }`}
                  >
                    {routingSettings.private_direct && <Check className="w-3.5 h-3.5" />}
                  </div>
                </div>

                {/* 2. Direct Mainland */}
                <div
                  onClick={() => handleToggleBuiltin("cn_direct")}
                  className={`p-4 rounded-xl border cursor-pointer select-none transition-all flex items-start justify-between ${
                    routingSettings.cn_direct
                      ? "bg-primary/5 border-primary/40 ring-1 ring-primary/20"
                      : "bg-secondary/30 border-border opacity-70"
                  }`}
                >
                  <div className="space-y-1">
                    <span className="text-xs font-semibold block text-foreground">
                      {t("rules.mainlandDirect")}
                    </span>
                    <span className="text-[11px] text-muted-foreground block">
                      {t("rules.mainlandDirectDesc")}
                    </span>
                  </div>
                  <div
                    className={`w-5 h-5 rounded-md flex items-center justify-center shrink-0 mt-0.5 ${
                      routingSettings.cn_direct
                        ? "bg-primary text-primary-foreground"
                        : "bg-secondary border border-border"
                    }`}
                  >
                    {routingSettings.cn_direct && <Check className="w-3.5 h-3.5" />}
                  </div>
                </div>

                {/* 3. Block Ads */}
                <div
                  onClick={() => handleToggleBuiltin("ads_block")}
                  className={`p-4 rounded-xl border cursor-pointer select-none transition-all flex items-start justify-between ${
                    routingSettings.ads_block
                      ? "bg-primary/5 border-primary/40 ring-1 ring-primary/20"
                      : "bg-secondary/30 border-border opacity-70"
                  }`}
                >
                  <div className="space-y-1">
                    <span className="text-xs font-semibold block text-foreground">
                      {t("rules.adBlock")}
                    </span>
                    <span className="text-[11px] text-muted-foreground block">
                      {t("rules.adBlockDesc")}
                    </span>
                  </div>
                  <div
                    className={`w-5 h-5 rounded-md flex items-center justify-center shrink-0 mt-0.5 ${
                      routingSettings.ads_block
                        ? "bg-primary text-primary-foreground"
                        : "bg-secondary border border-border"
                    }`}
                  >
                    {routingSettings.ads_block && <Check className="w-3.5 h-3.5" />}
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* Custom Rules List */}
          <div className="bg-card border border-border rounded-2xl p-6 shadow-sm space-y-4">
            <div className="flex items-center justify-between">
              <div>
                <h3 className="font-semibold text-sm">{t("rules.customRules")}</h3>
                <p className="text-xs text-muted-foreground">{t("rules.customRulesHint")}</p>
              </div>

              <button
                onClick={() => setIsAddOpen(true)}
                className="flex items-center gap-1.5 px-3 py-1.5 bg-primary text-primary-foreground text-xs font-medium rounded-xl hover:bg-primary/90 transition-colors shadow-sm"
              >
                <Plus className="w-4 h-4" />
                <span>{t("rules.addRule")}</span>
              </button>
            </div>

            {rules.length === 0 ? (
              <div className="p-8 text-center text-xs text-muted-foreground border border-dashed border-border rounded-xl">
                {t("rules.noCustomRules")}
              </div>
            ) : (
              <div className="space-y-2">
                {rules.map((rule, idx) => {
                  const actionType = rule.action.type;
                  const actionStyle =
                    actionType === "proxy"
                      ? "bg-primary/10 text-primary border-primary/20"
                      : actionType === "direct"
                      ? "bg-emerald-500/10 text-emerald-400 border-emerald-500/20"
                      : "bg-rose-500/10 text-rose-400 border-rose-500/20";

                  const isDragging = draggingId === rule.id;
                  const isDragOver = dragOverId === rule.id && draggingId !== rule.id;

                  return (
                    <div
                      key={rule.id}
                      draggable
                      onDragStart={(e) => {
                        setDraggingId(rule.id);
                        e.dataTransfer.effectAllowed = "move";
                        e.dataTransfer.setData("text/plain", rule.id);
                      }}
                      onDragOver={(e) => {
                        e.preventDefault();
                        e.dataTransfer.dropEffect = "move";
                        if (dragOverId !== rule.id) {
                          setDragOverId(rule.id);
                        }
                      }}
                      onDragLeave={() => {
                        if (dragOverId === rule.id) setDragOverId(null);
                      }}
                      onDrop={(e) => {
                        e.preventDefault();
                        handleDropOnRule(rule.id);
                      }}
                      onDragEnd={() => {
                        setDraggingId(null);
                        setDragOverId(null);
                      }}
                      className={`p-3 bg-secondary/30 border rounded-xl flex items-center justify-between gap-4 text-xs group transition-all ${
                        isDragging
                          ? "opacity-50 border-primary/50"
                          : isDragOver
                          ? "border-primary ring-2 ring-primary/30 bg-primary/5"
                          : "border-border/60 hover:border-primary/40"
                      }`}
                    >
                      <div className="flex items-center gap-3 min-w-0">
                        {/* Drag handle & accessible up/down buttons */}
                        <div className="flex items-center gap-1 text-muted-foreground">
                          <span
                            title={t("rules.dragHint", { defaultValue: "拖拽调整优先级顺序" })}
                            className="cursor-grab active:cursor-grabbing p-1 hover:text-foreground"
                          >
                            <GripVertical className="w-3.5 h-3.5" />
                          </span>
                          <button
                            disabled={idx === 0}
                            onClick={() => handleMoveRule(idx, "up")}
                            className="p-1 hover:text-foreground disabled:opacity-30"
                          >
                            <ArrowUp className="w-3.5 h-3.5" />
                          </button>
                          <button
                            disabled={idx === rules.length - 1}
                            onClick={() => handleMoveRule(idx, "down")}
                            className="p-1 hover:text-foreground disabled:opacity-30"
                          >
                            <ArrowDown className="w-3.5 h-3.5" />
                          </button>
                        </div>

                        {/* Rule Type */}
                        <span className="font-mono text-[11px] px-2 py-0.5 rounded-md bg-secondary text-foreground shrink-0 border border-border">
                          {rule.rule_type}
                        </span>

                        {/* Match Pattern */}
                        <span className="font-mono text-xs font-medium text-foreground truncate" title={rule.pattern}>
                          {rule.pattern}
                        </span>
                      </div>

                      <div className="flex items-center gap-3 shrink-0">
                        {/* Action Badge */}
                        <span className={`px-2.5 py-0.5 rounded-full text-[11px] font-semibold border capitalize ${actionStyle}`}>
                          {actionType}
                        </span>

                        {/* Delete button */}
                        <button
                          onClick={() => handleDeleteRule(rule.id)}
                          className="p-1 rounded-lg text-muted-foreground hover:text-destructive hover:bg-destructive/10 transition-colors"
                        >
                          <Trash2 className="w-3.5 h-3.5" />
                        </button>
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        </div>
      ) : (
        /* DNS & Anti-Leak Tab */
        dnsSettings && (
          <div className="bg-card border border-border rounded-2xl p-6 shadow-sm space-y-6">
            <h3 className="font-semibold text-sm flex items-center gap-2">
              <Database className="w-4 h-4 text-primary" />
              <span>{t("rules.dnsTitle")}</span>
            </h3>

            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div>
                <label className="block text-xs font-medium text-muted-foreground mb-1">
                  {t("rules.dnsStrategy")}
                </label>
                <select
                  value={dnsSettings.strategy}
                  onChange={(e) =>
                    handleSaveDns({ strategy: e.target.value as DnsStrategy })
                  }
                  className="w-full px-3 py-2 rounded-xl bg-secondary/50 border border-border text-xs focus:outline-none focus:ring-1 focus:ring-primary"
                >
                  <option value="prefer_ipv4">Prefer IPv4 (Recommended)</option>
                  <option value="ipv4_only">IPv4 Only</option>
                  <option value="prefer_ipv6">Prefer IPv6</option>
                  <option value="ipv6_only">IPv6 Only</option>
                </select>
              </div>

              <div>
                <label className="block text-xs font-medium text-muted-foreground mb-1">
                  {t("rules.domesticDns")}
                </label>
                <input
                  type="text"
                  value={domesticDnsInput}
                  onChange={(e) => setDomesticDnsInput(e.target.value)}
                  onBlur={() => commitDnsField("domestic_dns")}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.currentTarget.blur();
                    }
                  }}
                  placeholder="223.5.5.5, 119.29.29.29"
                  className="w-full px-3 py-2 rounded-xl bg-secondary/50 border border-border text-xs font-mono focus:outline-none focus:ring-1 focus:ring-primary"
                />
              </div>

              <div className="md:col-span-2">
                <label className="block text-xs font-medium text-muted-foreground mb-1">
                  {t("rules.remoteDns")}
                </label>
                <input
                  type="text"
                  value={remoteDnsInput}
                  onChange={(e) => setRemoteDnsInput(e.target.value)}
                  onBlur={() => commitDnsField("remote_dns")}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.currentTarget.blur();
                    }
                  }}
                  placeholder="https://1.1.1.1/dns-query, https://dns.google/dns-query"
                  className="w-full px-3 py-2 rounded-xl bg-secondary/50 border border-border text-xs font-mono focus:outline-none focus:ring-1 focus:ring-primary"
                />
              </div>
            </div>

            {/* Fake-IP Switch */}
            <div className="p-4 rounded-xl bg-secondary/30 border border-border flex items-center justify-between">
              <div>
                <span className="text-xs font-semibold block">{t("rules.fakeIp")}</span>
                <span className="text-[11px] text-muted-foreground">
                  Address Range: {dnsSettings.fake_ip_range}
                </span>
              </div>
              <button
                onClick={() => handleSaveDns({ fake_ip: !dnsSettings.fake_ip })}
                className={`w-11 h-6 rounded-full transition-colors relative flex items-center p-0.5 ${
                  dnsSettings.fake_ip ? "bg-primary" : "bg-secondary"
                }`}
              >
                <span
                  className={`w-5 h-5 rounded-full bg-white transition-transform ${
                    dnsSettings.fake_ip ? "translate-x-5" : "translate-x-0"
                  }`}
                />
              </button>
            </div>
          </div>
        )
      )}

      {/* Add Rule Modal */}
      <AddRuleModal
        isOpen={isAddOpen}
        onClose={() => setIsAddOpen(false)}
        onSuccess={() => {
          fetchAll();
          showToast("Rule added successfully");
        }}
      />
    </div>
  );
};

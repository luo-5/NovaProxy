import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Cpu,
  Shield,
  ExternalLink,
  Globe,
  Sun,
  Moon,
  Info,
} from "lucide-react";
import { useAppStore } from "../stores/appStore";
import {
  getAppVersion,
  getAppSettings,
  saveAppSettings,
  getAutostartStatus,
  setAutostartStatus,
  isProcessElevated,
  requestAdminRelaunch,
  type AppSettings,
  type CoreType,
} from "../lib/ipc";

export const Settings: React.FC = () => {
  const { t, i18n } = useTranslation();
  const { theme, setTheme, appSettings, setAppSettings } = useAppStore();

  const [version, setVersion] = useState<string>("0.1.0");
  const [autostart, setAutostart] = useState<boolean>(false);
  const [isElevated, setIsElevated] = useState<boolean>(false);
  const [toastMsg, setToastMsg] = useState<string | null>(null);

  // Form fields
  const [preferredCore, setPreferredCore] = useState<CoreType>("sing-box");
  const [mixedPort, setMixedPort] = useState<number>(7890);
  const [allowLan, setAllowLan] = useState<boolean>(false);
  const [autoPortFallback, setAutoPortFallback] = useState<boolean>(true);
  const [clashApiPort, setClashApiPort] = useState<number>(9090);

  useEffect(() => {
    getAppVersion()
      .then((ver) => setVersion(ver))
      .catch(() => setVersion("0.1.0"));

    getAutostartStatus()
      .then(setAutostart)
      .catch(() => {});

    isProcessElevated()
      .then(setIsElevated)
      .catch(() => {});

    getAppSettings()
      .then((s) => {
        setAppSettings(s);
        if (s.preferred_core) setPreferredCore(s.preferred_core);
        setMixedPort(s.mixed_port);
        setAllowLan(s.allow_lan);
        setAutoPortFallback(s.auto_port_fallback);
        setClashApiPort(s.clash_api_port);
      })
      .catch(() => {});
  }, [setAppSettings]);

  const showToast = (msg: string) => {
    setToastMsg(msg);
    setTimeout(() => setToastMsg(null), 2500);
  };

  const handleSaveSettings = async (partial: Partial<AppSettings>) => {
    if (!appSettings) return;
    const updated = { ...appSettings, ...partial };
    try {
      const saved = await saveAppSettings(updated);
      setAppSettings(saved);
      showToast(t("common.success"));
    } catch (err) {
      showToast(String(err));
    }
  };

  const handleToggleAutostart = async () => {
    try {
      const target = !autostart;
      const res = await setAutostartStatus(target);
      setAutostart(res);
      showToast(res ? "Autostart enabled" : "Autostart disabled");
    } catch (err) {
      showToast(String(err));
    }
  };

  const handleLanguageChange = (lang: string) => {
    i18n.changeLanguage(lang);
    showToast(`Language switched to ${lang === "zh-CN" ? "中文" : "English"}`);
  };

  return (
    <div className="p-8 max-w-5xl mx-auto space-y-6">
      {/* Toast Notification */}
      {toastMsg && (
        <div className="fixed bottom-6 right-6 z-50 px-4 py-2.5 rounded-xl bg-card border border-border text-foreground shadow-2xl text-xs font-medium animate-in fade-in slide-in-from-bottom-2">
          {toastMsg}
        </div>
      )}

      <div>
        <h2 className="text-xl font-bold">{t("settings.title")}</h2>
        <p className="text-xs text-muted-foreground">{t("settings.subtitle")}</p>
      </div>

      {/* 1. Core & Network Configuration */}
      <div className="bg-card border border-border rounded-2xl p-6 shadow-sm space-y-5">
        <h3 className="font-semibold text-xs text-foreground uppercase tracking-wider flex items-center gap-2">
          <Cpu className="w-4 h-4 text-primary" />
          <span>{t("settings.coreSettings")}</span>
        </h3>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div>
            <label className="block text-xs font-medium text-muted-foreground mb-1">
              {t("settings.preferredCore")}
            </label>
            <select
              value={preferredCore}
              onChange={(e) => {
                const core = e.target.value as CoreType;
                setPreferredCore(core);
                handleSaveSettings({ preferred_core: core });
              }}
              className="w-full px-3 py-2 rounded-xl bg-secondary/50 border border-border text-xs focus:outline-none focus:ring-1 focus:ring-primary"
            >
              <option value="sing-box">sing-box 1.14 (Primary / Recommended)</option>
              <option value="mihomo">Mihomo 1.19 (Clash.Meta)</option>
              <option value="xray">Xray-core 26.10</option>
            </select>
          </div>

          <div>
            <label className="block text-xs font-medium text-muted-foreground mb-1">
              {t("settings.mixedPort")}
            </label>
            <input
              type="number"
              value={mixedPort}
              onChange={(e) => {
                const p = Number(e.target.value);
                setMixedPort(p);
                handleSaveSettings({ mixed_port: p });
              }}
              className="w-full px-3 py-2 rounded-xl bg-secondary/50 border border-border text-xs font-mono focus:outline-none focus:ring-1 focus:ring-primary"
            />
          </div>

          <div>
            <label className="block text-xs font-medium text-muted-foreground mb-1">
              {t("settings.clashApiPort")}
            </label>
            <input
              type="number"
              value={clashApiPort}
              onChange={(e) => {
                const p = Number(e.target.value);
                setClashApiPort(p);
                handleSaveSettings({ clash_api_port: p });
              }}
              className="w-full px-3 py-2 rounded-xl bg-secondary/50 border border-border text-xs font-mono focus:outline-none focus:ring-1 focus:ring-primary"
            />
          </div>

          <div className="flex flex-col justify-end space-y-2">
            <label className="flex items-center gap-2.5 text-xs cursor-pointer select-none">
              <input
                type="checkbox"
                checked={allowLan}
                onChange={(e) => {
                  setAllowLan(e.target.checked);
                  handleSaveSettings({ allow_lan: e.target.checked });
                }}
                className="rounded border-border text-primary focus:ring-0"
              />
              <span>{t("settings.allowLan")}</span>
            </label>

            <label className="flex items-center gap-2.5 text-xs cursor-pointer select-none">
              <input
                type="checkbox"
                checked={autoPortFallback}
                onChange={(e) => {
                  setAutoPortFallback(e.target.checked);
                  handleSaveSettings({ auto_port_fallback: e.target.checked });
                }}
                className="rounded border-border text-primary focus:ring-0"
              />
              <span>{t("settings.autoPortFallback")}</span>
            </label>
          </div>
        </div>
      </div>

      {/* 2. System & Autostart */}
      <div className="bg-card border border-border rounded-2xl p-6 shadow-sm space-y-4">
        <h3 className="font-semibold text-xs text-foreground uppercase tracking-wider flex items-center gap-2">
          <Shield className="w-4 h-4 text-primary" />
          <span>{t("settings.systemSettings")}</span>
        </h3>

        <div className="space-y-3">
          <div className="p-4 rounded-xl bg-secondary/30 border border-border flex items-center justify-between">
            <div>
              <span className="text-xs font-semibold block">{t("settings.autostart")}</span>
              <span className="text-[11px] text-muted-foreground">
                HKCU\Software\Microsoft\Windows\CurrentVersion\Run
              </span>
            </div>
            <button
              onClick={handleToggleAutostart}
              className={`w-11 h-6 rounded-full transition-colors relative flex items-center p-0.5 ${
                autostart ? "bg-primary" : "bg-secondary"
              }`}
            >
              <span
                className={`w-5 h-5 rounded-full bg-white transition-transform ${
                  autostart ? "translate-x-5" : "translate-x-0"
                }`}
              />
            </button>
          </div>

          <div className="p-4 rounded-xl bg-secondary/30 border border-border flex items-center justify-between">
            <div>
              <div className="flex items-center gap-2">
                <span className="text-xs font-semibold block">UAC Administrator Elevation</span>
                <span
                  className={`text-[10px] px-2 py-0.5 rounded-full font-semibold ${
                    isElevated
                      ? "bg-emerald-500/10 text-emerald-400"
                      : "bg-amber-500/10 text-amber-500"
                  }`}
                >
                  {isElevated ? t("settings.isElevated") : "Standard User"}
                </span>
              </div>
              <p className="text-[11px] text-muted-foreground mt-0.5">
                {isElevated ? "Full privileges granted" : t("settings.notElevated")}
              </p>
            </div>

            {!isElevated && (
              <button
                onClick={() => requestAdminRelaunch().catch(() => {})}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-amber-500/10 hover:bg-amber-500/20 text-amber-500 text-xs font-medium transition-colors"
              >
                <ExternalLink className="w-3.5 h-3.5" />
                <span>{t("settings.runAsAdmin")}</span>
              </button>
            )}
          </div>
        </div>
      </div>

      {/* 3. Appearance & Language */}
      <div className="bg-card border border-border rounded-2xl p-6 shadow-sm space-y-4">
        <h3 className="font-semibold text-xs text-foreground uppercase tracking-wider flex items-center gap-2">
          <Globe className="w-4 h-4 text-primary" />
          <span>{t("settings.appearance")}</span>
        </h3>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div className="p-4 rounded-xl bg-secondary/30 border border-border flex items-center justify-between">
            <div className="flex items-center gap-3">
              {theme === "dark" ? (
                <Moon className="w-5 h-5 text-indigo-400" />
              ) : (
                <Sun className="w-5 h-5 text-amber-400" />
              )}
              <div>
                <span className="text-xs font-semibold block">{t("settings.theme")}</span>
                <span className="text-[11px] text-muted-foreground capitalize">{theme}</span>
              </div>
            </div>
            <button
              onClick={() => setTheme(theme === "dark" ? "light" : "dark")}
              className="px-3 py-1.5 rounded-xl bg-secondary hover:bg-secondary/80 text-xs font-medium transition-colors"
            >
              Toggle
            </button>
          </div>

          <div className="p-4 rounded-xl bg-secondary/30 border border-border flex items-center justify-between">
            <div className="flex items-center gap-3">
              <Globe className="w-5 h-5 text-primary" />
              <div>
                <span className="text-xs font-semibold block">{t("settings.language")}</span>
                <span className="text-[11px] text-muted-foreground">
                  {i18n.language === "zh-CN" ? "简体中文" : "English"}
                </span>
              </div>
            </div>
            <div className="flex items-center gap-1 p-1 bg-secondary rounded-xl">
              <button
                onClick={() => handleLanguageChange("zh-CN")}
                className={`px-2.5 py-1 text-xs rounded-lg transition-all ${
                  i18n.language === "zh-CN"
                    ? "bg-primary text-primary-foreground shadow-sm"
                    : "text-muted-foreground"
                }`}
              >
                中文
              </button>
              <button
                onClick={() => handleLanguageChange("en-US")}
                className={`px-2.5 py-1 text-xs rounded-lg transition-all ${
                  i18n.language === "en-US"
                    ? "bg-primary text-primary-foreground shadow-sm"
                    : "text-muted-foreground"
                }`}
              >
                EN
              </button>
            </div>
          </div>
        </div>
      </div>

      {/* 4. About & Version */}
      <div className="bg-card border border-border rounded-2xl p-6 shadow-sm space-y-4">
        <h3 className="font-semibold text-xs text-foreground uppercase tracking-wider flex items-center gap-2">
          <Info className="w-4 h-4 text-primary" />
          <span>{t("settings.about")}</span>
        </h3>

        <div className="flex flex-wrap items-center justify-between gap-3 text-xs">
          <div>
            <p className="font-semibold text-foreground">NovaProxy Windows Desktop</p>
            <p className="text-[11px] text-muted-foreground mt-0.5">
              Powered by sing-box (v1.14), mihomo (v1.19), and Xray-core (v26.10)
            </p>
          </div>
          <span className="font-mono text-xs px-3 py-1 rounded-xl bg-secondary text-foreground">
            v{version}
          </span>
        </div>
      </div>
    </div>
  );
};

import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Cpu,
  Shield,
  ShieldAlert,
  ExternalLink,
  Globe,
  Sun,
  Moon,
  Info,
  Check,
  Copy,
  RefreshCw,
  Download,
  Link2,
  CheckCircle2,
  HardDrive,
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
  checkDatabaseIntegrity,
  exportDiagnosticBundle,
  exportDiagnosticsReport,
  getCoreVersions,
  checkCoreUpdate,
  installCoreUpdate,
  checkWintunStatus,
  getServiceStatus,
  getUrlSchemeStatus,
  setClashUrlScheme,
  type AppSettings,
  type CoreBinaryStatus,
  type CoreType,
  type CoreUpdateCheckResult,
  type ServiceState,
  type UrlSchemeStatus,
  type WintunStatus,
} from "../lib/ipc";

export const Settings: React.FC = () => {
  const { t, i18n } = useTranslation();
  const { theme, setTheme, appSettings, setAppSettings, platformInfo } = useAppStore();

  const [version, setVersion] = useState<string>("0.0.1");
  const [autostart, setAutostart] = useState<boolean>(false);
  const [isElevated, setIsElevated] = useState<boolean>(false);
  const [toastMsg, setToastMsg] = useState<string | null>(null);

  // Form fields
  const [preferredCore, setPreferredCore] = useState<CoreType>("sing-box");
  const [mixedPortText, setMixedPortText] = useState<string>("7890");
  const [allowLan, setAllowLan] = useState<boolean>(false);
  const [autoPortFallback, setAutoPortFallback] = useState<boolean>(true);
  const [clashApiPortText, setClashApiPortText] = useState<string>("9090");
  const toastTimerRef = useRef<number | null>(null);

  // Core Updater & System Readiness state
  const [coreStatuses, setCoreStatuses] = useState<CoreBinaryStatus[]>([]);
  const [updateChecks, setUpdateChecks] = useState<Record<string, CoreUpdateCheckResult>>({});
  const [checkingCore, setCheckingCore] = useState<CoreType | null>(null);
  const [installingCore, setInstallingCore] = useState<CoreType | null>(null);
  const [updateViaProxy, setUpdateViaProxy] = useState<boolean>(true);
  const [wintunStatus, setWintunStatus] = useState<WintunStatus | null>(null);
  const [serviceState, setServiceState] = useState<ServiceState>("not_installed");
  const [urlSchemeStatus, setUrlSchemeStatus] = useState<UrlSchemeStatus | null>(null);

  const refreshSystemReadiness = async () => {
    const [coresRes, wintunRes, svcRes, schemeRes] = await Promise.allSettled([
      getCoreVersions(),
      checkWintunStatus(),
      getServiceStatus(),
      getUrlSchemeStatus(),
    ]);
    if (coresRes.status === "fulfilled") setCoreStatuses(coresRes.value);
    if (wintunRes.status === "fulfilled") setWintunStatus(wintunRes.value);
    if (svcRes.status === "fulfilled") setServiceState(svcRes.value);
    if (schemeRes.status === "fulfilled") setUrlSchemeStatus(schemeRes.value);
  };

  useEffect(() => {
    getAppVersion()
      .then((ver) => setVersion(ver))
      .catch(() => setVersion("0.0.1"));

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
        setMixedPortText(String(s.mixed_port));
        setAllowLan(s.allow_lan);
        setAutoPortFallback(s.auto_port_fallback);
        setClashApiPortText(String(s.clash_api_port));
      })
      .catch(() => {});

    refreshSystemReadiness().catch(() => {});

    return () => {
      if (toastTimerRef.current !== null) {
        window.clearTimeout(toastTimerRef.current);
      }
    };
  }, [setAppSettings]);

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

  const handleSaveSettings = async (partial: Partial<AppSettings>, toastOverride?: string) => {
    if (!appSettings) return;
    const updated = { ...appSettings, ...partial };
    try {
      const saved = await saveAppSettings(updated);
      setAppSettings(saved);
      showToast(toastOverride ?? t("common.success"));
    } catch (err) {
      showToast(String(err));
    }
  };

  const commitPortField = (field: "mixed_port" | "clash_api_port") => {
    if (!appSettings) return;
    const raw = field === "mixed_port" ? mixedPortText.trim() : clashApiPortText.trim();
    const parsed = Number(raw);
    const otherPort =
      field === "mixed_port" ? appSettings.clash_api_port : appSettings.mixed_port;

    if (!Number.isInteger(parsed) || parsed < 1 || parsed > 65535 || parsed === otherPort) {
      showToast(t("settings.invalidPortError"));
      setMixedPortText(String(appSettings.mixed_port));
      setClashApiPortText(String(appSettings.clash_api_port));
      return;
    }

    if (appSettings[field] === parsed) return;
    void handleSaveSettings(
      { [field]: parsed } as Partial<AppSettings>,
      t("settings.restartNeededHint")
    );
  };

  const handleToggleAutostart = async () => {
    try {
      const target = !autostart;
      const res = await setAutostartStatus(target);
      setAutostart(res);
      showToast(res ? t("settings.autostartEnabled") : t("settings.autostartDisabled"));
    } catch (err) {
      showToast(String(err));
    }
  };

  const handleLanguageChange = (lang: string) => {
    i18n.changeLanguage(lang);
    showToast(
      t("settings.languageSwitched", { lang: lang === "zh-CN" ? "中文" : "English" })
    );
  };

  const handleCheckCoreUpdate = async (core: CoreType) => {
    if (checkingCore || installingCore) return;
    setCheckingCore(core);
    try {
      const res = await checkCoreUpdate(core, updateViaProxy);
      setUpdateChecks((prev) => ({ ...prev, [core]: res }));
      showToast(
        res.has_update
          ? t("settings.updateAvailable", {
              core,
              version: res.latest_version,
              defaultValue: `${core} 发现新版本: ${res.latest_version}`,
            })
          : t("settings.coreUpToDate", {
              core,
              defaultValue: `${core} 已是最新版本 (${res.latest_version})`,
            })
      );
    } catch (err) {
      showToast(String(err));
    } finally {
      setCheckingCore(null);
    }
  };

  const handleInstallCoreUpdate = async (core: CoreType) => {
    if (installingCore || checkingCore) return;
    setInstallingCore(core);
    try {
      const res = await installCoreUpdate(core, updateViaProxy);
      showToast(
        t("settings.updateInstalled", {
          core,
          version: res.new_version,
          defaultValue: `${core} 已安全更新至 ${res.new_version}`,
        })
      );
      await refreshSystemReadiness();
      setUpdateChecks((prev) => {
        const copy = { ...prev };
        delete copy[core];
        return copy;
      });
    } catch (err) {
      showToast(String(err));
    } finally {
      setInstallingCore(null);
    }
  };

  const handleToggleClashScheme = async () => {
    const next = !(urlSchemeStatus?.clash_registered_to_us ?? false);
    try {
      const updated = await setClashUrlScheme(next);
      setUrlSchemeStatus(updated);
      if (appSettings) {
        const saved = await saveAppSettings({
          ...appSettings,
          register_clash_scheme: updated.clash_registered_to_us,
        });
        setAppSettings(saved);
      }
      showToast(
        updated.clash_registered_to_us
          ? t("settings.clashSchemeEnabled")
          : t("settings.clashSchemeDisabled")
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
              min={1}
              max={65535}
              value={mixedPortText}
              onChange={(e) => setMixedPortText(e.target.value)}
              onBlur={() => commitPortField("mixed_port")}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.currentTarget.blur();
                }
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
              min={1}
              max={65535}
              value={clashApiPortText}
              onChange={(e) => setClashApiPortText(e.target.value)}
              onBlur={() => commitPortField("clash_api_port")}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.currentTarget.blur();
                }
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

        {platformInfo?.is_mobile ? (
          <div className="p-4 rounded-xl bg-secondary/30 border border-border text-xs text-muted-foreground space-y-2">
            <p>{t("settings.mobileSystemManagedHint")}</p>
            <p className="text-[11px] font-mono text-foreground/80">
              {wintunStatus?.message ?? "Android VpnService + hev-socks5-tunnel"}
            </p>
          </div>
        ) : (
          <div className="space-y-3">
            {(platformInfo?.supports_autostart ?? true) && (
              <div className="p-4 rounded-xl bg-secondary/30 border border-border flex items-center justify-between">
                <div>
                  <span className="text-xs font-semibold block">{t("settings.autostart")}</span>
                  <span className="text-[11px] text-muted-foreground">
                    {platformInfo?.os === "macos"
                      ? "LaunchAgents (com.novaproxy.client)"
                      : platformInfo?.os === "linux"
                      ? "XDG Autostart (~/.config/autostart/novaproxy.desktop)"
                      : "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run"}
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
            )}

            <div className="p-4 rounded-xl bg-secondary/30 border border-border flex items-center justify-between">
              <div>
                <div className="flex items-center gap-2">
                  <span className="text-xs font-semibold block">
                    {platformInfo?.os === "windows" || !platformInfo
                      ? "UAC Administrator Elevation"
                      : "TUN Privilege Elevation"}
                  </span>
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

            {/* TUN & Helper Service Readiness */}
            <div className="p-4 rounded-xl bg-secondary/30 border border-border flex flex-wrap items-center justify-between gap-3">
              <div className="space-y-1">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="text-xs font-semibold">{t("settings.tunReadinessTitle")}</span>
                  <span
                    className={`text-[10px] px-2 py-0.5 rounded-full font-semibold ${
                      wintunStatus?.ready
                        ? "bg-emerald-500/10 text-emerald-400"
                        : "bg-amber-500/10 text-amber-500"
                    }`}
                  >
                    {wintunStatus?.ready ? t("settings.tunReady") : t("settings.tunNeedsElevation")}
                  </span>
                  {(!platformInfo || platformInfo.os === "windows") && (
                    <span className="text-[10px] px-2 py-0.5 rounded-full bg-secondary font-mono text-muted-foreground">
                      wintun.dll: {wintunStatus?.wintun_present ? "OK" : "Missing"}
                    </span>
                  )}
                  <span className="text-[10px] px-2 py-0.5 rounded-full bg-secondary font-mono text-muted-foreground">
                    Service: {serviceState}
                  </span>
                </div>
                <p className="text-[11px] text-muted-foreground">
                  {wintunStatus?.message ?? t("common.loading")}
                </p>
              </div>
              <button
                type="button"
                onClick={() => refreshSystemReadiness().catch(() => {})}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-secondary hover:bg-secondary/80 text-xs font-medium transition-colors"
              >
                <RefreshCw className="w-3.5 h-3.5" />
                <span>{t("common.refresh")}</span>
              </button>
            </div>

            {/* URL Scheme Registration */}
            <div className="p-4 rounded-xl bg-secondary/30 border border-border flex flex-wrap items-center justify-between gap-3">
              <div className="space-y-1">
                <div className="flex items-center gap-2">
                  <Link2 className="w-3.5 h-3.5 text-primary" />
                  <span className="text-xs font-semibold">{t("settings.urlSchemeTitle")}</span>
                  <span className="text-[10px] px-2 py-0.5 rounded-full bg-emerald-500/10 text-emerald-400 font-mono">
                    novaproxy://
                  </span>
                  {urlSchemeStatus?.clash_registered_to_us && (
                    <span className="text-[10px] px-2 py-0.5 rounded-full bg-primary/10 text-primary font-mono">
                      clash://
                    </span>
                  )}
                </div>
                <p className="text-[11px] text-muted-foreground">
                  {t("settings.urlSchemeHint")}
                </p>
              </div>
              <button
                type="button"
                onClick={handleToggleClashScheme}
                className={`px-3 py-1.5 rounded-xl text-xs font-medium transition-colors ${
                  urlSchemeStatus?.clash_registered_to_us
                    ? "bg-primary/15 text-primary border border-primary/30"
                    : "bg-secondary hover:bg-secondary/80 text-foreground"
                }`}
              >
                {urlSchemeStatus?.clash_registered_to_us
                  ? t("settings.unregisterClashScheme")
                  : t("settings.registerClashScheme")}
              </button>
            </div>
          </div>
        )}
      </div>

      {/* 2B. Core Binaries & Online Updater */}
      <div className="bg-card border border-border rounded-2xl p-6 shadow-sm space-y-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <h3 className="font-semibold text-xs text-foreground uppercase tracking-wider flex items-center gap-2">
            <HardDrive className="w-4 h-4 text-primary" />
            <span>{t("settings.coreUpdaterTitle")}</span>
          </h3>

          {(platformInfo?.supports_core_update ?? true) && (
            <label className="flex items-center gap-2 text-xs text-muted-foreground cursor-pointer select-none">
              <input
                type="checkbox"
                checked={updateViaProxy}
                onChange={(e) => setUpdateViaProxy(e.target.checked)}
                className="rounded border-border text-primary focus:ring-0"
              />
              <span>{t("settings.updateViaProxy")}</span>
            </label>
          )}
        </div>

        {platformInfo && !platformInfo.supports_core_update ? (
          <p className="text-xs text-muted-foreground p-3.5 rounded-xl bg-secondary/30 border border-border">
            {t("settings.mobileCoreBundledHint")}
          </p>
        ) : (

        <div className="space-y-2.5">
          {coreStatuses.map((cs) => {
            const checkRes = updateChecks[cs.core];
            const isChecking = checkingCore === cs.core;
            const isInstalling = installingCore === cs.core;

            return (
              <div
                key={cs.core}
                className="p-3.5 rounded-xl bg-secondary/30 border border-border flex flex-wrap items-center justify-between gap-3 text-xs"
              >
                <div className="space-y-0.5 min-w-0">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="font-semibold capitalize text-foreground">{cs.core}</span>
                    <span
                      className={`text-[10px] px-2 py-0.5 rounded-full font-medium ${
                        cs.installed
                          ? "bg-emerald-500/10 text-emerald-400"
                          : "bg-rose-500/10 text-rose-400"
                      }`}
                    >
                      {cs.installed ? t("settings.installed") : t("settings.missing")}
                    </span>
                    <span className="font-mono text-[11px] text-foreground">
                      {cs.installed_version ?? cs.lock_version ?? "unknown"}
                    </span>
                    {cs.lock_version && (
                      <span className="text-[10px] font-mono text-muted-foreground">
                        (lock: {cs.lock_version})
                      </span>
                    )}
                  </div>
                  <p className="text-[10px] font-mono text-muted-foreground truncate">
                    SHA-256: {cs.sha256 ? `${cs.sha256.slice(0, 16)}…` : "-"}
                  </p>
                </div>

                <div className="flex flex-wrap items-center gap-2">
                  {checkRes && (
                    <span
                      className={`text-[11px] px-2 py-0.5 rounded-lg font-mono flex items-center gap-1 ${
                        checkRes.has_update
                          ? "bg-amber-500/10 text-amber-500"
                          : "bg-emerald-500/10 text-emerald-400"
                      }`}
                    >
                      <CheckCircle2 className="w-3 h-3" />
                      {checkRes.has_update ? `-> ${checkRes.latest_version}` : checkRes.latest_version}
                    </span>
                  )}

                  <button
                    type="button"
                    disabled={Boolean(checkingCore || installingCore)}
                    onClick={() => handleCheckCoreUpdate(cs.core)}
                    className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-secondary hover:bg-secondary/80 text-xs font-medium transition-colors disabled:opacity-50"
                  >
                    <RefreshCw className={`w-3.5 h-3.5 ${isChecking ? "animate-spin" : ""}`} />
                    <span>{t("settings.checkUpdate")}</span>
                  </button>

                  {checkRes?.has_update && (
                    <button
                      type="button"
                      disabled={Boolean(checkingCore || installingCore)}
                      onClick={() => handleInstallCoreUpdate(cs.core)}
                      className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-primary text-primary-foreground hover:bg-primary/90 text-xs font-medium transition-colors disabled:opacity-50"
                    >
                      <Download className={`w-3.5 h-3.5 ${isInstalling ? "animate-bounce" : ""}`} />
                      <span>{t("settings.installUpdate")}</span>
                    </button>
                  )}
                </div>
              </div>
            );
          })}
        </div>
        )}
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

      {/* 4. Diagnostics & Disaster Recovery */}
      <div className="bg-card border border-border rounded-2xl p-6 shadow-sm space-y-4">
        <h3 className="font-semibold text-xs text-foreground uppercase tracking-wider flex items-center gap-2">
          <ShieldAlert className="w-4 h-4 text-primary" />
          <span>{t("settings.diagnosticsTitle", "Diagnostics & Disaster Recovery")}</span>
        </h3>

        <div className="grid grid-cols-1 sm:grid-cols-3 gap-3 pt-1">
          <button
            onClick={async () => {
              try {
                const ok = await checkDatabaseIntegrity();
                if (ok) {
                  showToast(t("settings.dbIntegrityOk", "Database integrity check: PASSED. All tables and indices are healthy."));
                } else {
                  showToast(t("settings.dbIntegrityFail", "Database integrity check returned errors."));
                }
              } catch (e) {
                showToast(`Error: ${String(e)}`);
              }
            }}
            className="flex items-center justify-center gap-2 px-4 py-2.5 rounded-xl border border-border hover:bg-secondary text-xs font-medium transition-all"
          >
            <Check className="w-4 h-4 text-emerald-500" />
            <span>{t("settings.verifyDbIntegrity", "Verify SQLite Integrity")}</span>
          </button>

          <button
            onClick={async () => {
              try {
                const report = await exportDiagnosticsReport();
                const jsonStr = JSON.stringify(report, null, 2);
                await navigator.clipboard.writeText(jsonStr);
                showToast(t("settings.diagnosticsCopied", "Sanitized diagnostics report copied to clipboard."));
              } catch (e) {
                showToast(`Error: ${String(e)}`);
              }
            }}
            className="flex items-center justify-center gap-2 px-4 py-2.5 rounded-xl border border-border hover:bg-secondary text-xs font-medium transition-all"
          >
            <Copy className="w-4 h-4 text-primary" />
            <span>{t("settings.exportDiagnostics", "Copy Diagnostics Report")}</span>
          </button>

          <button
            onClick={async () => {
              try {
                const res = await exportDiagnosticBundle();
                showToast(
                  t("logs.bundleExported", {
                    path: res.path,
                    defaultValue: `Diagnostic bundle saved to ${res.path}`,
                  })
                );
              } catch (e) {
                showToast(`Error: ${String(e)}`);
              }
            }}
            className="flex items-center justify-center gap-2 px-4 py-2.5 rounded-xl border border-border hover:bg-secondary text-xs font-medium transition-all"
          >
            <Download className="w-4 h-4 text-primary" />
            <span>{t("logs.exportBundle", "Export Diagnostic .zip")}</span>
          </button>
        </div>
      </div>

      {/* 5. About & Version */}
      <div className="bg-card border border-border rounded-2xl p-6 shadow-sm space-y-4">
        <h3 className="font-semibold text-xs text-foreground uppercase tracking-wider flex items-center gap-2">
          <Info className="w-4 h-4 text-primary" />
          <span>{t("settings.about")}</span>
        </h3>

        <div className="flex flex-wrap items-center justify-between gap-3 text-xs">
          <div>
            <p className="font-semibold text-foreground">
              {t("settings.platformSummary", {
                os: platformInfo?.os ?? "desktop",
                arch: platformInfo?.arch ?? "x86_64",
              })}
            </p>
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

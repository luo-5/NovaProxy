import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  LayoutDashboard,
  Server,
  Rss,
  Split,
  Activity,
  Terminal,
  Settings as SettingsIcon,
  ShieldCheck,
} from "lucide-react";
import { useAppStore, NavTab } from "../stores/appStore";
import { getAppVersion } from "../lib/ipc";

export const Sidebar: React.FC = () => {
  const { t } = useTranslation();
  const { currentTab, setCurrentTab, isConnected, appSettings, platformInfo } = useAppStore();
  const [appVersion, setAppVersion] = useState<string>("0.0.1");

  useEffect(() => {
    getAppVersion()
      .then((ver) => setAppVersion(ver))
      .catch(() => {});
  }, []);

  const navItems: { id: NavTab; label: string; icon: React.ReactNode }[] = [
    {
      id: "dashboard",
      label: t("nav.dashboard"),
      icon: <LayoutDashboard className="w-4 h-4" />,
    },
    {
      id: "proxies",
      label: t("nav.proxies"),
      icon: <Server className="w-4 h-4" />,
    },
    {
      id: "subscriptions",
      label: t("nav.subscriptions"),
      icon: <Rss className="w-4 h-4" />,
    },
    {
      id: "rules",
      label: t("nav.rules"),
      icon: <Split className="w-4 h-4" />,
    },
    {
      id: "connections",
      label: t("nav.connections"),
      icon: <Activity className="w-4 h-4" />,
    },
    {
      id: "logs",
      label: t("nav.logs"),
      icon: <Terminal className="w-4 h-4" />,
    },
    {
      id: "settings",
      label: t("nav.settings"),
      icon: <SettingsIcon className="w-4 h-4" />,
    },
  ];

  return (
    <>
      <aside className="hidden md:flex w-56 bg-card border-r border-border flex-col justify-between p-3 select-none shrink-0">
        <div>
          <div className="flex items-center gap-2.5 px-3 py-4 mb-2">
            <div className="w-8 h-8 rounded-xl bg-primary/10 border border-primary/20 flex items-center justify-center text-primary font-bold shadow-sm">
              <ShieldCheck className="w-5 h-5 text-primary" />
            </div>
            <div>
              <h1 className="font-bold text-sm leading-none text-foreground tracking-wide">
                NovaProxy
              </h1>
              <span className="text-[11px] text-muted-foreground">
                v{appVersion} {platformInfo ? `· ${platformInfo.os}` : "Multi-Core"}
              </span>
            </div>
          </div>

          <nav className="space-y-1">
            {navItems.map((item) => {
              const active = currentTab === item.id;
              return (
                <button
                  key={item.id}
                  onClick={() => setCurrentTab(item.id)}
                  className={`w-full flex items-center gap-3 px-3 py-2.5 rounded-xl text-xs font-medium transition-all ${
                    active
                      ? "bg-primary text-primary-foreground shadow-sm"
                      : "text-muted-foreground hover:bg-secondary/70 hover:text-foreground"
                  }`}
                >
                  {item.icon}
                  <span>{item.label}</span>
                </button>
              );
            })}
          </nav>
        </div>

        <div className="space-y-2 border-t border-border/50 pt-3">
          <div className="flex items-center justify-between px-3 py-1.5 rounded-lg bg-secondary/40 text-[11px]">
            <div className="flex items-center gap-2">
              <span
                className={`w-2 h-2 rounded-full ${
                  isConnected ? "bg-emerald-500 animate-pulse" : "bg-zinc-500"
                }`}
              />
              <span className="text-muted-foreground font-medium">
                {isConnected ? t("dashboard.connected") : t("dashboard.disconnected")}
              </span>
            </div>
            <span className="text-muted-foreground font-mono">
              :{appSettings?.mixed_port ?? 7890}
            </span>
          </div>
          <div className="px-2 text-[10px] text-muted-foreground/60 text-center">
            sing-box • mihomo • Xray
          </div>
        </div>
      </aside>

      {/* Mobile Bottom Navigation */}
      <nav className="md:hidden order-last border-t border-border bg-card/95 backdrop-blur-md px-1.5 py-1.5 flex items-center justify-between gap-1 overflow-x-auto no-scrollbar shrink-0 select-none">
        {navItems.map((item) => {
          const active = currentTab === item.id;
          return (
            <button
              key={item.id}
              onClick={() => setCurrentTab(item.id)}
              className={`flex flex-col items-center justify-center min-w-[54px] flex-1 py-1.5 px-1 rounded-xl text-[10px] font-medium transition-all ${
                active
                  ? "bg-primary/15 text-primary font-semibold"
                  : "text-muted-foreground hover:text-foreground"
              }`}
            >
              {item.icon}
              <span className="mt-1 truncate max-w-[60px]">{item.label}</span>
            </button>
          );
        })}
      </nav>
    </>
  );
};

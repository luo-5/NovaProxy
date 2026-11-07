import React from "react";
import { useTranslation } from "react-i18next";
import { 
  LayoutDashboard, 
  Server, 
  Rss, 
  Split, 
  Activity, 
  Terminal, 
  Settings as SettingsIcon,
  ShieldCheck
} from "lucide-react";
import { useAppStore, NavTab } from "../stores/appStore";

export const Sidebar: React.FC = () => {
  const { t } = useTranslation();
  const { currentTab, setCurrentTab } = useAppStore();

  const navItems: { id: NavTab; label: string; icon: React.ReactNode }[] = [
    { id: "dashboard", label: t("nav.dashboard"), icon: <LayoutDashboard className="w-5 h-5" /> },
    { id: "proxies", label: t("nav.proxies"), icon: <Server className="w-5 h-5" /> },
    { id: "subscriptions", label: t("nav.subscriptions"), icon: <Rss className="w-5 h-5" /> },
    { id: "rules", label: t("nav.rules"), icon: <Split className="w-5 h-5" /> },
    { id: "connections", label: t("nav.connections"), icon: <Activity className="w-5 h-5" /> },
    { id: "logs", label: t("nav.logs"), icon: <Terminal className="w-5 h-5" /> },
    { id: "settings", label: t("nav.settings"), icon: <SettingsIcon className="w-5 h-5" /> },
  ];

  return (
    <aside className="w-56 bg-card border-r border-border flex flex-col justify-between p-3 select-none">
      <div>
        <div className="flex items-center gap-2.5 px-3 py-4 mb-2">
          <div className="w-8 h-8 rounded-lg bg-primary/20 flex items-center justify-center text-primary font-bold">
            <ShieldCheck className="w-5 h-5 text-primary" />
          </div>
          <div>
            <h1 className="font-semibold text-sm leading-none text-foreground tracking-wide">NovaProxy</h1>
            <span className="text-[11px] text-muted-foreground">v0.1.0 Multi-Core</span>
          </div>
        </div>

        <nav className="space-y-1">
          {navItems.map((item) => {
            const active = currentTab === item.id;
            return (
              <button
                key={item.id}
                onClick={() => setCurrentTab(item.id)}
                className={`w-full flex items-center gap-3 px-3 py-2.5 rounded-md text-sm font-medium transition-colors ${
                  active
                    ? "bg-primary text-primary-foreground shadow-sm"
                    : "text-muted-foreground hover:bg-secondary hover:text-foreground"
                }`}
              >
                {item.icon}
                <span>{item.label}</span>
              </button>
            );
          })}
        </nav>
      </div>

      <div className="px-3 py-2 border-t border-border/50 text-[11px] text-muted-foreground text-center">
        Powered by sing-box • mihomo • Xray
      </div>
    </aside>
  );
};

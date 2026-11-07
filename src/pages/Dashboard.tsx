import React from "react";
import { useTranslation } from "react-i18next";
import { Power, ArrowUp, ArrowDown, Globe, Shield } from "lucide-react";
import { useAppStore } from "../stores/appStore";

export const Dashboard: React.FC = () => {
  const { t } = useTranslation();
  const { isConnected, setIsConnected, systemProxy, setSystemProxy, tunMode, setTunMode } = useAppStore();

  return (
    <div className="p-8 max-w-5xl mx-auto space-y-6">
      {/* Hero Connect Switch */}
      <div className="bg-card border border-border rounded-xl p-8 flex flex-col items-center justify-center text-center shadow-sm">
        <button
          onClick={() => setIsConnected(!isConnected)}
          className={`w-28 h-28 rounded-full flex items-center justify-center transition-all duration-300 shadow-lg ${
            isConnected
              ? "bg-emerald-500 text-white shadow-emerald-500/20 hover:bg-emerald-600 scale-105"
              : "bg-secondary text-muted-foreground hover:text-foreground hover:bg-secondary/80"
          }`}
        >
          <Power className="w-12 h-12" />
        </button>

        <div className="mt-5 space-y-1">
          <h2 className="text-xl font-bold">
            {isConnected ? t("dashboard.connected") : t("dashboard.disconnected")}
          </h2>
          <p className="text-sm text-muted-foreground">
            {isConnected ? t("dashboard.clickToDisconnect") : t("dashboard.clickToConnect")}
          </p>
        </div>
      </div>

      {/* Speed & Stats */}
      <div className="grid grid-cols-2 gap-4">
        <div className="bg-card border border-border rounded-xl p-5 flex items-center justify-between">
          <div className="flex items-center gap-3.5">
            <div className="w-10 h-10 rounded-lg bg-blue-500/10 text-blue-500 flex items-center justify-center">
              <ArrowDown className="w-5 h-5" />
            </div>
            <div>
              <p className="text-xs text-muted-foreground">{t("dashboard.downloadSpeed")}</p>
              <p className="text-lg font-semibold tracking-tight">0.0 KB/s</p>
            </div>
          </div>
        </div>

        <div className="bg-card border border-border rounded-xl p-5 flex items-center justify-between">
          <div className="flex items-center gap-3.5">
            <div className="w-10 h-10 rounded-lg bg-amber-500/10 text-amber-500 flex items-center justify-center">
              <ArrowUp className="w-5 h-5" />
            </div>
            <div>
              <p className="text-xs text-muted-foreground">{t("dashboard.uploadSpeed")}</p>
              <p className="text-lg font-semibold tracking-tight">0.0 KB/s</p>
            </div>
          </div>
        </div>
      </div>

      {/* Quick Mode Controls */}
      <div className="grid grid-cols-2 gap-4">
        <div className="bg-card border border-border rounded-xl p-5 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <Globe className="w-5 h-5 text-muted-foreground" />
            <div>
              <p className="text-sm font-medium">{t("dashboard.systemProxy")}</p>
              <p className="text-xs text-muted-foreground">127.0.0.1:7890</p>
            </div>
          </div>
          <button
            onClick={() => setSystemProxy(!systemProxy)}
            className={`w-12 h-6 rounded-full transition-colors relative ${
              systemProxy ? "bg-primary" : "bg-secondary"
            }`}
          >
            <div
              className={`w-4 h-4 rounded-full bg-white transition-transform absolute top-1 ${
                systemProxy ? "right-1" : "left-1"
              }`}
            />
          </button>
        </div>

        <div className="bg-card border border-border rounded-xl p-5 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <Shield className="w-5 h-5 text-muted-foreground" />
            <div>
              <p className="text-sm font-medium">{t("dashboard.tunMode")}</p>
              <p className="text-xs text-muted-foreground">wintun</p>
            </div>
          </div>
          <button
            onClick={() => setTunMode(!tunMode)}
            className={`w-12 h-6 rounded-full transition-colors relative ${
              tunMode ? "bg-primary" : "bg-secondary"
            }`}
          >
            <div
              className={`w-4 h-4 rounded-full bg-white transition-transform absolute top-1 ${
                tunMode ? "right-1" : "left-1"
              }`}
            />
          </button>
        </div>
      </div>
    </div>
  );
};

import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Sidebar } from "./components/Sidebar";
import { Dashboard } from "./pages/Dashboard";
import { Proxies } from "./pages/Proxies";
import { Subscriptions } from "./pages/Subscriptions";
import { Rules } from "./pages/Rules";
import { Connections } from "./pages/Connections";
import { Logs } from "./pages/Logs";
import { Settings } from "./pages/Settings";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { useAppStore } from "./stores/appStore";
import {
  getPlatformInfo,
  logFrontendEvent,
  onDeepLinkImport,
  refreshSubscription,
} from "./lib/ipc";

export const App: React.FC = () => {
  const { t } = useTranslation();
  const { currentTab, setCurrentTab, setPlatformInfo } = useAppStore();
  const [deepLinkNotice, setDeepLinkNotice] = useState<string | null>(null);

  useEffect(() => {
    getPlatformInfo()
      .then(setPlatformInfo)
      .catch(() => undefined);
  }, [setPlatformInfo]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let dismissTimer: number | undefined;

    onDeepLinkImport((sub) => {
      if (disposed || !sub) return;
      setDeepLinkNotice(
        t("common.deepLinkImported", {
          name: sub.name,
          defaultValue: `已导入订阅: ${sub.name}`,
        })
      );
      setCurrentTab("subscriptions");
      // Automatically pull nodes once for freshly imported deep-link subscription
      refreshSubscription(sub.id)
        .catch((err) => {
          void logFrontendEvent("warn", "deep-link", `Auto-refresh failed: ${String(err)}`);
        })
        .finally(() => {
          window.dispatchEvent(new CustomEvent("novaproxy:subscriptions-updated"));
        });
      window.dispatchEvent(new CustomEvent("novaproxy:subscriptions-updated"));
      if (dismissTimer) window.clearTimeout(dismissTimer);
      dismissTimer = window.setTimeout(() => setDeepLinkNotice(null), 5000);
    })
      .then((fn) => {
        if (disposed) {
          fn();
        } else {
          unlisten = fn;
        }
      })
      .catch(() => undefined);

    return () => {
      disposed = true;
      if (unlisten) unlisten();
      if (dismissTimer) window.clearTimeout(dismissTimer);
    };
  }, [setCurrentTab, t]);

  return (
    <ErrorBoundary>
      <div className="flex flex-col md:flex-row h-screen w-screen bg-background text-foreground overflow-hidden relative">
        <Sidebar />
        <main className="flex-1 overflow-y-auto min-w-0">
          {deepLinkNotice && (
            <div className="fixed top-4 right-4 z-50 flex items-center gap-3 px-4 py-2.5 rounded-xl bg-emerald-500/15 border border-emerald-500/30 text-emerald-400 text-xs font-medium shadow-lg backdrop-blur-md">
              <span>{deepLinkNotice}</span>
              <button
                type="button"
                onClick={() => setDeepLinkNotice(null)}
                className="text-emerald-300/80 hover:text-emerald-200 px-1"
              >
                ×
              </button>
            </div>
          )}
          {currentTab === "dashboard" && <Dashboard />}
          {currentTab === "proxies" && <Proxies />}
          {currentTab === "subscriptions" && <Subscriptions />}
          {currentTab === "rules" && <Rules />}
          {currentTab === "connections" && <Connections />}
          {currentTab === "logs" && <Logs />}
          {currentTab === "settings" && <Settings />}
        </main>
      </div>
    </ErrorBoundary>
  );
};

export default App;

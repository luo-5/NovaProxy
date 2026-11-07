import React from "react";
import { Sidebar } from "./components/Sidebar";
import { Dashboard } from "./pages/Dashboard";
import { Proxies } from "./pages/Proxies";
import { Subscriptions } from "./pages/Subscriptions";
import { Rules } from "./pages/Rules";
import { Connections } from "./pages/Connections";
import { Logs } from "./pages/Logs";
import { Settings } from "./pages/Settings";
import { useAppStore } from "./stores/appStore";

export const App: React.FC = () => {
  const { currentTab } = useAppStore();

  return (
    <div className="flex h-screen w-screen bg-background text-foreground overflow-hidden">
      <Sidebar />
      <main className="flex-1 overflow-y-auto">
        {currentTab === "dashboard" && <Dashboard />}
        {currentTab === "proxies" && <Proxies />}
        {currentTab === "subscriptions" && <Subscriptions />}
        {currentTab === "rules" && <Rules />}
        {currentTab === "connections" && <Connections />}
        {currentTab === "logs" && <Logs />}
        {currentTab === "settings" && <Settings />}
      </main>
    </div>
  );
};

export default App;

import { create } from "zustand";
import type { AppSettings, ProxyNode, RoutingMode } from "../types/bindings";

export type NavTab =
  | "dashboard"
  | "proxies"
  | "subscriptions"
  | "rules"
  | "connections"
  | "logs"
  | "settings";

export interface TrafficPoint {
  time: string;
  up: number;
  down: number;
}

interface AppState {
  currentTab: NavTab;
  setCurrentTab: (tab: NavTab) => void;

  isConnected: boolean;
  setIsConnected: (connected: boolean) => void;

  systemProxy: boolean;
  setSystemProxy: (enabled: boolean) => void;

  tunMode: boolean;
  setTunMode: (enabled: boolean) => void;

  routingMode: RoutingMode;
  setRoutingMode: (mode: RoutingMode) => void;

  theme: "dark" | "light";
  setTheme: (theme: "dark" | "light") => void;

  activeNode: ProxyNode | null;
  setActiveNode: (node: ProxyNode | null) => void;

  appSettings: AppSettings | null;
  setAppSettings: (settings: AppSettings) => void;

  trafficHistory: TrafficPoint[];
  addTrafficPoint: (up: number, down: number) => void;

  totalTraffic: { up: number; down: number };
  setTotalTraffic: (total: { up: number; down: number }) => void;
}

export const useAppStore = create<AppState>((set) => ({
  currentTab: "dashboard",
  setCurrentTab: (tab) => set({ currentTab: tab }),

  isConnected: false,
  setIsConnected: (connected) => set({ isConnected: connected }),

  systemProxy: false,
  setSystemProxy: (enabled) => set({ systemProxy: enabled }),

  tunMode: false,
  setTunMode: (enabled) => set({ tunMode: enabled }),

  routingMode: "rule",
  setRoutingMode: (mode) => set({ routingMode: mode }),

  theme: "dark",
  setTheme: (theme) => {
    if (theme === "dark") {
      document.documentElement.classList.add("dark");
    } else {
      document.documentElement.classList.remove("dark");
    }
    set({ theme });
  },

  activeNode: null,
  setActiveNode: (node) => set({ activeNode: node }),

  appSettings: null,
  setAppSettings: (settings) => set({ appSettings: settings }),

  trafficHistory: Array.from({ length: 30 }, (_, i) => ({
    time: `${i}s`,
    up: 0,
    down: 0,
  })),
  addTrafficPoint: (up, down) =>
    set((state) => {
      const now = new Date();
      const timeStr = `${now.getMinutes().toString().padStart(2, "0")}:${now
        .getSeconds()
        .toString()
        .padStart(2, "0")}`;
      const newHistory = [
        ...state.trafficHistory.slice(1),
        { time: timeStr, up, down },
      ];
      return { trafficHistory: newHistory };
    }),

  totalTraffic: { up: 0, down: 0 },
  setTotalTraffic: (total) => set({ totalTraffic: total }),
}));

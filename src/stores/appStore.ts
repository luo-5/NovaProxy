import { create } from "zustand";

export type NavTab = "dashboard" | "proxies" | "subscriptions" | "rules" | "connections" | "logs" | "settings";

interface AppState {
  currentTab: NavTab;
  setCurrentTab: (tab: NavTab) => void;
  isConnected: boolean;
  setIsConnected: (connected: boolean) => void;
  systemProxy: boolean;
  setSystemProxy: (enabled: boolean) => void;
  tunMode: boolean;
  setTunMode: (enabled: boolean) => void;
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
}));

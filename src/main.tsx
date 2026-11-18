import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import App from "./App";
import "./index.css";
import "./i18n";
import { logFrontendEvent } from "./lib/ipc";

if (typeof window !== "undefined") {
  window.addEventListener("error", (ev) => {
    const detail = ev.error instanceof Error ? ev.error.stack || ev.error.message : ev.message;
    void logFrontendEvent("error", "window.onerror", `${ev.message} (${ev.filename}:${ev.lineno}) ${detail ?? ""}`.trim());
  });
  window.addEventListener("unhandledrejection", (ev) => {
    const reason =
      ev.reason instanceof Error
        ? `${ev.reason.name}: ${ev.reason.message}`
        : String(ev.reason);
    void logFrontendEvent("error", "unhandledrejection", reason);
  });
}

const queryClient = new QueryClient();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </React.StrictMode>
);

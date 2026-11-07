import React from "react";
import { useTranslation } from "react-i18next";
import { Terminal } from "lucide-react";

export const Logs: React.FC = () => {
  const { t } = useTranslation();

  return (
    <div className="p-8 max-w-5xl mx-auto space-y-6">
      <div>
        <h2 className="text-xl font-bold">{t("nav.logs")}</h2>
        <p className="text-xs text-muted-foreground">Real-time core log stream from sing-box, mihomo, or Xray</p>
      </div>

      <div className="bg-card border border-border rounded-xl p-6 font-mono text-xs text-muted-foreground h-96 overflow-y-auto">
        <div className="flex items-center gap-2 text-primary font-sans font-medium mb-3">
          <Terminal className="w-4 h-4" />
          <span>Core Log Console</span>
        </div>
        <p className="text-muted-foreground/60">[System] NovaProxy initialized. Waiting for core start...</p>
      </div>
    </div>
  );
};

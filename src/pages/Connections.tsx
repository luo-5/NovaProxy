import React from "react";
import { useTranslation } from "react-i18next";
import { Activity } from "lucide-react";

export const Connections: React.FC = () => {
  const { t } = useTranslation();

  return (
    <div className="p-8 max-w-5xl mx-auto space-y-6">
      <div>
        <h2 className="text-xl font-bold">{t("nav.connections")}</h2>
        <p className="text-xs text-muted-foreground">Inspect active socket connections, destination IPs, and transfer stats</p>
      </div>

      <div className="bg-card border border-border rounded-xl p-12 text-center text-muted-foreground flex flex-col items-center justify-center">
        <Activity className="w-12 h-12 stroke-[1.5] mb-3 text-muted-foreground/60" />
        <p className="text-sm font-medium">No Active Connections</p>
        <p className="text-xs text-muted-foreground/80 mt-1">
          Realtime telemetry from Clash API /connections endpoint.
        </p>
      </div>
    </div>
  );
};

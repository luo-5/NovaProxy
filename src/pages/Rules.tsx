import React from "react";
import { useTranslation } from "react-i18next";
import { Split } from "lucide-react";

export const Rules: React.FC = () => {
  const { t } = useTranslation();

  return (
    <div className="p-8 max-w-5xl mx-auto space-y-6">
      <div>
        <h2 className="text-xl font-bold">{t("nav.rules")}</h2>
        <p className="text-xs text-muted-foreground">Configure routing mode, geosite, geoip, and custom domain/IP rules</p>
      </div>

      <div className="bg-card border border-border rounded-xl p-12 text-center text-muted-foreground flex flex-col items-center justify-center">
        <Split className="w-12 h-12 stroke-[1.5] mb-3 text-muted-foreground/60" />
        <p className="text-sm font-medium">Routing Engine</p>
        <p className="text-xs text-muted-foreground/80 mt-1">
          Rule-based / Global / Direct routing with sing-box SRS and mihomo rule-providers.
        </p>
      </div>
    </div>
  );
};

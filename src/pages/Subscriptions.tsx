import React from "react";
import { useTranslation } from "react-i18next";
import { Rss, Plus } from "lucide-react";

export const Subscriptions: React.FC = () => {
  const { t } = useTranslation();

  return (
    <div className="p-8 max-w-5xl mx-auto space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-bold">{t("nav.subscriptions")}</h2>
          <p className="text-xs text-muted-foreground">Manage Base64, Clash YAML, or sing-box JSON subscriptions</p>
        </div>
        <button className="flex items-center gap-1.5 px-3 py-1.5 bg-primary text-primary-foreground text-xs font-medium rounded-md hover:bg-primary/90 transition-colors">
          <Plus className="w-4 h-4" />
          <span>Add Subscription</span>
        </button>
      </div>

      <div className="bg-card border border-border rounded-xl p-12 text-center text-muted-foreground flex flex-col items-center justify-center">
        <Rss className="w-12 h-12 stroke-[1.5] mb-3 text-muted-foreground/60" />
        <p className="text-sm font-medium">No subscriptions added</p>
        <p className="text-xs text-muted-foreground/80 mt-1">
          Supports automatic format detection, auto-update interval, and userinfo quota tracking.
        </p>
      </div>
    </div>
  );
};

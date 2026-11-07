import React from "react";
import { useTranslation } from "react-i18next";
import { Server, Plus } from "lucide-react";

export const Proxies: React.FC = () => {
  const { t } = useTranslation();

  return (
    <div className="p-8 max-w-5xl mx-auto space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-bold">{t("nav.proxies")}</h2>
          <p className="text-xs text-muted-foreground">Manage and test your proxy nodes</p>
        </div>
        <button className="flex items-center gap-1.5 px-3 py-1.5 bg-primary text-primary-foreground text-xs font-medium rounded-md hover:bg-primary/90 transition-colors">
          <Plus className="w-4 h-4" />
          <span>Add Node</span>
        </button>
      </div>

      <div className="bg-card border border-border rounded-xl p-12 text-center text-muted-foreground flex flex-col items-center justify-center">
        <Server className="w-12 h-12 stroke-[1.5] mb-3 text-muted-foreground/60" />
        <p className="text-sm font-medium">No proxy nodes imported yet</p>
        <p className="text-xs text-muted-foreground/80 mt-1">
          Import from subscription or paste a proxy URI (ss, vmess, vless, trojan, hy2, tuic, etc.)
        </p>
      </div>
    </div>
  );
};

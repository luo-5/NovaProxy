import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Settings as SettingsIcon, Cpu } from "lucide-react";
import { getAppVersion } from "../lib/ipc";

export const Settings: React.FC = () => {
  const { t } = useTranslation();
  const [version, setVersion] = useState<string>("0.1.0");

  useEffect(() => {
    getAppVersion()
      .then((ver) => setVersion(ver))
      .catch(() => setVersion("0.1.0 (dev)"));
  }, []);

  return (
    <div className="p-8 max-w-5xl mx-auto space-y-6">
      <div>
        <h2 className="text-xl font-bold">{t("nav.settings")}</h2>
        <p className="text-xs text-muted-foreground">General preferences, kernel selection, ports, and autostart</p>
      </div>

      <div className="bg-card border border-border rounded-xl divide-y divide-border">
        <div className="p-5 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <Cpu className="w-5 h-5 text-muted-foreground" />
            <div>
              <p className="text-sm font-medium">Default Core Engine</p>
              <p className="text-xs text-muted-foreground">Automatic selection (sing-box primary / mihomo / Xray)</p>
            </div>
          </div>
          <span className="text-xs font-semibold px-2.5 py-1 bg-secondary rounded-md text-foreground">
            Auto-Detect
          </span>
        </div>

        <div className="p-5 flex items-center justify-between">
          <div className="flex items-center gap-3">
            <SettingsIcon className="w-5 h-5 text-muted-foreground" />
            <div>
              <p className="text-sm font-medium">App Version</p>
              <p className="text-xs text-muted-foreground">NovaProxy Windows x64</p>
            </div>
          </div>
          <span className="text-xs font-mono text-muted-foreground">v{version}</span>
        </div>
      </div>
    </div>
  );
};

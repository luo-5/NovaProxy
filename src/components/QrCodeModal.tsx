import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { X, Copy, Check, QrCode } from "lucide-react";
import { encodeNodeQr, exportNodeLink, type ProxyNode } from "../lib/ipc";

interface QrCodeModalProps {
  node: ProxyNode | null;
  isOpen: boolean;
  onClose: () => void;
}

export const QrCodeModal: React.FC<QrCodeModalProps> = ({
  node,
  isOpen,
  onClose,
}) => {
  const { t } = useTranslation();
  const [svgContent, setSvgContent] = useState<string>("");
  const [shareLink, setShareLink] = useState<string>("");
  const [copied, setCopied] = useState<boolean>(false);
  const [copyError, setCopyError] = useState<boolean>(false);
  const [loading, setLoading] = useState<boolean>(false);

  useEffect(() => {
    if (!isOpen || !node) {
      setSvgContent("");
      setShareLink("");
      setCopied(false);
      setCopyError(false);
      return;
    }

    setLoading(true);
    Promise.all([
      encodeNodeQr(node).catch(() => ""),
      exportNodeLink(node).catch(() => ""),
    ]).then(([svg, link]) => {
      setSvgContent(svg);
      setShareLink(link);
      setLoading(false);
    });
  }, [isOpen, node]);

  if (!isOpen || !node) return null;

  const handleCopy = () => {
    if (!shareLink) return;
    navigator.clipboard
      .writeText(shareLink)
      .then(() => {
        setCopyError(false);
        setCopied(true);
        setTimeout(() => setCopied(false), 2000);
      })
      .catch(() => {
        setCopyError(true);
        setTimeout(() => setCopyError(false), 2500);
      });
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
      <div className="bg-card border border-border rounded-2xl w-full max-w-sm shadow-2xl overflow-hidden animate-in fade-in zoom-in-95 duration-150">
        <div className="flex items-center justify-between px-5 py-4 border-b border-border">
          <div className="flex items-center gap-2">
            <QrCode className="w-4 h-4 text-primary" />
            <h3 className="font-semibold text-sm">{node.name}</h3>
          </div>
          <button
            onClick={onClose}
            className="p-1 rounded-lg text-muted-foreground hover:text-foreground hover:bg-secondary/80 transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="p-6 flex flex-col items-center justify-center space-y-4">
          <div className="p-4 bg-white rounded-xl shadow-inner border border-zinc-200 flex items-center justify-center min-w-[200px] min-h-[200px]">
            {loading ? (
              <span className="text-xs text-zinc-400">{t("common.loading")}</span>
            ) : svgContent ? (
              <div
                className="w-48 h-48 [&>svg]:w-full [&>svg]:h-full"
                dangerouslySetInnerHTML={{ __html: svgContent }}
              />
            ) : (
              <span className="text-xs text-red-500">{t("common.error")}</span>
            )}
          </div>

          <div className="w-full space-y-1.5 text-center">
            <p className="text-xs text-muted-foreground truncate max-w-[260px] mx-auto font-mono">
              {node.server}:{node.port}
            </p>
          </div>

          <button
            onClick={handleCopy}
            disabled={!shareLink}
            className="w-full flex items-center justify-center gap-2 py-2 px-4 rounded-xl bg-secondary text-secondary-foreground hover:bg-secondary/80 font-medium text-xs transition-colors"
          >
            {copied ? (
              <>
                <Check className="w-3.5 h-3.5 text-emerald-500" />
                <span className="text-emerald-500">{t("common.copied")}</span>
              </>
            ) : copyError ? (
              <span className="text-red-500">{t("common.clipboardUnavailable")}</span>
            ) : (
              <>
                <Copy className="w-3.5 h-3.5" />
                <span>{t("proxies.copyLink")}</span>
              </>
            )}
          </button>
        </div>
      </div>
    </div>
  );
};

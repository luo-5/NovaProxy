import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function formatBytes(bytes: number, decimals = 2) {
  if (!bytes || bytes <= 0) return "0 B";
  const k = 1024;
  const dm = decimals < 0 ? 0 : decimals;
  const sizes = ["B", "KB", "MB", "GB", "TB", "PB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  const idx = Math.min(i, sizes.length - 1);
  return parseFloat((bytes / Math.pow(k, idx)).toFixed(dm)) + " " + sizes[idx];
}

export function formatSpeed(bytesPerSec: number) {
  return `${formatBytes(bytesPerSec, 1)}/s`;
}

export function formatLatency(ms: number | null | undefined): {
  text: string;
  badgeClass: string;
  textClass: string;
} {
  if (ms === null || ms === undefined) {
    return {
      text: "-- ms",
      badgeClass: "bg-muted text-muted-foreground border-border",
      textClass: "text-muted-foreground",
    };
  }
  if (ms < 0) {
    return {
      text: "Timeout",
      badgeClass: "bg-red-500/10 text-red-500 border-red-500/20",
      textClass: "text-red-500",
    };
  }
  if (ms < 150) {
    return {
      text: `${ms} ms`,
      badgeClass: "bg-emerald-500/10 text-emerald-500 border-emerald-500/20",
      textClass: "text-emerald-500",
    };
  }
  if (ms < 300) {
    return {
      text: `${ms} ms`,
      badgeClass: "bg-amber-500/10 text-amber-500 border-amber-500/20",
      textClass: "text-amber-500",
    };
  }
  return {
    text: `${ms} ms`,
    badgeClass: "bg-orange-500/10 text-orange-500 border-orange-500/20",
    textClass: "text-orange-500",
  };
}

export function getProtocolBadge(protocol: string): {
  label: string;
  bg: string;
  text: string;
} {
  const p = protocol.toLowerCase();
  switch (p) {
    case "vmess":
      return { label: "VMess", bg: "bg-blue-500/10", text: "text-blue-400" };
    case "vless":
      return { label: "VLESS", bg: "bg-indigo-500/10", text: "text-indigo-400" };
    case "trojan":
      return { label: "Trojan", bg: "bg-purple-500/10", text: "text-purple-400" };
    case "shadowsocks":
    case "ss":
      return { label: "SS", bg: "bg-sky-500/10", text: "text-sky-400" };
    case "shadowsocksr":
    case "ssr":
      return { label: "SSR", bg: "bg-cyan-500/10", text: "text-cyan-400" };
    case "hysteria":
    case "hy":
      return { label: "Hy1", bg: "bg-rose-500/10", text: "text-rose-400" };
    case "hysteria2":
    case "hy2":
      return { label: "Hy2", bg: "bg-pink-500/10", text: "text-pink-400" };
    case "tuic":
      return { label: "TUIC", bg: "bg-orange-500/10", text: "text-orange-400" };
    case "wireguard":
    case "wg":
      return { label: "WG", bg: "bg-red-500/10", text: "text-red-400" };
    case "shadowtls":
      return { label: "ShadowTLS", bg: "bg-teal-500/10", text: "text-teal-400" };
    case "naive":
      return { label: "Naive", bg: "bg-emerald-500/10", text: "text-emerald-400" };
    case "mieru":
      return { label: "Mieru", bg: "bg-yellow-500/10", text: "text-yellow-400" };
    case "anytls":
      return { label: "AnyTLS", bg: "bg-violet-500/10", text: "text-violet-400" };
    case "ssh":
      return { label: "SSH", bg: "bg-lime-500/10", text: "text-lime-400" };
    default:
      return { label: protocol.toUpperCase(), bg: "bg-zinc-500/10", text: "text-zinc-400" };
  }
}

export function formatRelativeTime(
  timestampSec: number | null | undefined
): string {
  if (!timestampSec || isNaN(timestampSec) || timestampSec <= 0) return "Never";
  const now = Math.floor(Date.now() / 1000);
  const diff = now - timestampSec;
  if (diff < 60) return "Just now";
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

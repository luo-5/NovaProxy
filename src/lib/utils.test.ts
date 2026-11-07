import { describe, it, expect } from "vitest";
import {
  formatBytes,
  formatSpeed,
  formatLatency,
  getProtocolBadge,
  formatRelativeTime,
  cn,
} from "./utils";

describe("Frontend Utility Functions", () => {
  it("formats byte values accurately across scales", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(-10)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1024)).toBe("1 KB");
    expect(formatBytes(1024 * 1024)).toBe("1 MB");
    expect(formatBytes(1024 * 1024 * 1024)).toBe("1 GB");
    expect(formatBytes(1536)).toBe("1.5 KB");
  });

  it("formats transmission speed with /s suffix", () => {
    expect(formatSpeed(0)).toBe("0 B/s");
    expect(formatSpeed(1024)).toBe("1 KB/s");
    expect(formatSpeed(1024 * 1024 * 2.5)).toBe("2.5 MB/s");
  });

  it("categorizes latency ms correctly with color badges", () => {
    const unknown = formatLatency(null);
    expect(unknown.text).toBe("-- ms");
    expect(unknown.badgeClass).toContain("bg-muted");

    const timeout = formatLatency(-1);
    expect(timeout.text).toBe("Timeout");
    expect(timeout.textClass).toBe("text-red-500");

    const fast = formatLatency(80);
    expect(fast.text).toBe("80 ms");
    expect(fast.textClass).toBe("text-emerald-500");

    const medium = formatLatency(220);
    expect(medium.text).toBe("220 ms");
    expect(medium.textClass).toBe("text-amber-500");

    const high = formatLatency(650);
    expect(high.text).toBe("650 ms");
    expect(high.textClass).toBe("text-orange-500");
  });

  it("returns distinct badges for all proxy protocols", () => {
    const protocols = [
      "vmess",
      "vless",
      "trojan",
      "shadowsocks",
      "shadowsocksr",
      "hysteria",
      "hysteria2",
      "tuic",
      "wireguard",
      "shadowtls",
      "naive",
      "mieru",
      "anytls",
      "ssh",
      "socks5",
      "http",
      "snell",
    ];

    for (const proto of protocols) {
      const badge = getProtocolBadge(proto);
      expect(badge.label).toBeDefined();
      expect(badge.bg).toBeDefined();
      expect(badge.text).toBeDefined();
    }
  });

  it("formats relative timestamps gracefully", () => {
    expect(formatRelativeTime(null)).toBe("Never");
    expect(formatRelativeTime(undefined)).toBe("Never");
    expect(formatRelativeTime(0)).toBe("Never");
    expect(formatRelativeTime(NaN)).toBe("Never");

    const nowSec = Math.floor(Date.now() / 1000);
    expect(formatRelativeTime(nowSec - 10)).toBe("Just now");
    expect(formatRelativeTime(nowSec - 120)).toBe("2m ago");
    expect(formatRelativeTime(nowSec - 7200)).toBe("2h ago");
    expect(formatRelativeTime(nowSec - 172800)).toBe("2d ago");
  });

  it("merges tailwind classnames with clsx and twMerge", () => {
    const isHidden = false;
    const res = cn("px-2 py-1", isHidden && "hidden", "px-4");
    expect(res).toBe("py-1 px-4");
  });
});

import { describe, it, expect } from "vitest";
import zhCn from "../i18n/locales/zh-CN.json";
import enUs from "../i18n/locales/en-US.json";
import type { ProxyNode } from "../types/bindings";
import {
  computeVirtualGridWindow,
  getNodeCapabilityHints,
  reorderItemsById,
  reorderItemsByIndex,
} from "./listHelpers";

function makeBaseNode(overrides: Partial<ProxyNode>): ProxyNode {
  return {
    id: "n-1",
    name: "Test Node",
    server: "example.com",
    port: 443,
    protocol: "vless",
    tls: null,
    transport: null,
    settings: {
      kind: "vless",
      uuid: "00000000-0000-0000-0000-000000000001",
      flow: null,
      encryption: "none",
      packet_encoding: null,
    },
    subscription_id: null,
    group_id: null,
    latency: null,
    dialer_proxy: null,
    preferred_core: null,
    sort_order: 0,
    ...overrides,
  };
}

function collectKeyPaths(obj: Record<string, unknown>, prefix = ""): string[] {
  const paths: string[] = [];
  for (const [k, v] of Object.entries(obj)) {
    const nextPath = prefix ? `${prefix}.${k}` : k;
    if (v && typeof v === "object" && !Array.isArray(v)) {
      paths.push(...collectKeyPaths(v as Record<string, unknown>, nextPath));
    } else {
      paths.push(nextPath);
    }
  }
  return paths.sort();
}

describe("Virtual Grid Window & Rule Reorder Helpers", () => {
  it("handles empty and large 2000-node virtual scroll windows", () => {
    const empty = computeVirtualGridWindow({
      totalItems: 0,
      columns: 2,
      rowHeight: 108,
      scrollTop: 0,
      viewportHeight: 540,
    });
    expect(empty.startIndex).toBe(0);
    expect(empty.endIndex).toBe(0);
    expect(empty.paddingTop).toBe(0);
    expect(empty.paddingBottom).toBe(0);

    const mid = computeVirtualGridWindow({
      totalItems: 2001,
      columns: 2,
      rowHeight: 108,
      scrollTop: 108 * 50,
      viewportHeight: 540,
      overscanRows: 3,
    });
    expect(mid.totalRows).toBe(1001);
    expect(mid.startIndex).toBe((50 - 3) * 2);
    expect(mid.endIndex).toBe((50 + 5 + 3) * 2);
    expect(mid.endIndex - mid.startIndex).toBeLessThan(30);
    expect(mid.paddingTop).toBe((50 - 3) * 108);
    expect(mid.paddingBottom).toBe((1001 - (50 + 5 + 3)) * 108);

    const end = computeVirtualGridWindow({
      totalItems: 2001,
      columns: 2,
      rowHeight: 108,
      scrollTop: 108 * 2000,
      viewportHeight: 540,
      overscanRows: 2,
    });
    expect(end.endIndex).toBe(2001);
    expect(end.paddingBottom).toBe(0);
  });

  it("reorders custom rules cleanly by index and drag id", () => {
    const rules = [
      { id: "r1", pattern: "a.com" },
      { id: "r2", pattern: "b.com" },
      { id: "r3", pattern: "c.com" },
    ];

    const movedForward = reorderItemsById(rules, "r1", "r3");
    expect(movedForward.map((r) => r.id)).toEqual(["r2", "r3", "r1"]);
    // Must not mutate original array
    expect(rules.map((r) => r.id)).toEqual(["r1", "r2", "r3"]);

    const movedBack = reorderItemsByIndex(movedForward, 2, 0);
    expect(movedBack.map((r) => r.id)).toEqual(["r1", "r2", "r3"]);

    const noopMissing = reorderItemsById(rules, "r1", "missing");
    expect(noopMissing.map((r) => r.id)).toEqual(["r1", "r2", "r3"]);
  });

  it("extracts Hysteria2 hopping ports and specialized/relay engine badges", () => {
    const hy2Node = makeBaseNode({
      protocol: "hysteria2",
      settings: {
        kind: "hysteria2",
        password: "secret",
        up_mbps: 50,
        down_mbps: 200,
        obfs_type: null,
        obfs_password: null,
        ports: "20000-30000",
        hop_interval: 30,
      },
    });
    expect(getNodeCapabilityHints(hy2Node)).toEqual({
      hoppingPorts: "20000-30000",
      engineLabel: null,
      isRelayRequired: false,
    });

    const xhttpNode = makeBaseNode({
      protocol: "vless",
      transport: {
        type: "xhttp",
        host: "cdn.example.com",
        path: "/xh",
        mode: "auto",
        extra: null,
      },
    });
    expect(getNodeCapabilityHints(xhttpNode)).toEqual({
      hoppingPorts: null,
      engineLabel: "Xray",
      isRelayRequired: true,
    });

    const snellNode = makeBaseNode({
      protocol: "snell",
      settings: {
        kind: "snell",
        psk: "secret",
        version: 4,
        obfs_mode: null,
        obfs_host: null,
      },
    });
    expect(getNodeCapabilityHints(snellNode)).toEqual({
      hoppingPorts: null,
      engineLabel: "Mihomo",
      isRelayRequired: false,
    });
  });

  it("maintains 100% key parity between zh-CN and en-US locales", () => {
    const zhKeys = collectKeyPaths(zhCn as Record<string, unknown>);
    const enKeys = collectKeyPaths(enUs as Record<string, unknown>);
    expect(zhKeys).toEqual(enKeys);
  });
});

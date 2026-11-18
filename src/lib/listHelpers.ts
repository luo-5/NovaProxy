import type { ProxyNode } from "../types/bindings";

export interface VirtualGridWindow {
  startIndex: number;
  endIndex: number;
  paddingTop: number;
  paddingBottom: number;
  totalRows: number;
  columns: number;
  rowHeight: number;
}

/**
 * Computes the visible slice and spacer padding for a uniform-row-height virtual grid.
 */
export function computeVirtualGridWindow(params: {
  totalItems: number;
  columns: number;
  rowHeight: number;
  scrollTop: number;
  viewportHeight: number;
  overscanRows?: number;
}): VirtualGridWindow {
  const columns = Math.max(1, Math.floor(params.columns));
  const rowHeight = Math.max(1, Math.floor(params.rowHeight));
  const totalItems = Math.max(0, Math.floor(params.totalItems));
  const overscanRows = Math.max(0, Math.floor(params.overscanRows ?? 3));

  if (totalItems === 0) {
    return {
      startIndex: 0,
      endIndex: 0,
      paddingTop: 0,
      paddingBottom: 0,
      totalRows: 0,
      columns,
      rowHeight,
    };
  }

  const totalRows = Math.ceil(totalItems / columns);
  const safeViewport = Math.max(rowHeight, params.viewportHeight);
  const safeScrollTop = Math.max(0, params.scrollTop);

  const firstVisibleRow = Math.floor(safeScrollTop / rowHeight);
  const visibleRowCount = Math.ceil(safeViewport / rowHeight);

  const startRow = Math.max(0, firstVisibleRow - overscanRows);
  const endRow = Math.min(totalRows, firstVisibleRow + visibleRowCount + overscanRows);

  const startIndex = Math.min(totalItems, startRow * columns);
  const endIndex = Math.min(totalItems, endRow * columns);

  return {
    startIndex,
    endIndex,
    paddingTop: startRow * rowHeight,
    paddingBottom: Math.max(0, (totalRows - endRow) * rowHeight),
    totalRows,
    columns,
    rowHeight,
  };
}

/**
 * Reorders items by index without mutating the source array.
 */
export function reorderItemsByIndex<T>(
  items: readonly T[],
  fromIndex: number,
  toIndex: number
): T[] {
  if (
    fromIndex < 0 ||
    toIndex < 0 ||
    fromIndex >= items.length ||
    toIndex >= items.length ||
    fromIndex === toIndex
  ) {
    return [...items];
  }
  const next = [...items];
  const [moved] = next.splice(fromIndex, 1);
  next.splice(toIndex, 0, moved);
  return next;
}

/**
 * Reorders items by dragging `draggedId` onto `targetId`.
 */
export function reorderItemsById<T extends { id: string }>(
  items: readonly T[],
  draggedId: string,
  targetId: string
): T[] {
  if (!draggedId || !targetId || draggedId === targetId) {
    return [...items];
  }
  const fromIndex = items.findIndex((item) => item.id === draggedId);
  const toIndex = items.findIndex((item) => item.id === targetId);
  return reorderItemsByIndex(items, fromIndex, toIndex);
}

export interface NodeCapabilityHints {
  hoppingPorts: string | null;
  engineLabel: string | null;
  isRelayRequired: boolean;
}

/**
 * Extracts display badges for Hysteria2 port hopping and specialized/relay cores.
 */
export function getNodeCapabilityHints(node: ProxyNode): NodeCapabilityHints {
  const hoppingPorts =
    node.settings.kind === "hysteria2" && node.settings.ports
      ? node.settings.ports
      : null;

  const transportType = node.transport?.type;
  if (transportType === "xhttp" || transportType === "mkcp") {
    return {
      hoppingPorts,
      engineLabel: "Xray",
      isRelayRequired: true,
    };
  }

  if (
    node.protocol === "shadowsocksr" ||
    node.protocol === "snell" ||
    node.protocol === "mieru"
  ) {
    return {
      hoppingPorts,
      engineLabel: "Mihomo",
      isRelayRequired: false,
    };
  }

  if (node.preferred_core) {
    return {
      hoppingPorts,
      engineLabel: node.preferred_core,
      isRelayRequired: false,
    };
  }

  return {
    hoppingPorts,
    engineLabel: null,
    isRelayRequired: false,
  };
}

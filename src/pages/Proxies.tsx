import React, { useEffect, useState, useMemo } from "react";
import { useTranslation } from "react-i18next";
import {
  Server,
  Plus,
  Zap,
  Search,
  Check,
  QrCode,
  Copy,
  Trash2,
  RefreshCw,
  CheckSquare,
  Square,
} from "lucide-react";
import { useAppStore } from "../stores/appStore";
import {
  listNodes,
  listSubscriptions,
  deleteNode,
  selectActiveNode,
  testNodeLatency,
  testAllNodesLatency,
  exportNodeLink,
  type ProxyNode,
  type Subscription,
} from "../lib/ipc";
import { formatLatency, getProtocolBadge } from "../lib/utils";
import { AddNodeModal } from "../components/AddNodeModal";
import { QrCodeModal } from "../components/QrCodeModal";

export const Proxies: React.FC = () => {
  const { t } = useTranslation();
  const { activeNode, setActiveNode } = useAppStore();

  const [nodes, setNodes] = useState<ProxyNode[]>([]);
  const [subscriptions, setSubscriptions] = useState<Subscription[]>([]);
  const [loading, setLoading] = useState<boolean>(true);
  const [testingAll, setTestingAll] = useState<boolean>(false);
  const [testingIds, setTestingIds] = useState<Set<string>>(new Set());

  // Filters & Search
  const [searchQuery, setSearchQuery] = useState<string>("");
  const [selectedSubId, setSelectedSubId] = useState<string>("all");
  const [selectedProtocol, setSelectedProtocol] = useState<string>("all");
  const [sortBy, setSortBy] = useState<"order" | "latency" | "name">("order");

  // Selection & Modals
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [isAddOpen, setIsAddOpen] = useState<boolean>(false);
  const [qrNode, setQrNode] = useState<ProxyNode | null>(null);
  const [toastMsg, setToastMsg] = useState<string | null>(null);

  const fetchAll = async () => {
    try {
      const [nList, sList] = await Promise.all([listNodes(), listSubscriptions()]);
      setNodes(nList);
      setSubscriptions(sList);
    } catch {
      // ignore
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchAll();
  }, []);

  const showToast = (msg: string) => {
    setToastMsg(msg);
    setTimeout(() => setToastMsg(null), 2500);
  };

  // Select active node
  const handleSelectActive = async (node: ProxyNode) => {
    try {
      await selectActiveNode(node.id);
      setActiveNode(node);
      showToast(`Selected: ${node.name}`);
    } catch (err) {
      showToast(`Error: ${String(err)}`);
    }
  };

  // Test single node latency
  const handleTestLatency = async (e: React.MouseEvent, id: string) => {
    e.stopPropagation();
    if (testingIds.has(id)) return;
    setTestingIds((prev) => new Set(prev).add(id));
    try {
      const ms = await testNodeLatency(id);
      setNodes((prev) =>
        prev.map((n) => (n.id === id ? { ...n, latency: ms } : n))
      );
      if (activeNode?.id === id) {
        setActiveNode({ ...activeNode, latency: ms });
      }
    } finally {
      setTestingIds((prev) => {
        const next = new Set(prev);
        next.delete(id);
        return next;
      });
    }
  };

  // Test all nodes latency
  const handleTestAll = async () => {
    if (testingAll) return;
    setTestingAll(true);
    try {
      const results = await testAllNodesLatency();
      const map = new Map<string, number | null>(results);
      setNodes((prev) =>
        prev.map((n) => (map.has(n.id) ? { ...n, latency: map.get(n.id)! } : n))
      );
      if (activeNode && map.has(activeNode.id)) {
        setActiveNode({ ...activeNode, latency: map.get(activeNode.id)! });
      }
      showToast("Latency test completed");
    } finally {
      setTestingAll(false);
    }
  };

  // Copy share link
  const handleCopyLink = async (e: React.MouseEvent, node: ProxyNode) => {
    e.stopPropagation();
    try {
      const link = await exportNodeLink(node);
      await navigator.clipboard.writeText(link);
      showToast(t("common.copied"));
    } catch (err) {
      showToast(String(err));
    }
  };

  // Delete single node
  const handleDeleteNode = async (e: React.MouseEvent, id: string) => {
    e.stopPropagation();
    if (!window.confirm(t("proxies.deleteConfirm"))) return;
    try {
      await deleteNode(id);
      setNodes((prev) => prev.filter((n) => n.id !== id));
      if (activeNode?.id === id) {
        setActiveNode(null);
      }
      showToast("Node deleted");
    } catch (err) {
      showToast(String(err));
    }
  };

  // Batch delete selected
  const handleBatchDelete = async () => {
    if (selectedIds.size === 0) return;
    if (!window.confirm(`Delete ${selectedIds.size} selected nodes?`)) return;
    for (const id of selectedIds) {
      await deleteNode(id).catch(() => {});
    }
    setNodes((prev) => prev.filter((n) => !selectedIds.has(n.id)));
    setSelectedIds(new Set());
    showToast("Batch delete completed");
  };

  // Toggle selection
  const toggleSelect = (id: string) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  // Filtered & sorted nodes
  const filteredNodes = useMemo(() => {
    return nodes
      .filter((node) => {
        // 1. Subscription filter
        if (selectedSubId !== "all") {
          if (selectedSubId === "manual" && node.subscription_id !== null) return false;
          if (selectedSubId !== "manual" && node.subscription_id !== selectedSubId) return false;
        }
        // 2. Protocol filter
        if (selectedProtocol !== "all" && node.protocol.toLowerCase() !== selectedProtocol.toLowerCase()) {
          return false;
        }
        // 3. Search query
        if (searchQuery.trim()) {
          const q = searchQuery.toLowerCase();
          const matchName = node.name.toLowerCase().includes(q);
          const matchServer = node.server.toLowerCase().includes(q);
          const matchProto = node.protocol.toLowerCase().includes(q);
          if (!matchName && !matchServer && !matchProto) return false;
        }
        return true;
      })
      .sort((a, b) => {
        if (sortBy === "latency") {
          const latA = a.latency === null || a.latency === undefined ? 999999 : a.latency < 0 ? 999998 : a.latency;
          const latB = b.latency === null || b.latency === undefined ? 999999 : b.latency < 0 ? 999998 : b.latency;
          return latA - latB;
        }
        if (sortBy === "name") {
          return a.name.localeCompare(b.name);
        }
        return a.sort_order - b.sort_order;
      });
  }, [nodes, selectedSubId, selectedProtocol, searchQuery, sortBy]);

  // Distinct protocols for filter pill-bar
  const availableProtocols = useMemo(() => {
    const set = new Set<string>();
    for (const n of nodes) {
      set.add(n.protocol.toLowerCase());
    }
    return Array.from(set);
  }, [nodes]);

  const selectAll = () => {
    if (selectedIds.size === filteredNodes.length) {
      setSelectedIds(new Set());
    } else {
      setSelectedIds(new Set(filteredNodes.map((n) => n.id)));
    }
  };

  return (
    <div className="p-8 max-w-5xl mx-auto space-y-6">
      {/* Toast Notification */}
      {toastMsg && (
        <div className="fixed bottom-6 right-6 z-50 px-4 py-2.5 rounded-xl bg-card border border-border text-foreground shadow-2xl text-xs font-medium animate-in fade-in slide-in-from-bottom-2">
          {toastMsg}
        </div>
      )}

      {/* Header */}
      <div className="flex flex-wrap items-center justify-between gap-4">
        <div>
          <h2 className="text-xl font-bold">{t("proxies.title")}</h2>
          <p className="text-xs text-muted-foreground">{t("proxies.subtitle")}</p>
        </div>

        <div className="flex items-center gap-2">
          <button
            disabled={testingAll || nodes.length === 0}
            onClick={handleTestAll}
            className="flex items-center gap-1.5 px-3 py-1.5 bg-secondary text-secondary-foreground text-xs font-medium rounded-xl hover:bg-secondary/80 transition-colors disabled:opacity-50"
          >
            <Zap className={`w-3.5 h-3.5 text-amber-500 ${testingAll ? "animate-spin" : ""}`} />
            <span>{testingAll ? t("proxies.testing") : t("proxies.testAll")}</span>
          </button>

          <button
            onClick={() => setIsAddOpen(true)}
            className="flex items-center gap-1.5 px-3.5 py-1.5 bg-primary text-primary-foreground text-xs font-medium rounded-xl hover:bg-primary/90 transition-colors shadow-sm"
          >
            <Plus className="w-4 h-4" />
            <span>{t("proxies.addNode")}</span>
          </button>
        </div>
      </div>

      {/* Search & Filter Toolbar */}
      <div className="bg-card border border-border rounded-2xl p-4 shadow-sm space-y-3">
        <div className="grid grid-cols-1 md:grid-cols-4 gap-3">
          {/* Search Input */}
          <div className="relative md:col-span-2">
            <Search className="w-4 h-4 absolute left-3 top-2.5 text-muted-foreground" />
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder={t("proxies.searchPlaceholder")}
              className="w-full pl-9 pr-3 py-1.5 rounded-xl bg-secondary/50 border border-border text-xs focus:outline-none focus:ring-1 focus:ring-primary"
            />
          </div>

          {/* Subscription Filter */}
          <div>
            <select
              value={selectedSubId}
              onChange={(e) => setSelectedSubId(e.target.value)}
              className="w-full px-3 py-1.5 rounded-xl bg-secondary/50 border border-border text-xs focus:outline-none focus:ring-1 focus:ring-primary"
            >
              <option value="all">All Subscriptions</option>
              <option value="manual">Manual / Clipboard</option>
              {subscriptions.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
            </select>
          </div>

          {/* Sort By */}
          <div>
            <select
              value={sortBy}
              onChange={(e) => setSortBy(e.target.value as "order" | "latency" | "name")}
              className="w-full px-3 py-1.5 rounded-xl bg-secondary/50 border border-border text-xs focus:outline-none focus:ring-1 focus:ring-primary"
            >
              <option value="order">Sort: Custom Order</option>
              <option value="latency">Sort: Latency (Low to High)</option>
              <option value="name">Sort: Name (A-Z)</option>
            </select>
          </div>
        </div>

        {/* Protocol Filter Pills */}
        <div className="flex flex-wrap items-center gap-1.5 pt-1 border-t border-border/50">
          <button
            onClick={() => setSelectedProtocol("all")}
            className={`px-2.5 py-1 rounded-lg text-[11px] font-medium transition-all ${
              selectedProtocol === "all"
                ? "bg-primary text-primary-foreground shadow-sm"
                : "bg-secondary/40 text-muted-foreground hover:text-foreground"
            }`}
          >
            All ({nodes.length})
          </button>
          {availableProtocols.map((p) => {
            const count = nodes.filter((n) => n.protocol.toLowerCase() === p).length;
            const badge = getProtocolBadge(p);
            return (
              <button
                key={p}
                onClick={() => setSelectedProtocol(p)}
                className={`px-2.5 py-1 rounded-lg text-[11px] font-medium transition-all ${
                  selectedProtocol === p
                    ? "bg-primary text-primary-foreground shadow-sm"
                    : `${badge.bg} ${badge.text} hover:opacity-80`
                }`}
              >
                {badge.label} ({count})
              </button>
            );
          })}
        </div>
      </div>

      {/* Batch Action Bar if any selected */}
      {selectedIds.size > 0 && (
        <div className="bg-secondary/60 border border-border rounded-xl px-4 py-2.5 flex items-center justify-between text-xs animate-in fade-in">
          <div className="flex items-center gap-3">
            <button
              onClick={selectAll}
              className="flex items-center gap-1.5 font-semibold text-primary hover:underline"
            >
              <CheckSquare className="w-4 h-4" />
              <span>{t("proxies.selectedCount", { count: selectedIds.size })}</span>
            </button>
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={handleBatchDelete}
              className="flex items-center gap-1 px-3 py-1 bg-destructive/10 text-destructive rounded-lg hover:bg-destructive/20 font-medium transition-colors"
            >
              <Trash2 className="w-3.5 h-3.5" />
              <span>{t("proxies.deleteBatch")}</span>
            </button>
          </div>
        </div>
      )}

      {/* Nodes Grid / List */}
      {loading ? (
        <div className="p-12 text-center text-xs text-muted-foreground">
          {t("common.loading")}
        </div>
      ) : filteredNodes.length === 0 ? (
        <div className="bg-card border border-border rounded-2xl p-12 text-center text-muted-foreground flex flex-col items-center justify-center">
          <Server className="w-12 h-12 stroke-[1.5] mb-3 text-muted-foreground/60" />
          <p className="text-sm font-medium">{t("proxies.noNodes")}</p>
          <p className="text-xs text-muted-foreground/80 mt-1 max-w-sm">
            {t("proxies.noNodesHint")}
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
          {filteredNodes.map((node) => {
            const isActive = activeNode?.id === node.id;
            const isSelected = selectedIds.has(node.id);
            const isTesting = testingIds.has(node.id);
            const badge = getProtocolBadge(node.protocol);
            const lat = formatLatency(node.latency);

            return (
              <div
                key={node.id}
                onClick={() => handleSelectActive(node)}
                className={`p-4 rounded-2xl border transition-all cursor-pointer select-none relative group ${
                  isActive
                    ? "bg-primary/5 border-primary shadow-sm ring-1 ring-primary/30"
                    : "bg-card border-border hover:border-primary/50 hover:shadow-sm"
                }`}
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="flex items-start gap-3 min-w-0">
                    {/* Active check or multiselect checkbox */}
                    <div
                      onClick={(e) => {
                        e.stopPropagation();
                        toggleSelect(node.id);
                      }}
                      className="mt-0.5 text-muted-foreground hover:text-foreground"
                    >
                      {isSelected ? (
                        <CheckSquare className="w-4 h-4 text-primary" />
                      ) : (
                        <Square className="w-4 h-4 text-muted-foreground/40 group-hover:text-muted-foreground" />
                      )}
                    </div>

                    <div className="min-w-0">
                      <div className="flex items-center gap-2">
                        <span className="font-semibold text-xs truncate max-w-[200px]" title={node.name}>
                          {node.name}
                        </span>
                        <span
                          className={`text-[10px] px-2 py-0.2 rounded-full font-medium ${badge.bg} ${badge.text}`}
                        >
                          {badge.label}
                        </span>
                      </div>
                      <p className="text-[11px] text-muted-foreground font-mono truncate max-w-[240px] mt-0.5">
                        {node.server}:{node.port}
                      </p>
                    </div>
                  </div>

                  {/* Latency badge & Test button */}
                  <div className="flex items-center gap-1.5 shrink-0">
                    <button
                      onClick={(e) => handleTestLatency(e, node.id)}
                      className={`text-[11px] px-2 py-0.5 rounded-lg border font-mono transition-all flex items-center gap-1 ${lat.badgeClass}`}
                    >
                      <RefreshCw
                        className={`w-2.5 h-2.5 ${isTesting ? "animate-spin text-primary" : "opacity-60"}`}
                      />
                      <span>{lat.text}</span>
                    </button>

                    {/* Actions Menu */}
                    <div className="flex items-center opacity-0 group-hover:opacity-100 transition-opacity">
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          setQrNode(node);
                        }}
                        title={t("proxies.showQr")}
                        className="p-1 rounded-lg text-muted-foreground hover:text-foreground hover:bg-secondary"
                      >
                        <QrCode className="w-3.5 h-3.5" />
                      </button>
                      <button
                        onClick={(e) => handleCopyLink(e, node)}
                        title={t("proxies.copyLink")}
                        className="p-1 rounded-lg text-muted-foreground hover:text-foreground hover:bg-secondary"
                      >
                        <Copy className="w-3.5 h-3.5" />
                      </button>
                      <button
                        onClick={(e) => handleDeleteNode(e, node.id)}
                        title={t("proxies.deleteNode")}
                        className="p-1 rounded-lg text-muted-foreground hover:text-destructive hover:bg-destructive/10"
                      >
                        <Trash2 className="w-3.5 h-3.5" />
                      </button>
                    </div>
                  </div>
                </div>

                {isActive && (
                  <div className="absolute top-2 right-2 flex items-center gap-1 text-[10px] text-primary font-medium bg-primary/10 px-2 py-0.5 rounded-full">
                    <Check className="w-3 h-3" />
                    <span>Active</span>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {/* Add Node Modal */}
      <AddNodeModal
        isOpen={isAddOpen}
        onClose={() => setIsAddOpen(false)}
        onSuccess={() => {
          fetchAll();
          showToast("Node added successfully");
        }}
      />

      {/* QR Code Modal */}
      <QrCodeModal
        node={qrNode}
        isOpen={Boolean(qrNode)}
        onClose={() => setQrNode(null)}
      />
    </div>
  );
};

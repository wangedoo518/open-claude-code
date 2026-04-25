/**
 * ForceGraph — Rowboat 原版精确复刻
 *
 * 自研弹簧-排斥物理引擎 (非 d3-force)
 * 浮动呼吸动画 (sin/cos 微幅漂浮)
 * SVG 弧形边线 (arc path, 非直线)
 * SVG feGaussianBlur 辉光滤镜
 * CSS ::before radial-gradient 点阵画布
 * 搜索栏 + 可过滤图例
 * 纯圆节点，大小按度数 6-24px
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Search, X } from "lucide-react";
import type { WikiGraphResponse, RawEntry } from "@/api/wiki/types";

/* ─── Types ───────────────────────────────────────────────────── */

type NodeCategory = "raw" | "concept" | "people" | "topic" | "compare";

interface ForceNode {
  id: string;
  label: string;
  kind: "raw" | "concept";
  category: NodeCategory;
  degree: number;
  radius: number;
  color: string;
  stroke: string;
}

interface ForceEdge {
  source: string;
  target: string;
  kind: "derived-from" | "references";
}

interface NodePosition {
  x: number;
  y: number;
  vx: number;
  vy: number;
}

export interface ForceGraphProps {
  graphData: WikiGraphResponse;
  rawEntries: RawEntry[];
  onClickConcept: (slug: string) => void;
  onClickRaw: () => void;
  /**
   * G1 sprint — optional initial value for the internal search query.
   * When set (typically from `?focus=<slug>` on the Graph page URL),
   * the existing search-match pipeline lights up the node + its
   * immediate neighbors on first mount, giving a "focus on this slug"
   * entry point without a new code path through the physics engine.
   * Users can still clear or retype the query — this is an initial
   * value, not a controlled prop.
   */
  initialSearchQuery?: string;
}

/* ─── Physics Constants (精确复制 Rowboat) ────────────────────── */

const SIMULATION_STEPS = 240;
const SPRING_LENGTH = 80;
const SPRING_STRENGTH = 0.0038;
const REPULSION = 5800;
const DAMPING = 0.83;
const MIN_DISTANCE = 34;
const CLUSTER_STRENGTH = 0.0018;
const CLUSTER_RADIUS_MIN = 120;
const CLUSTER_RADIUS_MAX = 240;
const CLUSTER_RADIUS_STEP = 45;

/* ─── Floating Animation Constants ───────────────────────────── */

const FLOAT_BASE = 3.5;
const FLOAT_VARIANCE = 2;
const FLOAT_SPEED_BASE = 0.0006;
const FLOAT_SPEED_VARIANCE = 0.00025;

/* ─── Semantic Color Palette (HSL, Rowboat style) ────────────── */

interface CategoryMeta {
  hue: number;
  sat: number;
  light: number;
  label: string;
  icon: string;
}

const CATEGORY_META: Record<NodeCategory, CategoryMeta> = {
  people:  { hue: 210, sat: 72, light: 52, label: "人物", icon: "👤" },
  concept: { hue: 28,  sat: 78, light: 52, label: "概念", icon: "💡" },
  topic:   { hue: 280, sat: 70, light: 56, label: "主题", icon: "📚" },
  compare: { hue: 55,  sat: 80, light: 52, label: "对比", icon: "⚖️" },
  raw:     { hue: 220, sat: 8,  light: 55, label: "素材", icon: "📄" },
};

function catColor(cat: NodeCategory): string {
  const m = CATEGORY_META[cat];
  return `hsl(${m.hue}, ${m.sat}%, ${m.light}%)`;
}

function catStroke(cat: NodeCategory): string {
  const m = CATEGORY_META[cat];
  return `hsl(${m.hue}, ${Math.min(100, m.sat + 8)}%, ${m.light - 12}%)`;
}

function catColorHex(cat: NodeCategory): string {
  // Simple HSL → 6-char hex for SVG filter IDs
  const m = CATEGORY_META[cat];
  return `${m.hue}-${m.sat}-${m.light}`;
}

const SOURCE_LABELS: Record<string, string> = {
  paste: "粘贴", url: "网页", "wechat-text": "微信消息",
  "wechat-article": "微信文章", "wechat-url": "微信链接",
  voice: "语音", image: "图片", pdf: "PDF", pptx: "PPT",
  docx: "文档", video: "视频", card: "名片", chat: "聊天",
};

/* ─── Floating Motion ────────────────────────────────────────── */

function hashId(id: string): number {
  let h = 0;
  for (let i = 0; i < id.length; i++) {
    h = (h << 5) - h + id.charCodeAt(i);
    h |= 0;
  }
  return Math.abs(h);
}

function getMotionSeed(id: string) {
  const n = hashId(id);
  return {
    phase: ((n % 360) * Math.PI) / 180,
    amplitude: FLOAT_BASE + (n % 7) * (FLOAT_VARIANCE / 6),
    speed: FLOAT_SPEED_BASE + (n % 5) * FLOAT_SPEED_VARIANCE,
  };
}

function getDisplayPos(
  id: string,
  base: { x: number; y: number },
  time: number,
  skipMotion: boolean
) {
  if (skipMotion) return { x: base.x, y: base.y };
  const s = getMotionSeed(id);
  const phase = s.phase + time * s.speed;
  return {
    x: base.x + Math.sin(phase) * s.amplitude,
    y: base.y + Math.cos(phase * 0.9) * s.amplitude,
  };
}

/* ─── Component ──────────────────────────────────────────────── */

export function ForceGraph({
  graphData,
  rawEntries,
  onClickConcept,
  onClickRaw,
  initialSearchQuery,
}: ForceGraphProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const positionsRef = useRef<Map<string, NodePosition>>(new Map());
  const motionTimeRef = useRef(0);
  const draggingRef = useRef<{
    id: string;
    offsetX: number;
    offsetY: number;
    moved: boolean;
  } | null>(null);
  const panningRef = useRef<{
    startX: number;
    startY: number;
    originX: number;
    originY: number;
  } | null>(null);
  const hasCenteredRef = useRef(false);

  const [viewport, setViewport] = useState({ width: 1, height: 1 });
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [zoom, setZoom] = useState(0.6);
  const [hoveredNodeId, setHoveredNodeId] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState(initialSearchQuery ?? "");
  const [selectedGroup, setSelectedGroup] = useState<NodeCategory | null>(null);
  const [selectedRelation, setSelectedRelation] = useState<ForceEdge["kind"] | null>(null);
  const [, forceRender] = useState(0);

  // Build nodes + edges from graph data
  const { nodes, edges } = useMemo(() => {
    const idToRaw = new Map<number, RawEntry>();
    for (const e of rawEntries) idToRaw.set(e.id, e);

    // Pre-count degrees
    const degreeMap = new Map<string, number>();
    for (const e of graphData.edges) {
      degreeMap.set(e.from, (degreeMap.get(e.from) ?? 0) + 1);
      degreeMap.set(e.to, (degreeMap.get(e.to) ?? 0) + 1);
    }

    const nodes: ForceNode[] = graphData.nodes.map((n) => {
      const isRaw = n.kind === "raw";
      const cat = (n.category ?? (isRaw ? "raw" : "concept")) as NodeCategory;
      const degree = degreeMap.get(n.id) ?? 0;

      let friendlyLabel = n.label;
      if (isRaw) {
        const idNum = parseInt(n.id.replace(/\D/g, ""), 10);
        const raw = !isNaN(idNum) ? idToRaw.get(idNum) : undefined;
        if (raw) {
          const srcLabel = SOURCE_LABELS[raw.source] ?? raw.source;
          friendlyLabel = `${srcLabel} #${raw.id}`;
        } else if (n.label.includes(": ")) {
          const src = n.label.split(": ")[0];
          friendlyLabel = SOURCE_LABELS[src] ?? src;
        }
      }
      if (friendlyLabel.length > 14) friendlyLabel = friendlyLabel.slice(0, 12) + "…";

      return {
        id: n.id,
        label: friendlyLabel,
        kind: n.kind,
        category: cat,
        degree,
        radius: 6 + Math.min(18, degree * 2),
        color: catColor(cat),
        stroke: catStroke(cat),
      };
    });

    const nodeIds = new Set(nodes.map((n) => n.id));
    const edges: ForceEdge[] = graphData.edges
      .filter((e) => nodeIds.has(e.from) && nodeIds.has(e.to) && e.from !== e.to)
      .map((e) => ({ source: e.from, target: e.to, kind: e.kind }));

    return { nodes, edges };
  }, [graphData, rawEntries]);

  const visibleEdges = useMemo(
    () => edges.filter((edge) => !selectedRelation || edge.kind === selectedRelation),
    [edges, selectedRelation]
  );

  const relationItems = useMemo(() => {
    const counts = new Map<ForceEdge["kind"], number>();
    edges.forEach((edge) => counts.set(edge.kind, (counts.get(edge.kind) ?? 0) + 1));
    return [
      {
        kind: "derived-from" as const,
        label: "Sources",
        count: counts.get("derived-from") ?? 0,
      },
      {
        kind: "references" as const,
        label: "Wikilinks",
        count: counts.get("references") ?? 0,
      },
    ].filter((item) => item.count > 0);
  }, [edges]);

  // Group → category mapping & cluster centers
  const nodeGroupMap = useMemo(() => {
    const map = new Map<string, NodeCategory>();
    nodes.forEach((n) => map.set(n.id, n.category));
    return map;
  }, [nodes]);

  const groupCenters = useMemo(() => {
    const groups = Array.from(new Set(nodes.map((n) => n.category)));
    if (groups.length === 0) return new Map<NodeCategory, { x: number; y: number }>();
    const radius = Math.min(
      CLUSTER_RADIUS_MAX,
      Math.max(CLUSTER_RADIUS_MIN, groups.length * CLUSTER_RADIUS_STEP)
    );
    const centers = new Map<NodeCategory, { x: number; y: number }>();
    groups.forEach((g, i) => {
      const angle = (i / groups.length) * Math.PI * 2;
      centers.set(g, { x: radius * Math.cos(angle), y: radius * Math.sin(angle) });
    });
    return centers;
  }, [nodes]);

  // Legend items
  const legendItems = useMemo(() => {
    const seen = new Map<NodeCategory, { count: number }>();
    nodes.forEach((n) => {
      const existing = seen.get(n.category);
      if (existing) existing.count++;
      else seen.set(n.category, { count: 1 });
    });
    return Array.from(seen.entries())
      .map(([cat, { count }]) => ({ cat, count, ...CATEGORY_META[cat], color: catColor(cat), stroke: catStroke(cat) }))
      .sort((a, b) => b.count - a.count);
  }, [nodes]);

  // ResizeObserver
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const ro = new ResizeObserver((entries) => {
      const e = entries[0];
      if (!e) return;
      const { width, height } = e.contentRect;
      setViewport({ width, height });
      if (!hasCenteredRef.current) {
        setPan({ x: width / 2, y: height / 2 });
        hasCenteredRef.current = true;
      }
    });
    ro.observe(container);
    return () => ro.disconnect();
  }, []);

  // Physics simulation (自研弹簧-排斥, 精确复制 Rowboat)
  useEffect(() => {
    if (nodes.length === 0) {
      positionsRef.current = new Map();
      return;
    }

    const nextPositions = new Map<string, NodePosition>();
    const count = nodes.length;
    const radius = Math.max(110, Math.min(220, count * 9));

    nodes.forEach((node, index) => {
      const existing = positionsRef.current.get(node.id);
      if (existing) {
        nextPositions.set(node.id, { ...existing });
        return;
      }
      const angle = (index / count) * Math.PI * 2;
      nextPositions.set(node.id, {
        x: radius * Math.cos(angle),
        y: radius * Math.sin(angle),
        vx: 0,
        vy: 0,
      });
    });

    positionsRef.current = nextPositions;

    let step = 0;
    let rafId = 0;
    let active = true;

    const simulate = () => {
      if (!active) return;
      step++;

      const positions = positionsRef.current;
      const ids = nodes.map((n) => n.id);
      const forces = new Map<string, { x: number; y: number }>();
      ids.forEach((id) => forces.set(id, { x: 0, y: 0 }));

      // Repulsion (all pairs)
      for (let i = 0; i < ids.length; i++) {
        const posA = positions.get(ids[i]);
        if (!posA) continue;
        for (let j = i + 1; j < ids.length; j++) {
          const posB = positions.get(ids[j]);
          if (!posB) continue;
          const dx = posB.x - posA.x;
          const dy = posB.y - posA.y;
          const distance = Math.max(MIN_DISTANCE, Math.hypot(dx, dy));
          const force = REPULSION / (distance * distance);
          const fx = (force * dx) / distance;
          const fy = (force * dy) / distance;
          const fA = forces.get(ids[i]);
          const fB = forces.get(ids[j]);
          if (fA) { fA.x -= fx; fA.y -= fy; }
          if (fB) { fB.x += fx; fB.y += fy; }
        }
      }

      // Springs (edges)
      edges.forEach((edge) => {
        const posA = positions.get(edge.source);
        const posB = positions.get(edge.target);
        if (!posA || !posB) return;
        const dx = posB.x - posA.x;
        const dy = posB.y - posA.y;
        const distance = Math.max(20, Math.hypot(dx, dy));
        const delta = distance - SPRING_LENGTH;
        const force = delta * SPRING_STRENGTH;
        const fx = (force * dx) / distance;
        const fy = (force * dy) / distance;
        const fA = forces.get(edge.source);
        const fB = forces.get(edge.target);
        if (fA) { fA.x += fx; fA.y += fy; }
        if (fB) { fB.x -= fx; fB.y -= fy; }
      });

      // Cluster attraction
      ids.forEach((id) => {
        const pos = positions.get(id);
        const f = forces.get(id);
        if (!pos || !f) return;
        const group = nodeGroupMap.get(id) ?? "raw";
        const center = groupCenters.get(group);
        if (!center) return;
        f.x += (center.x - pos.x) * CLUSTER_STRENGTH;
        f.y += (center.y - pos.y) * CLUSTER_STRENGTH;
      });

      // Integrate
      ids.forEach((id) => {
        const pos = positions.get(id);
        const f = forces.get(id);
        if (!pos || !f) return;
        if (draggingRef.current?.id === id) {
          pos.vx = 0;
          pos.vy = 0;
          return;
        }
        pos.vx = (pos.vx + f.x) * DAMPING;
        pos.vy = (pos.vy + f.y) * DAMPING;
        pos.x += pos.vx;
        pos.y += pos.vy;
      });

      forceRender((p) => p + 1);
      if (step < SIMULATION_STEPS) rafId = requestAnimationFrame(simulate);
    };

    rafId = requestAnimationFrame(simulate);
    return () => { active = false; cancelAnimationFrame(rafId); };
  }, [nodes, edges, groupCenters, nodeGroupMap]);

  // Floating animation loop
  useEffect(() => {
    if (nodes.length === 0) return;
    let rafId = 0;
    let lastTime = performance.now();
    const animate = (time: number) => {
      const delta = time - lastTime;
      if (delta >= 32) {
        motionTimeRef.current += delta;
        lastTime = time;
        forceRender((p) => p + 1);
      }
      rafId = requestAnimationFrame(animate);
    };
    rafId = requestAnimationFrame(animate);
    return () => cancelAnimationFrame(rafId);
  }, [nodes.length]);

  // Pointer → graph coordinate
  const getGraphPoint = useCallback(
    (e: React.PointerEvent) => {
      const c = containerRef.current;
      if (!c) return { x: 0, y: 0 };
      const rect = c.getBoundingClientRect();
      return {
        x: (e.clientX - rect.left - pan.x) / zoom,
        y: (e.clientY - rect.top - pan.y) / zoom,
      };
    },
    [pan.x, pan.y, zoom]
  );

  // Interaction handlers (精确复制 Rowboat)
  const handlePointerDown = (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    panningRef.current = {
      startX: e.clientX,
      startY: e.clientY,
      originX: pan.x,
      originY: pan.y,
    };
  };

  const handlePointerMove = (e: React.PointerEvent) => {
    if (draggingRef.current) {
      const point = getGraphPoint(e);
      const pos = positionsRef.current.get(draggingRef.current.id);
      if (pos) {
        pos.x = point.x - draggingRef.current.offsetX;
        pos.y = point.y - draggingRef.current.offsetY;
        draggingRef.current.moved = true;
        forceRender((p) => p + 1);
      }
      return;
    }
    if (panningRef.current) {
      setPan({
        x: panningRef.current.originX + (e.clientX - panningRef.current.startX),
        y: panningRef.current.originY + (e.clientY - panningRef.current.startY),
      });
    }
  };

  const handlePointerUp = () => {
    const d = draggingRef.current;
    if (d) {
      if (!d.moved) {
        const node = nodes.find((n) => n.id === d.id);
        if (node) {
          if (node.kind === "concept") onClickConcept(node.id.replace(/^wiki-/, ""));
          else onClickRaw();
        }
      }
      draggingRef.current = null;
    }
    panningRef.current = null;
  };

  const handleWheel = (e: React.WheelEvent) => {
    e.preventDefault();
    const nd =
      e.deltaMode === 1 ? e.deltaY * 16 : e.deltaMode === 2 ? e.deltaY * viewport.height : e.deltaY;
    const sensitivity = Math.abs(nd) < 40 ? 0.004 : 0.0022;
    const factor = Math.exp(-nd * sensitivity);
    const next = Math.min(2.5, Math.max(0.4, zoom * factor));
    if (next === zoom) return;
    const c = containerRef.current;
    if (!c) { setZoom(next); return; }
    const rect = c.getBoundingClientRect();
    const cx = e.clientX - rect.left;
    const cy = e.clientY - rect.top;
    const gx = (cx - pan.x) / zoom;
    const gy = (cy - pan.y) / zoom;
    setZoom(next);
    setPan({ x: cx - gx * next, y: cy - gy * next });
  };

  const startDragNode = (e: React.PointerEvent, nodeId: string) => {
    e.stopPropagation();
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    const point = getGraphPoint(e);
    const pos = positionsRef.current.get(nodeId);
    if (!pos) return;
    const dp = getDisplayPos(nodeId, pos, motionTimeRef.current, false);
    draggingRef.current = {
      id: nodeId,
      offsetX: point.x - dp.x,
      offsetY: point.y - dp.y,
      moved: false,
    };
  };

  // Compute display positions
  const displayPositions = new Map<string, { x: number; y: number }>();
  nodes.forEach((node) => {
    const pos = positionsRef.current.get(node.id);
    if (!pos) return;
    const isDragging = draggingRef.current?.id === node.id;
    displayPositions.set(node.id, getDisplayPos(node.id, pos, motionTimeRef.current, isDragging));
  });

  // Active / connected
  const activeNodeId = hoveredNodeId ?? draggingRef.current?.id ?? null;
  const connectedNodes = useMemo(() => {
    if (!activeNodeId) return null;
    const s = new Set([activeNodeId]);
    visibleEdges.forEach((e) => {
      if (e.source === activeNodeId) s.add(e.target);
      if (e.target === activeNodeId) s.add(e.source);
    });
    return s;
  }, [activeNodeId, visibleEdges]);

  // Search
  const searchMatchingNodes = useMemo(() => {
    if (!searchQuery.trim()) return null;
    const q = searchQuery.toLowerCase();
    const direct = new Set<string>();
    nodes.forEach((n) => {
      if (n.label.toLowerCase().includes(q) || n.id.toLowerCase().includes(q)) direct.add(n.id);
    });
    const withConn = new Set(direct);
    visibleEdges.forEach((e) => {
      if (direct.has(e.source)) withConn.add(e.target);
      if (direct.has(e.target)) withConn.add(e.source);
    });
    return { matches: withConn, directMatches: direct };
  }, [searchQuery, nodes, visibleEdges]);

  // Unique colors for glow filters
  const uniqueColors = useMemo(
    () => Array.from(new Set(nodes.map((n) => n.category))),
    [nodes]
  );

  // Derive label color from theme
  const labelFill = "#9ca3af";

  return (
    <div ref={containerRef} className="graph-view relative h-full w-full">
      {/* Legend — top-right (精确复制 Rowboat) */}
      {legendItems.length > 0 && (
        <div
          className="absolute right-3 top-3 z-20 rounded-md border border-border/80 bg-background/90 px-3 py-2 text-xs text-foreground shadow-sm backdrop-blur"
          onPointerDown={(e) => e.stopPropagation()}
        >
          <div className="mb-2 text-[0.7rem] font-semibold uppercase tracking-wide text-muted-foreground">
            分类
          </div>
          <div className="grid gap-1">
            {legendItems.map((item) => {
              const isSelected = selectedGroup === item.cat;
              return (
                <button
                  key={item.cat}
                  onClick={() => setSelectedGroup(isSelected ? null : item.cat)}
                  className={`flex items-center gap-2 rounded px-1.5 py-1 text-left transition-colors hover:bg-foreground/10 ${
                    isSelected ? "bg-foreground/15" : ""
                  }`}
                >
                  <span
                    className="inline-flex h-2.5 w-2.5 rounded-full"
                    style={{ backgroundColor: item.color, boxShadow: `0 0 0 1px ${item.stroke}` }}
                  />
                  <span className="truncate">
                    {item.icon} {item.label}
                  </span>
                  <span className="ml-auto text-muted-foreground">{item.count}</span>
                  <X className={`size-3 ${isSelected ? "text-muted-foreground" : "invisible"}`} />
                </button>
              );
            })}
          </div>
        </div>
      )}

      {relationItems.length > 0 && (
        <div
          className="absolute left-3 top-3 z-20 rounded-md border border-border/80 bg-background/90 px-3 py-2 text-xs text-foreground shadow-sm backdrop-blur"
          onPointerDown={(e) => e.stopPropagation()}
        >
          <div className="mb-2 text-[0.7rem] font-semibold uppercase tracking-wide text-muted-foreground">
            Relations
          </div>
          <div className="grid gap-1">
            {relationItems.map((item) => {
              const isSelected = selectedRelation === item.kind;
              return (
                <button
                  key={item.kind}
                  onClick={() => setSelectedRelation(isSelected ? null : item.kind)}
                  className={`flex items-center gap-2 rounded px-1.5 py-1 text-left transition-colors hover:bg-foreground/10 ${
                    isSelected ? "bg-foreground/15" : ""
                  }`}
                >
                  <span className="inline-flex h-2.5 w-2.5 rounded-full bg-foreground/45" />
                  <span className="truncate">{item.label}</span>
                  <span className="ml-auto text-muted-foreground">{item.count}</span>
                  <X className={`size-3 ${isSelected ? "text-muted-foreground" : "invisible"}`} />
                </button>
              );
            })}
          </div>
        </div>
      )}

      {/* SVG Canvas */}
      <svg
        className="h-full w-full touch-none"
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onPointerLeave={() => {
          handlePointerUp();
          setHoveredNodeId(null);
        }}
        onWheel={handleWheel}
      >
        <rect width={viewport.width} height={viewport.height} fill="transparent" />

        {/* Glow filters */}
        <defs>
          {uniqueColors.map((cat) => (
            <filter
              key={cat}
              id={`glow-${catColorHex(cat)}`}
              x="-50%"
              y="-50%"
              width="200%"
              height="200%"
            >
              <feGaussianBlur stdDeviation="4" result="coloredBlur" />
              <feMerge>
                <feMergeNode in="coloredBlur" />
                <feMergeNode in="SourceGraphic" />
              </feMerge>
            </filter>
          ))}
        </defs>

        <g transform={`translate(${pan.x} ${pan.y}) scale(${zoom})`}>
          {/* Edges — arc paths */}
          {visibleEdges.map((edge, i) => {
            const source = displayPositions.get(edge.source);
            const target = displayPositions.get(edge.target);
            if (!source || !target) return null;

            const srcGroup = nodeGroupMap.get(edge.source) ?? "raw";
            const tgtGroup = nodeGroupMap.get(edge.target) ?? "raw";
            const isActiveEdge = activeNodeId
              ? edge.source === activeNodeId || edge.target === activeNodeId
              : false;
            const isSearchEdge = searchMatchingNodes
              ? searchMatchingNodes.matches.has(edge.source) && searchMatchingNodes.matches.has(edge.target)
              : false;
            const isGroupEdge = selectedGroup
              ? srcGroup === selectedGroup && tgtGroup === selectedGroup
              : false;

            let strokeOpacity = 0.4;
            let strokeWidth = 1;
            if (selectedGroup) {
              strokeOpacity = isGroupEdge ? 0.6 : 0.05;
              strokeWidth = isGroupEdge ? 1.5 : 1;
            } else if (searchMatchingNodes) {
              strokeOpacity = isSearchEdge ? 0.6 : 0.05;
              strokeWidth = isSearchEdge ? 1.5 : 1;
            } else if (activeNodeId) {
              strokeOpacity = isActiveEdge ? 0.8 : 0.1;
              strokeWidth = isActiveEdge ? 2 : 1;
            }

            const activeNode = activeNodeId ? nodes.find((n) => n.id === activeNodeId) : null;
            const strokeColor = isActiveEdge && activeNode ? activeNode.color : "#333";

            const dx = target.x - source.x;
            const dy = target.y - source.y;
            const dr = Math.sqrt(dx * dx + dy * dy) * 1.5;
            const pathD = `M${source.x},${source.y}A${dr},${dr} 0 0,1 ${target.x},${target.y}`;

            return (
              <path
                key={`${edge.source}-${edge.target}-${i}`}
                d={pathD}
                fill="none"
                stroke={strokeColor}
                strokeOpacity={strokeOpacity}
                strokeWidth={strokeWidth}
                style={{ transition: "stroke 0.2s, stroke-opacity 0.2s, stroke-width 0.2s" }}
              />
            );
          })}

          {/* Nodes */}
          {nodes.map((node) => {
            const pos = displayPositions.get(node.id);
            if (!pos) return null;

            const nodeGroup = node.category;
            const isConnected = connectedNodes ? connectedNodes.has(node.id) : true;
            const isSearchMatch = searchMatchingNodes
              ? searchMatchingNodes.matches.has(node.id)
              : true;
            const isDirectMatch = searchMatchingNodes
              ? searchMatchingNodes.directMatches.has(node.id)
              : false;
            const isGroupMatch = selectedGroup ? nodeGroup === selectedGroup : true;
            const isPrimary =
              activeNodeId === node.id || isDirectMatch || (selectedGroup != null && isGroupMatch);

            let nodeOpacity = 1;
            if (selectedGroup) {
              nodeOpacity = isGroupMatch ? 1 : 0.1;
            } else if (searchMatchingNodes) {
              nodeOpacity = isDirectMatch ? 1 : isSearchMatch ? 0.5 : 0.1;
            } else if (activeNodeId) {
              nodeOpacity = isConnected ? 1 : 0.3;
            }

            const glowId = `glow-${catColorHex(node.category)}`;

            return (
              <g
                key={node.id}
                transform={`translate(${pos.x} ${pos.y})`}
                className="cursor-pointer"
                onPointerEnter={() => setHoveredNodeId(node.id)}
                onPointerLeave={() => setHoveredNodeId(null)}
                onPointerDown={(e) => startDragNode(e, node.id)}
                style={{ transition: "opacity 0.2s" }}
                opacity={nodeOpacity}
              >
                {/* Outer glow circle */}
                <circle
                  r={30}
                  fill={node.color}
                  opacity={isPrimary ? 0.4 : 0}
                  style={{ transition: "opacity 0.2s" }}
                />
                {/* Main circle */}
                <circle
                  r={node.radius}
                  fill={node.color}
                  stroke={isDirectMatch ? "#fff" : node.stroke}
                  strokeWidth={isDirectMatch ? 3 : 2}
                  filter={isPrimary ? `url(#${glowId})` : undefined}
                  style={{ transition: "filter 0.2s, stroke 0.2s, stroke-width 0.2s" }}
                />
                {/* Label */}
                <text
                  y={node.radius + 16}
                  textAnchor="middle"
                  style={{
                    fontSize: 10,
                    fill: labelFill,
                    fontWeight: 500,
                    fontFamily: "system-ui, sans-serif",
                  }}
                >
                  {node.label}
                </text>
              </g>
            );
          })}
        </g>
      </svg>

      {/* Search bar — bottom center (精确复制 Rowboat) */}
      <div
        className="absolute bottom-4 left-1/2 z-20 -translate-x-1/2"
        onPointerDown={(e) => e.stopPropagation()}
      >
        <div className="relative flex items-center">
          <Search className="absolute left-3 size-4 text-muted-foreground" />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="搜索节点..."
            className="w-64 rounded-md border border-border bg-background/90 py-2 pl-9 pr-20 text-sm shadow-lg backdrop-blur focus:outline-none focus:ring-1 focus:ring-ring"
          />
          <div className="absolute right-3 flex items-center gap-2">
            {searchMatchingNodes && (
              <span className="text-xs text-muted-foreground">
                {searchMatchingNodes.directMatches.size}
              </span>
            )}
            {searchQuery && (
              <button
                onClick={() => setSearchQuery("")}
                className="text-muted-foreground hover:text-foreground"
              >
                <X className="size-4" />
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

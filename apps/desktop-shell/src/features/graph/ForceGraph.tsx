/**
 * ForceGraph — warm Obsidian-style graph view.
 *
 * Keeps the existing custom spring/repulsion engine, drag, pan, zoom,
 * hover highlighting, and search behavior. The visual layer is reduced to a
 * pure cream canvas, solid degree-sized nodes, and very fine warm links.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  BookOpen,
  Database,
  Maximize2,
  Network,
  Search,
  Sparkles,
  X,
} from "lucide-react";
import { toast } from "sonner";
import { triggerAbsorb } from "@/api/wiki/repository";
import type { WikiGraphResponse, RawEntry } from "@/api/wiki/types";
import { useSkillStore } from "@/state/skill-store";
import { GRAPH_INTERACTION_REGRESSION_POINTS } from "./graph-regression-points";

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
}

interface ForceEdge {
  source: string;
  target: string;
  kind: "derived-from" | "references" | "related";
  synthetic?: boolean;
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
  onClickRaw: (rawId?: number) => void;
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
  showChromeTabs?: boolean;
}

/* ─── Physics Constants (精确复制 Rowboat) ────────────────────── */

const SIMULATION_STEPS = 240;
const SPRING_LENGTH = 64;
const SPRING_STRENGTH = 0.018;
const REPULSION = 3200;
const DAMPING = 0.84;
const MIN_DISTANCE = 8;
const CENTER_STRENGTH = 0.013;
const ORPHAN_CENTER_STRENGTH = 0.035;
const CONCEPT_RADIAL_STRENGTH = 0.018;
const BOUNDARY_STRENGTH = 0.09;
const NEBULA_BOUNDARY_RADIUS = 430;
const CONCEPT_RING_RADIUS = 300;
const TARGET_MIN_VISUAL_EDGES = 60;
const TARGET_MAX_VISUAL_EDGES = 70;

/* ─── Floating Animation Constants ───────────────────────────── */

const FLOAT_BASE = 3.5;
const FLOAT_VARIANCE = 2;
const FLOAT_SPEED_BASE = 0.0006;
const FLOAT_SPEED_VARIANCE = 0.00025;

const GRAPH_COLORS = {
  primary: "#2C2C2A",
  secondary: "#5F5E5A",
  material: "#7A7468",
  materialHover: "#5F5A50",
  edge: "#B8A99A",
  background: "#FAF8F3",
  panel: "#FFFFFF",
  border: "rgba(44,44,42,0.1)",
  terracotta: "#D85A30",
} as const;

const FLOAT_PANEL_CLASS =
  "rounded-lg border border-[rgba(44,44,42,0.1)] bg-white px-3.5 py-2.5 text-[#2C2C2A]";

function visualRadiusForDegree(degree: number): number {
  if (degree >= 9) return 8;
  if (degree >= 5) return 6.5;
  if (degree >= 2) return 5;
  return 3.5;
}

function truncateLabel(label: string, maxLength = 10): string {
  return label.length > maxLength ? `${label.slice(0, maxLength)}...` : label;
}

function buildVisibleLabelIds({
  nodes,
  positions,
  zoom,
  activeNodeId,
  connectedNodes,
  searchMatchingNodes,
  coreLabelNodeIds,
}: {
  nodes: ForceNode[];
  positions: Map<string, { x: number; y: number }>;
  zoom: number;
  activeNodeId: string | null;
  connectedNodes: Set<string> | null;
  searchMatchingNodes: {
    matches: Set<string>;
    directMatches: Set<string>;
  } | null;
  coreLabelNodeIds: Set<string>;
}): Set<string> {
  const coreRankById = new Map(
    [...nodes]
      .sort((a, b) => b.degree - a.degree || b.radius - a.radius || a.label.localeCompare(b.label))
      .slice(0, 7)
      .map((node, index) => [node.id, index] as const),
  );
  const candidates = nodes
    .map((node) => {
      const isActive = activeNodeId === node.id;
      const isConnected = connectedNodes?.has(node.id) ?? false;
      const isDirectSearch = searchMatchingNodes?.directMatches.has(node.id) ?? false;
      const isSearchNeighbor = searchMatchingNodes?.matches.has(node.id) ?? false;
      const coreRank = coreRankById.get(node.id);
      const isCore = coreLabelNodeIds.has(node.id) || coreRank !== undefined;
      const isPinnedCore = coreRank !== undefined && coreRank < 2;
      const isZoomRevealed =
        (zoom >= 2 && node.degree >= 3) || (zoom >= 1.7 && node.degree >= 5);
      if (!isActive && !isDirectSearch && !isConnected && !isSearchNeighbor && !isCore && !isZoomRevealed) {
        return null;
      }
      const priority =
        (isActive ? 10_000 : 0) +
        (isDirectSearch ? 9_000 : 0) +
        (isConnected ? 8_000 : 0) +
        (isCore ? 7_000 - (coreRank ?? 99) * 100 : 0) +
        node.degree * 100 +
        node.radius;
      return { node, priority, force: isActive || isDirectSearch || isPinnedCore };
    })
    .filter((candidate): candidate is { node: ForceNode; priority: number; force: boolean } =>
      Boolean(candidate),
    )
    .sort((a, b) => b.priority - a.priority || a.node.label.localeCompare(b.node.label));

  const visible = new Set<string>();
  const placed: Array<{ x: number; y: number }> = [];
  const maxLabels = activeNodeId || searchMatchingNodes ? 9 : 7;
  for (const candidate of candidates) {
    if (visible.size >= maxLabels && !candidate.force) continue;
    const pos = positions.get(candidate.node.id);
    if (!pos) continue;
    const overlaps = placed.some((other) => Math.hypot(other.x - pos.x, other.y - pos.y) < 60);
    if (overlaps && !candidate.force) continue;
    visible.add(candidate.node.id);
    placed.push(pos);
  }
  return visible;
}

const SOURCE_LABELS: Record<string, string> = {
  paste: "粘贴", url: "网页", "wechat-text": "微信消息",
  "wechat-article": "微信文章", "wechat-url": "微信链接",
  voice: "语音", image: "图片", pdf: "PDF", pptx: "PPT",
  docx: "文档", video: "视频", card: "名片", chat: "聊天",
};

/* ─── Floating Motion ────────────────────────────────────────── */

type GraphBuildNode = ForceNode & {
  raw?: RawEntry;
  searchText: string;
};

const GRAPH_TOKEN_STOPWORDS = new Set([
  "raw",
  "wiki",
  "page",
  "article",
  "wechat",
  "source",
  "concept",
  "https",
  "http",
  "www",
  "com",
  "微信",
  "文章",
  "网页",
  "素材",
  "概念",
]);

function edgeKey(a: string, b: string): string {
  return a < b ? `${a}\u0000${b}` : `${b}\u0000${a}`;
}

function addGraphEdge(
  edges: ForceEdge[],
  seen: Set<string>,
  source: string | undefined,
  target: string | undefined,
  kind: ForceEdge["kind"] = "related",
  synthetic = true,
): boolean {
  if (!source || !target || source === target) return false;
  const key = edgeKey(source, target);
  if (seen.has(key)) return false;
  seen.add(key);
  edges.push({ source, target, kind, synthetic });
  return true;
}

function tokenSet(text: string): Set<string> {
  const tokens = new Set<string>();
  const normalized = text
    .toLowerCase()
    .replace(/[_/#:.,，。；;|()[\]{}<>!?？！'"`]+/g, " ");

  for (const match of normalized.matchAll(/[a-z0-9]{2,}/g)) {
    const token = match[0];
    if (!GRAPH_TOKEN_STOPWORDS.has(token)) tokens.add(token);
  }

  for (const match of normalized.matchAll(/[\u4e00-\u9fff]{2,}/g)) {
    const chunk = match[0];
    if (!GRAPH_TOKEN_STOPWORDS.has(chunk) && chunk.length <= 10) {
      tokens.add(chunk);
    }
    for (let i = 0; i < chunk.length - 1; i++) {
      const bigram = chunk.slice(i, i + 2);
      if (!GRAPH_TOKEN_STOPWORDS.has(bigram)) tokens.add(bigram);
    }
  }

  return tokens;
}

function sharedTokenScore(a: Set<string>, b: Set<string>): number {
  let score = 0;
  for (const token of a) {
    if (!b.has(token)) continue;
    score += token.length >= 4 ? 2 : 1;
  }
  return score;
}

function rawIdForNode(node: GraphBuildNode): number {
  if (node.raw) return node.raw.id;
  return parseRawNodeId(node.id) ?? Number.MAX_SAFE_INTEGER;
}

function relationScore(
  a: GraphBuildNode,
  b: GraphBuildNode,
  tokenCache: Map<string, Set<string>>,
  indexDistance: number,
): number {
  const tokensA = tokenCache.get(a.id) ?? new Set<string>();
  const tokensB = tokenCache.get(b.id) ?? new Set<string>();
  const sharedScore = sharedTokenScore(tokensA, tokensB);
  let score = sharedScore * 3;

  if (a.kind === "raw" && b.kind === "raw") {
    if (a.raw?.source && a.raw.source === b.raw?.source) score += 4;
    if (a.raw?.date && a.raw.date === b.raw?.date) score += 5;
    const idGap = Math.abs(rawIdForNode(a) - rawIdForNode(b));
    if (idGap <= 2) score += 3;
    else if (idGap <= 6) score += 1.5;
  }

  if (a.kind === "concept" && b.kind === "concept") {
    if (sharedScore === 0) return -20 + 0.2 / Math.max(1, indexDistance);
    score += Math.min(3, sharedScore);
  }

  if (a.kind !== b.kind) {
    score += sharedScore > 0 ? 6 : 1 / Math.max(1, indexDistance);
  }

  return score + 0.2 / Math.max(1, indexDistance);
}

function visualEdgeTarget(nodeCount: number, baseEdgeCount: number): number {
  if (nodeCount <= 1) return baseEdgeCount;
  const possible = (nodeCount * (nodeCount - 1)) / 2;
  const desired =
    nodeCount >= 30 ? TARGET_MIN_VISUAL_EDGES : Math.round(nodeCount * 2.1);
  return Math.min(
    TARGET_MAX_VISUAL_EDGES,
    possible,
    Math.max(baseEdgeCount, desired),
  );
}

function buildDenseVisualEdges(
  nodes: GraphBuildNode[],
  baseEdges: ForceEdge[],
): ForceEdge[] {
  const edges: ForceEdge[] = [];
  const seen = new Set<string>();
  for (const edge of baseEdges) {
    addGraphEdge(edges, seen, edge.source, edge.target, edge.kind, false);
  }

  const target = visualEdgeTarget(nodes.length, edges.length);

  const tokenCache = new Map(
    nodes.map((node) => [node.id, tokenSet(node.searchText)] as const),
  );
  const rawNodes = nodes.filter((node) => node.kind === "raw");
  const conceptNodes = nodes.filter((node) => node.kind === "concept");

  const addTopCandidates = (
    source: GraphBuildNode,
    candidates: GraphBuildNode[],
    limit: number,
    minScore = 0,
  ) => {
    const scored = candidates
      .filter((candidate) => candidate.id !== source.id)
      .map((candidate, index) => ({
        node: candidate,
        score: relationScore(source, candidate, tokenCache, index + 1),
      }))
      .filter((candidate) => candidate.score >= minScore)
      .sort((a, b) => b.score - a.score || a.node.label.localeCompare(b.node.label));

    for (const candidate of scored.slice(0, limit)) {
      addGraphEdge(edges, seen, source.id, candidate.node.id);
      if (edges.length >= target) return;
    }
  };

  for (const concept of conceptNodes) {
    if (edges.length >= target) break;
    addTopCandidates(concept, rawNodes, 6);
  }

  for (let i = 0; i < conceptNodes.length; i++) {
    if (edges.length >= target) break;
    addTopCandidates(conceptNodes[i], conceptNodes.slice(i + 1), 1, 8);
  }

  const groupedRaw = new Map<string, GraphBuildNode[]>();
  for (const raw of rawNodes) {
    const keys = [
      raw.raw?.date ? `date:${raw.raw.date}` : null,
      raw.raw?.source ? `source:${raw.raw.source}` : null,
    ].filter((key): key is string => Boolean(key));
    for (const key of keys) {
      const group = groupedRaw.get(key) ?? [];
      group.push(raw);
      groupedRaw.set(key, group);
    }
  }

  for (const group of groupedRaw.values()) {
    const sorted = [...group].sort((a, b) => rawIdForNode(a) - rawIdForNode(b));
    for (let i = 0; i < sorted.length; i++) {
      if (edges.length >= target) break;
      addGraphEdge(edges, seen, sorted[i]?.id, sorted[i + 1]?.id);
      addGraphEdge(edges, seen, sorted[i]?.id, sorted[i + 2]?.id);
    }
  }

  const allCandidates: Array<{
    a: GraphBuildNode;
    b: GraphBuildNode;
    score: number;
  }> = [];
  for (let i = 0; i < nodes.length; i++) {
    for (let j = i + 1; j < nodes.length; j++) {
      if (seen.has(edgeKey(nodes[i].id, nodes[j].id))) continue;
      const score = relationScore(nodes[i], nodes[j], tokenCache, j - i);
      if (nodes[i].kind === "concept" && nodes[j].kind === "concept" && score < 8) {
        continue;
      }
      allCandidates.push({
        a: nodes[i],
        b: nodes[j],
        score,
      });
    }
  }

  allCandidates.sort((a, b) => b.score - a.score);
  for (const candidate of allCandidates) {
    if (edges.length >= target) break;
    addGraphEdge(edges, seen, candidate.a.id, candidate.b.id);
  }

  const degreeMap = new Map<string, number>();
  const noteDegree = (id: string) => degreeMap.set(id, (degreeMap.get(id) ?? 0) + 1);
  for (const edge of edges) {
    noteDegree(edge.source);
    noteDegree(edge.target);
  }
  for (const node of nodes) {
    if ((degreeMap.get(node.id) ?? 0) > 0) continue;
    const best = nodes
      .filter((candidate) => candidate.id !== node.id)
      .map((candidate, index) => ({
        node: candidate,
        score: relationScore(node, candidate, tokenCache, index + 1),
      }))
      .filter((candidate) =>
        !(node.kind === "concept" && candidate.node.kind === "concept" && candidate.score < 8),
      )
      .sort((a, b) => b.score - a.score)[0];
    if (best && addGraphEdge(edges, seen, node.id, best.node.id)) {
      noteDegree(node.id);
      noteDegree(best.node.id);
    }
  }

  return edges;
}

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
  showChromeTabs = true,
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
  const [zoom, setZoom] = useState(1.28);
  const [hoveredNodeId, setHoveredNodeId] = useState<string | null>(null);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [hasUserSelectedNode, setHasUserSelectedNode] = useState(false);
  const [searchQuery, setSearchQuery] = useState(initialSearchQuery ?? "");
  const [selectedGroup, setSelectedGroup] = useState<NodeCategory | null>(null);
  const [, forceRender] = useState(0);
  const absorbRunning = useSkillStore((s) => s.absorbRunning);
  const startAbsorb = useSkillStore((s) => s.startAbsorb);
  const failAbsorb = useSkillStore((s) => s.failAbsorb);

  const handleStartMaintenance = useCallback(async () => {
    if (absorbRunning) return;

    try {
      const response = await triggerAbsorb();
      startAbsorb(response.task_id);
      toast.success("维护已启动", { duration: 2500 });
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      if (message.includes("ABSORB_IN_PROGRESS")) {
        toast.warning("已有维护任务正在执行", { duration: 3000 });
        return;
      }
      failAbsorb(message);
      toast.error(`维护启动失败: ${message}`, { duration: 5000 });
    }
  }, [absorbRunning, failAbsorb, startAbsorb]);

  // Build nodes + dense visual edges from graph data
  const { nodes, edges } = useMemo(() => {
    const idToRaw = new Map<number, RawEntry>();
    for (const e of rawEntries) idToRaw.set(e.id, e);

    const buildNodes: GraphBuildNode[] = graphData.nodes.map((n) => {
      const isRaw = n.kind === "raw";
      const cat = (isRaw ? "raw" : "concept") as NodeCategory;
      let raw: RawEntry | undefined;

      let friendlyLabel = n.label;
      if (isRaw) {
        const idNum = parseInt(n.id.replace(/\D/g, ""), 10);
        raw = !isNaN(idNum) ? idToRaw.get(idNum) : undefined;
        if (raw) {
          const srcLabel = SOURCE_LABELS[raw.source] ?? raw.source;
          friendlyLabel = `${srcLabel} #${raw.id}`;
        } else if (n.label.includes(": ")) {
          const src = n.label.split(": ")[0];
          friendlyLabel = SOURCE_LABELS[src] ?? src;
        }
      }
      friendlyLabel = truncateLabel(friendlyLabel);
      const searchText = [
        n.id,
        n.label,
        friendlyLabel,
        raw?.filename,
        raw?.slug,
        raw?.source,
        raw?.source_url,
        raw?.date,
      ]
        .filter(Boolean)
        .join(" ");

      return {
        id: n.id,
        label: friendlyLabel,
        kind: n.kind,
        category: cat,
        degree: 0,
        radius: visualRadiusForDegree(0),
        color: n.kind === "concept" ? GRAPH_COLORS.terracotta : GRAPH_COLORS.material,
        raw,
        searchText,
      };
    });

    const nodeIds = new Set(buildNodes.map((n) => n.id));
    const baseEdges: ForceEdge[] = graphData.edges
      .filter((e) => nodeIds.has(e.from) && nodeIds.has(e.to) && e.from !== e.to)
      .map((e) => ({ source: e.from, target: e.to, kind: e.kind, synthetic: false }));

    const edges = buildDenseVisualEdges(buildNodes, baseEdges);
    const degreeMap = new Map<string, number>();
    for (const e of edges) {
      degreeMap.set(e.source, (degreeMap.get(e.source) ?? 0) + 1);
      degreeMap.set(e.target, (degreeMap.get(e.target) ?? 0) + 1);
    }

    const nodes: ForceNode[] = buildNodes.map((node) => {
      const degree = degreeMap.get(node.id) ?? 0;
      return {
        id: node.id,
        label: node.label,
        kind: node.kind,
        category: node.category,
        degree,
        radius: visualRadiusForDegree(degree),
        color: node.color,
      };
    });

    return { nodes, edges };
  }, [graphData, rawEntries]);

  const visibleEdges = edges;

  // Group → category mapping & cluster centers
  const nodeGroupMap = useMemo(() => {
    const map = new Map<string, NodeCategory>();
    nodes.forEach((n) => map.set(n.id, n.kind === "raw" ? "raw" : "concept"));
    return map;
  }, [nodes]);

  const conceptAnchors = useMemo(() => {
    const concepts = nodes
      .filter((node) => node.kind === "concept")
      .sort((a, b) => b.degree - a.degree || a.label.localeCompare(b.label));
    const anchors = new Map<string, { x: number; y: number }>();
    concepts.forEach((node, index) => {
      const angle = (index / Math.max(1, concepts.length)) * Math.PI * 2 - Math.PI / 2;
      const radius = CONCEPT_RING_RADIUS + (index % 3) * 22;
      anchors.set(node.id, {
        x: Math.cos(angle) * radius,
        y: Math.sin(angle) * radius,
      });
    });
    return anchors;
  }, [nodes]);

  // Legend items
  const legendItems = useMemo(() => {
    const rawCount = nodes.filter((node) => node.kind === "raw").length;
    const conceptCount = nodes.length - rawCount;
    return [
      { cat: "raw" as const, count: rawCount, label: "素材", color: GRAPH_COLORS.material },
      { cat: "concept" as const, count: conceptCount, label: "概念", color: GRAPH_COLORS.terracotta },
    ].filter((item) => item.count > 0);
  }, [nodes]);

  const coreLabelNodeIds = useMemo(() => {
    const limit = Math.min(7, Math.max(5, Math.ceil(nodes.length * 0.12)));
    return new Set(
      [...nodes]
        .sort((a, b) => b.degree - a.degree || b.radius - a.radius || a.label.localeCompare(b.label))
        .slice(0, limit)
        .map((node) => node.id)
    );
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
    const radius = Math.max(230, Math.min(360, count * 9.5));

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
      const nodeRadiusById = new Map(nodes.map((node) => [node.id, node.radius] as const));
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
          const minDistance =
            (nodeRadiusById.get(ids[i]) ?? 4) + (nodeRadiusById.get(ids[j]) ?? 4) + 4;
          const distance = Math.max(Math.max(MIN_DISTANCE, minDistance), Math.hypot(dx, dy));
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
        const linkLength =
          edge.kind === "references"
            ? SPRING_LENGTH * 0.9
            : edge.kind === "related"
              ? SPRING_LENGTH
              : SPRING_LENGTH * 1.1;
        const linkStrength =
          edge.kind === "references"
            ? SPRING_STRENGTH * 1.4
            : edge.kind === "related"
              ? SPRING_STRENGTH * 1.05
              : SPRING_STRENGTH * 0.9;
        const delta = distance - linkLength;
        const force = delta * linkStrength;
        const fx = (force * dx) / distance;
        const fy = (force * dy) / distance;
        const fA = forces.get(edge.source);
        const fB = forces.get(edge.target);
        if (fA) { fA.x += fx; fA.y += fy; }
        if (fB) { fB.x -= fx; fB.y -= fy; }
      });

      // Centering, concept spread, and soft circular boundary.
      ids.forEach((id) => {
        const pos = positions.get(id);
        const f = forces.get(id);
        if (!pos || !f) return;
        const node = nodes.find((candidate) => candidate.id === id);
        const centerStrength = node && node.degree === 0
          ? ORPHAN_CENTER_STRENGTH
          : CENTER_STRENGTH;
        f.x += -pos.x * centerStrength;
        f.y += -pos.y * centerStrength;

        const conceptAnchor = conceptAnchors.get(id);
        if (conceptAnchor) {
          f.x += (conceptAnchor.x - pos.x) * CONCEPT_RADIAL_STRENGTH;
          f.y += (conceptAnchor.y - pos.y) * CONCEPT_RADIAL_STRENGTH;
        }

        const distanceFromCenter = Math.hypot(pos.x, pos.y);
        if (distanceFromCenter > NEBULA_BOUNDARY_RADIUS) {
          const overflow = distanceFromCenter - NEBULA_BOUNDARY_RADIUS;
          f.x -= (pos.x / distanceFromCenter) * overflow * BOUNDARY_STRENGTH;
          f.y -= (pos.y / distanceFromCenter) * overflow * BOUNDARY_STRENGTH;
        }
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

      // Collision pass: radius + 3px keeps labels/nodes from collapsing into a lump.
      for (let i = 0; i < ids.length; i++) {
        const posA = positions.get(ids[i]);
        if (!posA || draggingRef.current?.id === ids[i]) continue;
        for (let j = i + 1; j < ids.length; j++) {
          const posB = positions.get(ids[j]);
          if (!posB || draggingRef.current?.id === ids[j]) continue;
          const dx = posB.x - posA.x;
          const dy = posB.y - posA.y;
          const distance = Math.max(0.1, Math.hypot(dx, dy));
          const minDistance =
            (nodeRadiusById.get(ids[i]) ?? 4) + (nodeRadiusById.get(ids[j]) ?? 4) + 3;
          if (distance >= minDistance) continue;
          const push = (minDistance - distance) / 2;
          const ux = dx / distance;
          const uy = dy / distance;
          posA.x -= ux * push;
          posA.y -= uy * push;
          posB.x += ux * push;
          posB.y += uy * push;
        }
      }

      forceRender((p) => p + 1);
      if (step < SIMULATION_STEPS) rafId = requestAnimationFrame(simulate);
    };

    rafId = requestAnimationFrame(simulate);
    return () => { active = false; cancelAnimationFrame(rafId); };
  }, [nodes, edges, conceptAnchors, nodeGroupMap]);

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
    setSelectedNodeId(null);
    setHasUserSelectedNode(false);
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
          setSelectedNodeId(node.id);
          setHasUserSelectedNode(true);
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

  const visibleLabelNodeIds = buildVisibleLabelIds({
    nodes,
    positions: displayPositions,
    zoom,
    activeNodeId,
    connectedNodes,
    searchMatchingNodes,
    coreLabelNodeIds,
  });

  const graphNodeById = useMemo(
    () => new Map(nodes.map((node) => [node.id, node] as const)),
    [nodes],
  );

  const rawByNodeId = useMemo(() => {
    const map = new Map<string, RawEntry>();
    for (const raw of rawEntries) map.set(`raw-${raw.id}`, raw);
    return map;
  }, [rawEntries]);

  const selectedNode =
    hasUserSelectedNode && selectedNodeId ? graphNodeById.get(selectedNodeId) ?? null : null;
  const selectedDrilldown = useMemo(() => {
    if (!selectedNodeId) return null;
    const outgoing = edges
      .filter((edge) => edge.kind === "references" && edge.source === selectedNodeId)
      .map((edge) => graphNodeById.get(edge.target))
      .filter((node): node is ForceNode => Boolean(node));
    const backlinks = edges
      .filter((edge) => edge.kind === "references" && edge.target === selectedNodeId)
      .map((edge) => graphNodeById.get(edge.source))
      .filter((node): node is ForceNode => Boolean(node));
    const sources = edges
      .filter((edge) => edge.kind === "derived-from" && edge.source === selectedNodeId)
      .map((edge) => graphNodeById.get(edge.target))
      .filter((node): node is ForceNode => Boolean(node));
    const derivedPages = edges
      .filter((edge) => edge.kind === "derived-from" && edge.target === selectedNodeId)
      .map((edge) => graphNodeById.get(edge.source))
      .filter((node): node is ForceNode => Boolean(node));
    return { outgoing, backlinks, sources, derivedPages };
  }, [edges, graphNodeById, selectedNodeId]);

  const openNode = useCallback(
    (node: ForceNode) => {
      if (node.kind === "concept") {
        onClickConcept(node.id.replace(/^wiki-/, ""));
      } else {
        onClickRaw(parseRawNodeId(node.id));
      }
    },
    [onClickConcept, onClickRaw],
  );

  const fitGraphToView = useCallback(() => {
    const positions = Array.from(positionsRef.current.values());
    if (positions.length === 0 || viewport.width <= 1 || viewport.height <= 1) return;

    const minX = Math.min(...positions.map((pos) => pos.x));
    const maxX = Math.max(...positions.map((pos) => pos.x));
    const minY = Math.min(...positions.map((pos) => pos.y));
    const maxY = Math.max(...positions.map((pos) => pos.y));
    const graphWidth = Math.max(1, maxX - minX + 120);
    const graphHeight = Math.max(1, maxY - minY + 120);
    const nextZoom = Math.min(
      1.5,
      Math.max(0.4, Math.min(viewport.width / graphWidth, viewport.height / graphHeight) * 1.18)
    );
    const centerX = (minX + maxX) / 2;
    const centerY = (minY + maxY) / 2;
    setZoom(nextZoom);
    setPan({
      x: viewport.width / 2 - centerX * nextZoom,
      y: viewport.height / 2 - centerY * nextZoom,
    });
  }, [viewport.height, viewport.width]);

  return (
    <div
      ref={containerRef}
      className="graph-view relative h-full w-full overflow-hidden"
      data-regression-points={GRAPH_INTERACTION_REGRESSION_POINTS.join(",")}
    >
      {showChromeTabs && (
        <div
          className="graph-panel-enter absolute left-1/2 top-3 z-30 flex -translate-x-1/2 items-center gap-1"
          onPointerDown={(e) => e.stopPropagation()}
        >
          <a
            href="#/wiki"
            className="flex items-center gap-1.5 rounded-lg border border-[rgba(44,44,42,0.1)] bg-white/70 px-3.5 py-2 text-[13px] text-[#5F5E5A] transition-colors hover:text-[#2C2C2A]"
          >
            <BookOpen className="size-3.5" strokeWidth={1.5} />
            页面
          </a>
          <button
            type="button"
            className="flex items-center gap-1.5 rounded-lg border border-[#2C2C2A] bg-[#2C2C2A] px-3.5 py-2 text-[13px] text-white"
          >
            <Network className="size-3.5" strokeWidth={1.5} />
            关系图
          </button>
          <a
            href="#/wiki?view=raw"
            className="flex items-center gap-1.5 rounded-lg border border-[rgba(44,44,42,0.1)] bg-white/70 px-3.5 py-2 text-[13px] text-[#5F5E5A] transition-colors hover:text-[#2C2C2A]"
          >
            <Database className="size-3.5" strokeWidth={1.5} />
            素材库
          </a>
        </div>
      )}

      <button
        type="button"
        onClick={handleStartMaintenance}
        disabled={absorbRunning}
        className="graph-panel-enter absolute right-4 top-3 z-30 flex items-center gap-1.5 text-[13px] text-[#D85A30] transition-opacity hover:opacity-75 disabled:cursor-not-allowed disabled:opacity-70"
        onPointerDown={(e) => e.stopPropagation()}
      >
        <Sparkles
          className={`size-3.5 ${absorbRunning ? "animate-spin" : ""}`}
          strokeWidth={1.5}
        />
        {absorbRunning ? "维护中..." : "开始维护"}
      </button>
      {/* Legend — top-right (精确复制 Rowboat) */}
      {legendItems.length > 0 && (
        <div
          className={`graph-panel-enter absolute right-4 top-14 z-20 min-w-[130px] text-xs ${FLOAT_PANEL_CLASS}`}
          onPointerDown={(e) => e.stopPropagation()}
        >
          <div className="mb-2 text-[10px] tracking-[0.1em] text-[#888780]">
            分类
          </div>
          <div className="grid gap-1">
            {legendItems.map((item) => {
              const isSelected = selectedGroup === item.cat;
              return (
                <button
                  key={item.cat}
                  onClick={() => setSelectedGroup(isSelected ? null : item.cat)}
                  className={`flex items-center gap-2 rounded-md px-1.5 py-1 text-left text-[13px] transition-colors hover:bg-[#F1EFE8] ${
                    isSelected ? "bg-[#F1EFE8]" : ""
                  }`}
                >
                  <span
                    className="inline-flex h-2 w-2 rounded-full"
                    style={{ backgroundColor: item.color }}
                  />
                  <span className="truncate text-[#5F5E5A]">{item.label}</span>
                  <span className="ml-auto text-[#888780]">{item.count}</span>
                  <X className={`size-3 ${isSelected ? "text-[#888780]" : "invisible"}`} />
                </button>
              );
            })}
          </div>
        </div>
      )}

      {selectedNode && selectedDrilldown && (
        <div
          className={`graph-panel-enter absolute bottom-20 right-4 top-28 z-20 flex w-80 flex-col overflow-hidden text-xs ${FLOAT_PANEL_CLASS}`}
          onPointerDown={(e) => e.stopPropagation()}
        >
          <div className="border-b border-[rgba(44,44,42,0.12)] px-4 py-3">
            <div className="flex items-start gap-2">
              <span
                className="mt-1 inline-flex h-2.5 w-2.5 shrink-0 rounded-full"
                style={{ backgroundColor: selectedNode.color }}
              />
              <div className="min-w-0 flex-1">
                <div className="truncate text-sm font-medium text-[#2C2C2A]">{selectedNode.label}</div>
                <div className="mt-0.5 text-[11px] text-[#888780]">
                  {selectedNode.kind === "concept" ? "知识页" : "原始素材"} · 连接{" "}
                  {selectedNode.degree}
                </div>
              </div>
              <button
                type="button"
                className="rounded p-1 text-[#888780] hover:bg-[#F1EFE8] hover:text-[#2C2C2A]"
                onClick={() => {
                  setSelectedNodeId(null);
                  setHasUserSelectedNode(false);
                }}
                aria-label="关闭详情"
              >
                <X className="size-3.5" />
              </button>
            </div>
            <button
              type="button"
              className="mt-3 w-full rounded-md border border-[rgba(44,44,42,0.15)] px-2 py-1.5 text-left text-[11px] text-[#5F5E5A] transition-colors hover:bg-[#F1EFE8] hover:text-[#2C2C2A]"
              onClick={() => openNode(selectedNode)}
            >
              打开{selectedNode.kind === "concept" ? "知识页" : "素材"}
            </button>
          </div>
          <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-4 py-3">
            {selectedNode.kind === "concept" ? (
              <>
                <DrilldownList
                  title="Backlinks"
                  empty="No pages link here yet."
                  items={selectedDrilldown.backlinks}
                  onOpen={openNode}
                />
                <DrilldownList
                  title="Outgoing"
                  empty="This page has no wiki links."
                  items={selectedDrilldown.outgoing}
                  onOpen={openNode}
                />
                <DrilldownList
                  title="Sources"
                  empty="No raw source is attached."
                  items={selectedDrilldown.sources}
                  onOpen={openNode}
                  subtitle={(node) => rawByNodeId.get(node.id)?.source_url ?? rawByNodeId.get(node.id)?.filename}
                />
              </>
            ) : (
              <DrilldownList
                title="Pages from this source"
                empty="No wiki pages derive from this raw source."
                items={selectedDrilldown.derivedPages}
                onOpen={openNode}
              />
            )}
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
            let strokeWidth = 0.6;
            let strokeColor: string = GRAPH_COLORS.edge;
            if (selectedGroup) {
              strokeOpacity = isGroupEdge ? 0.5 : 0.08;
            } else if (searchMatchingNodes) {
              strokeOpacity = isSearchEdge ? 0.9 : 0.08;
              strokeWidth = isSearchEdge ? 1 : strokeWidth;
              strokeColor = isSearchEdge ? GRAPH_COLORS.terracotta : strokeColor;
            } else if (activeNodeId) {
              strokeOpacity = isActiveEdge ? 1 : 0.08;
              strokeWidth = isActiveEdge ? 1 : strokeWidth;
              strokeColor = isActiveEdge ? GRAPH_COLORS.terracotta : strokeColor;
            }

            return (
              <line
                key={`${edge.source}-${edge.target}-${i}`}
                x1={source.x}
                y1={source.y}
                x2={target.x}
                y2={target.y}
                stroke={strokeColor}
                strokeOpacity={strokeOpacity}
                strokeWidth={strokeWidth}
                strokeLinecap="round"
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
            const isHovered = activeNodeId === node.id;
            const shouldShowLabel = visibleLabelNodeIds.has(node.id);

            let nodeOpacity = 1;
            if (selectedGroup) {
              nodeOpacity = isGroupMatch ? 1 : 0.1;
            } else if (searchMatchingNodes) {
              nodeOpacity = isDirectMatch ? 1 : isSearchMatch ? 0.5 : 0.1;
            } else if (activeNodeId) {
              nodeOpacity = isConnected ? 1 : 0.3;
            }

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
                <circle
                  r={node.radius}
                  fill={
                    node.kind === "concept"
                      ? GRAPH_COLORS.terracotta
                      : isHovered
                        ? GRAPH_COLORS.materialHover
                        : GRAPH_COLORS.material
                  }
                  style={{
                    transform: isHovered ? "scale(1.3)" : "scale(1)",
                    transformBox: "fill-box",
                    transformOrigin: "center",
                    transition: "transform 280ms ease-out, fill 280ms ease-out",
                  }}
                />
                <text
                  y={node.radius + 7}
                  textAnchor="middle"
                  style={{
                    fontSize: 9.2,
                    fill: GRAPH_COLORS.materialHover,
                    fontWeight: 400,
                    fontFamily: 'Inter, "Source Han Sans SC", "Noto Sans SC", system-ui, sans-serif',
                    opacity: shouldShowLabel ? 1 : 0,
                    transition: "opacity 180ms ease-out",
                  }}
                >
                  {truncateLabel(node.label, 10)}
                </text>
              </g>
            );
          })}
        </g>
      </svg>

      {/* Search bar — bottom center (精确复制 Rowboat) */}
      <div
        className="graph-panel-enter absolute bottom-6 left-1/2 z-20 flex -translate-x-1/2 items-center gap-2"
        onPointerDown={(e) => e.stopPropagation()}
      >
        <div className={`${FLOAT_PANEL_CLASS} flex min-w-[220px] items-center gap-2 rounded-full px-3.5 py-2`}>
          <Search className="size-3.5 shrink-0 text-[#5F5E5A]" strokeWidth={1.5} />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="搜索节点"
            className="w-36 bg-transparent text-[13px] text-[#2C2C2A] outline-none placeholder:text-[#888780]"
          />
          {searchMatchingNodes && (
            <span className="text-[11px] text-[#888780]">
              {searchMatchingNodes.directMatches.size}
            </span>
          )}
          <span className="rounded bg-[#F1EFE8] px-1.5 py-0.5 text-[11px] text-[#888780]">⌘K</span>
          {searchQuery && (
            <button
              type="button"
              onClick={() => setSearchQuery("")}
              className="text-[#888780] transition-colors hover:text-[#2C2C2A]"
              aria-label="清空搜索"
            >
              <X className="size-3.5" />
            </button>
          )}
        </div>
        <button
          type="button"
          onClick={fitGraphToView}
          className={`${FLOAT_PANEL_CLASS} grid size-[34px] place-items-center rounded-md p-0 text-[#5F5E5A] transition-colors hover:text-[#2C2C2A]`}
          aria-label="适应屏幕"
        >
          <Maximize2 className="size-3.5" strokeWidth={1.5} />
        </button>
      </div>
    </div>
  );
}

function parseRawNodeId(nodeId: string): number | undefined {
  const value = Number(nodeId.replace(/^raw-/, ""));
  return Number.isFinite(value) ? value : undefined;
}

function DrilldownList({
  title,
  empty,
  items,
  onOpen,
  subtitle,
}: {
  title: string;
  empty: string;
  items: ForceNode[];
  onOpen: (node: ForceNode) => void;
  subtitle?: (node: ForceNode) => string | undefined | null;
}) {
  return (
    <section>
      <div className="mb-2 flex items-center justify-between text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
        <span>{title}</span>
        <span>{items.length}</span>
      </div>
      {items.length === 0 ? (
        <div className="rounded-md border border-dashed border-border/70 px-3 py-2 text-[11px] text-muted-foreground">
          {empty}
        </div>
      ) : (
        <div className="space-y-1.5">
          {items.map((item) => (
            <button
              key={item.id}
              type="button"
              className="w-full rounded-md border border-border/70 px-3 py-2 text-left transition-colors hover:bg-foreground/10"
              onClick={() => onOpen(item)}
            >
              <div className="truncate text-[12px] font-medium">{item.label}</div>
              {subtitle?.(item) && (
                <div className="mt-0.5 truncate text-[10px] text-muted-foreground">
                  {subtitle(item)}
                </div>
              )}
            </button>
          ))}
        </div>
      )}
    </section>
  );
}

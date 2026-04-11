/**
 * ForceGraph — Obsidian 风格力导向交互式知识图谱
 *
 * 功能：
 *   - d3-force 力导向布局（引力 + 排斥力 + 连线弹簧）
 *   - 鼠标拖拽节点
 *   - 滚轮缩放 + 平移画布
 *   - 悬停高亮直连节点，其余变灰
 *   - 节点按类型着色（concept=烧陶橙，raw=按来源）
 *   - 点击跳转（concept→wiki，raw→raw library）
 *   - 显示标题而非 #00001 编号
 */

import { useRef, useEffect, useCallback, useState } from "react";
import {
  forceSimulation,
  forceLink,
  forceManyBody,
  forceCenter,
  forceCollide,
  type SimulationNodeDatum,
  type SimulationLinkDatum,
} from "d3-force";
import type { WikiGraphResponse } from "@/features/ingest/types";
import type { RawEntry } from "@/features/ingest/types";

/* ─── Types ───────────────────────────────────────────────────── */

interface ForceNode extends SimulationNodeDatum {
  id: string;
  label: string;
  kind: "raw" | "concept";
  source?: string; // only for raw nodes — "wechat-text", "paste", etc.
  color: string;
  radius: number;
}

interface ForceLink extends SimulationLinkDatum<ForceNode> {
  edgeKind: string;
}

interface ForceGraphProps {
  graphData: WikiGraphResponse;
  rawEntries: RawEntry[];
  onClickConcept: (slug: string) => void;
  onClickRaw: () => void;
}

/* ─── Colors ──────────────────────────────────────────────────── */

const SOURCE_COLORS: Record<string, string> = {
  paste: "#C35A2C",
  url: "#8B5CF6",
  "wechat-text": "#3F8F5E",
  "wechat-article": "#3F8F5E",
  "wechat-url": "#2D2B28",
  voice: "#C88B1A",
  image: "#8B5CF6",
  pdf: "#3F8F5E",
  pptx: "#DB2777",
  docx: "#8B5CF6",
  video: "#D44A3C",
  card: "#0891B2",
  chat: "#CA8A04",
};

const CONCEPT_COLOR = "#C35A2C"; // 烧陶橙
const DEFAULT_RAW_COLOR = "#8B8580";
const LINK_COLOR = "rgba(139,133,128,0.25)";
const LINK_HIGHLIGHT_COLOR = "rgba(195,90,44,0.6)";
const DIM_OPACITY = 0.12;

/** 来源名称中文映射 */
const SOURCE_LABELS: Record<string, string> = {
  paste: "粘贴",
  url: "网页",
  "wechat-text": "微信消息",
  "wechat-article": "微信文章",
  "wechat-url": "微信链接",
  voice: "语音",
  image: "图片",
  pdf: "PDF",
  pptx: "PPT",
  docx: "文档",
  video: "视频",
  card: "名片",
  chat: "聊天",
};

export function ForceGraph({ graphData, rawEntries, onClickConcept, onClickRaw }: ForceGraphProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const nodesRef = useRef<ForceNode[]>([]);
  const linksRef = useRef<ForceLink[]>([]);
  const simRef = useRef<ReturnType<typeof forceSimulation<ForceNode>> | null>(null);
  const rafRef = useRef(0);

  // Interaction state
  const [hoverNode, setHoverNode] = useState<string | null>(null);
  const dragNodeRef = useRef<ForceNode | null>(null);
  const transformRef = useRef({ x: 0, y: 0, k: 1 });

  // Build nodes + links from graph data
  useEffect(() => {
    // Build lookup: slug → raw entry for matching
    const slugToRaw = new Map<string, RawEntry>();
    const idToRaw = new Map<number, RawEntry>();
    for (const e of rawEntries) {
      slugToRaw.set(e.slug, e);
      idToRaw.set(e.id, e);
    }

    const nodes: ForceNode[] = graphData.nodes.map((n, idx) => {
      const isRaw = n.kind === "raw";

      // Try multiple strategies to find the matching raw entry
      let rawEntry: RawEntry | undefined;
      let src = "";

      if (isRaw) {
        // Strategy 1: match by ID number (e.g., id="1" or id="raw-00001")
        const idNum = parseInt(n.id.replace(/\D/g, ""), 10);
        if (!isNaN(idNum)) rawEntry = idToRaw.get(idNum);

        // Strategy 2: match by slug in label (e.g., label="wechat-text: wechat-f3ykfhpe")
        if (!rawEntry && n.label.includes(": ")) {
          const slug = n.label.split(": ").slice(1).join(": ");
          rawEntry = slugToRaw.get(slug);
        }

        // Strategy 3: extract source from label prefix
        if (!rawEntry && n.label.includes(": ")) {
          src = n.label.split(": ")[0];
        }

        if (rawEntry) src = rawEntry.source;
      }

      // Friendly label
      let friendlyLabel = n.label;
      if (isRaw) {
        const srcLabel = SOURCE_LABELS[src] ?? src;
        if (rawEntry) {
          friendlyLabel = `${srcLabel} #${rawEntry.id}`;
        } else if (src) {
          friendlyLabel = `${srcLabel} ${idx + 1}`;
        }
      }

      return {
        id: n.id,
        label: friendlyLabel,
        kind: n.kind,
        source: src,
        color: isRaw ? (SOURCE_COLORS[src] ?? DEFAULT_RAW_COLOR) : CONCEPT_COLOR,
        radius: isRaw ? 6 : 12,
      };
    });

    const nodeIds = new Set(nodes.map((n) => n.id));
    const links: ForceLink[] = graphData.edges
      .filter((e) => nodeIds.has(e.from) && nodeIds.has(e.to))
      .map((e) => ({
        source: e.from,
        target: e.to,
        edgeKind: e.kind,
      }));

    nodesRef.current = nodes;
    linksRef.current = links;

    // Create simulation
    const sim = forceSimulation<ForceNode>(nodes)
      .force("link", forceLink<ForceNode, ForceLink>(links).id((d) => d.id).distance(80).strength(0.4))
      .force("charge", forceManyBody().strength(-120))
      .force("center", forceCenter(0, 0))
      .force("collide", forceCollide<ForceNode>().radius((d) => d.radius + 4))
      .alphaDecay(0.02);

    sim.on("tick", () => {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = requestAnimationFrame(() => draw());
    });

    simRef.current = sim;

    return () => {
      sim.stop();
      cancelAnimationFrame(rafRef.current);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [graphData, rawEntries]);

  // Get connected node IDs for hover highlighting
  const getConnected = useCallback((nodeId: string): Set<string> => {
    const connected = new Set<string>();
    connected.add(nodeId);
    for (const link of linksRef.current) {
      const s = typeof link.source === "object" ? (link.source as ForceNode).id : link.source;
      const t = typeof link.target === "object" ? (link.target as ForceNode).id : link.target;
      if (s === nodeId) connected.add(t as string);
      if (t === nodeId) connected.add(s as string);
    }
    return connected;
  }, []);

  // Draw function
  const draw = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth;
    const h = canvas.clientHeight;
    canvas.width = w * dpr;
    canvas.height = h * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    const t = transformRef.current;
    ctx.clearRect(0, 0, w, h);
    ctx.save();
    ctx.translate(w / 2 + t.x, h / 2 + t.y);
    ctx.scale(t.k, t.k);

    const connected = hoverNode ? getConnected(hoverNode) : null;

    // Draw links
    for (const link of linksRef.current) {
      const s = link.source as ForceNode;
      const t2 = link.target as ForceNode;
      if (s.x == null || t2.x == null) continue;

      const isHighlighted = connected && connected.has(s.id) && connected.has(t2.id);
      ctx.beginPath();
      ctx.moveTo(s.x, s.y!);
      ctx.lineTo(t2.x, t2.y!);
      ctx.strokeStyle = isHighlighted ? LINK_HIGHLIGHT_COLOR : LINK_COLOR;
      ctx.lineWidth = isHighlighted ? 1.5 : 0.8;
      ctx.globalAlpha = connected && !isHighlighted ? DIM_OPACITY : 1;
      ctx.stroke();
      ctx.globalAlpha = 1;
    }

    // Draw nodes
    for (const node of nodesRef.current) {
      if (node.x == null) continue;
      const isActive = !connected || connected.has(node.id);

      ctx.globalAlpha = isActive ? 1 : DIM_OPACITY;

      // Circle
      ctx.beginPath();
      ctx.arc(node.x, node.y!, node.radius, 0, Math.PI * 2);
      ctx.fillStyle = node.color;
      ctx.fill();

      // Hover ring
      if (node.id === hoverNode) {
        ctx.strokeStyle = node.color;
        ctx.lineWidth = 2;
        ctx.stroke();
      }

      // Label
      if (t.k > 0.5 || node.kind === "concept") {
        ctx.fillStyle = isActive
          ? (canvas.isConnected ? getComputedStyle(canvas).getPropertyValue("--color-foreground").trim() : "") || "#2D2B28"
          : "#999";
        ctx.font = node.kind === "concept"
          ? `600 ${Math.max(10, 11 / t.k)}px "Lora", serif`
          : `400 ${Math.max(8, 9 / t.k)}px system-ui, sans-serif`;
        ctx.textAlign = "center";
        ctx.textBaseline = "top";
        ctx.fillText(
          node.label.length > 20 ? node.label.slice(0, 18) + "…" : node.label,
          node.x,
          node.y! + node.radius + 3,
        );
      }

      ctx.globalAlpha = 1;
    }

    ctx.restore();
  }, [hoverNode, getConnected]);

  // Redraw when hover changes
  useEffect(() => {
    draw();
  }, [hoverNode, draw]);

  // Find node at canvas coordinates
  const hitTest = useCallback((cx: number, cy: number): ForceNode | null => {
    const t = transformRef.current;
    const canvas = canvasRef.current;
    if (!canvas) return null;
    const w = canvas.clientWidth;
    const h = canvas.clientHeight;
    // Convert screen coords to simulation coords
    const sx = (cx - w / 2 - t.x) / t.k;
    const sy = (cy - h / 2 - t.y) / t.k;

    for (let i = nodesRef.current.length - 1; i >= 0; i--) {
      const n = nodesRef.current[i];
      if (n.x == null) continue;
      const dx = n.x - sx;
      const dy = n.y! - sy;
      if (dx * dx + dy * dy < (n.radius + 4) * (n.radius + 4)) return n;
    }
    return null;
  }, []);

  // Mouse handlers
  const handleMouseMove = useCallback((e: React.MouseEvent) => {
    const rect = canvasRef.current?.getBoundingClientRect();
    if (!rect) return;
    const cx = e.clientX - rect.left;
    const cy = e.clientY - rect.top;

    if (dragNodeRef.current) {
      const t = transformRef.current;
      const w = rect.width;
      const h = rect.height;
      dragNodeRef.current.fx = (cx - w / 2 - t.x) / t.k;
      dragNodeRef.current.fy = (cy - h / 2 - t.y) / t.k;
      simRef.current?.alpha(0.1).restart();
      return;
    }

    const hit = hitTest(cx, cy);
    setHoverNode(hit?.id ?? null);
    if (canvasRef.current) {
      canvasRef.current.style.cursor = hit ? "pointer" : "grab";
    }
  }, [hitTest]);

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    const rect = canvasRef.current?.getBoundingClientRect();
    if (!rect) return;
    const cx = e.clientX - rect.left;
    const cy = e.clientY - rect.top;
    const hit = hitTest(cx, cy);

    if (hit) {
      dragNodeRef.current = hit;
      const t = transformRef.current;
      hit.fx = (cx - rect.width / 2 - t.x) / t.k;
      hit.fy = (cy - rect.height / 2 - t.y) / t.k;
      simRef.current?.alphaTarget(0.3).restart();
      if (canvasRef.current) canvasRef.current.style.cursor = "grabbing";
    } else {
      // Pan start
      const startX = e.clientX;
      const startY = e.clientY;
      const startTx = transformRef.current.x;
      const startTy = transformRef.current.y;

      const onMove = (ev: MouseEvent) => {
        transformRef.current.x = startTx + (ev.clientX - startX);
        transformRef.current.y = startTy + (ev.clientY - startY);
        draw();
      };
      const onUp = () => {
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
      };
      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
    }
  }, [hitTest, draw]);

  const handleMouseUp = useCallback(() => {
    if (dragNodeRef.current) {
      dragNodeRef.current.fx = null;
      dragNodeRef.current.fy = null;
      simRef.current?.alphaTarget(0);
      dragNodeRef.current = null;
      if (canvasRef.current) canvasRef.current.style.cursor = "grab";
    }
  }, []);

  const handleClick = useCallback((e: React.MouseEvent) => {
    if (dragNodeRef.current) return; // Was dragging, not clicking
    const rect = canvasRef.current?.getBoundingClientRect();
    if (!rect) return;
    const hit = hitTest(e.clientX - rect.left, e.clientY - rect.top);
    if (!hit) return;

    if (hit.kind === "concept") {
      // Extract slug from id (e.g., "concept-inventory" → "concept-inventory")
      onClickConcept(hit.id);
    } else {
      onClickRaw();
    }
  }, [hitTest, onClickConcept, onClickRaw]);

  const handleWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    const delta = e.deltaY > 0 ? 0.9 : 1.1;
    const newK = Math.min(4, Math.max(0.2, transformRef.current.k * delta));
    transformRef.current.k = newK;
    draw();
  }, [draw]);

  // Resize observer
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const ro = new ResizeObserver(() => draw());
    ro.observe(container);
    return () => ro.disconnect();
  }, [draw]);

  return (
    <div ref={containerRef} className="relative h-full w-full">
      <canvas
        ref={canvasRef}
        className="h-full w-full"
        style={{ cursor: "grab" }}
        onMouseMove={handleMouseMove}
        onMouseDown={handleMouseDown}
        onMouseUp={handleMouseUp}
        onMouseLeave={handleMouseUp}
        onClick={handleClick}
        onWheel={handleWheel}
      />
    </div>
  );
}

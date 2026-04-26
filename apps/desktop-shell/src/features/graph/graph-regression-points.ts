export const GRAPH_INTERACTION_REGRESSION_POINTS = [
  "force-layout",
  "node-drag",
  "wheel-zoom",
  "canvas-pan",
  "edge-draw-animation",
  "node-enter-animation",
  "hover-highlight",
  "search-filter",
] as const;

export const GRAPH_VISUAL_LIMITS = {
  maxNodeDiameter: 16,
  maxEdgeStrokeWidth: 1,
  maxClusterOpacity: 0,
  dotGridSize: 0,
  dotDiameter: 0,
} as const;

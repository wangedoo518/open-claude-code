/**
 * Slice E28.1 — inline-SVG sparkline.
 *
 * Pure-SVG implementation, no charting library. Lives in
 * components/ so any future surface (e.g. a draft list, an
 * inbox row) can drop one in without bundle cost.
 *
 * Renders nothing when `data.length === 0` — caller doesn't need
 * to gate the mount. Flat data (all values equal) renders as a
 * horizontal line at the y-midpoint.
 *
 * Width / height default to a compact 60×16 — adjust at the call
 * site for taller / wider variants. Color defaults to the primary
 * terracotta token; pass `color` to override.
 */

export interface SparklineProps {
  /** Series to plot. Order = oldest → newest, left → right. */
  data: number[];
  /** SVG width in px. Default 60. */
  width?: number;
  /** SVG height in px. Default 16. */
  height?: number;
  /** Stroke color. Default `var(--color-primary)`. */
  color?: string;
  /** Optional aria-label. Default "趋势". */
  ariaLabel?: string;
}

export function Sparkline({
  data,
  width = 60,
  height = 16,
  color = "var(--color-primary)",
  ariaLabel = "趋势",
}: SparklineProps) {
  const path = computeSparklinePath(data, { width, height });
  if (!path) return null;
  return (
    <svg
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      role="img"
      aria-label={ariaLabel}
      style={{ display: "inline-block", verticalAlign: "middle" }}
    >
      <path
        d={path}
        fill="none"
        stroke={color}
        strokeWidth={1.5}
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

/**
 * Pure helper — compute the SVG path `d` attribute from a series.
 * Exported so tests can verify the path geometry without rendering
 * to the DOM. Empty input → empty string.
 *
 * Uses 1px padding on all sides so stroke isn't clipped at edges.
 * Flat data (all equal values) renders as a horizontal line at the
 * y-midpoint (range falls back to 1 to avoid div-by-zero).
 */
export function computeSparklinePath(
  data: number[],
  opts: { width: number; height: number },
): string {
  if (data.length === 0) return "";
  const { width, height } = opts;
  const pad = 1;
  const w = width - pad * 2;
  const h = height - pad * 2;
  const min = Math.min(...data);
  const max = Math.max(...data);
  const range = max - min || 1; // avoid div-by-zero on flat data
  const stepX = data.length > 1 ? w / (data.length - 1) : 0;
  const points = data.map((v, i) => {
    const x = pad + i * stepX;
    // Invert y so larger values render higher.
    const y = pad + h - ((v - min) / range) * h;
    return `${x.toFixed(2)},${y.toFixed(2)}`;
  });
  return `M${points.join(" L")}`;
}

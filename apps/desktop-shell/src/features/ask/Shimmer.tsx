/**
 * Shimmer — 流光文字效果，用于流式状态指示。
 * 参考 CodePilot shimmer.tsx：背景渐变横移 + bg-clip-text 透明文字。
 * 纯 CSS 实现，无需 framer-motion。
 */

import type { ReactNode } from "react";

interface ShimmerProps {
  children: ReactNode;
  duration?: number; // seconds, default 1.5
  className?: string;
}

export function Shimmer({ children, duration = 1.5, className = "" }: ShimmerProps) {
  return (
    <span
      className={`ask-shimmer-text ${className}`}
      style={{ animationDuration: `${duration}s` }}
    >
      {children}
    </span>
  );
}

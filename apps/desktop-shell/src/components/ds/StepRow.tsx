/**
 * StepRow — onboarding step primitive.
 *
 * v2 kit source: ui_kits/desktop-shell-v2/Connect.jsx:17-51 (the three
 * hand-written `<div className="step-row ...">` blocks). kit.css
 * selectors at ui_kits/desktop-shell-v2/kit.css:328-332 define the
 * `.step-row .n` circle + done/active accent.
 *
 * DS class contract: `.ds-step-row` + `data-state={pending|active|done}`
 * in apps/desktop-shell/src/globals.css.
 *
 * `children` is a CTA slot that renders below the `desc` line — used
 * by WeChat onboarding to drop the "开始扫码绑定" button onto the
 * active step.
 *
 * Migrated from WeChatBridgePage.tsx:538-572 inline (DS1.7-B-γ).
 */

import type { ReactNode } from "react";
import { CheckCircle2, type LucideIcon } from "lucide-react";

export type StepState = "pending" | "active" | "done";

export interface StepRowProps {
  /** 1-based step index shown inside the circle when not `done`. */
  n: number;
  /** Short serif title. */
  title: string;
  /** Muted one-line description. */
  desc?: string;
  /** Drives colour state + checkmark on `done`. */
  state: StepState;
  /** CTA slot — rendered below `desc`. Typically a button. */
  children?: ReactNode;
  /**
   * Optional icon override. By default `done` shows a `CheckCircle2`
   * and `active`/`pending` show the numeric `n`. Pass a Lucide icon
   * to override the numeric rendering (e.g. for custom step glyphs).
   */
  icon?: LucideIcon;
}

export function StepRow({
  n,
  title,
  desc,
  state,
  children,
  icon: IconOverride,
}: StepRowProps) {
  return (
    <div className="ds-step-row" data-state={state}>
      <div className="ds-step-n">
        {state === "done" ? (
          <CheckCircle2 className="size-3.5" strokeWidth={2} />
        ) : IconOverride ? (
          <IconOverride className="size-3.5" strokeWidth={1.75} />
        ) : (
          n
        )}
      </div>
      <div className="min-w-0 flex-1">
        <div className="ds-step-title">{title}</div>
        {desc && <p className="ds-step-desc">{desc}</p>}
        {children}
      </div>
    </div>
  );
}

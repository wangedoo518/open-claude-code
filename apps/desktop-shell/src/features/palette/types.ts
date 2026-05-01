/**
 * Palette type contract — shared by Worker A (UI) and Worker B (data).
 *
 * The UI shell consumes `PaletteGroup[]` and dispatches `onSelect(item)`
 * with a concrete `PaletteItem`. The data layer produces these items
 * from React Query data + CLAWWIKI_ROUTES + the recent store.
 *
 * `value` is the cmdk item identity — format is `"kind:id"` so the
 * UI dispatcher can look up the action regardless of shape. Parsing
 * is done via `paletteValueFor(...)` which both sides MUST use.
 *
 * Palette item schema.
 *
 * S1 sprint extension — adds:
 *   kind              typed source (route | wiki | raw | inbox)
 *   score             sort score assigned by usePaletteItems
 *   why               short human-readable "reason this appeared"
 *   secondaryActions  per-item action chips (ask_with / focus_graph / ...)
 *
 * Consumed by:
 *   - usePaletteItems.ts     assigns kind/score/why/secondaryActions
 *   - CommandPalette.tsx     renders kind badge + why + chips
 *   - actions.ts             dispatches secondary actions via buildAskBindUrl /
 *                            buildGraphFocusUrl (see navigate-helpers.ts)
 */

import type { ComponentType } from "react";
import type { LucideProps } from "lucide-react";

/** Icon type alias (Lucide forwards refs). */
export type PaletteIcon = ComponentType<LucideProps>;

export type PaletteItemKind = "route" | "wiki" | "raw" | "inbox";

/**
 * S1 sprint — secondary action chips rendered on a palette row.
 *
 * Each action is dispatched by the palette's action layer; the `id`
 * is the discriminator the dispatcher switches on, and `label` is the
 * Chinese user-facing chip text.
 *
 *   - `ask_with`    — open a fresh Ask session bound to this source.
 *                      Uses `buildAskBindUrl(...)` from
 *                      `features/wiki/navigate-helpers.ts`.
 *   - `focus_graph` — open the Graph page focused on a wiki slug.
 *                      Uses `buildGraphFocusUrl(...)`.
 *   - `open_raw`    — jump to the Raw detail view for this entry.
 *   - `open_wiki`   — jump to the Wiki article tab for this slug.
 */
export interface PaletteItemSecondaryAction {
  id: "ask_with" | "focus_graph" | "open_raw" | "open_wiki";
  /** Chinese user-facing chip label. */
  label: string;
}

interface PaletteItemBase {
  /** Unique value for cmdk; format `${kind}:${id}`. */
  value: string;
  /** Primary display text in the row. */
  label: string;
  /** Secondary muted text (e.g. source type, status, id). */
  hint?: string;
  /** Optional leading icon. */
  icon?: PaletteIcon;
  /**
   * S1 sprint — sort score assigned by `usePaletteItems`.
   * Higher is better. Absent when the item was produced by a
   * pre-S1 path that doesn't compute scores.
   */
  score?: number;
  /**
   * S1 sprint — short Chinese-language "why this appeared" explanation
   * (e.g. "最近打开过", "匹配当前 slug", "最高相关度"). Rendered as a
   * muted caption on the row when present.
   */
  why?: string;
  /**
   * S1 sprint — per-row action chips (ask_with / focus_graph / ...).
   * The palette shell renders these as keyboard-reachable chips and
   * dispatches to the corresponding handler in `actions.ts`.
   */
  secondaryActions?: PaletteItemSecondaryAction[];
  /** Stable command registry id when this row is backed by a command. */
  commandId?: string;
}

export interface RoutePaletteItem extends PaletteItemBase {
  kind: "route";
  /** Route key from CLAWWIKI_ROUTES (stable across renames). */
  routeKey: string;
  /** Pathname to navigate to. */
  path: string;
}

export interface WikiPaletteItem extends PaletteItemBase {
  kind: "wiki";
  slug: string;
  title: string;
}

export interface RawPaletteItem extends PaletteItemBase {
  kind: "raw";
  id: number;
}

export interface InboxPaletteItem extends PaletteItemBase {
  kind: "inbox";
  id: number;
}

export type PaletteItem =
  | RoutePaletteItem
  | WikiPaletteItem
  | RawPaletteItem
  | InboxPaletteItem;

/** Stable group ids (order preserved in the UI). */
export type PaletteGroupId =
  | "recent"
  | "pages"
  | "wiki"
  | "raw"
  | "inbox"
  | "ask-mode";

export interface PaletteGroup {
  id: PaletteGroupId;
  /** Section heading shown above items. */
  heading: string;
  items: PaletteItem[];
  /** Shows a CommandLoading row when true (only meaningful for async groups). */
  isLoading?: boolean;
  /** Tags the group's fetch as failed; UI renders a muted warning row. */
  isError?: boolean;
}

/**
 * Recent store schema — persisted via zustand + namespacedStorage.
 *
 * We store a display-label *snapshot* so that if the underlying record
 * is deleted (raw entry purged, wiki page removed), the recent item
 * still reads correctly. Re-navigation uses F2's deep-link banner for
 * the degraded path on Raw/Inbox; Wiki gets its own graceful-remove
 * path in the action dispatcher.
 */
export interface PaletteRecentItem {
  kind: PaletteItemKind;
  /** route.key | wiki slug | raw id toString | inbox id toString */
  id: string;
  label: string;
  hint?: string;
  timestamp: number;
}

/** Build the cmdk `value` string from kind + id. Both sides MUST use this. */
export function paletteValueFor(kind: PaletteItemKind, id: string | number): string {
  return `${kind}:${id}`;
}

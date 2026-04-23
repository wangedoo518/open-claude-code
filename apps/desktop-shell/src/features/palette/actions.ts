/**
 * Palette action dispatcher — executes a selected PaletteItem.
 *
 * `executePaletteItem` is a synchronous `void` function so the UI
 * caller (CommandPalette) can do exactly:
 *
 *     executePaletteItem(item, ctx);
 *     closePalette();
 *
 * Never touch `open` state here — the UI owns closing. Each branch is
 * responsible for:
 *   1. Performing the canonical navigation for that kind
 *   2. Pushing a dedup'd entry onto the Recent list so it appears
 *      first next time the palette opens with empty query
 *
 * Stale deep-link handling: Raw/Inbox use their existing F2
 * `DeepLinkNotFoundBanner` when the id has been purged; Wiki's
 * `openTab` with a missing slug surfaces as a page-level loading
 * state. We intentionally keep the action dispatcher minimal and
 * let downstream pages degrade gracefully.
 *
 * ───────────────────────────────────────────────────────────────
 * S1 — Unified Search secondary actions
 *
 * In addition to the primary "select" path (Enter / click), each
 * palette item can expose a small set of secondary actions
 * (`PaletteItemSecondaryAction[]`) that render as chips on the
 * right edge of the row. The `executePaletteItemAction` dispatcher
 * below handles those — they navigate the user elsewhere (Ask with
 * this page, Focus graph, Open Wiki, Open Raw) and always close
 * the palette afterwards.
 *
 * Secondary action id vocabulary (see `PaletteItemSecondaryAction`
 * in ./types for the canonical union):
 *   - "ask_with"    — Ask flow, prebinding this item as source
 *   - "focus_graph" — Graph page focused on this wiki slug
 *   - "open_wiki"   — Jump straight to /wiki/<slug>
 *   - "open_raw"    — Jump straight to /raw/<id>
 *
 * The URL helpers `buildAskBindUrl` / `buildGraphFocusUrl` live in
 * `@/features/wiki/navigate-helpers` (Worker C canonical). This file
 * only performs dispatch and leaves URL shape authority there, so a
 * future URL-format change can land in one place.
 */

import type { NavigateFunction } from "react-router-dom";

import {
  buildAskBindUrl,
  buildGraphFocusUrl,
} from "@/features/wiki/navigate-helpers";
import type {
  PaletteItem,
  PaletteItemKind,
  PaletteItemSecondaryAction,
  PaletteRecentItem,
} from "./types";
import type { WikiTabItem } from "@/state/wiki-tab-store";
import type { AppMode } from "@/state/settings-store";
import type { SourceRef } from "@/lib/tauri";
import { useAskUiStore } from "@/state/ask-ui-store";

export interface PaletteActionContext {
  navigate: NavigateFunction;
  openTab: (item: WikiTabItem) => void;
  setAppMode: (mode: AppMode) => void;
  pushRecent: (item: Omit<PaletteRecentItem, "timestamp">) => void;
  removeRecent: (kind: PaletteItemKind, id: string) => void;
}

/**
 * Shorthand for the discriminator of `PaletteItemSecondaryAction`.
 * Lets call sites write `actionId: PaletteSecondaryActionId` without
 * needing to index into the interface every time.
 */
export type PaletteSecondaryActionId = PaletteItemSecondaryAction["id"];

/**
 * Derive a `SourceRef` from a palette item, when the item's kind
 * is one the Ask flow can bind to. Returns `null` for `route`
 * items (routes have no source ref).
 */
function sourceRefFromPaletteItem(item: PaletteItem): SourceRef | null {
  switch (item.kind) {
    case "wiki":
      return { kind: "wiki", slug: item.slug, title: item.title };
    case "raw":
      return { kind: "raw", id: item.id, title: item.label };
    case "inbox":
      return { kind: "inbox", id: item.id, title: item.label };
    case "route":
      return null;
  }
}

export function executePaletteItem(
  item: PaletteItem,
  ctx: PaletteActionContext,
): void {
  const { navigate, openTab, setAppMode, pushRecent } = ctx;

  switch (item.kind) {
    case "route": {
      // Batch E §1 — synthetic `ask.demo` route fires the demo toggle
      // in the Ask UI store in addition to navigating to /ask. The
      // store drives AskWorkbench's banner + mock-message injection;
      // see state/ask-ui-store.ts + features/ask/AskWorkbench.tsx.
      if (item.routeKey === "ask.demo") {
        useAskUiStore.getState().setShowDemo(true);
      }
      navigate(item.path);
      pushRecent({
        kind: "route",
        id: item.routeKey,
        label: item.label,
        hint: item.hint,
      });
      return;
    }
    case "wiki": {
      // Wiki target: switch app mode, open a tab, then route to /wiki.
      setAppMode("wiki");
      openTab({
        id: item.slug,
        kind: "article",
        slug: item.slug,
        title: item.title,
        closable: true,
      });
      navigate("/wiki");
      pushRecent({
        kind: "wiki",
        id: item.slug,
        label: item.label,
        hint: item.hint,
      });
      return;
    }
    case "raw": {
      navigate(`/raw?entry=${item.id}`);
      pushRecent({
        kind: "raw",
        id: String(item.id),
        label: item.label,
        hint: item.hint,
      });
      return;
    }
    case "inbox": {
      navigate(`/inbox?task=${item.id}`);
      pushRecent({
        kind: "inbox",
        id: String(item.id),
        label: item.label,
        hint: item.hint,
      });
      return;
    }
  }
}

/**
 * Dispatch a secondary action from a palette item — chip click or
 * Shift+Enter shortcut.
 *
 * The caller (CommandPalette) is responsible for closing the
 * palette after this returns; closing on every branch here keeps
 * the parent UI simpler.
 *
 * Unknown (action, kind) pairs are silently ignored. The UI
 * already guards by only rendering chips for supported kinds, but
 * we defend against stale item data on disk too.
 */
export function executePaletteItemAction(
  item: PaletteItem,
  actionId: PaletteSecondaryActionId,
  ctx: PaletteActionContext,
): void {
  const { navigate } = ctx;

  switch (actionId) {
    case "ask_with": {
      const source = sourceRefFromPaletteItem(item);
      if (!source) return;
      navigate(stripHashPrefix(buildAskBindUrl(source)));
      return;
    }
    case "focus_graph": {
      if (item.kind !== "wiki") return;
      navigate(stripHashPrefix(buildGraphFocusUrl(item.slug)));
      return;
    }
    case "open_wiki": {
      if (item.kind !== "wiki") return;
      navigate(`/wiki/${encodeURIComponent(item.slug)}`);
      return;
    }
    case "open_raw": {
      if (item.kind !== "raw") return;
      navigate(`/raw/${item.id}`);
      return;
    }
  }
}

/**
 * `buildAskBindUrl` / `buildGraphFocusUrl` return hash-prefixed URLs
 * (e.g. `#/graph?focus=foo`) because they're also meant to be used
 * directly in `<a href>` or `window.location.hash` — the prefix is
 * required there. React Router's `navigate()` under HashRouter, on
 * the other hand, expects a plain path (`/graph?focus=foo`) and would
 * otherwise append the hash to the current location, producing
 * `#/wiki#/graph?...`. Strip the single leading `#` before dispatch.
 */
function stripHashPrefix(url: string): string {
  return url.startsWith("#") ? url.slice(1) : url;
}

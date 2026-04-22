# Iconography — ClawWiki

## Primary: Lucide React

All in-app iconography is **Lucide React** (`lucide-react` 0.3.x), imported individually per component. Example, straight from `DashboardPage.tsx`:

```tsx
import {
  Loader2, MessageCircle, FileStack, ServerCog, Brain,
  Inbox as InboxIcon, ArrowRight,
} from "lucide-react";
```

**Stroke weight:** Lucide default (1.5px). Do not override.
**Sizes (use Tailwind's `size-*` helpers):**

| Size | Token | Use |
|------|-------|-----|
| `size-3` | 12px | Inline with dense body text, badge glyphs, list-item markers |
| `size-3.5` | 14px | Default inline icon next to a 13–14px label (e.g. `MessageCircle` in `QuickAsk` CTA) |
| `size-4` | 16px | Button icon (shadcn Button's default `[&_svg:not([class*='size-'])]:size-4`) |
| `size-5` | 20px | Standalone nav or toolbar icon |
| `size-6`+ | 24px+ | Hero / empty-state illustration only |

**Color:** icons inherit via `currentColor`; tint by setting `color` on the parent or via `style={{ color: "var(--claude-orange)" }}` for a single-use accent. Never `fill=` on a Lucide icon.

**Animation:** `Loader2` + `.animate-spin` for loading. `Sparkles` is the canonical Maintainer-AI glyph.

## Spinner & streaming glyphs

- Loading: `<Loader2 className="size-3 animate-spin" />`
- Streaming dot pulse: `<span className="animate-shimmer">●</span>`
- "AI is thinking" text: the whole run uses `.ask-shimmer-text` — a flowing gradient across Muted → Terracotta → Foreground → Terracotta → Muted, not an icon.

## Unicode figures (terminal heritage)

From `figures.ts` in the Claude Code source — use for status indicators where a Lucide import would be overkill (e.g. terminal-ish tool cards):

| Glyph | Const | Use |
|------|-------|-----|
| `⏺` / `●` | `BLACK_CIRCLE` | active / recording dot |
| `∙` | `BULLET_OPERATOR` | list item separator |
| `↯` | `LIGHTNING_BOLT` | fast mode |
| `○ ◐ ● ◉` | effort levels | low / med / high / max |
| `▶` `⏸` | play / pause | demo mode |
| `▎` | blockquote bar | thin left rule |
| `◇` `◆` | diamond | review pending / done |
| `⚑` | flag | Issue / conflict |

## Emoji

**Product copy: never.** No emoji in headlines, body, buttons, or empty states. The only current emoji usage is in `clawwiki-routes.ts` as route-label prefixes (`📊 Dashboard`, `💬 Ask`, `📨 Inbox`…) — treat this as legacy, pending swap to a Lucide-only sidebar.

## Never

- Never draw a custom SVG icon. If Lucide doesn't have it, pick the closest Lucide icon and document the mapping.
- Never use Font Awesome, Material Icons, Heroicons, or any other kit in the same screen as Lucide.
- Never colorize icons with gradients. Flat `currentColor` only.
- Never animate an icon beyond `animate-spin` or opacity shimmer.

## Logo assets

| File | Use |
|------|-----|
| `assets/favicon.svg` | App icon — orange `#F97316` roundel with white "C". Window/tab favicon, splash screens |
| `assets/openclaw-logo.svg` | OpenClaw sub-brand — shown in settings "about" panel |
| `assets/warwolf-logo.png` | Warwolf desktop-server mark — shown only in developer/debug views |

The sidebar header does **not** use a logo file. It uses a 32×32 `bg-primary rounded-lg` swatch with a white serif "C" glyph — see `preview/brand-lockups.html`. This keeps the sidebar icon-mode (48px collapsed) crisp at any DPI without raster assets.

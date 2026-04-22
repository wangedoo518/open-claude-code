# ClawWiki Design System

> A warm, editorial design system for **ClawWiki** — the desktop app that turns every WeChat forward into a maintained knowledge base. Built on the Anthropic / Claude visual language: parchment canvas, Terracotta CTA, warm-only neutrals, serif headlines, ring-based depth.

---

## Sources

All tokens and components were reverse-engineered from:

| Path | What it gave us |
|------|-----------------|
| `claudewiki/apps/desktop-shell/src/globals.css` | Canonical CSS variables (OkLCH) — palette, radius, shadow, type scale |
| `claudewiki/docs/desktop-shell/tokens/design-tokens.md` | 89 semantic tokens (terminal-origin), claude orange `#d77757`, semantic feedback colors |
| `claudewiki/apps/desktop-shell/src/components/ui/*` | shadcn-derived Button / Dialog / Badge / Sidebar primitives |
| `claudewiki/apps/desktop-shell/src/shell/ClawWikiShell.tsx` | Three-pane shell: 256px Sidebar → main → 320px ChatSidePanel |
| `claudewiki/apps/desktop-shell/src/features/dashboard/DashboardPage.tsx` | Hero layout, stat-card pattern, activity feed |
| `claudewiki/apps/desktop-shell/src/features/ask/*` | Message bubbles, tool-actions group, streaming shimmer, composer |
| `claudewiki/apps/desktop-shell/src/features/inbox/InboxPage.tsx` | Maintainer Workbench §1/§2/§3 three-section pattern |
| `claudewiki/docs/desktop-shell/wireframes/claudewiki-wireframe.html` | High-level IA: topbar + sidebar + content + chat panel |
| `claudewiki/README.md` | Product philosophy — "放弃做瑞士军刀，打造一把手术刀" |

Attached codebase path (read-only): `claudewiki/` — full Tauri 2 + React workspace.
GitHub mirror: `wangedoo518/claudewiki@main`.

---

## Product context

**ClawWiki = 你的外脑。** The product philosophy, in one sentence:

> 微信喂料 → AI 审阅 → 认知资产沉淀。

It is a **Tauri 2 + React** desktop app with a single user story: the user forwards anything (text, voice, PPT, video, mp.weixin.qq.com URL, PDF…) to a WeChat 外联机器人, and a few seconds later a **Maintainer AI** has summarised, deduplicated, linked, and filed it into a Karpathy-style three-layer wiki (`raw/` → `wiki/` → `schema/`). The desktop shell is where the user does the other half of the loop: **审阅** (approve merges in Inbox) and **问答** (ask the wiki in Ask).

Seven primary routes (1 — Dashboard, 2 — Ask, 3 — Inbox, 4 — Raw, 5 — Wiki, 6 — Graph, 7 — Schema) plus a single funnel route (WeChat Bridge) and Settings.

**This is not a general-purpose AI client.** The `README.md` explicitly lists eleven things it doesn't do — no MinApp gallery, no CLI launcher, no mobile, no vault sync. The design language is tuned to match: quiet, editorial, and unhurried.

---

## What's in this folder

| Path | What it is |
|---|---|
| `colors_and_type.css` | Single source of truth — CSS variables for every color, font size, radius, shadow, and semantic alias |
| `fonts/` | Nearest-match webfonts (Lora / Inter / JetBrains Mono — see Fonts note below) |
| `assets/` | Logos (OpenClaw, Warwolf) and the ClawWiki favicon |
| `preview/` | One self-contained HTML card per token cluster — registered into the Design System tab |
| `ui_kits/desktop-shell/` | Interactive recreation of the ClawWiki desktop shell (Dashboard / Ask / Inbox) |
| `SKILL.md` | Agent-skill front-matter for reuse in Claude Code |

---

## Content fundamentals

**Voice is quiet-intellectual, not product-marketing.** Headlines read like book chapter titles, not feature launches. The README ships with metaphor ("手术刀" / "外脑" / "漏斗") and specific time-stamped user stories ("周二下午 2:14 PM…") rather than capability lists.

**Bilingual — 中文 first, English for technical nouns.** Chinese carries the feeling ("你的外脑" / "认知资产沉淀") and English carries the architecture ("Maintainer AI", "Karpathy 三层", "CCD 4 件套"). Never translate proper product nouns. Inline tech terms stay English inside a Chinese sentence.

**Casing is natural sentence case.** No Title Case Marketing Headlines. UI labels capitalise only product nouns. Uppercase is reserved for `OVERLINE` metadata (tracking 0.5px, 10–11px), never for buttons or titles.

**You / I framing.** The product is **你的** external brain — second-person, possessive, warm. The app refers to itself in third person ("Maintainer AI 审阅了 23 个页面"). Never "we" or "our AI". Copy examples, verbatim from the codebase:

- Hero: "你的外脑"
- Empty state: "还没有素材。粘贴第一条 →"
- Stat card: "今日入库" · value · "共 142 条"
- CTA: "开始对话", "立即巡检", "打开 Wiki"
- Error: "代理不可达", "加载失败：{message}"

**No emoji inside product copy** — the sidebar uses emoji (📊 💬 📨 📥 📖 🕸 📐 🔗 ⚙️) as route glyphs *only* because they substitute for an unshipped icon font. Everywhere else, the rule is strict: zero emoji in headlines, body, or button labels. Unicode figures (`⏺ ∙ ↯ ◐ ◆ ⚑`) are used for terminal/status indicators per `figures.ts`.

**Numbers are plain and tabular.** `tabular-nums` on every stat. No "+142%" growth decorations. Times render as `14:23`, dates as `2026-04-20`.

**Vibe:** a well-kept personal library — bookish, confident, never breathless.

---

## Visual foundations

**Canvas.** Everything starts on **Parchment `#f5f4ed`** — a warm cream with a faint yellow-green tint, chosen to feel like aged paper, *never* a screen. Cards sit on **Ivory `#faf9f5`**, one step up. Pure white is reserved for a single button variant. Dark mode inverts to **Near Black `#141413`** (warm, olive-tinted) page and **Dark Surface `#30302e`** cards — there is no cool charcoal anywhere in the system.

**Color vibe.** Exclusively warm. Every neutral has a yellow-brown undertone; every border is cream; every "grey" is Olive (`#5e5d59`), Stone (`#87867f`), or Charcoal Warm (`#4d4c48`). The one saturated color is **Terracotta `#c96442`** (with the Coral `#d97757` link/hover variant), used sparingly: primary CTA, active-nav left bar, streaming message border pulse. **Focus Blue `#3898ec` is the only cool color in the system** — restricted entirely to `:focus-visible` rings for accessibility, never decoration.

**Type.** Serif for authority, sans for utility. **Lora 500** (Anthropic Serif substitute — see Fonts note) for every `h1`–`h6`, at a single weight, line-height 1.10–1.30. **Inter** for all UI chrome, body, and buttons. **JetBrains Mono** for code. No bold serifs — 500 is the ceiling. Body line-height is **1.60** — more generous than a typical dashboard, less dense than a book, intentionally editorial.

**Backgrounds.** Flat. No background gradients. No repeating patterns. No grain. The only "gradient" in the system is a 3px `section-divider-warm` bar (`parchment → terracotta → near-black`) used as a chapter break. Imagery, when it appears, is placeholder rectangles or product screenshots embedded in `radius-2xl` containers — never full-bleed, never overlaid.

**Depth.** Five levels, ring-first:

| Level | Treatment |
|---|---|
| 0 Flat | No shadow, no border — inline text, page background |
| 1 Contained | `1px solid #f0eee6` — standard card |
| 2 Ring | `0 0 0 1px #d1cfc5` — interactive card / button hover |
| 3 Whisper | `0 4px 24px rgba(0,0,0,0.05)` — elevated screenshot |
| 4 Inset | `inset 0 0 0 1px rgba(0,0,0,0.15)` — active / pressed |

Shadows are **warm-toned rings that pretend to be borders**, not drop shadows. The signature move is `box-shadow: 0 0 0 1px` — zero blur, zero spread from the edge, a halo that hugs the element. Drop shadows, when they appear (Whisper), are barely visible — 5% opacity, 24px blur.

**Radii.** Soft, generous, 7-tier scale (see `colors_and_type.css`). Sharp corners (<6px) are forbidden on interactive surfaces. Standard button/card is `8–10px`; primary button and inputs jump to `12–14px`; hero containers and embedded media go up to `22–26px`.

**Borders.** Mostly `border-cream #f0eee6` — the faintest possible containment. Stronger `#e8e6dc` appears on section dividers and list-item separators (`border-top: 1px`, never all four). Dark-mode borders are `#30302e` — borders that step *darker*, not lighter, on dark.

**Hover / press.** Hover = ring deepens (`--ring-warm` → `--ring-deep`) + optional `translateY(-1px)`. Never brightness shifts, never scale. Press = inset ring at 15% opacity, no shrink, no bounce. Active sidebar item gets a **3px Terracotta left border** (`border-left: 3px solid #c96442`) — the single visual anchor of the brand.

**Transparency & blur.** Used sparingly: `backdrop-filter: blur(12px)` on the sidebar (`rgba(255,255,255,0.7)`) per the wireframe; selection highlight `color-mix(--primary 30%, transparent)`. Never on cards, modals, or content.

**Animation.** Functional, not decorative. Three canonical keyframes from `globals.css`:

- `shimmer` — 1.5s ease-in-out infinite opacity 0.3 ↔ 1, for streaming indicator
- `dt-border-pulse` — 2s ease-in-out infinite, Terracotta ↔ Coral, on streaming message left border
- `fade-in` — 0.2s ease-out, `translateY(4px) → 0`, on new messages and route transitions
- `ask-shimmer-flow` — 1.5s linear, flowing gradient across text for "AI is thinking"

Easing is always `ease-in-out` or `ease-out`. No springs, no bounces, no `cubic-bezier` easter eggs.

**Layout rules.** Desktop-first. Three-pane shell (Sidebar 256 / collapsed 48 · main · ChatSidePanel 320 in Wiki mode). Max content width ~1200px. Section vertical rhythm is generous (editorial pacing — `py-6` to `py-12`). 8-based spacing scale with half-steps at 3/6/10/30.

**Imagery.** This codebase has essentially no decorative imagery — the product's entire visual personality is type + warm neutrals + terracotta accent. When imagery is needed, it's product screenshots in `radius-2xl` containers with `shadow-whisper`. Placeholder convention: a Warm Sand rectangle with an Olive Gray `◇` diamond glyph, never lorem-ipsum faces or stock photos.

---

## Iconography

See `ICONOGRAPHY.md` — short version: **Lucide React** is the icon system, imported individually (`<FileStack />`, `<InboxIcon />`, `<MessageCircle />`, `<Sparkles />`, `<Loader2 className="animate-spin" />`). Stroke weight 1.5 (Lucide default), size 14–16px inline, 12px (`size-3`) inside dense tables.

The sidebar is the one place emoji glyphs leak through (`📊 💬 📨 📥 📖 🕸 📐 🔗 ⚙️`) because routes are declared in plain TypeScript and haven't been swapped to Lucide yet — treat this as legacy, not canonical. In new work, always reach for Lucide first; fall back to Unicode figures from `figures.ts` (`⏺ ∙ ↯ ◐ ◆ ⚑`) for terminal-inspired status indicators. Never draw icons with custom SVG.

**Logos live in `assets/`:**
- `openclaw-logo.svg` — the OpenClaw sub-brand mark
- `warwolf-logo.png` — the Warwolf (desktop-server) mark
- `favicon.svg` — the orange "C" roundel used as the app icon

The in-app header uses a plain `bg-primary` rounded-lg swatch with a white **C** glyph — no logo file is shipped into the sidebar; this is deliberate. See `preview/brand-lockups.html`.

---

## Index

- **Tokens:** `colors_and_type.css` · `preview/*.html` cards
- **Brand assets:** `assets/openclaw-logo.svg`, `assets/warwolf-logo.png`, `assets/favicon.svg`
- **UI kit:** `ui_kits/desktop-shell/index.html` · components in `ui_kits/desktop-shell/*.jsx`
- **Iconography guide:** `ICONOGRAPHY.md`
- **Reusable skill:** `SKILL.md`

---

## Fonts — known substitution

The codebase references a proprietary family called **Anthropic Serif / Sans / Mono** which has no public distribution. We substitute:

| Role | Anthropic family | Substitute (this system) |
|------|------------------|--------------------------|
| Serif headings | Anthropic Serif | **Lora 500** (closest open metric + feel) |
| UI / body | Anthropic Sans | **Inter** |
| Code | Anthropic Mono | **JetBrains Mono** |

Lora is loaded via Google Fonts at weights 400/500/600 in `colors_and_type.css`. The `globals.css` in the codebase already uses Lora as its first fallback — we're aligned. **If you have the real Anthropic typefaces, drop them into `fonts/` and prepend them in the `--font-serif` / `--font-sans` / `--font-mono` stacks.**

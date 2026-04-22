---
name: clawwiki-design
description: Use this skill to generate well-branded interfaces and assets for ClawWiki, either for production or throwaway prototypes/mocks/etc. Contains essential design guidelines, colors, type, fonts, assets, and UI kit components for prototyping.
user-invocable: true
---

Read the README.md file within this skill, and explore the other available files.
If creating visual artifacts (slides, mocks, throwaway prototypes, etc), copy assets out and create static HTML files for the user to view. If working on production code, you can copy assets and read the rules here to become an expert in designing with this brand.
If the user invokes this skill without any other guidance, ask them what they want to build or design, ask some questions, and act as an expert designer who outputs HTML artifacts _or_ production code, depending on the need.

## Quick orientation

ClawWiki is a desktop app that turns WeChat forwards into a maintained personal wiki. Visual language = Anthropic / Claude: **Parchment `#f5f4ed`** canvas, **Terracotta `#c96442`** CTA, warm-only neutrals, **Lora 500** serif headlines + **Inter** UI + **JetBrains Mono** code, **ring-based** depth (`0 0 0 1px warm-gray`), generous 1.60 body line-height, flat backgrounds (no gradients except the chapter divider).

## Files

- `colors_and_type.css` — single source of truth; import this before anything else
- `README.md` — CONTENT FUNDAMENTALS + VISUAL FOUNDATIONS in detail
- `ICONOGRAPHY.md` — Lucide React rules + Unicode figures
- `assets/` — `favicon.svg`, `openclaw-logo.svg`, `warwolf-logo.png`
- `preview/*.html` — one small card per token cluster (colors, type, spacing, components, brand)
- `ui_kits/desktop-shell/` — interactive recreation of Dashboard / Ask / Inbox, with reusable JSX components (`Sidebar`, `TopBar`, `StatCard`, `ChatMessage`, `Composer`, `InboxPage`)

## Hard rules

1. No cool grays anywhere. Every neutral has a warm (yellow-brown) undertone.
2. Lora serif headings at weight **500 only** — never 700+.
3. Ring shadows (`0 0 0 1px`), never heavy drop shadows. `--shadow-whisper` is the only soft-blur shadow.
4. Background is **Parchment**, cards are **Ivory**. Pure white is reserved for one button variant.
5. Focus Blue `#3898ec` is the only cool color — use it only for `:focus-visible` rings.
6. Terracotta is a scalpel, not a brush — only on primary CTAs and active-nav left bars.
7. Zero emoji in product copy. (Sidebar route emoji is legacy, pending a Lucide swap.)
8. Body line-height `1.60`. Heading line-height `1.10–1.30`.
9. Bilingual: 中文 first for feel, English for technical nouns. Never translate proper product nouns.
10. Numbers are `font-variant-numeric: tabular-nums`, never decorated with `+xx%` growth arrows.

## Fonts — substitution note

The codebase references proprietary **Anthropic Serif / Sans / Mono**. This skill substitutes:

- Serif → **Lora 500** (Google Fonts)
- Sans → **Inter**
- Mono → **JetBrains Mono**

All three are loaded via Google Fonts in `colors_and_type.css`. If the real Anthropic typefaces become available, prepend them to the `--font-*` stacks.

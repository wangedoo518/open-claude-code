# Learned Rules

Stable rules extracted from repeated execution experience.
Only promote here when evidence is strong (systemic pattern + successful fix).

> Last updated: 2026-04-16

## LR-1: Heading typography — class over inline

**Rule**: New page-level headings (`<h1>`–`<h3>`) MUST use Tailwind size
classes (`text-lg`, `text-xl`, `text-2xl`, etc.) instead of inline
`style={{ fontSize }}`. The `@layer base` rule provides `font-family:
Lora`, `font-weight: 500`, `line-height: 1.30` — agents only need to
specify size.

**Evidence**: P1-6 through P2-1 cleaned 8 headings across 7 files that
all had the same pattern: `style={{ fontSize: 18, fontWeight: 600 }}`.
The 600 weight fought the base 500 rule; the inline fontSize prevented
class-based consistency.

**Counter-example**: Overline labels (`<h2 style={{ fontSize: 11 }}>`
with `uppercase tracking-widest`) are NOT semantic headings — they are
visual section dividers. These are intentionally small and should NOT
be forced into the heading scale. Leave them until a deliberate
semantic refactor.

## LR-2: ReactMarkdown in read surfaces — minimal components only

**Rule**: Full-read markdown surfaces (WikiArticle, RawLibrary reader,
WikiExplorer detail, WikiQueryMessage) should use `.markdown-content`
CSS class for typography and pass at most `components={{ a: Anchor }}`
for link semantics. Do NOT re-create per-page h1/h2/h3/p/ul/code/
blockquote component presets — they drift from the shared CSS rules.

**Evidence**: P1-8 removed ~290 lines of duplicated ReactMarkdown
component overrides across 4 files. The shared CSS covers heading
sizes, code blocks, blockquotes, tables, links, and list styling.

## LR-3: TreeNode action model — type-safe, no dead nodes

**Rule**: WikiFileTree `TreeNode` must carry an explicit
`action: { type: "openTab"; tab: WikiTabItem } | { type: "navigate"; to: string }`.
Never use `kind` field to infer action in a handler switch — that
creates dead-click nodes when a new kind is added without a matching
case.

**Evidence**: P1-12 found Schema/CLAUDE.md and Raw child nodes were
dead clicks because `handleNodeClick` only handled `article` and `log`.
The data-driven action model eliminated the pattern.

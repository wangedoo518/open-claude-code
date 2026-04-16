# Path Risk Matrix

Paths in this repository ranked by modification risk. Agents MUST check
this matrix before editing files outside their task scope.

> Last verified: 2026-04-16

## Critical (break user-facing flow or data)

| Path | Risk | Why |
|------|------|-----|
| `rust/crates/desktop-core/src/lib.rs` | `append_user_message` / `maybe_enrich_url` | Only backend URL ingest entry point. Double-write or removal breaks Ask ingest pipeline |
| `rust/crates/desktop-server/src/lib.rs` | `append_message` / `query_wiki_handler` | HTTP surface — SSE protocol, session lifecycle. Breaking change = frontend can't talk to backend |
| `rust/crates/wiki_store/src/lib.rs` | `write_raw_entry` / `slugify` | Raw persistence + content validation gate. Data-loss risk |
| `rust/crates/wiki_maintainer/src/lib.rs` | `query_wiki` / `absorb_batch` | LLM prompt + source ranking + crystallization. Semantic drift = wrong answers |
| `apps/desktop-shell/src/features/ask/useAskSession.ts` | Session create/send lifecycle | Session pile-up bug (P0-1) was here. Lazy-create contract must stay |

## High (break layout, navigation, or component tree)

| Path | Risk | Why |
|------|------|-----|
| `apps/desktop-shell/src/shell/ClawWikiShell.tsx` | SidebarProvider + scroll wrapper | Overflow-hidden / scroll bug (v3 design) was here |
| `apps/desktop-shell/src/shell/Sidebar.tsx` | Rowboat SidebarProvider adapter | Active-route highlighting + ModeToggle + InboxBadge wiring |
| `apps/desktop-shell/src/components/ThemeProvider.tsx` | Theme class toggle | Both light+dark class coexistence bug was here |
| `apps/desktop-shell/src/globals.css` | OkLCH tokens + @layer base + .markdown-content | All v3 visual identity. Editing wrong section breaks warm palette |
| `apps/desktop-shell/src/state/wiki-tab-store.ts` | Tab kind union + fixed tabs | Adding/removing kinds affects WikiContent switch + hydration |

## Medium (break feature-specific rendering)

| Path | Risk | Why |
|------|------|-----|
| `apps/desktop-shell/src/features/wiki/WikiArticle.tsx` | Markdown renderer + wiki-link interceptor | Only custom component is `a: Anchor`; adding more re-creates override drift |
| `apps/desktop-shell/src/features/wiki/WikiFileTree.tsx` | TreeNode action model | Dead-node bug was here; action type union prevents it |
| `apps/desktop-shell/src/features/wiki/wiki-link-utils.tsx` | Shared link renderer | Used by WikiArticle + WikiExplorer + WikiTab SpecialFilePage |
| `apps/desktop-shell/src/features/ask/Composer.tsx` | URL bypass removed (P0-1) | Re-adding fetch calls re-creates double ingest |
| `apps/desktop-shell/src/features/ask/useWikiQuery.ts` | SSE parser + chunk-boundary safety | query_done/error protocol. Regression = sources disappear |

## Low (styling, docs, tests)

| Path | Risk | Why |
|------|------|-----|
| `docs/design/*` | Design docs | Factual drift if not synced with code |
| `docs/desktop-shell/plans/*` / `specs/*` | Historical plans | OK to annotate, not to rewrite as current truth |
| `apps/desktop-shell/src/features/wiki/WikiExplorerPage.tsx` | Legacy quarantine block | Dead code — MUST NOT receive new features |

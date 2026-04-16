# Trigger Map

When a modification touches certain paths, these validation steps MUST
trigger. Agents MUST NOT skip them even if the change "looks small."

> Last verified: 2026-04-16

## Mandatory triggers

| Trigger condition | Required action |
|-------------------|-----------------|
| Any `.tsx` / `.ts` under `apps/desktop-shell/src/` | `cd apps/desktop-shell && npx tsc --noEmit` |
| Any `.rs` under `rust/crates/` | `cd rust && cargo check -p <affected-crate>` |
| Any `.rs` test file or test function added | `cd rust && cargo test -p <crate> --lib <test-name>` |
| `globals.css` modified | Visual smoke on at least 2 routes (light + dark) |
| `wiki-tab-store.ts` kind union changed | Verify WikiContent switch has matching case + file-tree action exists |
| `useAskSession.ts` modified | Verify: 5x route toggle creates 0 new sessions |
| `desktop-server/src/lib.rs` SSE handlers | Verify: query_done has sources, query_error has error, no silent done |
| `Composer.tsx` send path | Verify: no `ingestRawEntry` / `/api/wiki/fetch` / `/api/desktop/wechat-fetch` calls |
| `ThemeProvider.tsx` | Verify: `document.documentElement.className` is exactly one of `light` or `dark` |
| Any `memory/*.jsonl` | Must be strict JSONL + append-only. Run both: `node scripts/check-memory-jsonl.mjs` (format) and `node scripts/check-memory-append-only.mjs` (no deletions/edits) |
| Any `rules/*.md` | Record change in `memory/evolution-log.md` |

## Conditional triggers

| Trigger condition | Required action |
|-------------------|-----------------|
| New heading element (`<h1>`–`<h6>`) in any page | Check: no inline `fontSize` / `fontWeight`; must use Tailwind class + base rule |
| New `<ReactMarkdown>` usage | Check: wrapped in `.markdown-content` OR has `components={{ a: ... }}` for link safety |
| New file in `rust/crates/desktop-core/src/wechat_kefu/` | Check: KefuCapabilities::current() still matches handler behavior |
| Change to `docs/design/modules/04-wechat-kefu.md` | Check: §0 Implementation Snapshot matches code |

## Pre-commit checklist

Before asking the commander to commit:

1. `npx tsc --noEmit` — exit 0
2. `cargo check --workspace` — exit 0 (if Rust touched)
3. `git diff --check` — exit 0
4. `git diff --stat` — verify only intended files are modified
5. No unintended formatting changes in untouched files
6. No `.clawwiki/` / `__MACOSX/` / `.claude/` / `llm-wiki/` / `rust/.claw/` staged

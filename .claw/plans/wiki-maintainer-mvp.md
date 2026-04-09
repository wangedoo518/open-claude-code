# Plan: wiki_maintainer MVP (engram-style)

**Author:** Wang-Yeah623 / Claude
**Date:** 2026-04-09
**Parent:** `docs/clawwiki/product-design.md` §4 blade 3, §7 triggers, §11.2 new crate, §12 S4 sprint
**Depends on:** `feat(A): codex_broker::chat_completion` (already landed, commit `4cd823b`)

## 1. Goal

Land the smallest `wiki_maintainer` that ticks these boxes:

1. A new raw/ file triggers a proposal for a concept wiki page.
2. The proposal is produced by **one** `codex_broker::chat_completion` call (engram style — no multi-pass sage-wiki orchestration).
3. The proposal lands in the Inbox as a reviewable artifact.
4. Human approves → wiki page written to `~/.clawwiki/wiki/concepts/{slug}.md`.
5. Human rejects → nothing written, inbox task status flips to `rejected`.

This unblocks canonical §2's "AI 已经维护了 5 个页面" narrative and fulfills canonical §4 blade 3.

## 2. Non-goals (deferred)

- Multi-pass maintainer (5-pass sage-wiki style) — engram is explicitly the MVP choice per canonical §7.3 row 7.
- Conflict detection + `mark_conflict` — needs 2+ pages to reason across.
- `link_pages` bidirectional backlinks — needs a page graph first.
- `touch_changelog` + `rebuild_index` — post-MVP.
- Streaming proposal output — canonical §9.2 `chat_completion_streaming` is backlog.
- Background task watching raw/ for new files — MVP is trigger-on-click from Inbox.
- Per-tool-call approve/reject (MaintainerTaskTree with per-step diff) — inbox still approves at the entry level.

## 3. Architecture

```
┌───────────────┐   ┌───────────────┐   ┌──────────────────┐
│ InboxPage     │──▶│ POST /wiki/    │──▶│ wiki_maintainer  │
│ "Maintain     │   │ inbox/:id/     │   │ ::propose_for_   │
│  this" button │   │ propose        │   │  raw_entry       │
└───────────────┘   └───────┬───────┘   └────────┬─────────┘
                            │                    │
                            ▼                    ▼
                   Read raw entry         Build prompt +
                   from wiki_store        call broker
                                              │
                                              ▼
                                   MessageRequest → broker
                                              │
                                   MessageResponse (JSON body)
                                              │
                                              ▼
                                   Parse WikiPageProposal
                                              │
                      return JSON proposal to frontend
                            │
                            ▼
                   InboxPage shows proposal body
                   [Approve & write] / [Reject]
                            │
                            ▼
                   POST /wiki/inbox/:id/approve-with-write
                            │
                            ▼
                   wiki_store::write_wiki_page(proposal)
                   resolve_inbox_entry(id, "approve")
```

Key trait for testability:

```rust
// wiki_maintainer::broker_sender
#[async_trait]
pub trait BrokerSender: Send + Sync {
    async fn chat_completion(&self, req: MessageRequest)
        -> Result<MessageResponse, MaintainerError>;
}
```

`CodexBroker` implements it in an adapter crate (desktop-core). Tests use a `MockBrokerSender` with canned responses.

## 4. Tasks

### Batch 1 — Pure Rust foundation (NO LLM, dependency-injected broker)

**T1 — wiki_maintainer crate skeleton + types + prompt template**
- New crate `rust/crates/wiki_maintainer/`
  - `Cargo.toml` with deps: `api.workspace`, `async-trait`, `serde`, `serde_json.workspace`, `thiserror`, `wiki_store` path, `tokio` (dev)
  - `src/lib.rs`: `WikiPageProposal`, `MaintainerError`, `Result`, `BrokerSender` trait
  - `src/prompt.rs`: `build_concept_prompt(raw_entry, body) -> String` (system + user messages that instruct the LLM to return strict JSON)
- Add to workspace members
- TDD: 3 tests
  - `proposal_parse_valid_json`
  - `proposal_parse_rejects_missing_slug`
  - `build_concept_prompt_includes_raw_body_and_filename`

**T2 — wiki_store::write_wiki_page + list/read helpers**
- New module `rust/crates/wiki_store/src/wiki.rs` with:
  - `WikiPage { slug, title, body, frontmatter }` (re-use `RawFrontmatter` shape for now; later sprint adds `WikiFrontmatter`)
  - `write_wiki_page(paths, slug, title, body) -> Result<PathBuf>` writes to `wiki/concepts/{slug}.md`
  - `list_wiki_pages(paths) -> Result<Vec<WikiPageSummary>>`
  - `read_wiki_page(paths, slug) -> Result<(WikiPageSummary, String)>`
  - slug validation (RFC 3986 unreserved, same as `slugify` output)
- TDD: 4 tests
  - `write_wiki_page_creates_concepts_dir_and_file`
  - `write_wiki_page_is_idempotent_by_slug`
  - `list_wiki_pages_returns_empty_for_fresh_wiki`
  - `read_wiki_page_roundtrip`

**T3 — wiki_maintainer::propose_for_raw_entry (with MockBrokerSender)**
- `pub async fn propose_for_raw_entry<B: BrokerSender>(paths, raw_id, broker) -> Result<WikiPageProposal>`
  - reads the raw file via `wiki_store::read_raw_entry`
  - builds the concept prompt
  - calls `broker.chat_completion(request).await`
  - parses the response content as JSON → `WikiPageProposal`
- `MockBrokerSender` test harness inside `#[cfg(test)]`
- TDD: 3 tests
  - `propose_roundtrips_canned_json_response`
  - `propose_raises_on_missing_raw_entry`
  - `propose_raises_on_malformed_llm_response`

**Checkpoint: `cargo test -p wiki_maintainer -p wiki_store`, all green.**

### Batch 2 — HTTP + frontend wiring

**T4 — desktop-server routes**
- `POST /api/wiki/inbox/:id/propose` → calls `wiki_maintainer::propose_for_raw_entry` using the process-global broker adapter. Returns `{proposal: WikiPageProposal}`. On empty pool / broker error: 503 with an actionable message.
- `POST /api/wiki/pages` body `{slug, title, body}` → calls `write_wiki_page`
- `GET /api/wiki/pages` → list all wiki pages
- `GET /api/wiki/pages/:slug` → single page
- `POST /api/wiki/inbox/:id/approve-with-write` → body `{proposal}` — runs `write_wiki_page` then `resolve_inbox_entry(approve)` atomically

**T5 — CodexBroker adapter**
- In `desktop-core`, add `impl wiki_maintainer::BrokerSender for std::sync::Arc<CodexBroker>` so desktop-server can pass the process-global broker straight through.
- Alternatively: inline wrapper struct `BrokerAdapter(Arc<CodexBroker>)` if the trait can't be impl'd directly due to orphan rules. (Probably will need the wrapper.)

**T6 — Frontend Inbox detail upgrade**
- `features/ingest/types.ts` add `WikiPageProposal` interface
- `features/ingest/persist.ts` add `proposeForInboxEntry(id)`, `approveWithWrite(id, proposal)`, `listWikiPages()`, `getWikiPage(slug)`
- `features/inbox/InboxPage.tsx` detail pane: when selected entry is kind=`new-raw` and status=`pending`, render a "Maintain this" button. On click → call propose, show a collapsible preview of the proposed page (title + slug + body). Add "Approve & Write Wiki Page" and "Reject" buttons.

**Checkpoint: `tsc --noEmit` green, preview verifies the full click flow with the empty-pool 503 path.**

### Batch 3 — Verify, ship, close branch

**T7 — End-to-end verification**
- `cargo test --workspace`
- `RUSTFLAGS="-D warnings" cargo build -p desktop-core -p desktop-server`
- `tsc --noEmit` + `npm run build`
- preview: manual smoke-test
  - Ingest a raw entry via the existing paste-text form
  - Open Inbox, select the new task, click "Maintain this"
  - If broker is empty (expected in dev): verify error card mentions empty pool
  - If broker has a test token (via a manual CODEX_BASE_URL override): verify proposal renders and Approve writes the file
- `ls ~/.clawwiki/wiki/concepts/` shows the file if Approve was clicked

**T8 — Commit**
- Single commit: `feat(wiki-maintainer): engram-style MVP proposes concept pages`
- Commit message includes the `A.x`-style structured breakdown

**T9 — Finishing the branch**
- Invoke `superpowers:finishing-a-development-branch`
- Main path: push to origin/main (precedent of the prior 20 commits)

## 5. Risks

| Risk | Mitigation |
|---|---|
| `codex_broker` is empty in dev — no real LLM round-trip | MVP targets the "user clicks Maintain, sees a clear 503" flow. Real round-trip verified only when a token lands in the pool (separate concern). |
| LLM returns non-JSON → parse fails | Pin the prompt to JSON-only output; `MaintainerError::BadJson(_)` surfaces to frontend as a readable error. Test covers malformed responses. |
| Slug collision with an existing concept page | `write_wiki_page` is idempotent by slug (overwrites); a future sprint adds an "already exists" confirm. For MVP the behavior is "overwrite with latest proposal". |
| Inbox `approve-with-write` fails halfway (write ok, status update fails) | Write first, then resolve. Worst case: page on disk, task still pending. Next click retries the resolve. Not catastrophic. |
| `BrokerSender` trait orphan rule on `Arc<CodexBroker>` | Wrap in `BrokerAdapter` struct in desktop-core if needed. |

## 6. Non-MVP polish to defer

- MaintainerTaskTree visualization of tool calls
- Streaming proposal output
- Proposal diff view (new vs existing page)
- Background task watching raw/ for new files
- Automatic retry with exponential backoff on transient broker errors
- Prompt caching for the per-raw body
- sha256 tracker in `.clawwiki/manifest.json`

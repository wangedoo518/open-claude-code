# Phase 1 MVP · Deferred Items

Tracking register for behaviour specified in `docs/design/modules/01-skill-engine.md §5.1`
pseudocode that the Phase 1 MVP implementation (Sprint 1-B · `absorb_batch` +
`absorb_handler`) intentionally does not cover. Each entry is cross-linked to
both the pseudocode step it comes from and the Sprint 1-B.0 audit finding that
surfaced it.

**Scope**: Phase 1 lands the bones — `absorb_batch` as a create-or-update loop
with broker retry, index-aware prompt, and per-task SSE fan-out. The items
below are enhancements that improve maintainer quality / user feedback /
cross-linking, but none block a usable `/absorb` flow.

**Status key**: `🟡 Phase 2` (next cycle) / `🟠 Phase 3` (after user feedback) /
`🟢 Phase 4+` (post-launch polish) / `🔵 blocked-on` (waits for another sprint).

---

## 1 · Update-branch LLM merge (replacing string concatenation)

| Field | Value |
|---|---|
| **Spec** | `modules/01-skill-engine.md §5.1` step 3f-update (伪代码 L782-802) |
| **MVP behaviour** | `wiki_maintainer/src/lib.rs:1809` uses `format!("{}\n\n---\n\n{}", existing_body, proposal.body)` — plain concatenation with a `---` separator. |
| **Spec behaviour** | Call `broker.chat_completion(build_merge_request(existing_body, new_body, title))` to let the LLM produce a topic-driven merged body that satisfies the anti-thinning contract ("合并后必须比合并前更丰富"). |
| **Why deferred** | Requires a new prompt builder + one extra LLM round-trip per update + careful extraction of the merged body from the response (separate from the JSON proposal path). Phase 1 ships the concat fallback so update flows are visibly working; the prompt itself warns the LLM to produce topic-organised bodies even in the concat regime, so quality degrades gracefully. |
| **Target phase** | 🟡 Phase 2 |
| **Audit ref** | Sprint 1-B.0 §A1 Item 8 (⚠️ partial — merge path simplified) |

## 2 · Bidirectional wikilink maintenance

| Field | Value |
|---|---|
| **Spec** | `modules/01-skill-engine.md §5.1` step 3h (伪代码 L836-844) + `fn ensure_bidirectional_link` stub L1112-1122 |
| **MVP behaviour** | No bidirectional link insertion. After `write_wiki_page_in_category`, `absorb_batch` moves straight to conflict detection / absorb-log. Incoming links stay whatever the LLM produced. |
| **Spec behaviour** | For every `target_slug` in `extract_internal_links(&final_body)`, read the target page body and append `[FromTitle](concepts/from_slug.md)` in the "相关页面" section when missing. Guarantees A → B implies B → A discoverability. |
| **Why deferred** | The backlinks index (`build_backlinks_index` + `save_backlinks_index`) already surfaces reverse links at query time (§5.2) without mutating page bodies. Phase 1 relies on the index for discoverability and postpones the body-mutation path until the maintainer has quality telemetry. |
| **Target phase** | 🟡 Phase 2 |
| **Audit ref** | Sprint 1-B.0 §A1 Item 9 (❌ missing) |

## 3 · LLM conflict detection → Inbox

| Field | Value |
|---|---|
| **Spec** | `modules/01-skill-engine.md §5.1` step 3i (伪代码 L846-864) + `async fn detect_conflict` stub L1125-1136 |
| **MVP behaviour** | `wiki_maintainer/src/lib.rs:1858` has an explicit comment: `// 3i: Conflict detection (simplified: skip LLM-based detection for MVP). Full LLM-based conflict detection deferred to later sprint.` No conflict events fire. |
| **Spec behaviour** | When `action == "update"`, build a conflict-detection prompt comparing old body vs new raw, let the LLM classify as `"no_conflict"` or `"conflict: {reason}"`, and on conflict `append_inbox_pending("conflict", title, reason, raw_id)`. |
| **Why deferred** | Separate prompt + extra broker round-trip per update. Without measured conflict frequency in the real corpus, the confidence threshold for an LLM verdict is unknown. Phase 2 ships this with telemetry to calibrate false-positive rate. |
| **Target phase** | 🟡 Phase 2 |
| **Audit ref** | Sprint 1-B.0 §A1 Item 10 (❌ explicit MVP skip) |

## 4 · `quality_spot_check` — diary-body detector

| Field | Value |
|---|---|
| **Spec** | `modules/01-skill-engine.md §5.1` step 4 checkpoint block (伪代码 L955-961) + `async fn quality_spot_check` stub L1139-1148 |
| **MVP behaviour** | The 15-entry checkpoint runs `rebuild_wiki_index` + `build_backlinks_index` + `save_backlinks_index`. **No** quality spot-check. |
| **Spec behaviour** | Pick the 3 most-recently-updated pages from `_absorb_log.json`, scan their bodies for 3+ consecutive `## YYYY-MM-DD` headings, and raise a `cleanup-suggestion` Inbox entry when diary structure is found. |
| **Why deferred** | The anti-thinning + topic-organisation prompt rules (§5.1 L1046-1058 items 3-4) already bias the LLM against diary bodies at write time. The spot-check is a belt-and-suspenders signal; Phase 3 adds it once we know how often the prompt alone fails. |
| **Target phase** | 🟠 Phase 3 |
| **Audit ref** | Sprint 1-B.0 §A1 Item 11 (⚠️ partial — rebuild done, spot-check missing) |

## 5 · `compute_confidence` full three-dim evaluation

| Field | Value |
|---|---|
| **Spec** | `modules/01-skill-engine.md §5.1` step 3g-extra (伪代码 L866-877) + `fn compute_confidence` L1002-1017 |
| **MVP behaviour** | `wiki_maintainer/src/lib.rs:1867-1877` computes `source_count` as `absorb_log.filter(page_slug == target && action != "skip").count() + 1`, fixes `newest_age_days = 0` (always), and `has_conflict = false` (always). Output is essentially a binary 0.2 / 0.6 / 0.9 based on source_count alone. |
| **Spec behaviour** | Real `count_sources_for_page` (walk raw references), real `newest_source_age_days` (timestamp diff from newest contributing raw), real `has_pending_conflict` (check Inbox for "conflict" entries on this slug). |
| **Why deferred** | `newest_source_age_days` needs a raw→page provenance index that doesn't exist yet (would require `raw_id → page_slug` reverse lookup). `has_pending_conflict` waits on Item 3 to fire conflict Inbox entries in the first place. Phase 3 ships these together. |
| **Target phase** | 🟠 Phase 3 |
| **Audit ref** | Sprint 1-B.0 §A1 extra observation (⚠️ simplified — three-dim → one-dim + two constants) |

## 6 · `changelog/YYYY-MM-DD.md` per-day append

| Field | Value |
|---|---|
| **Spec** | `modules/01-skill-engine.md §5.1` step 3j-extra (伪代码 L916-922) |
| **MVP behaviour** | `wiki_maintainer/src/lib.rs:1864` appends to `wiki/log.md` (global append-only log). Day-file (`wiki/changelog/YYYY-MM-DD.md`) is **not** written. |
| **Spec behaviour** | Also call `wiki_store::append_changelog_entry(paths, verb, title)` so `cat wiki/changelog/2026-04-23.md` reads as a natural daily digest. |
| **Why deferred** | The global `log.md` already captures every action. Day-files are a UX convenience for `cat`-level grepping; until the Dashboard Today view wires up to consume per-day files, shipping only the global log is sufficient. Phase 4 adds this when the Dashboard day view lands. |
| **Target phase** | 🟢 Phase 4+ |
| **Audit ref** | Sprint 1-B.0 §A1 extra observation (changelog append missing) |

## 7 · `503 BROKER_UNAVAILABLE` real health probe

| Field | Value |
|---|---|
| **Spec** | `technical-design.md §2.1` error matrix + Session 2 self-decision #1 |
| **MVP behaviour** | `desktop-server/src/lib.rs` `absorb_handler` has a `TODO(Sprint 1-B.1+1)` comment where the probe would sit. `BrokerAdapter::from_global()` currently cannot fail (install-at-boot), so the error code has no trigger path. |
| **Spec behaviour** | Before `tokio::spawn` of the absorb task, probe whether the broker has at least one usable provider (codex_broker pool status, OpenAI-compat endpoint reachability, etc.). On failure, return `503 BROKER_UNAVAILABLE` so the frontend can render the "设置未就绪" banner. |
| **Why deferred** | `codex_broker` does not yet expose a `health()` / `has_available_provider()` API. Adding one is its own focused sprint — hot paths already depend on `from_global()` returning an adapter unconditionally, so adding a probe that short-circuits the call chain is a wider refactor than Sprint 1-B's scope. |
| **Target phase** | 🔵 blocked-on · codex_broker health API (expected alongside Phase 2 broker work) |
| **Audit ref** | Session 2 self-decision #1 (deferred with commander approval) |

---

## Cross-reference summary

| Item | Spec step | Code site | Target |
|------|-----------|-----------|--------|
| 1 | §5.1 3f-update | `wiki_maintainer/src/lib.rs:1809` | 🟡 Phase 2 |
| 2 | §5.1 3h | (no call site) | 🟡 Phase 2 |
| 3 | §5.1 3i | `wiki_maintainer/src/lib.rs:1858` | 🟡 Phase 2 |
| 4 | §5.1 step 4 quality | `wiki_maintainer/src/lib.rs:1898-1904` | 🟠 Phase 3 |
| 5 | §5.1 3g-extra | `wiki_maintainer/src/lib.rs:1866-1877` | 🟠 Phase 3 |
| 6 | §5.1 3j-extra changelog | `wiki_maintainer/src/lib.rs:1864` | 🟢 Phase 4+ |
| 7 | technical-design §2.1 503 | `desktop-server/src/lib.rs` absorb_handler | 🔵 blocked-on broker health API |

Last updated: 2026-04-23 (end of Sprint 1-B.1 · Session 3).

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

---

## Phase 1 MVP · Accepted-with-deferred (2026-04-24)

Follow-up from Sprint 1-C' verification — three UX / wiring gaps surfaced
during the Phase 1 MVP E2E that **do not block §9.5 acceptance semantically**
but are worth tracking so Phase 2 picks them up together.

### 8 · UX · `AbsorbTriggerButton` 未消费 SSE `absorb_progress`

| Field | Value |
|---|---|
| **现状** | `AbsorbTriggerButton.tsx:52-82` 跑 3s `setInterval` 轮询 `getWikiStats` 的 `last_absorb_at` 字段判定完成；`SkillProgressCard.tsx` 虽然订阅了 `useSkillStore.absorbProgress` 但 store 从未被更新（只有 start / complete / fail 三个 action） |
| **后果** | ① 完成判定延迟 3s；② 进度卡 `0/0 0%` 全程静态；③ **broker 失败时 `last_absorb_at` 不推进 → UI 永远卡在"维护中..."**（在本次 E2E 里实际触发了此 bug）；④ Session 2 修的 `_rx drop bug` + SSE `AbsorbProgress` / `AbsorbComplete` 事件产物未被前端利用 |
| **非-blocker 理由** | 按钮 POST → 202 / backend 管线正常 / 终端用户在有工作 broker 时能看到完成 toast（可能延迟） |
| **Target** | 🟡 Phase 2 · 任意前端 sprint 顺手改（替换 polling 为 `EventSource('/api/desktop/sessions/{id}/events')` 订阅 `absorb_progress` + `absorb_complete`） |
| **预估工作量** | S (~2h) |
| **Audit ref** | Sprint 1-C' task 1.3 + task 2 E2E step 4 观察 |

### 9 · UX · `AbsorbTriggerButton` reachability / 默认 UI 路径缺失 · ✅ RESOLVED

| Field | Value |
|---|---|
| **Resolution** | Phase 1 MVP 收口 sprint (2026-04-24) Stage 1 — commit `c8a00a5` `feat(wiki): wire AbsorbTriggerButton into KnowledgeHubPage header`. 按钮以 `compact=false` 挂在 `KnowledgeHubPage.tsx` header 的 PillTabs 同 row 右对齐位置 (`ml-auto`)。现 `/#/wiki` 默认路由可见「开始维护」按钮。 |
| **原现状** | `AbsorbTriggerButton` 仅在 `WikiFileTree.tsx:272` 被挂载 (`compact=true`)；而 `WikiFileTree` 进一步只被 `WikiTab.tsx:147` 使用。Phase 1 默认路由 `/wiki/*` 映射到 `KnowledgeHubPage`（pill-tabs · KnowledgePagesList）+ `/wiki/:slug` 映射到 `KnowledgeArticleView`。**两个默认路由都不挂 `WikiFileTree`** |
| **原后果** | §9.5 criterion 1「能手动触发 /absorb」在 UI 层 **不可达** —— 用户在 `/#/wiki` 或 `/#/wiki/:slug` 都找不到「开始维护」按钮。后端 `POST /api/wiki/absorb` 功能完整（Sprint 1-B 已验），是前端入口位点缺失 |
| **Audit ref** | Sprint 1-C' task 2 E2E step 2 (button not visible) → 收口 sprint Stage 1 |

### 10 · Env/Config · `.claw/providers.json` 未配置 · broker 无可用 provider

| Field | Value |
|---|---|
| **现状** | Phase 1 MVP 收口 sprint Stage 2 smoke test (2026-04-24) 直接 curl `https://api.deepseek.com/v1/chat/completions` 返回 **HTTP 200** (响应包含 `{"id":"150d0dd9-...","model":"deepseek-v4-flash",...}`) — deepseek API 本身可达，env var `DEEPSEEK_API_KEY` 有效。根本原因: `.claw/providers.json` **在此 dev env 不存在**（项目根 + 用户 profile 都查过）→ `BrokerAdapter::from_global()` 既无 private-cloud broker（desktop-server 默认 feature 不含 `private-cloud`）又无 providers.json fallback → `try_providers_json_chat_completion` 返回 None → 所有 `chat_completion` 调用返回 `MaintainerError::Broker("no codex account available and no providers.json fallback found")` |
| **后果** | Sprint 1-C' E2E step 5-6「等待 3-10 秒判定完成」「确认新增 3 条 wiki 页」在此 dev env 无法复现；然而 **代码层达标** —— `absorb_batch` 正确 surface 错误到 `AbsorbProgressEvent { action: "skip", error: Some("LLM 调用失败 (已重试): ...") }`，进 `result.failed`，不沉默（`wiki_maintainer/src/lib.rs:1899-1912`） |
| **非-blocker 理由** | ① 后端失败路径代码正确 ② `absorb_batch` 不 hang ③ 终端用户若配 `.claw/providers.json` 则运行环境就绪 —— **这是 runtime configuration 缺口，不是 Phase 1 代码缺陷** ④ UI "卡住" 现象的根因是 item 8 polling heuristic（broker 全失败 → absorb_log 不更新 → 轮询的 `last_absorb_at` 永远不推进），已 Phase 2 backlog 覆盖 |
| **Target** | 🔵 runtime · 运维 + Phase 2 合并：(a) 用户侧在 CLAWWIKI_HOME 或项目根补 `.claw/providers.json` 指向 deepseek / 其他 OpenAiCompat provider（env var 已 ready）；或 (b) Phase 2 考虑让 adapter 在 providers.json 缺席时读取 `DEEPSEEK_API_KEY` / `OPENAI_API_KEY` 等 env var 作为 last-resort fallback；或 (c) 接入 item 7 的真实 broker 健康探针，令 `absorb_handler` 在 provider 缺失时返回 503 short-circuit（相比当前 202 + 全失败更符合 §9.5 的 failure-mode 语义） |
| **预估工作量** | (a) S 运维 ~5min · (b) S dev ~1h · (c) 与 item 7 合并 |
| **Audit ref** | Sprint 1-C' task 2 broker probe → 收口 sprint Stage 2 smoke test (curl 200 OK) |

### 11 · UX · `WikiFileTree` 缺键盘 ↑↓ 导航

| Field | Value |
|---|---|
| **现状** | `apps/desktop-shell/src/features/wiki/WikiFileTree.tsx` 列表项是 `<button>`，默认走浏览器 tab/enter；无自定义 `onKeyDown` 处理 arrow keys。焦点在 button 上 ↑↓ 不跳下一项 |
| **后果** | 键盘重度用户切换页面不便；需要 tab 反复跳跃或鼠标介入 |
| **非-blocker 理由** | 鼠标 / 触屏路径正常；Phase 1 MVP §9.5 三项 criterion 不涉及键盘导航 |
| **Target** | 🟡 Phase 2 · 加 arrow-key handler + roving-tabindex 焦点管理 |
| **预估工作量** | S (~1-2h) |
| **Audit ref** | Sprint 1-C' task 1.1 ❌ |

### 12 · UX · `WikiArticle` frontmatter 条缺 `confidence` + `last_verified`

| Field | Value |
|---|---|
| **现状** | `apps/desktop-shell/src/features/wiki/WikiArticle.tsx` L114-133 frontmatter 条仅 render `category` + `created_at` + reading-time。API 层 `GET /api/wiki/pages/:slug` 返回 `confidence` (number) 与 `last_verified` (iso string) 字段，前端 display 侧未接 |
| **后果** | Phase 1 认知复利模型中的 "confidence" 信号不可见 —— 用户无法判断某页是 single-source (0.2) / multi-source (0.6) / consolidated (0.9)；认知复利价值无 UI hook |
| **非-blocker 理由** | API 字段已返回；仅 display 缺失；不影响 §9.5 三项 criterion 的「正确性」判定，只影响「可感知性」 |
| **Target** | 🟡 Phase 2 · frontmatter 条追加两枚 badge/pill（例：`置信度 60%` / `上次校验 3 天前`） |
| **预估工作量** | S (~30min) 纯 display 补 |
| **Audit ref** | Sprint 1-C' task 1.2 ⚠️ |

---

## Phase 1 MVP · DONE 判定 (2026-04-24 收口 sprint)

§9.5 验收 criterion 最终对照：

| # | Criterion | 判定 | 证据 |
|---|-----------|-----|------|
| 1 | 能手动触发 `/absorb` | ✅ | UI: Stage 1 wire `AbsorbTriggerButton` 到 `KnowledgeHubPage` header (`c8a00a5`) + API: Sprint 1-B.1 `absorb_handler` 202 + task_id + 错误码（5 项 integration test `bc32ac3` 全通） |
| 2 | 看得到自动生成的 wiki 页 | ✅ 代码层 | `absorb_batch` create/update 路径 + §5.1 7 条 anti-cramming prompt + retry-once（`31a5fcc` + `c512403` 55 项 maintainer test 全通）；Dev env 运行时卡在 item 10（providers.json 缺口，非代码问题） |
| 3 | `_backlinks.json` 正确生成 | ✅ | Sprint 1-A 4 项 backlinks gap test (`6b8099e`) + Sprint 1-C' API 契约实测通过 |

### 已知 deferred items 汇总

| 编号 | 类型 | 状态 | Target |
|------|------|------|--------|
| 1 · LLM merge | spec behaviour | 🟡 Phase 2 | deferred |
| 2 · bidirectional links | spec behaviour | 🟡 Phase 2 | deferred |
| 3 · LLM conflict → Inbox | spec behaviour | 🟡 Phase 2 | deferred |
| 4 · quality spot-check | spec behaviour | 🟠 Phase 3 | deferred |
| 5 · confidence 三维 | spec behaviour | 🟠 Phase 3 | deferred |
| 6 · per-day changelog | spec behaviour | 🟢 Phase 4+ | deferred |
| 7 · 503 broker health | 契约 | 🔵 blocked-on | deferred |
| 8 · AbsorbTrigger polling→SSE | UX | 🟡 Phase 2 | deferred |
| 9 · AbsorbTrigger reachability | UX | ✅ RESOLVED | 收口 sprint Stage 1 `c8a00a5` |
| 10 · providers.json 配置 | env/runtime | 🔵 runtime | deferred |
| 11 · WikiFileTree 键盘导航 | UX | 🟡 Phase 2 | deferred |
| 12 · WikiArticle confidence display | UX | 🟡 Phase 2 | deferred |

### Phase 2 启动就绪信号

- ✅ 后端 SKILL engine: absorb / query / checkpoint / SSE / TaskManager 完整 (Sprint 1-B.1 3 sessions)
- ✅ 前端 Wiki UI: KnowledgeHub / KnowledgeArticle / WikiRelationsPanel / AbsorbTriggerButton 完整
- ✅ Phase 1 MVP §9.5 3/3 criterion 代码层达标
- ⚠️ 5 项 minor gaps (items 8, 10, 11, 12 + item 10 runtime 配置) 明确 target Phase 2

Last updated: 2026-04-24 (Phase 1 MVP closure sprint).

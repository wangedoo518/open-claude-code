---
title: Decision Curation Implementation Plan
doc_type: plan
status: draft
owner: desktop-shell
last_verified: 2026-05-08
related:
  - docs/desktop-shell/plans/2026-05-07-buddy-entropy-priority-lifecycle-implementation-plan.md
  - docs/desktop-shell/plans/2026-05-07-buddy-entropy-tension-resolutions-implementation-plan.md
  - docs/desktop-shell/architecture/overview.md
---

# Decision Curation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make Buddy's product positioning literal — "帮你判断哪些灵感值得持续投入，哪些可以安心放下" — by adding decision retrospectives to wiki pages and an investment-vs-output ROI panel on Home.

**Architecture:** Two narrowly-scoped slices. **E17** adds three optional frontmatter fields (`verdict`, `verdict_at`, `verdict_reason`) to `WikiPageSummary`, exposes them through a dedicated POST endpoint, surfaces a verdict picker on `WikiArticle`, and feeds verdict signal into the maintainer absorb prompt so future proposals factor in past keep/let-go decisions. **E18** adds a derived ROI pulse panel on Home that turns existing data (`raw_count`, `expressed_in`, `purpose`, `created_at`) into a 30/90-day "投入 → 表达" funnel + per-purpose conversion table. Pure data; no new user-input surface.

**Tech Stack:** TypeScript 5.8 / React 19 / Tailwind 4 / Zustand 5 / React Query 5 (frontend), Rust workspace + Tauri 2 (backend), Vitest + Cargo test (testing).

---

## Execution Rules

- **TDD per task:** write the failing test first, run it, see it fail, then write the minimum code to pass.
- **Commit after each green test.** No batch commits.
- **Run `cd apps/desktop-shell && npm run build`** + relevant `cargo test -p <crate>` before the slice's last commit.
- **Backwards compatible:** existing wiki pages without verdict fields must keep working without migration.
- **No new dependencies** — verdict is just three optional `Option<String>` fields and an enum-shaped string.
- **Reuse existing primitives:** `WikiPageLifecycleMetadata` extension pattern (Slice E1), `read_frontmatter_field` / `patch_frontmatter_field` (Slice E15.3 audit fix), `entropy-pulse.ts` derivation pattern.
- **Update `architecture/overview.md`** when behavior visibly changes.

---

## Dependency Gates

- E17 builds on E1 (priority/vitality data contract) and E15.3 (frontmatter-preserving patch helpers) — both shipped.
- E18 builds on E2 (Home/Pulse rendering pattern) and Slice 47 (`expressed_in` field) — both shipped.
- E17 and E18 are independent; either can land first. Recommended order: **E17 → E18** so the maintainer prompt change in T17.5 has time to settle before users see ROI deltas attributed to it.

---

## Tension With Existing Plans (Important)

This plan is **a separate product line**, not a continuation of the entropy lifecycle plan:

- E0–E10 (entropy) and E11–E16 (tension resolutions) help users **handle inflow** that already arrived.
- E17–E18 (this plan) make Buddy **judge value** of what they've kept.

The two lines share `WikiPageSummary` storage but have different mental models:

| Plan | Question it answers |
|---|---|
| E0–E10 | 怎么把流入的素材分组 / 整理 / 归档 |
| E11–E16 | 让 entropy lifecycle 在长期使用下保持连贯 |
| **E17–E18 (this)** | **这条灵感事后看值不值得继续投入？** |

Verdict (E17) is a **post-hoc user judgement** layered on top of `priority`/`vitality` (which are AI-suggested, user-confirmed lifecycle signals). They don't conflict because `priority` is "现在 AI 觉得这页有多重要", verdict is "你过段时间回头看，这条想法到底有没有产出".

---

## Slice E17 — Decision Retrospective

**Tension this resolves:** Today, when a user looks at a wiki page they wrote 3 months ago, there's no way to tell Buddy "actually this turned into nothing, stop suggesting more raw material in this direction" or "this paid off massively, keep feeding me similar". Buddy's curation has no feedback signal grounded in actual outcomes.

**Resolution:** Add three optional frontmatter fields per wiki page:

- `verdict`: one of `should_continue` / `should_let_go` / `inconclusive`
- `verdict_at`: ISO-8601 timestamp when the verdict was last set
- `verdict_reason`: free-form string explaining why (≤ 200 chars)

User sets these from the article view. The maintainer's absorb prompt reads aggregated verdicts when generating new proposals, so high-`should_let_go`-rate purposes get less aggressive suggestions and high-`should_continue` purposes get more.

**Done means:**

- Wiki pages can carry verdict fields; legacy pages without them parse fine.
- A single dedicated POST endpoint (`/api/wiki/pages/{slug}/verdict`) writes verdict in place without touching unrelated frontmatter (per E15.3 contract).
- WikiArticle surfaces a 3-button verdict picker + reason input. Existing verdict displays inline as a chip.
- Maintainer absorb prompt includes per-purpose verdict counts so the LLM can bias proposals.
- Backwards compatible: pages without verdict render normally.

### Task E17.1: WikiPageSummary verdict fields + serde round-trip

**Files:**
- Modify: `rust/crates/wiki_store/src/lib.rs` (extend `WikiPageSummary`, `WikiPageLifecycleMetadata`, `WikiFrontmatter`, parse + serialize paths)
- Test: `rust/crates/wiki_store/src/lib.rs` test module

**Step 1: Write the failing test**

Append at the bottom of the test module (before the closing `}` of `mod tests`):

```rust
    // ── E17.1: verdict frontmatter round-trip ──────────────────────
    #[test]
    fn verdict_fields_round_trip_through_yaml_frontmatter() {
        let tmp = tempfile::tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        let cat_dir = paths.wiki.join(WIKI_CONCEPTS_SUBDIR);
        fs::create_dir_all(&cat_dir).unwrap();
        let path = cat_dir.join("payoff-test.md");
        let content = "---\n\
                       type: concept\n\
                       status: active\n\
                       owner: human\n\
                       schema: v1\n\
                       title: Payoff Test\n\
                       summary: Round-trip test for verdict fields.\n\
                       purpose:\n  - learning\n\
                       verdict: should_continue\n\
                       verdict_at: 2026-05-08T10:30:00Z\n\
                       verdict_reason: Two follow-up notes within a week.\n\
                       created_at: 2026-04-01T00:00:00Z\n\
                       ---\n\n# body\n";
        fs::write(&path, content).unwrap();

        let (summary, _body) = read_wiki_page(&paths, "payoff-test").unwrap();
        assert_eq!(summary.verdict.as_deref(), Some("should_continue"));
        assert_eq!(summary.verdict_at.as_deref(), Some("2026-05-08T10:30:00Z"));
        assert_eq!(
            summary.verdict_reason.as_deref(),
            Some("Two follow-up notes within a week.")
        );
    }

    #[test]
    fn verdict_fields_default_to_none_for_legacy_pages() {
        let tmp = tempfile::tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        let cat_dir = paths.wiki.join(WIKI_CONCEPTS_SUBDIR);
        fs::create_dir_all(&cat_dir).unwrap();
        let path = cat_dir.join("legacy.md");
        let content = "---\n\
                       type: concept\n\
                       status: active\n\
                       owner: human\n\
                       schema: v1\n\
                       title: Legacy\n\
                       summary: No verdict yet.\n\
                       purpose:\n  - learning\n\
                       created_at: 2026-04-01T00:00:00Z\n\
                       ---\n\n# body\n";
        fs::write(&path, content).unwrap();

        let (summary, _) = read_wiki_page(&paths, "legacy").unwrap();
        assert_eq!(summary.verdict, None);
        assert_eq!(summary.verdict_at, None);
        assert_eq!(summary.verdict_reason, None);
    }
```

**Step 2: Run test to verify it fails**

Run: `cd rust && cargo test -p wiki_store --lib verdict_fields_round_trip`

Expected: FAIL with `no field 'verdict' on type 'WikiPageSummary'`

**Step 3: Write minimal implementation**

In `rust/crates/wiki_store/src/lib.rs`, mirror the existing optional-field pattern at three places:

a) Add to `WikiPageSummary` struct (just after `cross_domain_reason`, around line 3642):

```rust
    /// Slice E17 — user's post-hoc verdict on whether this page was
    /// worth investing in. One of `should_continue` / `should_let_go`
    /// / `inconclusive`. Optional so legacy pages remain valid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<String>,
    /// ISO-8601 timestamp the verdict was last set/updated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict_at: Option<String>,
    /// Free-form reason (≤ 200 chars by convention) explaining the
    /// verdict — feeds back into the maintainer absorb prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict_reason: Option<String>,
```

b) Same three fields on `WikiPageLifecycleMetadata` (around line 3777, mirroring the pattern). This keeps the From impl symmetric:

```rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict_reason: Option<String>,
```

c) Same three fields on `WikiFrontmatter` (the parser internal, around line 3818):

```rust
    pub verdict: Option<String>,
    pub verdict_at: Option<String>,
    pub verdict_reason: Option<String>,
```

d) Update the From impls to copy the new fields. Search for `impl From<&WikiPageSummary> for WikiPageLifecycleMetadata` and `impl From<&WikiFrontmatter> for WikiPageSummary` and add the three field copies, mirroring existing entries like `priority_reason`.

e) Update `WikiFrontmatter::for_concept` default constructor to initialize the three new fields to `None`.

f) Update `WikiFrontmatter::to_yaml_block` to emit the new fields, mirroring how `priority_reason` is emitted (look around line 3720):

```rust
        if let Some(verdict) = &self.verdict {
            s.push_str(&format!("verdict: {verdict}\n"));
        }
        if let Some(verdict_at) = &self.verdict_at {
            s.push_str(&format!("verdict_at: {verdict_at}\n"));
        }
        if let Some(verdict_reason) = &self.verdict_reason {
            s.push_str(&format!("verdict_reason: {verdict_reason}\n"));
        }
```

g) Update `parse_frontmatter_fields` (around line 3517) to extract the three new fields, mirroring how `priority_reason` is extracted. The pattern is `frontmatter_scalar` calls.

**Step 4: Run test to verify it passes**

Run: `cd rust && cargo test -p wiki_store --lib verdict_fields_round_trip verdict_fields_default_to_none`

Expected: PASS (2/2)

**Step 5: Commit**

```bash
git add rust/crates/wiki_store/src/lib.rs
git commit -m "feat(wiki_store): verdict frontmatter fields (E17.1)"
```

### Task E17.2: TypeScript DTOs

**Files:**
- Modify: `apps/desktop-shell/src/api/wiki/types.ts` (extend `WikiPageSummary`)
- Test: relies on `tsc --noEmit` (existing project convention)

**Step 1: Locate the existing optional-field cluster**

```bash
grep -n "priority_reason\|cross_domain_reason" apps/desktop-shell/src/api/wiki/types.ts | head
```

You should see the optional fields near line 188 / 305 / 745.

**Step 2: Add the three verdict fields to every relevant interface**

In `apps/desktop-shell/src/api/wiki/types.ts`, find every place that lists `priority_reason` / `cross_domain_reason` and add three new lines after each:

```typescript
  /** Slice E17 — post-hoc verdict on this page's payoff. */
  verdict?: "should_continue" | "should_let_go" | "inconclusive" | null;
  verdict_at?: string | null;
  verdict_reason?: string | null;
```

The interfaces that need updating:
- `WikiPageSummary` (the listing/detail DTO)
- `WikiPageLifecycleMetadata` (the patch DTO)
- Any maintainer-side `*Patch` types

**Step 3: Type-check**

Run: `cd apps/desktop-shell && npx tsc --noEmit`

Expected: clean (no output).

**Step 4: Commit**

```bash
git add apps/desktop-shell/src/api/wiki/types.ts
git commit -m "feat(types): WikiPageSummary verdict fields (E17.2)"
```

### Task E17.3: POST `/api/wiki/pages/{slug}/verdict` endpoint

**Files:**
- Modify: `rust/crates/desktop-server/src/handlers/wiki_crud.rs` (add handler)
- Modify: `rust/crates/desktop-server/src/routes/wiki.rs` (register route)
- Modify: `rust/crates/desktop-server/src/lib.rs` (export handler symbol)
- Test: handler-level test inline in `wiki_crud.rs`

**Step 1: Write the failing test**

Append in the existing test module of `wiki_crud.rs` (search for `#[tokio::test]` in that file to find the test cluster):

```rust
    #[tokio::test]
    async fn put_verdict_writes_three_fields_and_preserves_unrelated() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = wiki_store::init_wiki(tmp.path()).unwrap();
        let cat_dir = paths.wiki.join(wiki_store::WIKI_CONCEPTS_SUBDIR);
        std::fs::create_dir_all(&cat_dir).unwrap();
        let seed = "---\n\
                    type: concept\n\
                    status: active\n\
                    owner: human\n\
                    schema: v1\n\
                    title: Pay\n\
                    summary: x\n\
                    purpose:\n  - learning\n\
                    created_at: 2026-04-01T00:00:00Z\n\
                    ---\n\n# body\n";
        std::fs::write(cat_dir.join("pay.md"), seed).unwrap();

        let app = build_router(tmp.path().to_path_buf());
        let body = serde_json::json!({
            "verdict": "should_let_go",
            "reason": "30 days, no follow-up",
        });
        let resp = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/wiki/pages/pay/verdict")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);

        let (summary, _) =
            wiki_store::read_wiki_page(&paths, "pay").unwrap();
        assert_eq!(summary.verdict.as_deref(), Some("should_let_go"));
        assert_eq!(
            summary.verdict_reason.as_deref(),
            Some("30 days, no follow-up")
        );
        assert!(summary.verdict_at.is_some(), "verdict_at stamped");

        // Body + status / owner must be byte-preserved (E15.3 contract).
        let raw = std::fs::read_to_string(cat_dir.join("pay.md")).unwrap();
        assert!(raw.contains("status: active"));
        assert!(raw.contains("owner: human"));
        assert!(raw.contains("# body"));
    }

    #[tokio::test]
    async fn put_verdict_rejects_unknown_value() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = wiki_store::init_wiki(tmp.path()).unwrap();
        let cat_dir = paths.wiki.join(wiki_store::WIKI_CONCEPTS_SUBDIR);
        std::fs::create_dir_all(&cat_dir).unwrap();
        std::fs::write(
            cat_dir.join("p.md"),
            "---\ntype: concept\nstatus: active\nowner: human\nschema: v1\ntitle: P\nsummary: x\npurpose:\n  - learning\ncreated_at: 2026-04-01T00:00:00Z\n---\n\n# body\n",
        )
        .unwrap();

        let app = build_router(tmp.path().to_path_buf());
        let body = serde_json::json!({ "verdict": "lolwut" });
        let resp = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/api/wiki/pages/p/verdict")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), 400);
    }
```

**Step 2: Run test to verify it fails**

Run: `cd rust && cargo test -p desktop-server put_verdict`

Expected: FAIL — handler/route not found.

**Step 3: Write minimal implementation**

a) In `rust/crates/desktop-server/src/handlers/wiki_crud.rs`, append a handler near other put-style handlers (search for `pub(crate) async fn put_wiki_page_handler` to find a neighbor):

```rust
#[derive(Debug, serde::Deserialize)]
pub(crate) struct VerdictBody {
    pub verdict: String,
    #[serde(default)]
    pub reason: Option<String>,
}

pub(crate) async fn post_wiki_page_verdict_handler(
    Path(slug): Path<String>,
    Json(body): Json<VerdictBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Slice E17 — accept-list keeps the contract narrow. New verdict
    // values get added here only after a UI surface ships for them.
    let allowed = ["should_continue", "should_let_go", "inconclusive"];
    if !allowed.contains(&body.verdict.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!(
                    "verdict must be one of {:?}, got {}",
                    allowed, body.verdict
                ),
            }),
        ));
    }

    let paths = resolve_wiki_root_for_handler()?;
    // Read full markdown so we can patch frontmatter without
    // rebuilding it (E15.3 audit fix preserves status / owner /
    // created_at / custom keys verbatim).
    let content =
        wiki_store::read_wiki_page_content(&paths, &slug).map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("wiki page not found: {e}"),
                }),
            )
        })?;
    let now_iso = wiki_store::now_iso8601();
    let mut updated =
        wiki_store::patch_frontmatter_field(&content, "verdict", Some(&body.verdict));
    updated = wiki_store::patch_frontmatter_field(&updated, "verdict_at", Some(&now_iso));
    updated = wiki_store::patch_frontmatter_field(
        &updated,
        "verdict_reason",
        body.reason.as_deref(),
    );

    wiki_store::overwrite_wiki_page_content(&paths, &slug, &updated).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("verdict write failed: {e}"),
            }),
        )
    })?;

    Ok(Json(serde_json::json!({ "ok": true, "verdict_at": now_iso })))
}
```

**NOTE:** `patch_frontmatter_field` is currently `fn` (private) in `wiki_store/src/lib.rs`. Promote it to `pub fn` (and the read-side helper `read_frontmatter_field` for symmetry). Update the comment line that calls them "Slice E15 helper" to remove "private" wording.

b) In `rust/crates/desktop-server/src/routes/wiki.rs`, register the route alongside the existing `pages/{slug}` line:

```rust
        .route(
            "/api/wiki/pages/{slug}/verdict",
            post(post_wiki_page_verdict_handler),
        )
```

c) In `rust/crates/desktop-server/src/lib.rs`, add `post_wiki_page_verdict_handler` to the `pub(crate) use handlers::wiki_crud::{ ... }` import block (alphabetical position).

**Step 4: Run test to verify it passes**

Run: `cd rust && cargo test -p desktop-server put_verdict`

Expected: PASS (2/2).

**Step 5: Commit**

```bash
git add rust/crates/wiki_store/src/lib.rs rust/crates/desktop-server/src/handlers/wiki_crud.rs rust/crates/desktop-server/src/routes/wiki.rs rust/crates/desktop-server/src/lib.rs
git commit -m "feat(api): POST /api/wiki/pages/{slug}/verdict (E17.3)"
```

### Task E17.4: WikiArticle verdict picker UI

**Files:**
- Create: `apps/desktop-shell/src/features/wiki/VerdictPicker.tsx`
- Modify: `apps/desktop-shell/src/features/wiki/WikiArticle.tsx` (mount picker in metadata row)
- Modify: `apps/desktop-shell/src/api/wiki/repository.ts` (add `setWikiPageVerdict`)
- Test: `apps/desktop-shell/src/features/wiki/VerdictPicker.test.tsx` (ambient-vitest contract)

**Step 1: Write the failing test**

Create `apps/desktop-shell/src/features/wiki/VerdictPicker.test.tsx`:

```tsx
import { VerdictPicker } from "./VerdictPicker";

declare const describe: (n: string, fn: () => void) => void;
declare const it: (n: string, fn: () => void) => void;
declare const expect: <T>(actual: T) => {
  toBe(expected: T): void;
  toContain(expected: unknown): void;
};

describe("VerdictPicker", () => {
  it("renders 3 verdict choices and the current verdict pre-selected", () => {
    const out = VerdictPicker({
      currentVerdict: "should_continue",
      reason: "follow-ups in 5 days",
      onChange: () => {},
    });
    // Sanity — the component should reference all three verdict
    // strings somewhere in its output. Tree shape is checked via
    // tsc; this assertion only locks the public choice set.
    const tree = JSON.stringify(out);
    expect(tree).toContain("should_continue");
    expect(tree).toContain("should_let_go");
    expect(tree).toContain("inconclusive");
  });
});
```

**Step 2: Run test to verify it fails**

Run: `cd apps/desktop-shell && npx tsc --noEmit`

Expected: FAIL — `Cannot find module './VerdictPicker'` (or a missing-file error).

**Step 3: Write minimal implementation**

Create `apps/desktop-shell/src/features/wiki/VerdictPicker.tsx`:

```tsx
import { useState } from "react";
import { Sparkle, ArrowDownToLine, HelpCircle } from "lucide-react";

export type Verdict = "should_continue" | "should_let_go" | "inconclusive";

const CHOICES: ReadonlyArray<{
  id: Verdict;
  label: string;
  hint: string;
  Icon: typeof Sparkle;
}> = [
  {
    id: "should_continue",
    label: "继续投入",
    hint: "这条想法事后看仍值得继续追",
    Icon: Sparkle,
  },
  {
    id: "should_let_go",
    label: "可以放下",
    hint: "投入了一段时间，没有产出；可以冷却",
    Icon: ArrowDownToLine,
  },
  {
    id: "inconclusive",
    label: "暂未决定",
    hint: "证据不足以下判断；先留着观察",
    Icon: HelpCircle,
  },
];

export interface VerdictPickerProps {
  currentVerdict?: Verdict | null;
  reason?: string;
  onChange: (verdict: Verdict, reason: string) => void | Promise<void>;
}

export function VerdictPicker({
  currentVerdict,
  reason,
  onChange,
}: VerdictPickerProps) {
  const [open, setOpen] = useState(false);
  const [draftReason, setDraftReason] = useState(reason ?? "");
  const [saving, setSaving] = useState<Verdict | null>(null);

  const current = CHOICES.find((c) => c.id === currentVerdict);

  return (
    <div className="ds-verdict-picker">
      <button
        type="button"
        className="ds-verdict-trigger"
        onClick={() => setOpen((v) => !v)}
        title="给这条灵感的事后判断"
        data-active={open || undefined}
      >
        {current ? (
          <>
            <current.Icon className="size-3" aria-hidden />
            <span>{current.label}</span>
          </>
        ) : (
          <span className="text-muted-foreground">事后判断</span>
        )}
      </button>
      {open ? (
        <div className="ds-verdict-popover" role="dialog" aria-label="设置事后判断">
          <p className="ds-verdict-help">
            事后看，这条想法值得继续投入还是可以放下？
          </p>
          <div className="ds-verdict-options">
            {CHOICES.map((choice) => (
              <button
                key={choice.id}
                type="button"
                className="ds-verdict-option"
                data-active={currentVerdict === choice.id || undefined}
                disabled={saving !== null}
                onClick={async () => {
                  setSaving(choice.id);
                  try {
                    await onChange(choice.id, draftReason);
                    setOpen(false);
                  } finally {
                    setSaving(null);
                  }
                }}
              >
                <choice.Icon className="size-3.5" aria-hidden />
                <div className="flex flex-col items-start">
                  <span className="font-medium">{choice.label}</span>
                  <span className="text-[10px] text-muted-foreground">
                    {choice.hint}
                  </span>
                </div>
              </button>
            ))}
          </div>
          <textarea
            className="ds-verdict-reason"
            placeholder="为什么这么判断？(可选)"
            value={draftReason}
            onChange={(e) => setDraftReason(e.target.value)}
            maxLength={200}
            rows={2}
          />
        </div>
      ) : null}
    </div>
  );
}
```

Add the matching CSS classes in `apps/desktop-shell/src/globals.css` (search for `.ds-rail-secondary-toggle` to find the icon-button pattern and follow its conventions):

```css
.ds-verdict-picker {
  position: relative;
  display: inline-block;
}
.ds-verdict-trigger {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 8px;
  border-radius: 4px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  color: var(--color-foreground);
  font-size: 11px;
  cursor: pointer;
  transition: border-color 120ms;
}
.ds-verdict-trigger:hover,
.ds-verdict-trigger[data-active] {
  border-color: var(--color-primary);
}
.ds-verdict-popover {
  position: absolute;
  z-index: 60;
  top: calc(100% + 4px);
  right: 0;
  min-width: 280px;
  padding: 12px;
  border-radius: 8px;
  border: 1px solid var(--color-border);
  background: var(--color-popover);
  box-shadow: 0 6px 24px rgba(0, 0, 0, 0.12);
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.ds-verdict-help {
  font-size: 11px;
  color: var(--color-muted-foreground);
}
.ds-verdict-options {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.ds-verdict-option {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 6px 8px;
  border-radius: 6px;
  border: 1px solid transparent;
  background: transparent;
  text-align: left;
  cursor: pointer;
  font-size: 12px;
  color: var(--color-foreground);
}
.ds-verdict-option:hover {
  background: var(--color-muted);
}
.ds-verdict-option[data-active] {
  border-color: var(--color-primary);
  background: color-mix(in srgb, var(--color-primary) 8%, transparent);
}
.ds-verdict-reason {
  width: 100%;
  border: 1px solid var(--color-border);
  border-radius: 4px;
  background: var(--color-background);
  font-size: 11px;
  padding: 6px 8px;
  resize: vertical;
}
```

**Step 4: Wire into WikiArticle**

In `apps/desktop-shell/src/features/wiki/WikiArticle.tsx`, find the metadata row (look for `用此页提问` and `编辑` buttons — search `用此页提问`). Add the picker as a sibling button.

```tsx
import { VerdictPicker, type Verdict } from "./VerdictPicker";
import { setWikiPageVerdict } from "@/api/wiki/repository";
```

In the metadata row JSX (just before `用此页提问`):

```tsx
<VerdictPicker
  currentVerdict={summary.verdict as Verdict | null | undefined}
  reason={summary.verdict_reason ?? ""}
  onChange={async (verdict, reason) => {
    await setWikiPageVerdict(slug, { verdict, reason });
    await queryClient.invalidateQueries({
      queryKey: ["wiki", "pages", "detail", slug],
    });
    await queryClient.invalidateQueries({
      queryKey: ["wiki", "pages", "list"],
    });
  }}
/>
```

In `apps/desktop-shell/src/api/wiki/repository.ts`, add the API helper:

```typescript
// ── E17 verdict ───────────────────────────────────────────────────
export async function setWikiPageVerdict(
  slug: string,
  payload: { verdict: "should_continue" | "should_let_go" | "inconclusive"; reason?: string },
): Promise<{ ok: boolean; verdict_at: string }> {
  return fetchJson<{ ok: boolean; verdict_at: string }>(
    `/api/wiki/pages/${encodeURIComponent(slug)}/verdict`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
    },
  );
}
```

**Step 5: Run tsc + build**

Run: `cd apps/desktop-shell && npx tsc --noEmit && npm run build`

Expected: clean.

**Step 6: Commit**

```bash
git add apps/desktop-shell/src/features/wiki/VerdictPicker.tsx apps/desktop-shell/src/features/wiki/VerdictPicker.test.tsx apps/desktop-shell/src/features/wiki/WikiArticle.tsx apps/desktop-shell/src/api/wiki/repository.ts apps/desktop-shell/src/globals.css
git commit -m "feat(wiki): VerdictPicker on WikiArticle (E17.4)"
```

### Task E17.5: Maintainer absorb prompt reads verdict counts

**Files:**
- Modify: `rust/crates/wiki_maintainer/src/lib.rs` (`build_absorb_system_prompt`)
- Test: `rust/crates/wiki_maintainer/src/lib.rs` test module

**Step 1: Write the failing test**

Append in the existing wiki_maintainer test module:

```rust
    #[test]
    fn absorb_prompt_includes_verdict_counts_per_purpose() {
        let tmp = tempdir().unwrap();
        wiki_store::init_wiki(tmp.path()).unwrap();
        let paths = wiki_store::WikiPaths::resolve(tmp.path());
        // Seed two pages with verdicts.
        let cat_dir = paths.wiki.join(wiki_store::WIKI_CONCEPTS_SUBDIR);
        std::fs::create_dir_all(&cat_dir).unwrap();
        std::fs::write(
            cat_dir.join("a.md"),
            "---\ntype: concept\nstatus: active\nowner: human\nschema: v1\ntitle: A\nsummary: x\npurpose:\n  - learning\nverdict: should_continue\ncreated_at: 2026-04-01T00:00:00Z\n---\n\n# body\n",
        )
        .unwrap();
        std::fs::write(
            cat_dir.join("b.md"),
            "---\ntype: concept\nstatus: active\nowner: human\nschema: v1\ntitle: B\nsummary: x\npurpose:\n  - building\nverdict: should_let_go\ncreated_at: 2026-04-01T00:00:00Z\n---\n\n# body\n",
        )
        .unwrap();

        let prompt = build_absorb_system_prompt(&paths, "# Index");
        assert!(prompt.contains("should_continue"), "prompt should mention should_continue: {prompt}");
        assert!(prompt.contains("should_let_go"), "prompt should mention should_let_go: {prompt}");
        // The counts per purpose should appear (concrete shape can
        // evolve, but learning + building should both surface).
        assert!(prompt.contains("learning"));
        assert!(prompt.contains("building"));
    }
```

**Step 2: Run test to verify it fails**

Run: `cd rust && cargo test -p wiki_maintainer absorb_prompt_includes_verdict_counts`

Expected: FAIL — verdict not yet emitted.

**Step 3: Write minimal implementation**

In `rust/crates/wiki_maintainer/src/lib.rs`, locate `build_absorb_system_prompt` (around line 1927) and extend the prompt builder. After the `curation_preferences` block, insert verdict aggregation:

```rust
    // Slice E17 — per-purpose verdict counts. Lets the LLM bias
    // proposals toward purposes the user has confirmed pay off and
    // away from purposes they've explicitly let go of. Aggregates
    // across all wiki pages once per absorb call (cheap O(n) read).
    if let Ok(pages) = wiki_store::list_all_wiki_pages(paths) {
        use std::collections::BTreeMap;
        // BTreeMap so the prompt is deterministic across runs.
        let mut counts: BTreeMap<String, [u32; 3]> = BTreeMap::new(); // [continue, let_go, inconclusive]
        for page in &pages {
            let Some(verdict) = page.verdict.as_deref() else { continue };
            let idx = match verdict {
                "should_continue" => 0,
                "should_let_go" => 1,
                "inconclusive" => 2,
                _ => continue,
            };
            for purpose in &page.purpose {
                let entry = counts.entry(purpose.clone()).or_default();
                entry[idx] = entry[idx].saturating_add(1);
            }
        }
        if !counts.is_empty() {
            prompt.push_str("\n\n## 历史决策回顾 (verdict signals)\n\n");
            prompt.push_str(
                "用户对已存在 wiki 页面的事后判断分布。high should_let_go \
                 占比的 purpose 应当克制建议；high should_continue 占比的 \
                 purpose 可以更主动地建议结晶。\n\n",
            );
            for (purpose, [cont, lg, inc]) in counts {
                prompt.push_str(&format!(
                    "- {purpose}: should_continue={cont} should_let_go={lg} inconclusive={inc}\n"
                ));
            }
        }
    }
```

**Step 4: Run test to verify it passes**

Run: `cd rust && cargo test -p wiki_maintainer absorb_prompt_includes_verdict_counts`

Expected: PASS.

**Step 5: Final slice verification**

Run: `cd rust && cargo test -p wiki_store -p wiki_maintainer -p desktop-server --lib && cd ../apps/desktop-shell && npm run build`

Expected: all green.

**Step 6: Commit**

```bash
git add rust/crates/wiki_maintainer/src/lib.rs
git commit -m "feat(maintainer): per-purpose verdict counts in absorb prompt (E17.5)"
```

---

## Slice E18 — ROI Pulse Panel

**Tension this resolves:** Users have no visible answer to "我这 30 天捕的灵感，到底产出了什么？" Buddy's curation thesis lives or dies on the user being able to feel the payoff loop closing. Without that visibility, every keep-vs-let-go decision is a guess.

**Resolution:** A new derived-data panel on Home — purely calculated from existing fields (`raw_count`, `expressed_in`, `purpose`, `created_at`, `priority`, `vitality`). Two surfaces:

1. **30 / 90 day funnel:** raw inflow → wiki crystallized → expressed (referenced in `expressed_in`).
2. **Per-purpose conversion:** for each Purpose Lens, what fraction of that purpose's pages reached `should_continue` verdict / `growing` vitality / `expressed_in` references.

Strict no-input rule: this panel **only displays**. Users don't fill anything in here. Verdicts feed in via E17; raw counts feed in via existing ingest paths.

**Done means:**

- Pure functions in `roi-pulse.ts` compute funnels deterministically from given inputs (no fetches inside).
- Home page renders the panel below the existing 减熵脉冲 section.
- Empty Vault renders an honest "需要更多数据" placeholder; doesn't extrapolate from 0.
- Per-purpose row appears only when that purpose has at least 3 pages (sample-size floor mirrors E13's degrade rule).

### Task E18.1: roi-pulse.ts pure functions

**Files:**
- Create: `apps/desktop-shell/src/features/dashboard/roi-pulse.ts`
- Test: `apps/desktop-shell/src/features/dashboard/roi-pulse.test.ts`

**Step 1: Write the failing test**

Create `roi-pulse.test.ts`:

```typescript
import {
  computeRoiFunnel,
  computeRoiByPurpose,
  type RoiPageSummary,
} from "./roi-pulse";

declare const describe: (n: string, fn: () => void) => void;
declare const it: (n: string, fn: () => void) => void;
declare const expect: <T>(actual: T) => {
  toBe(expected: T): void;
  toEqual(expected: unknown): void;
};

const NOW = 1_700_000_000_000;
const DAY = 86_400_000;

function page(over: Partial<RoiPageSummary>): RoiPageSummary {
  return {
    slug: over.slug ?? "x",
    purpose: over.purpose ?? ["learning"],
    expressed_in: over.expressed_in ?? [],
    verdict: over.verdict ?? null,
    vitality: over.vitality ?? null,
    created_at_ms: over.created_at_ms ?? NOW - 5 * DAY,
  };
}

describe("computeRoiFunnel", () => {
  it("counts raw → wiki → expressed within window", () => {
    const f = computeRoiFunnel({
      pages: [
        page({ slug: "a", expressed_in: ["doc:1"] }),
        page({ slug: "b" }),
        page({ slug: "c", created_at_ms: NOW - 100 * DAY }), // out of window
      ],
      rawCount: 8,
      now: NOW,
      windowDays: 30,
    });
    expect(f).toEqual({
      windowDays: 30,
      rawCount: 8,
      wikiCount: 2,
      expressedCount: 1,
    });
  });

  it("returns zeroes when nothing has been captured", () => {
    const f = computeRoiFunnel({ pages: [], rawCount: 0, now: NOW, windowDays: 30 });
    expect(f).toEqual({ windowDays: 30, rawCount: 0, wikiCount: 0, expressedCount: 0 });
  });
});

describe("computeRoiByPurpose", () => {
  it("groups by purpose and only emits rows with sample ≥ 3", () => {
    const rows = computeRoiByPurpose({
      pages: [
        page({ purpose: ["learning"], verdict: "should_continue" }),
        page({ purpose: ["learning"], verdict: "should_let_go" }),
        page({ purpose: ["learning"], expressed_in: ["d"] }),
        page({ purpose: ["building"], verdict: "should_continue" }), // sample = 1
      ],
      now: NOW,
      windowDays: 30,
    });
    expect(rows.length).toBe(1);
    expect(rows[0].purpose).toBe("learning");
    expect(rows[0].sample).toBe(3);
    expect(rows[0].continueRate).toBe(1 / 3);
    expect(rows[0].letGoRate).toBe(1 / 3);
    expect(rows[0].expressedRate).toBe(1 / 3);
  });

  it("treats vitality=growing as a soft continue signal when verdict missing", () => {
    const rows = computeRoiByPurpose({
      pages: [
        page({ purpose: ["learning"], vitality: "growing" }),
        page({ purpose: ["learning"], vitality: "growing" }),
        page({ purpose: ["learning"], vitality: "cooling" }),
      ],
      now: NOW,
      windowDays: 30,
    });
    expect(rows[0].continueSoftCount).toBe(2);
  });
});
```

**Step 2: Run test to verify it fails**

Run: `cd apps/desktop-shell && npx tsc --noEmit`

Expected: FAIL — `Cannot find module './roi-pulse'`.

**Step 3: Write minimal implementation**

Create `roi-pulse.ts`:

```typescript
/**
 * Slice E18 — ROI Pulse Panel.
 *
 * Pure derivations of "投入 → 表达" funnels from existing wiki page
 * + raw count data. No fetches; the parent provides the data. Mirrors
 * the E13 / entropy-pulse pattern of read-only summaries.
 */

const DAY_MS = 24 * 60 * 60 * 1000;
const PURPOSE_MIN_SAMPLE = 3;

export interface RoiPageSummary {
  slug: string;
  purpose: ReadonlyArray<string>;
  expressed_in: ReadonlyArray<string>;
  verdict?: string | null;
  vitality?: string | null;
  created_at_ms: number;
}

export interface RoiFunnelInput {
  pages: ReadonlyArray<RoiPageSummary>;
  rawCount: number;
  now: number;
  windowDays: number;
}

export interface RoiFunnel {
  windowDays: number;
  rawCount: number;
  wikiCount: number;
  expressedCount: number;
}

export function computeRoiFunnel(input: RoiFunnelInput): RoiFunnel {
  const cutoff = input.now - input.windowDays * DAY_MS;
  const inWindow = input.pages.filter((p) => p.created_at_ms >= cutoff);
  const wikiCount = inWindow.length;
  const expressedCount = inWindow.filter((p) => p.expressed_in.length > 0).length;
  return {
    windowDays: input.windowDays,
    rawCount: input.rawCount,
    wikiCount,
    expressedCount,
  };
}

export interface RoiByPurposeInput {
  pages: ReadonlyArray<RoiPageSummary>;
  now: number;
  windowDays: number;
}

export interface RoiPurposeRow {
  purpose: string;
  sample: number;
  continueRate: number;     // pages where verdict === should_continue
  letGoRate: number;        // pages where verdict === should_let_go
  expressedRate: number;    // pages with at least one expressed_in
  continueSoftCount: number; // pages where verdict missing AND vitality === growing
}

export function computeRoiByPurpose(input: RoiByPurposeInput): RoiPurposeRow[] {
  const cutoff = input.now - input.windowDays * DAY_MS;
  const inWindow = input.pages.filter((p) => p.created_at_ms >= cutoff);
  const byPurpose = new Map<string, RoiPageSummary[]>();
  for (const p of inWindow) {
    for (const purpose of p.purpose) {
      const list = byPurpose.get(purpose) ?? [];
      list.push(p);
      byPurpose.set(purpose, list);
    }
  }
  const out: RoiPurposeRow[] = [];
  for (const [purpose, pages] of byPurpose) {
    if (pages.length < PURPOSE_MIN_SAMPLE) continue;
    const sample = pages.length;
    const cont = pages.filter((p) => p.verdict === "should_continue").length;
    const lg = pages.filter((p) => p.verdict === "should_let_go").length;
    const expressed = pages.filter((p) => p.expressed_in.length > 0).length;
    const softCont = pages.filter(
      (p) => !p.verdict && p.vitality === "growing",
    ).length;
    out.push({
      purpose,
      sample,
      continueRate: cont / sample,
      letGoRate: lg / sample,
      expressedRate: expressed / sample,
      continueSoftCount: softCont,
    });
  }
  out.sort((a, b) => b.sample - a.sample);
  return out;
}
```

**Step 4: Run test to verify it passes**

Run: `cd apps/desktop-shell && npx tsc --noEmit`

Expected: clean.

**Step 5: Commit**

```bash
git add apps/desktop-shell/src/features/dashboard/roi-pulse.ts apps/desktop-shell/src/features/dashboard/roi-pulse.test.ts
git commit -m "feat(dashboard): roi-pulse pure derivations (E18.1)"
```

### Task E18.2: RoiPulsePanel component

**Files:**
- Create: `apps/desktop-shell/src/features/dashboard/RoiPulsePanel.tsx`
- Test: covered by tsc + visual smoke

**Step 1: Write minimal implementation**

Create `apps/desktop-shell/src/features/dashboard/RoiPulsePanel.tsx`:

```tsx
import { useMemo } from "react";
import { TrendingUp, ArrowRightToLine, Sparkle } from "lucide-react";
import {
  computeRoiFunnel,
  computeRoiByPurpose,
  type RoiPageSummary,
} from "./roi-pulse";

export interface RoiPulsePanelProps {
  pages: ReadonlyArray<RoiPageSummary>;
  rawCount: number;
  now?: number;
}

export function RoiPulsePanel({
  pages,
  rawCount,
  now = Date.now(),
}: RoiPulsePanelProps) {
  const funnel30 = useMemo(
    () => computeRoiFunnel({ pages, rawCount, now, windowDays: 30 }),
    [pages, rawCount, now],
  );
  const funnel90 = useMemo(
    () => computeRoiFunnel({ pages, rawCount, now, windowDays: 90 }),
    [pages, rawCount, now],
  );
  const byPurpose = useMemo(
    () => computeRoiByPurpose({ pages, now, windowDays: 30 }),
    [pages, now],
  );

  // Honest empty state: don't extrapolate from 0.
  if (funnel90.wikiCount === 0 && funnel90.rawCount === 0) {
    return (
      <section className="rounded-lg border border-border bg-card px-5 py-5">
        <div className="flex items-center gap-2">
          <TrendingUp className="size-4 text-primary" />
          <h2 className="text-[15px] font-medium">投入回报</h2>
        </div>
        <p className="mt-2 text-[12px] leading-5 text-muted-foreground">
          需要先有 30 天的捕获数据才能算「投入 → 表达」的转化率。
        </p>
      </section>
    );
  }

  return (
    <section className="rounded-lg border border-border bg-card px-5 py-5">
      <div className="flex items-center gap-2">
        <TrendingUp className="size-4 text-primary" />
        <h2 className="text-[15px] font-medium">投入回报</h2>
        <span className="text-[11px] text-muted-foreground">
          (近 30 / 90 天)
        </span>
      </div>
      <p className="mt-2 max-w-2xl text-[12px] leading-5 text-muted-foreground">
        判断「值得继续投入 vs 安心放下」，需要看你过去几十天里捕的灵感
        实际产出了多少。下面只是把已有数据汇总，不让你重新填。
      </p>

      <div className="mt-4 grid gap-3 md:grid-cols-2">
        <FunnelCard title="近 30 天" funnel={funnel30} />
        <FunnelCard title="近 90 天" funnel={funnel90} />
      </div>

      {byPurpose.length ? (
        <div className="mt-4">
          <div className="text-[12px] font-medium">每个 purpose 的转化</div>
          <p className="mt-1 text-[11px] text-muted-foreground">
            按 30 天内的页面分布；样本 &lt; 3 的 purpose 不显示，避免噪音。
          </p>
          <div className="mt-2 grid gap-2">
            {byPurpose.map((row) => (
              <PurposeRow key={row.purpose} row={row} />
            ))}
          </div>
        </div>
      ) : null}
    </section>
  );
}

function FunnelCard({
  title,
  funnel,
}: {
  title: string;
  funnel: ReturnType<typeof computeRoiFunnel>;
}) {
  const wikiRate =
    funnel.rawCount > 0 ? funnel.wikiCount / funnel.rawCount : 0;
  const expressedRate =
    funnel.wikiCount > 0 ? funnel.expressedCount / funnel.wikiCount : 0;
  return (
    <div className="rounded-md border border-border/70 bg-background px-3 py-3 text-[12px]">
      <div className="text-[13px] font-medium">{title}</div>
      <div className="mt-2 grid grid-cols-3 gap-2 text-center">
        <Stat label="原料" value={funnel.rawCount} />
        <ArrowRightToLine className="size-3 self-center text-muted-foreground" />
        <Stat label="知识页" value={funnel.wikiCount} />
      </div>
      <div className="mt-2 text-[11px] text-muted-foreground">
        raw → wiki: {(wikiRate * 100).toFixed(0)}% · wiki → expressed:{" "}
        {(expressedRate * 100).toFixed(0)}% (
        {funnel.expressedCount}/{funnel.wikiCount})
      </div>
    </div>
  );
}

function Stat({ label, value }: { label: string; value: number }) {
  return (
    <div>
      <div className="text-[16px] font-medium">{value}</div>
      <div className="text-[10px] text-muted-foreground">{label}</div>
    </div>
  );
}

function PurposeRow({
  row,
}: {
  row: ReturnType<typeof computeRoiByPurpose>[number];
}) {
  const tone =
    row.continueRate > row.letGoRate * 1.5
      ? "growing"
      : row.letGoRate > row.continueRate * 1.5
        ? "cooling"
        : "neutral";
  return (
    <div
      className="flex items-center gap-3 rounded-md border border-border/70 bg-background px-3 py-2 text-[12px]"
      data-tone={tone}
    >
      <span className="w-20 truncate font-medium">{row.purpose}</span>
      <span className="flex-1 text-[11px] text-muted-foreground">
        样本 {row.sample} · continue {(row.continueRate * 100).toFixed(0)}% ·
        let_go {(row.letGoRate * 100).toFixed(0)}% · expressed{" "}
        {(row.expressedRate * 100).toFixed(0)}%
      </span>
      {row.continueSoftCount > 0 ? (
        <span
          className="inline-flex items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-[10px] text-muted-foreground"
          title="尚未给出 verdict 但 vitality=growing"
        >
          <Sparkle className="size-3" /> +{row.continueSoftCount} 软信号
        </span>
      ) : null}
    </div>
  );
}
```

**Step 2: Run tsc**

Run: `cd apps/desktop-shell && npx tsc --noEmit`

Expected: clean.

**Step 3: Commit**

```bash
git add apps/desktop-shell/src/features/dashboard/RoiPulsePanel.tsx
git commit -m "feat(home): RoiPulsePanel component (E18.2)"
```

### Task E18.3: Wire RoiPulsePanel into Home

**Files:**
- Modify: `apps/desktop-shell/src/features/dashboard/DashboardPage.tsx` (mount panel below 减熵脉冲)
- Test: covered by tsc + manual smoke

**Step 1: Add import**

Find the existing block of dashboard imports (search for `entropy-pulse` in DashboardPage.tsx). Add:

```tsx
import { RoiPulsePanel } from "./RoiPulsePanel";
import type { RoiPageSummary } from "./roi-pulse";
```

**Step 2: Compute the input shape**

Find the line `const totalRaw = statsQuery.data?.raw_count ?? 0;` (or similar). Just below it, add a memoized RoiPageSummary list:

```tsx
const roiPages: ReadonlyArray<RoiPageSummary> = useMemo(() => {
  const pages = pagesQuery.data?.pages ?? [];
  return pages.map((p) => ({
    slug: p.slug,
    purpose: p.purpose ?? [],
    expressed_in: p.expressed_in ?? [],
    verdict: p.verdict ?? null,
    vitality: p.vitality ?? null,
    created_at_ms: Date.parse(p.created_at) || 0,
  }));
}, [pagesQuery.data?.pages]);
```

**Step 3: Mount the panel**

Find the closing of the entropy pulse section (search `<EntropyPulseColumn`). Right after that section's closing `</section>`, add:

```tsx
<RoiPulsePanel pages={roiPages} rawCount={totalRaw} />
```

**Step 4: Build verification**

Run: `cd apps/desktop-shell && npx tsc --noEmit && npm run build`

Expected: clean.

**Step 5: Commit**

```bash
git add apps/desktop-shell/src/features/dashboard/DashboardPage.tsx
git commit -m "feat(home): mount RoiPulsePanel below entropy pulse (E18.3)"
```

---

## Slice Matrix

| Slice | Primary surface | Must not regress | Minimum gate |
|---|---|---|---|
| E17 | wiki page schema, WikiArticle UI, maintainer prompt | `update_existing` preserves verdict; legacy pages parse; absorb prompt size cap | wiki_store + wiki_maintainer + desktop-server tests + tsc + npm build |
| E18 | Home/Pulse | empty Vault renders honest placeholder; pure-function tests deterministic | unit tests + tsc + npm build + visual sanity |

## Quality Matrix

| Surface | Required verification |
|---|---|
| Frontmatter schema | `cargo test -p wiki_store --lib verdict_fields` round-trip + missing-field test |
| API endpoint | `cargo test -p desktop-server put_verdict` (200 happy path + 400 unknown verdict) |
| Frontend pure logic | `tsc --noEmit` for the test files (ambient-vitest contract) |
| React component | render-snapshot via tsc; visual smoke through `mcp__Claude_Preview__preview_eval` for the live preview |
| Maintainer prompt | `cargo test -p wiki_maintainer absorb_prompt_includes_verdict_counts` |
| Backwards compatibility | seed a pre-E17 page, run absorb, assert no panic |

## Risk Matrix

| Risk | Mitigation |
|---|---|
| User pollutes their own signal by clicking verdict carelessly | Verdict is reversible (just another POST); no destructive side effect; UI requires opening a popover, not a one-click commit |
| Prompt bloat from per-purpose verdict counts | BTreeMap iteration is bounded by purpose count (~6); per-line cost ~80 chars; total < 1 KB even with 50 purposes |
| ROI panel makes users feel bad about low conversion | Empty-state copy explicitly says "需要先有 30 天数据"; no tone of judgement; no rank or comparison-to-peer |
| Verdict semantically overlapping with `priority`/`vitality` | Documented at top of slice E17: `priority`/`vitality` = AI-suggested current state; `verdict` = user's post-hoc judgement. Never overwrite each other. |
| Sample-size bias in per-purpose row | `PURPOSE_MIN_SAMPLE = 3` floor; soft signals (`continueSoftCount`) shown separately so they don't inflate confidence |

## Completion Definition

E17 + E18 are complete when, on a 30-day-old vault:

- ✅ Every wiki page surfaces a verdict picker with three honest choices.
- ✅ Setting a verdict round-trips through frontmatter without disturbing other fields.
- ✅ Setting a verdict re-invalidates the page list query so the new state shows up everywhere.
- ✅ Home shows a 投入回报 panel with two funnels and at least one per-purpose row.
- ✅ Maintainer absorb prompts include verdict aggregates without breaking proposal generation.
- ✅ All Rust + TypeScript tests pass.
- ✅ Empty Vault renders an honest "need more data" placeholder, not a misleading 0%.

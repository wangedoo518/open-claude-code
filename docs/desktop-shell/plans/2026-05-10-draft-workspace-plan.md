# Draft Workspace (E20) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a "草稿" surface that lets users compose derivative content (XHS post / blog / 微信 / 长文) by pinning N source wiki pages, editing markdown, then exporting — at which point we auto-write `expressed_in: draft:<slug>` back to each source page so the **ROI funnel reflects real expression instead of a hand-maintained shell**.

**Architecture:** Drafts live as ordinary markdown files under `wiki/drafts/<slug>.md` with their own frontmatter shape (`type: draft`, `target`, `source_pages`, `exported_at?`). They reuse all existing wiki infra (slugify, atomic write via `.tmp` rename, frontmatter parser, git audit) but are listed and edited through dedicated `/api/wiki/drafts/*` endpoints + a new `/drafts` route. The export action calls the existing `wiki_store::append_wiki_page_expressed_ref` helper once per pinned source page — so the loop **closes through the same byte-preserving frontmatter contract** the verdict system already uses (E15.3 / E17 contract).

**Tech Stack:** Rust (wiki_store + desktop-server axum), TypeScript (React 19 + react-router 7 + React Query 5 + Tailwind 4), CodeMirror 6 (markdown editor — already in repo via `@uiw/react-codemirror`).

**Slicing:** 2 user-shippable slices.
- **E20.1** = end-to-end "create + edit + save" of a draft body (no source picker, no export yet — but a real draft is on disk and editable round-trip)
- **E20.2** = source pinning + export action + ROI funnel verification

Each task below is sized 2–5 minutes for the implementing engineer.

**Out of scope** (deferred to E20.3+):
- Per-platform body templates (XHS hook scaffolds, etc.)
- AI-assisted "draft from these pages" prompt
- Draft search / filtering / tags beyond `target`
- Image bundling for XHS upload (XHS app handles)

---

## Slice E20.1 — Draft storage + list + create + body editing

After this slice ships, a user can: open `/drafts`, click "新建草稿", give it a title + target, edit its body in the same CodeMirror used by wiki pages, see the saved draft in the list. No source picker yet, no export. ROI funnel unchanged.

### Task 1: wiki_store — drafts subdir constant + DRAFT_TARGETS const + init

**Files:**
- Modify: `rust/crates/wiki_store/src/lib.rs:155-181` (the WIKI_*_SUBDIR block + WIKI_CATEGORIES table)
- Modify: `rust/crates/wiki_store/src/lib.rs:498-503` (`init_wiki` directory creation)

**Step 1: Write the failing test**

Add at the end of the existing `#[cfg(test)] mod tests` block in `lib.rs`:

```rust
#[test]
fn init_wiki_creates_drafts_subdir() {
    let tmp = tempfile::tempdir().unwrap();
    init_wiki(tmp.path()).unwrap();
    let paths = WikiPaths::resolve(tmp.path());
    assert!(
        paths.wiki.join(WIKI_DRAFTS_SUBDIR).is_dir(),
        "expected wiki/drafts/ to be created on init",
    );
}

#[test]
fn draft_targets_constant_is_canonical() {
    // Drift hazard parity with VERDICT_VALUES — handler + UI both
    // rely on this exact list.
    assert_eq!(DRAFT_TARGETS, &["xhs", "blog", "wechat", "other"]);
}
```

**Step 2: Run test to verify it fails**

Run: `cd rust && cargo test -p wiki_store init_wiki_creates_drafts_subdir draft_targets_constant_is_canonical -- --nocapture`
Expected: FAIL — `WIKI_DRAFTS_SUBDIR` and `DRAFT_TARGETS` undefined.

**Step 3: Add the constants**

In `rust/crates/wiki_store/src/lib.rs` near line 172 (after `WIKI_INSPIRATION_SUBDIR`):

```rust
pub const WIKI_DRAFTS_SUBDIR: &str = "drafts";

/// Slice E20 — canonical list of accepted `target` values for a draft.
/// Lives in wiki_store so handler, frontend types, and the on-disk
/// frontmatter contract all reference one source of truth (mirrors
/// the VERDICT_VALUES pattern). Add new targets here only after a UI
/// surface ships for them.
pub const DRAFT_TARGETS: &[&str] = &["xhs", "blog", "wechat", "other"];
```

**Step 4: Add to init_wiki**

In `init_wiki` (around line 498), after the existing `fs::create_dir_all(paths.wiki.join(WIKI_INSPIRATION_SUBDIR))?;` line, add:

```rust
fs::create_dir_all(paths.wiki.join(WIKI_DRAFTS_SUBDIR))?;
```

Note: do NOT add `("draft", WIKI_DRAFTS_SUBDIR)` to `WIKI_CATEGORIES` (line 175). Drafts are intentionally excluded from `list_all_wiki_pages` so they don't pollute the wiki index, search, dashboard counts, or the maintainer absorb prompt.

**Step 5: Run tests to verify pass**

Run: `cd rust && cargo test -p wiki_store init_wiki_creates_drafts_subdir draft_targets_constant_is_canonical`
Expected: PASS.

**Step 6: Commit**

```bash
git add rust/crates/wiki_store/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(wiki_store): drafts subdir + DRAFT_TARGETS canonical list (E20.1)

Reserve wiki/drafts/ as the on-disk home for derivative content
(XHS posts, blog drafts, 微信, 长文). Mirrors the verdict-allowlist
pattern: target values are a single const consumed by handler +
TS DTO + UI so they can't drift. Drafts are deliberately NOT in
WIKI_CATEGORIES so list_all_wiki_pages / search / dashboard /
maintainer absorb prompt continue to ignore them — the wiki index
stays a "knowledge crystal" index, not a "stuff I wrote" index.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: wiki_store — DraftSummary type + frontmatter parse

**Files:**
- Modify: `rust/crates/wiki_store/src/lib.rs` (add new struct + parse fn near `WikiPageSummary` block at line 3778)

**Step 1: Write the failing test**

Add to the existing `#[cfg(test)] mod tests` block:

```rust
#[test]
fn draft_frontmatter_round_trips() {
    let raw = "---\n\
        type: draft\n\
        title: My XHS Post\n\
        target: xhs\n\
        source_pages:\n  - notion-rebuild\n  - flow-state\n\
        created_at: 2026-05-10T10:00:00Z\n\
        updated_at: 2026-05-10T10:30:00Z\n\
        ---\n\n\
        # 正文 hook\n";
    let summary = parse_draft_frontmatter("my-xhs-post", raw).unwrap();
    assert_eq!(summary.slug, "my-xhs-post");
    assert_eq!(summary.title, "My XHS Post");
    assert_eq!(summary.target, "xhs");
    assert_eq!(summary.source_pages, vec!["notion-rebuild", "flow-state"]);
    assert_eq!(summary.created_at, "2026-05-10T10:00:00Z");
    assert_eq!(summary.updated_at, "2026-05-10T10:30:00Z");
    assert_eq!(summary.exported_at, None);
}

#[test]
fn draft_frontmatter_treats_missing_source_pages_as_empty() {
    let raw = "---\n\
        type: draft\n\
        title: x\n\
        target: blog\n\
        created_at: 2026-05-10T10:00:00Z\n\
        updated_at: 2026-05-10T10:00:00Z\n\
        ---\n\nbody\n";
    let s = parse_draft_frontmatter("x", raw).unwrap();
    assert!(s.source_pages.is_empty());
}
```

**Step 2: Run test to verify it fails**

Run: `cd rust && cargo test -p wiki_store draft_frontmatter`
Expected: FAIL — type + function undefined.

**Step 3: Add the struct + parse function**

In `rust/crates/wiki_store/src/lib.rs`, immediately after the `WikiPageSummary` struct (around line 3845), add:

```rust
/// Slice E20 — public summary of a draft on disk. Drafts are
/// derivative content (XHS post, blog draft, 微信, 长文) composed
/// from N source wiki pages. Lives at `wiki/drafts/<slug>.md` so it
/// gets git history + atomic-write semantics, but is intentionally
/// NOT a `WikiPageSummary` — the wiki index treats drafts as a
/// separate kind so they don't dilute knowledge-crystal counts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DraftSummary {
    pub slug: String,
    pub title: String,
    /// One of DRAFT_TARGETS. Validated at the handler boundary; this
    /// struct holds whatever was on disk so legacy / hand-edited
    /// drafts can still be listed (handler can flag invalid targets
    /// in a follow-up audit).
    pub target: String,
    /// Slugs of wiki pages this draft pulls from. Empty until the
    /// user pins sources in the editor. Source identity drives the
    /// expressed_in writeback on export (E20.2).
    #[serde(default)]
    pub source_pages: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Set on first export; refreshed on every subsequent export.
    /// `None` means the draft has never been exported.
    #[serde(default)]
    pub exported_at: Option<String>,
    pub byte_size: u64,
}

/// Parse a single draft markdown file into its summary. Lighter than
/// `parse_wiki_file` because draft frontmatter is a much smaller
/// schema (no purpose / verdict / source_refs / vitality etc.).
pub fn parse_draft_frontmatter(slug: &str, content: &str) -> Result<DraftSummary> {
    let mut title = String::new();
    let mut target = String::new();
    let mut created_at = String::new();
    let mut updated_at = String::new();
    let mut exported_at: Option<String> = None;
    let mut source_pages: Vec<String> = Vec::new();

    let mut in_source_pages_list = false;
    let lines: Vec<&str> = content.split('\n').collect();
    if lines.first().copied() != Some("---") {
        return Err(WikiStoreError::Invalid(
            "draft missing leading frontmatter fence".to_string(),
        ));
    }
    for line in lines.iter().skip(1) {
        if *line == "---" {
            break;
        }
        // List-item continuation lines (YAML "- item" inside source_pages).
        if in_source_pages_list {
            if let Some(item) = line.trim_start().strip_prefix("- ") {
                source_pages.push(item.trim().to_string());
                continue;
            }
            in_source_pages_list = false;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("title:") {
            title = rest.trim().trim_matches('"').to_string();
        } else if let Some(rest) = line.strip_prefix("target:") {
            target = rest.trim().trim_matches('"').to_string();
        } else if let Some(rest) = line.strip_prefix("created_at:") {
            created_at = rest.trim().trim_matches('"').to_string();
        } else if let Some(rest) = line.strip_prefix("updated_at:") {
            updated_at = rest.trim().trim_matches('"').to_string();
        } else if let Some(rest) = line.strip_prefix("exported_at:") {
            let v = rest.trim().trim_matches('"');
            if !v.is_empty() && v != "null" {
                exported_at = Some(v.to_string());
            }
        } else if line.trim_end() == "source_pages:" {
            in_source_pages_list = true;
        }
    }
    Ok(DraftSummary {
        slug: slug.to_string(),
        title,
        target,
        source_pages,
        created_at,
        updated_at,
        exported_at,
        byte_size: content.len() as u64,
    })
}
```

**Step 4: Run tests to verify pass**

Run: `cd rust && cargo test -p wiki_store draft_frontmatter`
Expected: PASS (both tests).

**Step 5: Commit**

```bash
git add rust/crates/wiki_store/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(wiki_store): DraftSummary + parse_draft_frontmatter (E20.1)

Drafts get their own summary type rather than reusing
WikiPageSummary — the schemas diverge significantly (no purpose,
no verdict, no source_refs, but adds target + source_pages +
exported_at) and conflating them would force every existing
WikiPage consumer to grow conditional logic.

The parser is a small purpose-built walker (≈ 50 lines) instead of
threading drafts through parse_wiki_frontmatter_fields, which would
have meant changing the 19-element tuple again.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: wiki_store — write_draft, list_drafts, read_draft + tests

**Files:**
- Modify: `rust/crates/wiki_store/src/lib.rs` (add three pub fns)

**Step 1: Write failing tests**

Append to the test module:

```rust
#[test]
fn write_then_read_draft_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    init_wiki(tmp.path()).unwrap();
    let paths = WikiPaths::resolve(tmp.path());
    let path = write_draft(
        &paths,
        "first-post",
        "First Post",
        "xhs",
        "# hello\n\nbody\n",
    )
    .unwrap();
    assert!(path.is_file());
    let (summary, body) = read_draft(&paths, "first-post").unwrap();
    assert_eq!(summary.title, "First Post");
    assert_eq!(summary.target, "xhs");
    assert_eq!(summary.source_pages, Vec::<String>::new());
    assert!(summary.exported_at.is_none());
    assert_eq!(body.trim_end(), "# hello\n\nbody");
}

#[test]
fn list_drafts_returns_all_drafts_sorted_by_updated_at_desc() {
    let tmp = tempfile::tempdir().unwrap();
    init_wiki(tmp.path()).unwrap();
    let paths = WikiPaths::resolve(tmp.path());
    write_draft(&paths, "older", "Older", "xhs", "a").unwrap();
    // Force the second draft's mtime to be later by sleeping briefly.
    std::thread::sleep(std::time::Duration::from_millis(20));
    write_draft(&paths, "newer", "Newer", "blog", "b").unwrap();
    let drafts = list_drafts(&paths).unwrap();
    assert_eq!(drafts.len(), 2);
    assert_eq!(drafts[0].slug, "newer");
    assert_eq!(drafts[1].slug, "older");
}

#[test]
fn list_drafts_returns_empty_when_no_drafts() {
    let tmp = tempfile::tempdir().unwrap();
    init_wiki(tmp.path()).unwrap();
    let paths = WikiPaths::resolve(tmp.path());
    assert!(list_drafts(&paths).unwrap().is_empty());
}
```

**Step 2: Run tests to verify they fail**

Run: `cd rust && cargo test -p wiki_store write_then_read_draft list_drafts`
Expected: FAIL — `write_draft` / `read_draft` / `list_drafts` undefined.

**Step 3: Implement the three functions**

Add to `rust/crates/wiki_store/src/lib.rs` (anywhere in the public API section — alongside `write_wiki_page` is natural):

```rust
/// Slice E20 — write or overwrite a draft markdown file. Atomic via
/// `.tmp` + rename so partial writes never corrupt an in-progress
/// draft. The frontmatter is regenerated from scratch (drafts are
/// the user's own composed content; we don't need the byte-
/// preservation contract that wiki pages have).
pub fn write_draft(
    paths: &WikiPaths,
    slug: &str,
    title: &str,
    target: &str,
    body: &str,
) -> Result<PathBuf> {
    validate_wiki_slug(slug)?;
    let drafts_dir = paths.wiki.join(WIKI_DRAFTS_SUBDIR);
    fs::create_dir_all(&drafts_dir).map_err(|e| WikiStoreError::io(drafts_dir.clone(), e))?;
    let path = drafts_dir.join(format!("{slug}.md"));

    // Read existing frontmatter to preserve created_at + source_pages
    // + exported_at on subsequent writes (i.e. body edits should not
    // reset these fields).
    let now_iso = now_iso8601();
    let (created_at, source_pages, exported_at) = match fs::read_to_string(&path) {
        Ok(prev) => match parse_draft_frontmatter(slug, &prev) {
            Ok(s) => (s.created_at, s.source_pages, s.exported_at),
            Err(_) => (now_iso.clone(), Vec::new(), None),
        },
        Err(_) => (now_iso.clone(), Vec::new(), None),
    };

    let mut fm = String::new();
    fm.push_str("---\n");
    fm.push_str("type: draft\n");
    fm.push_str(&format!("title: {title}\n"));
    fm.push_str(&format!("target: {target}\n"));
    if source_pages.is_empty() {
        fm.push_str("source_pages: []\n");
    } else {
        fm.push_str("source_pages:\n");
        for sp in &source_pages {
            fm.push_str(&format!("  - {sp}\n"));
        }
    }
    fm.push_str(&format!("created_at: {created_at}\n"));
    fm.push_str(&format!("updated_at: {now_iso}\n"));
    if let Some(exp) = &exported_at {
        fm.push_str(&format!("exported_at: {exp}\n"));
    }
    fm.push_str("---\n\n");
    fm.push_str(body);
    if !body.ends_with('\n') {
        fm.push('\n');
    }

    let tmp = path.with_extension("md.tmp");
    fs::write(&tmp, fm.as_bytes()).map_err(|e| WikiStoreError::io(tmp.clone(), e))?;
    fs::rename(&tmp, &path).map_err(|e| WikiStoreError::io(path.clone(), e))?;
    Ok(path)
}

/// Slice E20 — read a draft into (summary, body). Body excludes the
/// leading frontmatter fence. Returns `Invalid` when the file is
/// missing (callers normally hit the handler 404 path before this).
pub fn read_draft(paths: &WikiPaths, slug: &str) -> Result<(DraftSummary, String)> {
    validate_wiki_slug(slug)?;
    let path = paths
        .wiki
        .join(WIKI_DRAFTS_SUBDIR)
        .join(format!("{slug}.md"));
    if !path.is_file() {
        return Err(WikiStoreError::Invalid(format!("draft not found: {slug}")));
    }
    let content = fs::read_to_string(&path).map_err(|e| WikiStoreError::io(path.clone(), e))?;
    let summary = parse_draft_frontmatter(slug, &content)?;
    // Body = everything after the second `---` fence + a single
    // blank line; mirrors how Message.tsx renders concept body.
    let body = content
        .split_once("\n---\n")
        .and_then(|(_, after)| after.split_once('\n').map(|(_, body)| body.to_string()))
        .unwrap_or_default();
    Ok((summary, body))
}

/// Slice E20 — list all drafts, sorted by file mtime descending so
/// the most recently edited surfaces first in the UI. Errors on
/// individual files (corrupt frontmatter, etc.) are swallowed —
/// they're surfaced through a follow-up `validate_drafts()` (not in
/// scope for E20.1).
pub fn list_drafts(paths: &WikiPaths) -> Result<Vec<DraftSummary>> {
    let drafts_dir = paths.wiki.join(WIKI_DRAFTS_SUBDIR);
    if !drafts_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut entries: Vec<(DraftSummary, std::time::SystemTime)> = Vec::new();
    for entry in fs::read_dir(&drafts_dir)
        .map_err(|e| WikiStoreError::io(drafts_dir.clone(), e))?
    {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let slug = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let summary = match parse_draft_frontmatter(&slug, &content) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let mtime = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        entries.push((summary, mtime));
    }
    entries.sort_by(|a, b| b.1.cmp(&a.1));
    Ok(entries.into_iter().map(|(s, _)| s).collect())
}
```

**Step 4: Run tests to verify pass**

Run: `cd rust && cargo test -p wiki_store write_then_read_draft list_drafts`
Expected: PASS (3 tests).

**Step 5: Commit**

```bash
git add rust/crates/wiki_store/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(wiki_store): write_draft / read_draft / list_drafts (E20.1)

Three pub fns rounding out the storage contract. write_draft is
atomic via .tmp + rename and preserves created_at + source_pages
+ exported_at across body edits (subsequent writes only update
updated_at + body). list_drafts orders by mtime descending so the
"recently edited" UX is free.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: desktop-server — list/get/put/post draft handlers + routes + tests

**Files:**
- Modify: `rust/crates/desktop-server/src/handlers/wiki_crud.rs` (add 4 handlers + tests submodule)
- Modify: `rust/crates/desktop-server/src/routes/wiki.rs` (4 route registrations)
- Modify: `rust/crates/desktop-server/src/lib.rs` (4 `pub(crate) use` exports)

**Step 1: Write the failing tests**

In `wiki_crud.rs`, add a new test submodule below `verdict_tests`:

```rust
#[cfg(test)]
mod draft_tests {
    use super::*;
    use crate::WIKI_ENV_GUARD;

    #[tokio::test]
    async fn post_draft_creates_file_with_frontmatter() {
        let _g = WIKI_ENV_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("CLAWWIKI_HOME", tmp.path());
        wiki_store::init_wiki(tmp.path()).unwrap();

        let body = CreateDraftBody {
            title: "First XHS".to_string(),
            target: "xhs".to_string(),
        };
        let resp = post_draft_handler(Json(body)).await.expect("create succeeds");
        let payload = resp.0;
        let slug = payload["slug"].as_str().expect("slug returned").to_string();

        let paths = wiki_store::WikiPaths::resolve(tmp.path());
        let (summary, body) = wiki_store::read_draft(&paths, &slug).unwrap();
        assert_eq!(summary.title, "First XHS");
        assert_eq!(summary.target, "xhs");
        assert!(body.is_empty() || body.trim().is_empty());

        std::env::remove_var("CLAWWIKI_HOME");
    }

    #[tokio::test]
    async fn post_draft_rejects_unknown_target() {
        let _g = WIKI_ENV_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("CLAWWIKI_HOME", tmp.path());
        wiki_store::init_wiki(tmp.path()).unwrap();

        let body = CreateDraftBody {
            title: "x".to_string(),
            target: "tiktok".to_string(),
        };
        let err = post_draft_handler(Json(body))
            .await
            .expect_err("unknown target rejected");
        assert_eq!(err.0, StatusCode::BAD_REQUEST);

        std::env::remove_var("CLAWWIKI_HOME");
    }

    #[tokio::test]
    async fn put_draft_body_updates_updated_at_only() {
        let _g = WIKI_ENV_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("CLAWWIKI_HOME", tmp.path());
        wiki_store::init_wiki(tmp.path()).unwrap();
        let paths = wiki_store::WikiPaths::resolve(tmp.path());
        wiki_store::write_draft(&paths, "p", "Title", "xhs", "old\n").unwrap();
        let (before, _) = wiki_store::read_draft(&paths, "p").unwrap();

        std::thread::sleep(std::time::Duration::from_millis(20));
        let body = PutDraftBody {
            title: "Title".to_string(),
            target: "xhs".to_string(),
            body: "new content\n".to_string(),
        };
        put_draft_handler(axum::extract::Path("p".to_string()), Json(body))
            .await
            .expect("put succeeds");

        let (after, body_after) = wiki_store::read_draft(&paths, "p").unwrap();
        assert_eq!(after.created_at, before.created_at, "created_at preserved");
        assert_ne!(after.updated_at, before.updated_at, "updated_at refreshed");
        assert!(body_after.contains("new content"));

        std::env::remove_var("CLAWWIKI_HOME");
    }
}
```

**Step 2: Run tests to verify failure**

Run: `cd rust && cargo test -p desktop-server draft_tests`
Expected: FAIL — handlers + bodies not yet defined.

**Step 3: Add handler bodies + types to `wiki_crud.rs`**

Append to `rust/crates/desktop-server/src/handlers/wiki_crud.rs` (anywhere — natural place is right after `post_wiki_page_verdict_handler`):

```rust
// ────────────────────────────────────────────────────────────────────
// Slice E20 — Draft workspace handlers.
//
// Drafts are derivative content composed from N source wiki pages.
// Storage is `wiki/drafts/<slug>.md`; see wiki_store::write_draft for
// the on-disk contract. Endpoints intentionally do NOT live under
// `/api/wiki/pages/*` because drafts are not part of the wiki index.
// ────────────────────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
pub(crate) struct CreateDraftBody {
    pub title: String,
    pub target: String,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct PutDraftBody {
    pub title: String,
    pub target: String,
    pub body: String,
}

fn validate_draft_target(target: &str) -> Result<(), ApiError> {
    if !wiki_store::DRAFT_TARGETS.contains(&target) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!(
                    "target must be one of {:?}, got {target}",
                    wiki_store::DRAFT_TARGETS
                ),
            }),
        ));
    }
    Ok(())
}

pub(crate) async fn list_drafts_handler() -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let drafts = wiki_store::list_drafts(&paths).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("list drafts failed: {e}"),
            }),
        )
    })?;
    Ok(Json(serde_json::json!({
        "drafts": drafts,
        "total_count": drafts.len(),
    })))
}

pub(crate) async fn get_draft_handler(
    Path(slug): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let (summary, body) = wiki_store::read_draft(&paths, &slug).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("draft not found: {e}"),
            }),
        )
    })?;
    Ok(Json(serde_json::json!({ "summary": summary, "body": body })))
}

pub(crate) async fn post_draft_handler(
    Json(body): Json<CreateDraftBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if body.title.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "title must not be empty".to_string(),
            }),
        ));
    }
    validate_draft_target(&body.target)?;
    let paths = resolve_wiki_root_for_handler()?;
    let slug = wiki_store::slugify(&body.title);
    wiki_store::write_draft(&paths, &slug, &body.title, &body.target, "")
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("create draft failed: {e}"),
                }),
            )
        })?;
    Ok(Json(serde_json::json!({ "ok": true, "slug": slug })))
}

pub(crate) async fn put_draft_handler(
    Path(slug): Path<String>,
    Json(body): Json<PutDraftBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if body.title.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "title must not be empty".to_string(),
            }),
        ));
    }
    validate_draft_target(&body.target)?;
    let paths = resolve_wiki_root_for_handler()?;
    wiki_store::write_draft(&paths, &slug, &body.title, &body.target, &body.body)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("write draft failed: {e}"),
                }),
            )
        })?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
```

**Step 4: Register routes**

In `rust/crates/desktop-server/src/routes/wiki.rs`, find the block of `.route("/api/wiki/pages/...")` calls and add (alphabetical order is fine):

```rust
.route("/api/wiki/drafts", get(list_drafts_handler).post(post_draft_handler))
.route(
    "/api/wiki/drafts/{slug}",
    get(get_draft_handler).put(put_draft_handler),
)
```

In `rust/crates/desktop-server/src/lib.rs`, extend the existing `pub(crate) use handlers::wiki_crud::{...}` block to include:

```rust
get_draft_handler, list_drafts_handler, post_draft_handler, put_draft_handler,
```

**Step 5: Run tests to verify pass**

Run: `cd rust && cargo test -p desktop-server draft_tests`
Expected: PASS (3 tests).

Run: `cd rust && cargo test --workspace --quiet 2>&1 | grep -E "test result|FAILED"`
Expected: All `ok`. No regressions in existing test counts (wiki_store should now be +6, desktop-server +3).

**Step 6: Commit**

```bash
git add rust/crates/desktop-server/src/handlers/wiki_crud.rs rust/crates/desktop-server/src/routes/wiki.rs rust/crates/desktop-server/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(api): GET/POST /api/wiki/drafts + GET/PUT /api/wiki/drafts/{slug} (E20.1)

Four endpoints rounding out the draft API. validate_draft_target
consults wiki_store::DRAFT_TARGETS so handler + UI + on-disk
contract all reference the same canonical list (parity with the
verdict allowlist pattern from E17). post_draft slugifies the
title via wiki_store::slugify so the slug is consistent with the
rest of the wiki.

Tests: post + reject-bad-target + put-preserves-created_at.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: TS DTO + repository helpers

**Files:**
- Create: `apps/desktop-shell/src/api/wiki/types.ts` (extend with DraftSummary)
- Create: `apps/desktop-shell/src/api/wiki/repository.ts` (extend with 4 helpers)

**Step 1: Add the DTO**

In `apps/desktop-shell/src/api/wiki/types.ts`, append:

```typescript
/**
 * Slice E20 — DraftSummary mirrors Rust struct in
 * `rust/crates/wiki_store/src/lib.rs` (`pub struct DraftSummary`).
 * Drafts are derivative content (XHS, blog, 微信, 长文) composed
 * from N source wiki pages; expressed_in is written back to each
 * source on export.
 */
export interface DraftSummary {
  slug: string;
  title: string;
  /** "xhs" | "blog" | "wechat" | "other" — see DRAFT_TARGETS in wiki_store. */
  target: string;
  source_pages: string[];
  created_at: string;
  updated_at: string;
  /** ISO 8601; null when the draft has never been exported. */
  exported_at?: string | null;
  byte_size: number;
}

export interface DraftsListResponse {
  drafts: DraftSummary[];
  total_count: number;
}

export interface DraftDetailResponse {
  summary: DraftSummary;
  body: string;
}
```

**Step 2: Add the repository helpers**

Append to `apps/desktop-shell/src/api/wiki/repository.ts` (mirror the `setWikiPageVerdict` shape):

```typescript
import type {
  // ... existing imports
  DraftSummary,
  DraftsListResponse,
  DraftDetailResponse,
} from "./types";

/** Allowlist mirrored from wiki_store::DRAFT_TARGETS. Keep in sync. */
export const DRAFT_TARGETS = ["xhs", "blog", "wechat", "other"] as const;
export type DraftTarget = (typeof DRAFT_TARGETS)[number];

export async function listDrafts(): Promise<DraftsListResponse> {
  const res = await wikiHttp("/api/wiki/drafts");
  return res as DraftsListResponse;
}

export async function getDraft(slug: string): Promise<DraftDetailResponse> {
  const res = await wikiHttp(`/api/wiki/drafts/${encodeURIComponent(slug)}`);
  return res as DraftDetailResponse;
}

export async function createDraft(payload: {
  title: string;
  target: DraftTarget;
}): Promise<{ ok: boolean; slug: string }> {
  const res = await wikiHttp("/api/wiki/drafts", {
    method: "POST",
    body: JSON.stringify(payload),
  });
  return res as { ok: boolean; slug: string };
}

export async function putDraft(
  slug: string,
  payload: { title: string; target: DraftTarget; body: string },
): Promise<{ ok: boolean }> {
  const res = await wikiHttp(`/api/wiki/drafts/${encodeURIComponent(slug)}`, {
    method: "PUT",
    body: JSON.stringify(payload),
  });
  return res as { ok: boolean };
}
```

(Note: `wikiHttp` is the existing private helper used elsewhere in this file — read it first if the signature isn't obvious.)

**Step 3: Type-check**

Run: `cd apps/desktop-shell && npx tsc --noEmit`
Expected: clean (no output).

**Step 4: Commit**

```bash
git add apps/desktop-shell/src/api/wiki/types.ts apps/desktop-shell/src/api/wiki/repository.ts
git commit -m "$(cat <<'EOF'
feat(api): TS DTO + repository helpers for drafts (E20.1)

DraftSummary mirrors the Rust struct field-by-field. DRAFT_TARGETS
const is duplicated TS-side rather than fetched at runtime —
compile-time exhaustiveness check matters more than the slight
drift hazard (TS tsc fails loudly if the union type is wrong;
silent backend drift is what we'd prefer to surface in tests).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 6: DraftsPage list view + sidebar route

**Files:**
- Create: `apps/desktop-shell/src/features/draft/DraftsPage.tsx`
- Create: `apps/desktop-shell/src/features/draft/draft-list.test.ts` (ambient-vitest contract)
- Modify: `apps/desktop-shell/src/shell/clawwiki-routes.tsx` (add new route)

**Step 1: Write the ambient test (type-check contract only)**

Create `apps/desktop-shell/src/features/draft/draft-list.test.ts`:

```typescript
/**
 * Slice E20.1 — DraftsPage type contract test.
 * Type-checks via `tsc --noEmit`; runs verbatim once vitest is wired.
 */
import { DraftsPage } from "./DraftsPage";
declare const describe: (n: string, fn: () => void) => void;
declare const it: (n: string, fn: () => void) => void;
declare const expect: <T>(actual: T) => { toBe(expected: T): void };

describe("DraftsPage", () => {
  it("is exported as a named symbol", () => {
    expect(typeof DraftsPage).toBe("function");
  });
});
```

**Step 2: Type-check (file doesn't exist yet → fails)**

Run: `cd apps/desktop-shell && npx tsc --noEmit 2>&1 | head -10`
Expected: error — `Cannot find module './DraftsPage'`.

**Step 3: Create DraftsPage**

Create `apps/desktop-shell/src/features/draft/DraftsPage.tsx`:

```tsx
import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate } from "react-router-dom";
import { FileText, Plus, ArrowRight } from "lucide-react";
import {
  DRAFT_TARGETS,
  createDraft,
  listDrafts,
  type DraftTarget,
} from "@/api/wiki/repository";
import type { DraftSummary } from "@/api/wiki/types";

/**
 * Slice E20.1 — Drafts list page.
 *
 * Buddy positioning: closes the 搜集→整理→表达 loop. Wiki pages are
 * knowledge crystals; drafts are what users actually publish. Each
 * row links to the editor at /drafts/<slug>.
 *
 * Strict no-input on rows: read-only list. Creation is the single
 * input affordance, gated behind an inline form.
 */
export function DraftsPage() {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const draftsQuery = useQuery({
    queryKey: ["wiki", "drafts", "list"],
    queryFn: () => listDrafts(),
  });
  const [showNewForm, setShowNewForm] = useState(false);
  const [newTitle, setNewTitle] = useState("");
  const [newTarget, setNewTarget] = useState<DraftTarget>("xhs");

  const createMutation = useMutation({
    mutationFn: () => createDraft({ title: newTitle, target: newTarget }),
    onSuccess: async ({ slug }) => {
      await queryClient.invalidateQueries({ queryKey: ["wiki", "drafts"] });
      setShowNewForm(false);
      setNewTitle("");
      navigate(`/drafts/${slug}`);
    },
  });

  const drafts = draftsQuery.data?.drafts ?? [];

  return (
    <div className="flex h-full flex-col gap-4 p-6">
      <header className="flex items-center gap-2">
        <FileText className="size-5 text-primary" />
        <h1 className="text-xl font-medium">草稿</h1>
        <span className="text-[12px] text-muted-foreground">
          ({drafts.length})
        </span>
        <button
          type="button"
          className="ml-auto inline-flex items-center gap-1.5 rounded-md border border-border bg-card px-3 py-1.5 text-[13px] hover:bg-muted/50"
          onClick={() => setShowNewForm((v) => !v)}
        >
          <Plus className="size-3.5" />
          新建草稿
        </button>
      </header>

      <p className="max-w-2xl text-[12px] leading-5 text-muted-foreground">
        草稿是你从知识库里派生出来的可发布内容（小红书、博客、微信、长文）。
        编辑器里 pin 几个 wiki 页面作为来源，导出后会自动把这条草稿写进
        每个来源页面的 <code>expressed_in</code> 字段，让"投入回报"面板第一次
        看见真实的表达数据。
      </p>

      {showNewForm && (
        <form
          className="rounded-lg border border-border bg-card p-4"
          onSubmit={(e) => {
            e.preventDefault();
            if (newTitle.trim()) createMutation.mutate();
          }}
        >
          <div className="flex flex-wrap items-center gap-3">
            <input
              autoFocus
              type="text"
              placeholder="草稿标题"
              value={newTitle}
              onChange={(e) => setNewTitle(e.target.value)}
              className="min-w-[200px] flex-1 rounded-md border border-border bg-background px-3 py-1.5 text-[13px]"
            />
            <select
              value={newTarget}
              onChange={(e) => setNewTarget(e.target.value as DraftTarget)}
              className="rounded-md border border-border bg-background px-2 py-1.5 text-[13px]"
            >
              {DRAFT_TARGETS.map((t) => (
                <option key={t} value={t}>
                  {labelForTarget(t)}
                </option>
              ))}
            </select>
            <button
              type="submit"
              className="rounded-md bg-primary px-3 py-1.5 text-[13px] text-primary-foreground hover:opacity-90 disabled:opacity-50"
              disabled={createMutation.isPending || !newTitle.trim()}
            >
              {createMutation.isPending ? "创建中…" : "创建"}
            </button>
          </div>
          {createMutation.error && (
            <div className="mt-2 text-[12px] text-destructive">
              创建失败：{(createMutation.error as Error).message}
            </div>
          )}
        </form>
      )}

      <div className="flex-1 overflow-y-auto">
        {draftsQuery.isLoading && (
          <div className="text-[12px] text-muted-foreground">加载中…</div>
        )}
        {draftsQuery.isSuccess && drafts.length === 0 && !showNewForm && (
          <div className="rounded-lg border border-dashed border-border p-8 text-center text-[13px] text-muted-foreground">
            还没有草稿。点右上角「新建草稿」开始。
          </div>
        )}
        <ul className="grid gap-2">
          {drafts.map((d) => (
            <DraftRow key={d.slug} draft={d} />
          ))}
        </ul>
      </div>
    </div>
  );
}

function DraftRow({ draft }: { draft: DraftSummary }) {
  return (
    <li>
      <Link
        to={`/drafts/${draft.slug}`}
        className="flex items-center gap-3 rounded-md border border-border bg-card px-3 py-2.5 text-[13px] transition-colors hover:border-primary/40 hover:bg-muted/30"
      >
        <span className="min-w-0 flex-1 truncate font-medium">{draft.title}</span>
        <span className="shrink-0 rounded-full bg-muted px-2 py-0.5 text-[10px] text-muted-foreground">
          {labelForTarget(draft.target)}
        </span>
        {draft.exported_at ? (
          <span className="shrink-0 text-[10px] text-primary">
            已导出 {formatRelative(draft.exported_at)}
          </span>
        ) : (
          <span className="shrink-0 text-[10px] text-muted-foreground">
            草稿
          </span>
        )}
        <ArrowRight className="size-3.5 shrink-0 text-muted-foreground" />
      </Link>
    </li>
  );
}

function labelForTarget(t: string): string {
  switch (t) {
    case "xhs":
      return "小红书";
    case "blog":
      return "博客";
    case "wechat":
      return "微信";
    case "other":
      return "其它";
    default:
      return t;
  }
}

function formatRelative(iso: string): string {
  const t = Date.parse(iso);
  if (!Number.isFinite(t)) return "";
  const diff = Date.now() - t;
  const m = Math.round(diff / 60_000);
  if (m < 60) return `${m} 分钟前`;
  const h = Math.round(m / 60);
  if (h < 48) return `${h} 小时前`;
  return `${Math.round(h / 24)} 天前`;
}
```

**Step 4: Add the route**

In `apps/desktop-shell/src/shell/clawwiki-routes.tsx`, add a new entry to `CLAWWIKI_ROUTES` after the wiki entry (around line 112):

```tsx
{
  key: "drafts",
  path: "/drafts",
  routePath: "/drafts/*",
  icon: FileText,
  label: "草稿",
  section: "daily",
  sprint: "E20",
  render: () => <DraftsPage />,
  // E20.2 will switch this to a wrapper that picks DraftsPage vs
  // DraftEditor based on the URL.
},
```

Add the import at the top:

```tsx
import { FileText } from "lucide-react";
import { DraftsPage } from "@/features/draft/DraftsPage";
```

**Step 5: Type-check + build**

Run: `cd apps/desktop-shell && npx tsc --noEmit && npm run build 2>&1 | tail -3`
Expected: tsc clean; build succeeds with `✓ built in <Ns>`.

**Step 6: Commit**

```bash
git add apps/desktop-shell/src/features/draft/ apps/desktop-shell/src/shell/clawwiki-routes.tsx
git commit -m "$(cat <<'EOF'
feat(draft): /drafts list page + sidebar entry (E20.1)

Read-only list of drafts with an inline "新建草稿" form. Creation
slugifies the title backend-side and navigates to /drafts/<slug>
(editor lands in E20.2 — for now the route renders the list page
even for sub-paths so the UX doesn't 404 mid-build).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 7: DraftEditor (markdown body via CodeMirror) + create-then-edit flow

**Files:**
- Create: `apps/desktop-shell/src/features/draft/DraftEditor.tsx`
- Modify: `apps/desktop-shell/src/shell/clawwiki-routes.tsx` (route render → wrapper that picks editor vs list based on slug presence)
- Create: `apps/desktop-shell/src/features/draft/DraftPage.tsx` (small wrapper component)

**Step 1: Create the editor**

Create `apps/desktop-shell/src/features/draft/DraftEditor.tsx`:

```tsx
import { useEffect, useState, useCallback } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { ArrowLeft, Save, Loader2 } from "lucide-react";
import {
  DRAFT_TARGETS,
  getDraft,
  putDraft,
  type DraftTarget,
} from "@/api/wiki/repository";
import { CodeMirrorEditor } from "@/components/CodeMirrorEditor";

/**
 * Slice E20.1 — DraftEditor.
 *
 * Title + target picker + CodeMirror markdown body. Save is manual
 * (no autosave for v1) — explicit "保存" button so users have a
 * clear "I committed this revision" mental model. Source picker +
 * export button arrive in E20.2.
 */
export function DraftEditor({ slug }: { slug: string }) {
  const queryClient = useQueryClient();
  const draftQuery = useQuery({
    queryKey: ["wiki", "drafts", "detail", slug],
    queryFn: () => getDraft(slug),
  });

  const [title, setTitle] = useState("");
  const [target, setTarget] = useState<DraftTarget>("xhs");
  const [body, setBody] = useState("");
  const [dirty, setDirty] = useState(false);

  // Hydrate local state from the server snapshot once per slug load.
  // Subsequent server refetches do NOT clobber in-progress edits
  // (dirty flag tracks user mutation).
  useEffect(() => {
    if (draftQuery.data && !dirty) {
      setTitle(draftQuery.data.summary.title);
      setTarget((draftQuery.data.summary.target as DraftTarget) || "other");
      setBody(draftQuery.data.body);
    }
  }, [draftQuery.data, dirty]);

  const onTitleChange = useCallback((v: string) => {
    setTitle(v);
    setDirty(true);
  }, []);
  const onTargetChange = useCallback((v: DraftTarget) => {
    setTarget(v);
    setDirty(true);
  }, []);
  const onBodyChange = useCallback((v: string) => {
    setBody(v);
    setDirty(true);
  }, []);

  const saveMutation = useMutation({
    mutationFn: () => putDraft(slug, { title, target, body }),
    onSuccess: async () => {
      setDirty(false);
      await queryClient.invalidateQueries({
        queryKey: ["wiki", "drafts", "detail", slug],
      });
      await queryClient.invalidateQueries({
        queryKey: ["wiki", "drafts", "list"],
      });
    },
  });

  if (draftQuery.isLoading) {
    return (
      <div className="flex h-full items-center justify-center text-[13px] text-muted-foreground">
        <Loader2 className="mr-2 size-4 animate-spin" />
        加载草稿…
      </div>
    );
  }
  if (draftQuery.isError) {
    return (
      <div className="p-6 text-[13px] text-destructive">
        加载失败：{(draftQuery.error as Error).message}
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col gap-3 p-6">
      <header className="flex items-center gap-3">
        <Link
          to="/drafts"
          className="inline-flex items-center gap-1 text-[12px] text-muted-foreground hover:text-foreground"
        >
          <ArrowLeft className="size-3.5" />
          返回列表
        </Link>
        <input
          type="text"
          value={title}
          onChange={(e) => onTitleChange(e.target.value)}
          placeholder="草稿标题"
          className="min-w-0 flex-1 rounded-md border border-transparent bg-transparent px-2 py-1 text-[18px] font-medium hover:border-border focus:border-border focus:outline-none"
        />
        <select
          value={target}
          onChange={(e) => onTargetChange(e.target.value as DraftTarget)}
          className="rounded-md border border-border bg-background px-2 py-1 text-[12px]"
        >
          {DRAFT_TARGETS.map((t) => (
            <option key={t} value={t}>
              {t}
            </option>
          ))}
        </select>
        <button
          type="button"
          className="inline-flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-[13px] text-primary-foreground hover:opacity-90 disabled:opacity-50"
          onClick={() => saveMutation.mutate()}
          disabled={saveMutation.isPending || !dirty || !title.trim()}
          title={dirty ? "保存修改" : "已保存"}
        >
          <Save className="size-3.5" />
          {saveMutation.isPending ? "保存中…" : dirty ? "保存" : "已保存"}
        </button>
      </header>

      {saveMutation.error && (
        <div className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-[12px] text-destructive">
          保存失败：{(saveMutation.error as Error).message}
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-hidden rounded-lg border border-border">
        <CodeMirrorEditor
          value={body}
          onChange={onBodyChange}
          language="markdown"
          minHeight="100%"
          ariaLabel="草稿正文"
        />
      </div>
    </div>
  );
}
```

**Step 2: Create the route wrapper**

Create `apps/desktop-shell/src/features/draft/DraftPage.tsx`:

```tsx
import { useParams } from "react-router-dom";
import { DraftsPage } from "./DraftsPage";
import { DraftEditor } from "./DraftEditor";

/** Sub-route splitter: /drafts → list, /drafts/<slug> → editor. */
export function DraftPage() {
  const { slug } = useParams<{ slug?: string }>();
  if (slug) return <DraftEditor slug={slug} />;
  return <DraftsPage />;
}
```

**Step 3: Update the route to use the wrapper**

In `apps/desktop-shell/src/shell/clawwiki-routes.tsx`, change the drafts route's `render`:

```tsx
render: () => <DraftPage />,
```

And update the import:

```tsx
import { DraftPage } from "@/features/draft/DraftPage";
```

(remove the now-unused `DraftsPage` import in this file — `DraftPage` re-exports the routing decision.)

You also need to add a sub-route for `/drafts/:slug`. Locate where `routePath` is consumed in the main router (likely `apps/desktop-shell/src/shell/ClawWikiShell.tsx` — search for `routePath`). Confirm `/drafts/*` already wildcards to the same render fn (the `useParams` inside DraftPage handles the split). If the existing router uses Routes/Route with `path={routePath}`, the `*` glob will work.

**Step 4: Type-check + build**

Run: `cd apps/desktop-shell && npx tsc --noEmit && npm run build 2>&1 | tail -3`
Expected: tsc clean; build succeeds.

**Step 5: Manual smoke**

```bash
cd apps/desktop-shell && npm run tauri:dev
```

Manual checks (in the Tauri window):
1. Click 草稿 in the left sidebar — list page renders empty.
2. Click "新建草稿" → enter title "Test post", target "xhs" → 创建. URL navigates to `/drafts/test-post`. Editor loads with empty body.
3. Type some markdown in the editor. "保存" button enables. Click save. Watch button text flip to "已保存".
4. Hit ⌘R / F5 (full reload). Editor reloads with the saved body. Title + target preserved.
5. Navigate back to `/drafts`. The list shows the row with "草稿" badge (not yet exported).

**Step 6: Commit**

```bash
git add apps/desktop-shell/src/features/draft/ apps/desktop-shell/src/shell/clawwiki-routes.tsx
git commit -m "$(cat <<'EOF'
feat(draft): DraftEditor with CodeMirror markdown body (E20.1)

Title + target picker + CodeMirror body. Save is explicit (no
autosave) — users keep a clear mental model of "I committed this
revision". Server-side write preserves created_at across body
edits per wiki_store::write_draft contract. DraftPage wraps the
/drafts/* route with a useParams splitter so /drafts → list and
/drafts/<slug> → editor without an extra route entry.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### E20.1 finishing checklist

Before moving to E20.2:

```bash
cd "D:/Users/111/Desktop/Project/Claude Desktop/buddy/rust" && cargo test --workspace --quiet 2>&1 | grep -E "test result|FAILED" | tail -20
cd "D:/Users/111/Desktop/Project/Claude Desktop/buddy/apps/desktop-shell" && npx tsc --noEmit && npm run build 2>&1 | tail -3
```

Expected: workspace +9 tests (wiki_store +6, desktop-server +3), all green; tsc clean; build succeeds.

Ship-or-don't gate: a real user can now create + edit + save drafts. ROI funnel still doesn't move (no expressed_in writeback yet) but the storage + UI foundation is real.

---

## Slice E20.2 — Source picker + export + expressed_in writeback

After this slice ships: a draft can pin N wiki pages as sources; clicking 导出 batch-writes `expressed_in: draft:<slug>` to each pinned page AND copies the body to the clipboard AND stamps `exported_at` on the draft. The ROI panel's `expressed_count` actually moves.

### Task 8: wiki_store — set_draft_source_pages helper + tests

**Files:**
- Modify: `rust/crates/wiki_store/src/lib.rs` (add `set_draft_source_pages` + tests)

**Why a separate helper:** `write_draft` preserves source_pages across body edits (Task 3 contract). Updating sources needs its own entry point so we don't lose body content during a sources-only edit.

**Step 1: Write failing tests**

Append to test module:

```rust
#[test]
fn set_draft_source_pages_replaces_list_preserves_body() {
    let tmp = tempfile::tempdir().unwrap();
    init_wiki(tmp.path()).unwrap();
    let paths = WikiPaths::resolve(tmp.path());
    write_draft(&paths, "p", "T", "xhs", "body content\n").unwrap();
    set_draft_source_pages(&paths, "p", &["a".to_string(), "b".to_string()]).unwrap();
    let (s, body) = read_draft(&paths, "p").unwrap();
    assert_eq!(s.source_pages, vec!["a", "b"]);
    assert!(body.contains("body content"));
}

#[test]
fn set_draft_source_pages_can_clear_to_empty() {
    let tmp = tempfile::tempdir().unwrap();
    init_wiki(tmp.path()).unwrap();
    let paths = WikiPaths::resolve(tmp.path());
    write_draft(&paths, "p", "T", "xhs", "x").unwrap();
    set_draft_source_pages(&paths, "p", &["a".to_string()]).unwrap();
    set_draft_source_pages(&paths, "p", &[]).unwrap();
    let (s, _) = read_draft(&paths, "p").unwrap();
    assert!(s.source_pages.is_empty());
}
```

**Step 2: Run to confirm failure**

Run: `cd rust && cargo test -p wiki_store set_draft_source_pages`
Expected: FAIL — `set_draft_source_pages` undefined.

**Step 3: Implement**

```rust
/// Slice E20.2 — replace the source_pages list of a draft. Body +
/// title + target + created_at + exported_at are all preserved;
/// only source_pages + updated_at are touched. Atomic via .tmp +
/// rename.
pub fn set_draft_source_pages(
    paths: &WikiPaths,
    slug: &str,
    new_sources: &[String],
) -> Result<()> {
    validate_wiki_slug(slug)?;
    let path = paths
        .wiki
        .join(WIKI_DRAFTS_SUBDIR)
        .join(format!("{slug}.md"));
    if !path.is_file() {
        return Err(WikiStoreError::Invalid(format!("draft not found: {slug}")));
    }
    let prev = fs::read_to_string(&path).map_err(|e| WikiStoreError::io(path.clone(), e))?;
    let summary = parse_draft_frontmatter(slug, &prev)?;
    let body = prev
        .split_once("\n---\n")
        .and_then(|(_, after)| after.split_once('\n').map(|(_, body)| body.to_string()))
        .unwrap_or_default();

    let now_iso = now_iso8601();
    let mut fm = String::new();
    fm.push_str("---\n");
    fm.push_str("type: draft\n");
    fm.push_str(&format!("title: {}\n", summary.title));
    fm.push_str(&format!("target: {}\n", summary.target));
    if new_sources.is_empty() {
        fm.push_str("source_pages: []\n");
    } else {
        fm.push_str("source_pages:\n");
        for sp in new_sources {
            fm.push_str(&format!("  - {sp}\n"));
        }
    }
    fm.push_str(&format!("created_at: {}\n", summary.created_at));
    fm.push_str(&format!("updated_at: {now_iso}\n"));
    if let Some(exp) = &summary.exported_at {
        fm.push_str(&format!("exported_at: {exp}\n"));
    }
    fm.push_str("---\n\n");
    fm.push_str(&body);

    let tmp = path.with_extension("md.tmp");
    fs::write(&tmp, fm.as_bytes()).map_err(|e| WikiStoreError::io(tmp.clone(), e))?;
    fs::rename(&tmp, &path).map_err(|e| WikiStoreError::io(path.clone(), e))?;
    Ok(())
}
```

**Step 4: Run + commit**

```bash
cd rust && cargo test -p wiki_store set_draft_source_pages
git add rust/crates/wiki_store/src/lib.rs
git commit -m "feat(wiki_store): set_draft_source_pages preserves body + exported_at (E20.2)

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

### Task 9: desktop-server — POST /api/wiki/drafts/{slug}/sources + POST /api/wiki/drafts/{slug}/export

**Files:**
- Modify: `rust/crates/desktop-server/src/handlers/wiki_crud.rs` (2 handlers + tests)
- Modify: `rust/crates/desktop-server/src/routes/wiki.rs` (2 routes)
- Modify: `rust/crates/desktop-server/src/lib.rs` (2 exports)

**Step 1: Write failing tests**

Add to `draft_tests` submodule:

```rust
#[tokio::test]
async fn post_draft_sources_replaces_pinned_list() {
    let _g = WIKI_ENV_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("CLAWWIKI_HOME", tmp.path());
    let paths = wiki_store::WikiPaths::resolve(tmp.path());
    wiki_store::init_wiki(tmp.path()).unwrap();
    wiki_store::write_draft(&paths, "p", "T", "xhs", "body").unwrap();

    let body = SetDraftSourcesBody { source_pages: vec!["a".to_string(), "b".to_string()] };
    post_draft_sources_handler(axum::extract::Path("p".to_string()), Json(body))
        .await.expect("set sources succeeds");

    let (summary, _) = wiki_store::read_draft(&paths, "p").unwrap();
    assert_eq!(summary.source_pages, vec!["a", "b"]);

    std::env::remove_var("CLAWWIKI_HOME");
}

#[tokio::test]
async fn post_draft_export_writes_expressed_in_to_each_source() {
    let _g = WIKI_ENV_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("CLAWWIKI_HOME", tmp.path());
    let paths = wiki_store::WikiPaths::resolve(tmp.path());
    wiki_store::init_wiki(tmp.path()).unwrap();
    // Two source wiki pages.
    wiki_store::write_wiki_page(&paths, "src-a", "Src A", "x", "body a\n").unwrap();
    wiki_store::write_wiki_page(&paths, "src-b", "Src B", "x", "body b\n").unwrap();
    // Draft pinning both.
    wiki_store::write_draft(&paths, "post", "Post", "xhs", "the body").unwrap();
    wiki_store::set_draft_source_pages(
        &paths, "post", &["src-a".to_string(), "src-b".to_string()],
    ).unwrap();

    let resp = post_draft_export_handler(axum::extract::Path("post".to_string()))
        .await.expect("export succeeds");
    let payload = resp.0;
    assert_eq!(payload["ok"], serde_json::Value::Bool(true));
    assert!(payload["exported_at"].is_string());

    // Each source has expressed_in: draft:post.
    let (summary_a, _) = wiki_store::read_wiki_page(&paths, "src-a").unwrap();
    let (summary_b, _) = wiki_store::read_wiki_page(&paths, "src-b").unwrap();
    assert!(summary_a.expressed_in.iter().any(|r| r == "draft:post"));
    assert!(summary_b.expressed_in.iter().any(|r| r == "draft:post"));

    // Draft itself stamped with exported_at.
    let (draft_after, _) = wiki_store::read_draft(&paths, "post").unwrap();
    assert!(draft_after.exported_at.is_some());

    std::env::remove_var("CLAWWIKI_HOME");
}

#[tokio::test]
async fn post_draft_export_is_idempotent_for_already_pinned_sources() {
    let _g = WIKI_ENV_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("CLAWWIKI_HOME", tmp.path());
    let paths = wiki_store::WikiPaths::resolve(tmp.path());
    wiki_store::init_wiki(tmp.path()).unwrap();
    wiki_store::write_wiki_page(&paths, "s", "S", "x", "b\n").unwrap();
    wiki_store::write_draft(&paths, "p", "P", "xhs", "body").unwrap();
    wiki_store::set_draft_source_pages(&paths, "p", &["s".to_string()]).unwrap();

    post_draft_export_handler(axum::extract::Path("p".to_string())).await.unwrap();
    post_draft_export_handler(axum::extract::Path("p".to_string())).await.unwrap();

    let (summary, _) = wiki_store::read_wiki_page(&paths, "s").unwrap();
    let count = summary.expressed_in.iter().filter(|r| r.as_str() == "draft:p").count();
    assert_eq!(count, 1, "expressed_in dedup; expected exactly 1 entry, got {count}");

    std::env::remove_var("CLAWWIKI_HOME");
}
```

**Step 2: Run to confirm failure**

Run: `cd rust && cargo test -p desktop-server post_draft_sources post_draft_export`
Expected: FAIL — handlers + body type undefined.

**Step 3: Implement handlers**

Append to `wiki_crud.rs`:

```rust
#[derive(Debug, serde::Deserialize)]
pub(crate) struct SetDraftSourcesBody {
    pub source_pages: Vec<String>,
}

pub(crate) async fn post_draft_sources_handler(
    Path(slug): Path<String>,
    Json(body): Json<SetDraftSourcesBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    // Source slug validation: each must already exist as a wiki page.
    for src in &body.source_pages {
        if wiki_store::read_wiki_page(&paths, src).is_err() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("source page not found: {src}"),
                }),
            ));
        }
    }
    wiki_store::set_draft_source_pages(&paths, &slug, &body.source_pages).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("set sources failed: {e}"),
            }),
        )
    })?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Slice E20.2 — atomic export. For each pinned source page, append
/// `expressed_in: draft:<slug>` (deduped by
/// append_wiki_page_expressed_ref). Then stamp the draft with
/// exported_at so the UI can show "已导出 N 分钟前" without a
/// separate audit log.
///
/// Body is NOT returned; the client already has it cached. The UI
/// copies to clipboard from its local state.
pub(crate) async fn post_draft_export_handler(
    Path(slug): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let paths = resolve_wiki_root_for_handler()?;
    let (summary, _body) = wiki_store::read_draft(&paths, &slug).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("draft not found: {e}"),
            }),
        )
    })?;
    let reference = format!("draft:{slug}");
    for source in &summary.source_pages {
        // Skip silently if the source no longer exists (user deleted
        // a wiki page after pinning it). Not a hard error — the
        // export should still proceed for the remaining sources.
        let _ = wiki_store::append_wiki_page_expressed_ref(&paths, source, &reference);
    }
    // Stamp exported_at on the draft via patch_frontmatter_field
    // (preserves body + other frontmatter byte-for-byte).
    let path = paths
        .wiki
        .join(wiki_store::WIKI_DRAFTS_SUBDIR)
        .join(format!("{slug}.md"));
    let content = std::fs::read_to_string(&path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("read draft failed: {e}"),
            }),
        )
    })?;
    let now_iso = wiki_store::now_iso8601();
    let updated =
        wiki_store::patch_frontmatter_field(&content, "exported_at", Some(&now_iso));
    std::fs::write(&path, updated).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("stamp exported_at failed: {e}"),
            }),
        )
    })?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "exported_at": now_iso,
        "source_count": summary.source_pages.len(),
    })))
}
```

**Step 4: Register routes + exports**

In `rust/crates/desktop-server/src/routes/wiki.rs`:

```rust
.route(
    "/api/wiki/drafts/{slug}/sources",
    post(post_draft_sources_handler),
)
.route(
    "/api/wiki/drafts/{slug}/export",
    post(post_draft_export_handler),
)
```

In `rust/crates/desktop-server/src/lib.rs`, extend the existing `pub(crate) use handlers::wiki_crud::{...}` block:

```rust
post_draft_export_handler, post_draft_sources_handler,
```

**Step 5: Run + commit**

```bash
cd rust && cargo test -p desktop-server draft_tests --quiet
git add rust/crates/desktop-server/src/handlers/wiki_crud.rs rust/crates/desktop-server/src/routes/wiki.rs rust/crates/desktop-server/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(api): POST /api/wiki/drafts/{slug}/{sources,export} (E20.2)

Two endpoints. /sources validates each pinned slug exists as a
wiki page (early 400) before calling set_draft_source_pages.
/export iterates summary.source_pages and calls
append_wiki_page_expressed_ref once per source, then stamps
exported_at via patch_frontmatter_field (single-line surgical
edit). expressed_in dedupe is delegated to
append_wiki_page_expressed_ref's existing logic so re-export
is idempotent.

Tests: sources replacement, full export round-trip, idempotency.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 10: TS API + DraftEditor source picker + export button

**Files:**
- Modify: `apps/desktop-shell/src/api/wiki/repository.ts` (2 helpers)
- Modify: `apps/desktop-shell/src/features/draft/DraftEditor.tsx` (mount picker + export)
- Create: `apps/desktop-shell/src/features/draft/DraftSourcePicker.tsx`

**Step 1: Add API helpers**

Append to `apps/desktop-shell/src/api/wiki/repository.ts`:

```typescript
export async function setDraftSources(
  slug: string,
  sourcePages: string[],
): Promise<{ ok: boolean }> {
  const res = await wikiHttp(
    `/api/wiki/drafts/${encodeURIComponent(slug)}/sources`,
    {
      method: "POST",
      body: JSON.stringify({ source_pages: sourcePages }),
    },
  );
  return res as { ok: boolean };
}

export async function exportDraft(
  slug: string,
): Promise<{ ok: boolean; exported_at: string; source_count: number }> {
  const res = await wikiHttp(
    `/api/wiki/drafts/${encodeURIComponent(slug)}/export`,
    { method: "POST" },
  );
  return res as { ok: boolean; exported_at: string; source_count: number };
}
```

**Step 2: Create the source picker**

`apps/desktop-shell/src/features/draft/DraftSourcePicker.tsx`:

```tsx
import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Search, X } from "lucide-react";
import { listWikiPages } from "@/api/wiki/repository";
import type { WikiPageSummary } from "@/api/wiki/types";

/**
 * Slice E20.2 — pin wiki pages as source for a draft.
 *
 * Read-only list of available wiki pages with a search filter.
 * Selection is a slug Set; parent owns the state and persists via
 * setDraftSources on every change (debounced upstream).
 */
export function DraftSourcePicker({
  selected,
  onChange,
}: {
  selected: ReadonlyArray<string>;
  onChange: (next: string[]) => void;
}) {
  const [query, setQuery] = useState("");
  const pagesQuery = useQuery({
    queryKey: ["wiki", "pages", "list"],
    queryFn: () => listWikiPages(),
  });
  const all: WikiPageSummary[] = pagesQuery.data?.pages ?? [];
  const filtered = useMemo(() => {
    if (!query.trim()) return all;
    const q = query.toLowerCase();
    return all.filter(
      (p) =>
        p.slug.toLowerCase().includes(q) ||
        p.title.toLowerCase().includes(q),
    );
  }, [all, query]);
  const selectedSet = useMemo(() => new Set(selected), [selected]);

  const toggle = (slug: string) => {
    if (selectedSet.has(slug)) {
      onChange(selected.filter((s) => s !== slug));
    } else {
      onChange([...selected, slug]);
    }
  };

  return (
    <div className="flex h-full flex-col gap-2">
      <div className="text-[12px] font-medium text-muted-foreground">
        来源页面 ({selected.length})
      </div>
      {selected.length > 0 && (
        <ul className="flex flex-wrap gap-1.5">
          {selected.map((slug) => (
            <li
              key={slug}
              className="inline-flex items-center gap-1 rounded-full border border-border bg-muted px-2 py-0.5 text-[11px]"
            >
              <span className="max-w-[120px] truncate">{slug}</span>
              <button
                type="button"
                onClick={() => toggle(slug)}
                className="text-muted-foreground hover:text-foreground"
                aria-label={`移除 ${slug}`}
              >
                <X className="size-3" />
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="flex items-center gap-1.5 rounded-md border border-border bg-background px-2 py-1">
        <Search className="size-3 text-muted-foreground" />
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="搜索 wiki 页面…"
          className="min-w-0 flex-1 bg-transparent text-[12px] outline-none"
        />
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto rounded-md border border-border bg-card">
        {pagesQuery.isLoading && (
          <div className="p-3 text-[11px] text-muted-foreground">加载中…</div>
        )}
        {filtered.length === 0 && !pagesQuery.isLoading && (
          <div className="p-3 text-[11px] text-muted-foreground">
            没有匹配的页面
          </div>
        )}
        <ul>
          {filtered.map((p) => (
            <li key={p.slug}>
              <button
                type="button"
                onClick={() => toggle(p.slug)}
                className="flex w-full items-center gap-2 px-2 py-1.5 text-left text-[12px] hover:bg-muted/40"
                data-active={selectedSet.has(p.slug) || undefined}
              >
                <input
                  type="checkbox"
                  checked={selectedSet.has(p.slug)}
                  readOnly
                  className="pointer-events-none size-3"
                />
                <span className="min-w-0 flex-1 truncate">{p.title}</span>
                <span className="shrink-0 text-[10px] text-muted-foreground">
                  {p.category ?? ""}
                </span>
              </button>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
```

**Step 3: Wire into DraftEditor**

Modify `DraftEditor.tsx` — replace the layout to add a left source-picker column + an export button. Diff is large; key additions:

```tsx
// imports
import { DraftSourcePicker } from "./DraftSourcePicker";
import { exportDraft, setDraftSources } from "@/api/wiki/repository";
import { Send, Check } from "lucide-react";

// inside component, after existing state:
const [sources, setSources] = useState<string[]>([]);
useEffect(() => {
  if (draftQuery.data) {
    setSources(draftQuery.data.summary.source_pages);
  }
}, [draftQuery.data?.summary.slug]);  // re-init only on slug change

const sourcesMutation = useMutation({
  mutationFn: (next: string[]) => setDraftSources(slug, next),
  onSuccess: () => queryClient.invalidateQueries({
    queryKey: ["wiki", "drafts", "detail", slug],
  }),
});

const exportMutation = useMutation({
  mutationFn: () => exportDraft(slug),
  onSuccess: async ({ exported_at }) => {
    // Copy current body to clipboard.
    try {
      await navigator.clipboard.writeText(body);
    } catch {
      /* clipboard may be denied; export still succeeded server-side */
    }
    await queryClient.invalidateQueries({ queryKey: ["wiki", "drafts"] });
    await queryClient.invalidateQueries({ queryKey: ["wiki", "pages"] });
    // Optimistic local stamp so the button text flips immediately.
    void exported_at;
  },
});

const onSourcesChange = (next: string[]) => {
  setSources(next);
  sourcesMutation.mutate(next);
};
```

In the JSX, change the editor body section to a 2-column layout:

```tsx
<div className="grid min-h-0 flex-1 gap-3 overflow-hidden md:grid-cols-[260px_1fr]">
  <div className="min-h-0 overflow-hidden">
    <DraftSourcePicker selected={sources} onChange={onSourcesChange} />
  </div>
  <div className="min-h-0 overflow-hidden rounded-lg border border-border">
    <CodeMirrorEditor ... />
  </div>
</div>
```

In the header, after the save button, add the export button:

```tsx
<button
  type="button"
  className="inline-flex items-center gap-1.5 rounded-md border border-primary bg-card px-3 py-1.5 text-[13px] text-primary hover:bg-primary/5 disabled:opacity-50"
  onClick={() => exportMutation.mutate()}
  disabled={
    exportMutation.isPending ||
    dirty ||  // require explicit save first so the export captures latest body
    sources.length === 0
  }
  title={
    dirty ? "请先保存" : sources.length === 0 ? "请先 pin 来源页面" : "复制正文 + 写回 expressed_in"
  }
>
  {exportMutation.isSuccess ? (
    <Check className="size-3.5" />
  ) : (
    <Send className="size-3.5" />
  )}
  {exportMutation.isPending
    ? "导出中…"
    : exportMutation.isSuccess
      ? "已复制"
      : "导出"}
</button>
```

**Step 4: tsc + build**

```bash
cd apps/desktop-shell && npx tsc --noEmit && npm run build 2>&1 | tail -3
```

Expected: clean.

**Step 5: Manual smoke**

1. `npm run tauri:dev`
2. Create a wiki page (or use an existing one in your dev vault)
3. Navigate to `/drafts`, create a draft
4. In the editor: pick the wiki page from the source picker — verify it appears in the chip list
5. Type body content. Save.
6. Click 导出 → button flips to "已复制". Body is in clipboard.
7. Open the source wiki page (`/wiki/<slug>`). Inspect frontmatter → `expressed_in:` now contains `draft:<your-draft-slug>`.
8. Re-export. Check expressed_in still has exactly one entry (idempotency).
9. Navigate to Home/Dashboard → ROI panel's "wiki → expressed" rate moves up.

**Step 6: Commit**

```bash
git add apps/desktop-shell/src/api/wiki/repository.ts apps/desktop-shell/src/features/draft/
git commit -m "$(cat <<'EOF'
feat(draft): source picker + export action (E20.2)

Two-column editor: left = source picker (search + checkbox list of
wiki pages, selected ones rendered as removable chips), right =
CodeMirror body. Sources persist per change (debounced via React
Query; mutation re-fires on every toggle but the server-side
write is a single .tmp+rename so it's safe).

Export button: requires a saved (non-dirty) state + ≥1 pinned
source. On click: server appends expressed_in: draft:<slug> to
each source page (deduped), stamps exported_at on the draft, and
the client copies the body to the clipboard. ROI funnel's
expressed_count finally reflects real expression.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 11: ROI verification + version bump + release

**Files:**
- Modify: `apps/desktop-shell/src-tauri/tauri.conf.json` (version)
- Modify: `apps/desktop-shell/src-tauri/Cargo.toml` (version)
- Modify: `docs/desktop-shell/plans/README.md` (link this plan)

**Step 1: Verify ROI funnel reflects the new data**

In the dev vault, with at least 2 wiki pages exported via at least 1 draft:

1. Navigate to `/` (Home / Dashboard)
2. Confirm RoiPulsePanel's "wiki → expressed" rate is non-zero
3. Confirm the per-purpose row (if applicable) shows expressedRate > 0 for the source pages' purposes

If the panel still reads 0%, debug:

- Inspect the source wiki page's frontmatter — `expressed_in:` must be a YAML list, not inline `[]`
- Check `read_wiki_page` parses it correctly (it goes through `parse_wiki_frontmatter_fields` — confirm in the lifecycle_metadata that `expressed_in.len() > 0`)
- Re-fetch wiki pages list (React Query may have stale cache — wait 30s polling or hard reload)

**Step 2: Full workspace test**

```bash
cd rust && cargo test --workspace --quiet 2>&1 | grep -E "test result|FAILED" | tail -20
```

Expected: all green. New tests: wiki_store +2 (set_draft_source_pages); desktop-server +3 (sources, export, idempotency).

```bash
cd apps/desktop-shell && npx tsc --noEmit && npm run build 2>&1 | tail -3
```

Expected: clean.

**Step 3: Update plan index**

In `docs/desktop-shell/plans/README.md`, add the link near the existing entries:

```markdown
- [Draft Workspace Implementation Plan](./2026-05-10-draft-workspace-plan.md)
```

**Step 4: Bump version**

```bash
sed -i 's/"version": "0.1.11"/"version": "0.1.12"/' apps/desktop-shell/src-tauri/tauri.conf.json
sed -i 's/^version = "0.1.11"$/version = "0.1.12"/' apps/desktop-shell/src-tauri/Cargo.toml
cd apps/desktop-shell/src-tauri && cargo check 2>&1 | tail -2
```

**Step 5: Commit + tag + push**

```bash
git add docs/desktop-shell/plans/README.md docs/desktop-shell/plans/2026-05-10-draft-workspace-plan.md \
  apps/desktop-shell/src-tauri/tauri.conf.json apps/desktop-shell/src-tauri/Cargo.toml \
  apps/desktop-shell/src-tauri/Cargo.lock
git commit -m "$(cat <<'EOF'
chore: bump desktop-shell to v0.1.12 + Draft Workspace plan

E20 Draft Workspace (草稿) fully shipped. /drafts route + sidebar
entry; create + edit + save + source-pick + export with auto
expressed_in writeback. ROI funnel's expressed_count reflects
real expression for the first time.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git tag -a v0.1.12 -m "v0.1.12: Draft Workspace (E20)"
git push origin main
git push origin v0.1.12
```

---

## Cross-cutting checklist (apply throughout)

- **LR-1**: title sizes use Tailwind size classes (`text-xl`, `text-[15px]`, etc.) — no inline `style={{ fontSize }}`.
- **LR-2**: WikiArticle / DraftEditor reading surfaces use the existing `.markdown-content` CSS, not per-page h1/h2 overrides.
- **LR-4**: every `var(--color-TOKEN)` in any new CSS lives in BOTH `light` and `dark` `@theme` blocks. Run `node scripts/check-mojibake.mjs` before committing.
- **LR-5**: any URL-driven selection (e.g. `/drafts/:slug`) uses `useParams` + `useNavigate` — no manual `searchParams` plumbing for this slice.

## Done criteria

- A new wiki page exported via a draft moves the ROI funnel's `expressed_count`.
- All workspace cargo tests green; tsc clean; vite build clean; mojibake check clean.
- Draft Editor handles: empty body save, ≥1 source pin, non-dirty export gate, clipboard copy on export, expressed_in dedupe on re-export.
- New users land on `/drafts`, see a non-confusing empty state with a working "新建草稿" CTA.

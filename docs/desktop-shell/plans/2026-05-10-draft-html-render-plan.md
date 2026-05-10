# Draft HTML Render (E21) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a button on DraftEditor that calls the maintainer LLM to render the draft markdown into a single self-contained HTML file (target-aware: xhs/blog/wechat/other), saves it to `wiki/drafts/<slug>.html`, and opens it in a new browser tab via blob URL. The user finally sees what their draft will look like styled — closes the "表达" gap from "trust the markdown will render somewhere" to "actually preview it before publishing."

**Architecture:** Mirror the existing absorb pipeline pattern: maintainer crate gets a new `render_draft_html()` async fn that calls `broker.chat_completion(request)` with a target-aware system prompt, extracts the first text block, returns the HTML string. desktop-server adds `POST /api/wiki/drafts/{slug}/render-html` that wires draft data → maintainer fn → `write_draft_html()` → JSON response with the HTML payload. Frontend creates a blob URL from the response and opens in a new tab. The MD stays canonical; the HTML is a regeneratable presentation artifact.

**Why this design (over alternatives):**
- **Sync handler, not SSE**: LLM takes 10-30s per render but partial HTML is useless to render. Spinner > streaming complexity.
- **Single self-contained HTML doc**: no external CSS/JS/images — works offline, prints, emails, screenshots.
- **LLM as renderer (not template)**: per Thariq's HTML-effectiveness post — let the model design the layout per target instead of locking to a template.
- **HTML in vault, gitignore'd**: the user can find/share/print it from disk; git status stays clean.

**Tech Stack:** Rust (wiki_maintainer + wiki_store + desktop-server axum), TypeScript (React 19 + React Query 5), no new deps.

**Slicing:** 2 slices, single shippable point at end.
- **E21.1** = Backend: prompt + maintainer fn + handler + storage + .gitignore + tests
- **E21.2** = Frontend: TS API + DraftEditor button + verify + ship

**Out of scope** (defer to E22+):
- Inline iframe preview (open-in-new-tab is enough for v1)
- HTML for wiki pages (drafts only — validate the philosophy first)
- HTML for Ask conversation summaries
- Per-render cost telemetry / budget caps
- Style customization UI (regenerate is the only knob)
- Multi-version HTML history (only latest kept)

---

## Slice E21.1 — Backend: prompt + maintainer fn + handler + storage

After this slice: the API endpoint works end-to-end. Hitting `POST /api/wiki/drafts/<slug>/render-html` produces a real HTML file on disk + returns the HTML payload. No frontend yet.

### Task 1: Maintainer prompt — RENDER_HTML_SYSTEM_PROMPT + build_render_html_request

**Files:**
- Modify: `rust/crates/wiki_maintainer/src/prompt.rs` (add new const + builder fn near the existing `SYSTEM_PROMPT` + `build_concept_request`)

**Step 1: Write the failing test**

Append to `prompt.rs` (or to its sibling `prompt::tests` module, whichever exists):

```rust
#[cfg(test)]
mod render_html_tests {
    use super::*;

    #[test]
    fn build_render_html_request_picks_correct_target_section_for_xhs() {
        let req = build_render_html_request(
            "post-slug",
            "Title",
            "xhs",
            "# hello\n\nbody\n",
        );
        let system = req.system.expect("system prompt");
        // Target-specific section MUST appear so the LLM picks the
        // right styling — this is the contract we lean on.
        assert!(system.contains("[xhs]"), "missing xhs section in:\n{system}");
        assert!(system.contains("phone"), "xhs hint should mention phone-frame");
        // Generic rules also present.
        assert!(system.contains("Output ONLY"));
    }

    #[test]
    fn build_render_html_request_includes_body_in_user_message() {
        let req = build_render_html_request(
            "post-slug",
            "My Title",
            "blog",
            "# heading\n\nparagraph text\n",
        );
        assert_eq!(req.messages.len(), 1);
        let user_text = req.messages[0]
            .content
            .iter()
            .filter_map(|c| match c {
                api::InputContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>();
        assert!(user_text.contains("# heading"));
        assert!(user_text.contains("My Title"));
        assert!(user_text.contains("blog"));
    }

    #[test]
    fn build_render_html_request_caps_body_to_max_input_chars() {
        let huge = "x".repeat(20_000);
        let req = build_render_html_request("p", "T", "other", &huge);
        let user_text = req.messages[0]
            .content
            .iter()
            .filter_map(|c| match c {
                api::InputContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>();
        // The user message includes the body PLUS some boilerplate;
        // assert the body portion is capped.
        assert!(
            user_text.len() < 12_000,
            "user message should be truncated; got {} chars",
            user_text.len()
        );
    }
}
```

**Step 2: Run to confirm failure**

```bash
cd rust && cargo test -p wiki_maintainer build_render_html_request
```
Expected: FAIL — `build_render_html_request` undefined.

**Step 3: Implement the prompt + builder**

In `rust/crates/wiki_maintainer/src/prompt.rs`, near the existing `SYSTEM_PROMPT` const and `build_concept_request()` fn:

```rust
/// Slice E21 — system prompt for rendering a draft markdown into a
/// single self-contained HTML file. The LLM IS the renderer here:
/// no template, no CSS framework, the model picks layout +
/// typography per target. Mirrors Thariq's "HTML effectiveness"
/// pattern (MD as canonical source, HTML as regeneratable
/// presentation).
pub const RENDER_HTML_SYSTEM_PROMPT: &str = r#"You are a presentation renderer. Convert a draft markdown document into a single self-contained HTML file for the user to preview / print / share.

Hard rules (these are non-negotiable):
1. Output ONLY the HTML. No commentary, no markdown fences, no "here is the HTML:" preface. Your entire response must be parseable as a single HTML document.
2. The output must be a complete document: <!DOCTYPE html> through </html>.
3. Use inline <style> tags or style attributes — NO external CSS, NO external JavaScript, NO images from outside the document. The file must work offline.
4. Preserve the visual hierarchy of the source markdown (headings, lists, code blocks, blockquotes, links, emphasis).
5. Stay under 30 KB of HTML output total.
6. Pick reasonable typography, line-height, color contrast — the page must be comfortable to read on a normal monitor.

Target-specific styling:

[xhs] — 小红书 post preview
Render as a phone-frame mockup. Outer page background neutral; inner content max-width 375px, centered, with rounded corners + soft shadow that simulates a phone screen. Large hero title (28-32px). Body text 15-16px with generous line-height. Hashtags (#xxx) styled with a soft brand-colored background pill. Mobile-first spacing. The user will screenshot this preview before pasting into the XHS app.

[blog] — Long-form blog post
Single readable column, max-width 720px, centered. Generous line-height (1.7+) and clear section spacing. Code blocks with monospace + light background + small padding. Blockquotes with a left-border accent. If there are 3 or more H2 headings, include a small table of contents at the top.

[wechat] — 微信公众号 article
Max-width 600px. Spacing and font sizing similar to native 公众号 articles. Blockquotes with a left-border accent in a muted color. Add minimal vertical padding between paragraphs (公众号 reads tighter than blog).

[other] — Generic clean document
Max-width 800px. Sans-serif body, simple navigation, minimal styling. Default to a clean "looks like a Google Doc" feel.

If the target value is unrecognised, fall back to the [other] styling.
"#;

/// Slice E21 — input character cap for the draft body before it
/// goes into the render prompt. Mirrors the absorb truncation
/// pattern (oversize bodies → clipped). Drafts ≤ 10 KB cover
/// every realistic XHS / blog / 微信 / 长文 length.
const RENDER_HTML_MAX_INPUT_BYTES: usize = 10_000;

/// Slice E21 — output token cap. HTML is verbose but a single
/// self-contained doc with inline styles fits well under 8K tokens.
/// Larger than absorb's 800-token cap because HTML wraps every
/// content fragment in tags.
pub const RENDER_HTML_MAX_OUTPUT_TOKENS: u32 = 8_000;

pub fn build_render_html_request(
    slug: &str,
    title: &str,
    target: &str,
    body: &str,
) -> api::MessageRequest {
    let truncated_body = if body.len() > RENDER_HTML_MAX_INPUT_BYTES {
        &body[..RENDER_HTML_MAX_INPUT_BYTES]
    } else {
        body
    };
    let user_text = format!(
        "Render this draft as a single self-contained HTML document.\n\
         \n\
         slug: {slug}\n\
         title: {title}\n\
         target: {target}\n\
         \n\
         --- markdown body ---\n\
         {truncated_body}\n\
         --- end body ---\n\
         \n\
         Output ONLY the HTML. Pick the styling section that matches the target.",
    );
    api::MessageRequest {
        model: String::new(), // filled in by the broker adapter
        max_tokens: RENDER_HTML_MAX_OUTPUT_TOKENS,
        messages: vec![api::InputMessage {
            role: api::InputRole::User,
            content: vec![api::InputContentBlock::Text { text: user_text }],
        }],
        system: Some(RENDER_HTML_SYSTEM_PROMPT.to_string()),
        tools: None,
        tool_choice: None,
        stream: false,
    }
}
```

(Adjust the field names to match the actual `api::MessageRequest` shape — confirm against `vendor/api/src/types.rs:6-18` if compile errors surface. The recon listed: `model`, `max_tokens`, `messages`, `system`, `tools`, `tool_choice`, `stream`.)

**Step 4: Run to confirm pass**

```bash
cd rust && cargo test -p wiki_maintainer build_render_html_request
```
Expected: PASS (3 tests).

**Step 5: Commit**

```bash
git add rust/crates/wiki_maintainer/src/prompt.rs
git commit -m "$(cat <<'EOF'
feat(maintainer): RENDER_HTML_SYSTEM_PROMPT + build_render_html_request (E21)

Per-target system prompt (xhs phone-frame, blog long-form, wechat
公众号, other generic) that tells the LLM to act as a renderer:
output ONLY a single self-contained HTML document with inline
styles, no externals. Mirrors the absorb prompt pattern (static
SYSTEM_PROMPT + builder fn). Body truncated to 10KB before
inclusion to bound token cost.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: Maintainer `render_draft_html()` async fn + mock-broker tests

**Files:**
- Modify: `rust/crates/wiki_maintainer/src/lib.rs` (new pub async fn near `propose_for_raw_entry` + tests)

**Step 1: Write the failing test**

Append to existing `#[cfg(test)] mod tests` (or wherever `MockBrokerSender` is in scope):

```rust
#[tokio::test]
async fn render_draft_html_returns_text_block_from_broker() {
    let canned_html = "<!DOCTYPE html><html><body>hello</body></html>";
    let broker = MockBrokerSender::with_text_response(canned_html);
    let html = render_draft_html(&broker, "post-slug", "T", "xhs", "# hi\n")
        .await
        .expect("render succeeds");
    assert_eq!(html, canned_html);
}

#[tokio::test]
async fn render_draft_html_errors_when_broker_returns_no_text() {
    let broker = MockBrokerSender::with_no_text();
    let result = render_draft_html(&broker, "p", "T", "blog", "body").await;
    assert!(result.is_err(), "no-text response must surface as error");
}

#[tokio::test]
async fn render_draft_html_errors_when_broker_call_fails() {
    let broker = MockBrokerSender::with_error("boom");
    let result = render_draft_html(&broker, "p", "T", "blog", "body").await;
    assert!(result.is_err());
}
```

If `MockBrokerSender` doesn't already have these factory helpers, peek at how existing maintainer tests construct it (recon says it's referenced at `lib.rs:144`; look for patterns like `MockBrokerSender::new(...)` or test-only constructors) and add the helpers as part of this task.

**Step 2: Run to confirm failure**

```bash
cd rust && cargo test -p wiki_maintainer render_draft_html
```
Expected: FAIL — `render_draft_html` undefined.

**Step 3: Implement**

Append to `rust/crates/wiki_maintainer/src/lib.rs`:

```rust
/// Slice E21 — render a draft markdown document into self-contained
/// HTML via the LLM. The MD stays canonical; the HTML is a
/// regeneratable presentation artifact (Thariq's "HTML
/// effectiveness" pattern).
///
/// Returns the raw HTML string. Caller is responsible for
/// persisting via `wiki_store::write_draft_html` and surfacing to
/// the UI.
pub async fn render_draft_html(
    broker: &impl BrokerSender,
    slug: &str,
    title: &str,
    target: &str,
    body: &str,
) -> Result<String, MaintainerError> {
    let request = prompt::build_render_html_request(slug, title, target, body);
    let response = broker
        .chat_completion(request)
        .await
        .map_err(|e| MaintainerError::Llm(format!("render html call failed: {e}")))?;
    extract_first_text(&response)
        .ok_or_else(|| {
            MaintainerError::Llm("render html response had no text block".to_string())
        })
        .map(|s| s.to_string())
}
```

(Confirm error type name — recon mentions errors flow through `MaintainerError` or similar; check the existing `propose_for_raw_entry` signature for the right type.)

**Step 4: Run to confirm pass**

```bash
cd rust && cargo test -p wiki_maintainer render_draft_html
```
Expected: PASS (3 tests).

**Step 5: Commit**

```bash
git add rust/crates/wiki_maintainer/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(maintainer): render_draft_html async fn + mock-broker tests (E21)

Single LLM round-trip (sync, no SSE — partial HTML is useless to
preview). Uses extract_first_text to pull the rendered document
out of the response. Three tests with MockBrokerSender cover the
happy path, no-text response, and broker error.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: wiki_store `write_draft_html()` + tests

**Files:**
- Modify: `rust/crates/wiki_store/src/lib.rs` (add pub fn alongside `write_draft`)

**Step 1: Write the failing test**

Append to the existing `#[cfg(test)] mod tests`:

```rust
#[test]
fn write_draft_html_creates_sibling_to_md() {
    let tmp = tempfile::tempdir().unwrap();
    init_wiki(tmp.path()).unwrap();
    let paths = WikiPaths::resolve(tmp.path());
    write_draft(&paths, "post", "T", "xhs", "body\n").unwrap();
    let html = "<!DOCTYPE html><html></html>";
    let path = write_draft_html(&paths, "post", html).unwrap();
    assert!(path.is_file());
    assert_eq!(path.extension().and_then(|s| s.to_str()), Some("html"));
    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert_eq!(on_disk, html);
}

#[test]
fn write_draft_html_overwrites_existing_atomically() {
    let tmp = tempfile::tempdir().unwrap();
    init_wiki(tmp.path()).unwrap();
    let paths = WikiPaths::resolve(tmp.path());
    write_draft(&paths, "post", "T", "xhs", "body\n").unwrap();
    write_draft_html(&paths, "post", "<html>v1</html>").unwrap();
    write_draft_html(&paths, "post", "<html>v2</html>").unwrap();
    let on_disk = std::fs::read_to_string(
        paths.wiki.join(WIKI_DRAFTS_SUBDIR).join("post.html"),
    )
    .unwrap();
    assert_eq!(on_disk, "<html>v2</html>");
}

#[test]
fn write_draft_html_works_when_md_does_not_exist_yet() {
    // Defensive: the LLM render handler reads the .md before
    // calling this fn, but the storage helper itself shouldn't
    // require a sibling .md. Lets future callers (e.g. wiki page
    // render in E22) reuse the helper.
    let tmp = tempfile::tempdir().unwrap();
    init_wiki(tmp.path()).unwrap();
    let paths = WikiPaths::resolve(tmp.path());
    write_draft_html(&paths, "p", "<html></html>").unwrap();
}
```

**Step 2: Run to confirm failure**

```bash
cd rust && cargo test -p wiki_store write_draft_html
```
Expected: FAIL.

**Step 3: Implement**

In `rust/crates/wiki_store/src/lib.rs`, near `write_draft`:

```rust
/// Slice E21 — write the LLM-rendered HTML for a draft. Lives at
/// `wiki/drafts/<slug>.html` next to the canonical `.md`. Atomic
/// via `.tmp` + rename so a crashed render doesn't leave a half-
/// written file. The HTML is intentionally NOT linked to the .md
/// existence (callers can render before saving the .md), and is
/// NOT git-tracked by default (see `.gitignore` in init_wiki).
pub fn write_draft_html(
    paths: &WikiPaths,
    slug: &str,
    html: &str,
) -> Result<PathBuf> {
    validate_wiki_slug(slug)?;
    let drafts_dir = paths.wiki.join(WIKI_DRAFTS_SUBDIR);
    fs::create_dir_all(&drafts_dir)
        .map_err(|e| WikiStoreError::io(drafts_dir.clone(), e))?;
    let path = drafts_dir.join(format!("{slug}.html"));
    let tmp = path.with_extension("html.tmp");
    fs::write(&tmp, html.as_bytes()).map_err(|e| WikiStoreError::io(tmp.clone(), e))?;
    fs::rename(&tmp, &path).map_err(|e| WikiStoreError::io(path.clone(), e))?;
    Ok(path)
}
```

**Step 4: Run to confirm pass**

```bash
cd rust && cargo test -p wiki_store write_draft_html
```
Expected: PASS (3 tests).

**Step 5: Commit**

```bash
git add rust/crates/wiki_store/src/lib.rs
git commit -m "feat(wiki_store): write_draft_html atomic helper (E21)

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

### Task 4: Drafts dir `.gitignore` for `*.html` (auto-seeded by init_wiki)

**Files:**
- Modify: `rust/crates/wiki_store/src/lib.rs` (`init_wiki` — write `.gitignore` into the drafts dir on first init)

**Why:** HTML files are regeneratable presentation artifacts. They shouldn't pollute `vault_git_status` or git diffs. Auto-seeding `.gitignore` keeps it transparent to the user; advanced users can delete the file if they want HTML version-controlled.

**Step 1: Write the failing test**

Append to test module:

```rust
#[test]
fn init_wiki_seeds_gitignore_in_drafts_subdir() {
    let tmp = tempfile::tempdir().unwrap();
    init_wiki(tmp.path()).unwrap();
    let paths = WikiPaths::resolve(tmp.path());
    let gi = paths.wiki.join(WIKI_DRAFTS_SUBDIR).join(".gitignore");
    assert!(gi.is_file(), "expected wiki/drafts/.gitignore");
    let content = std::fs::read_to_string(&gi).unwrap();
    assert!(
        content.contains("*.html"),
        "expected *.html ignore pattern, got: {content}",
    );
}

#[test]
fn init_wiki_does_not_overwrite_existing_drafts_gitignore() {
    let tmp = tempfile::tempdir().unwrap();
    init_wiki(tmp.path()).unwrap();
    let paths = WikiPaths::resolve(tmp.path());
    let gi = paths.wiki.join(WIKI_DRAFTS_SUBDIR).join(".gitignore");
    // User customised the file.
    std::fs::write(&gi, "# my custom ignore\n").unwrap();
    // Re-init shouldn't clobber.
    init_wiki(tmp.path()).unwrap();
    let content = std::fs::read_to_string(&gi).unwrap();
    assert_eq!(content, "# my custom ignore\n");
}
```

**Step 2: Run to confirm failure**

```bash
cd rust && cargo test -p wiki_store init_wiki_seeds_gitignore
```
Expected: FAIL.

**Step 3: Implement**

In `init_wiki`, right after the drafts subdir creation block, add:

```rust
// Slice E21 — seed .gitignore in wiki/drafts/ so the LLM-rendered
// .html files (regeneratable presentation artifacts) don't
// pollute vault_git_status. Idempotent: only write if absent so
// users who customise the file aren't clobbered on re-init.
{
    let gi_path = paths
        .wiki
        .join(WIKI_DRAFTS_SUBDIR)
        .join(".gitignore");
    if !gi_path.exists() {
        fs::write(
            &gi_path,
            "# E21 — LLM-rendered HTML preview files. Regenerate via the\n\
             # 渲染 HTML 预览 button in DraftEditor; not worth git-tracking.\n\
             # Delete this file if you DO want the .html files committed.\n\
             *.html\n",
        )
        .map_err(|e| WikiStoreError::io(gi_path.clone(), e))?;
    }
}
```

**Step 4: Run to confirm pass**

```bash
cd rust && cargo test -p wiki_store init_wiki_seeds_gitignore
```
Expected: PASS (2 tests).

**Step 5: Commit**

```bash
git add rust/crates/wiki_store/src/lib.rs
git commit -m "feat(wiki_store): seed wiki/drafts/.gitignore for *.html (E21)

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

### Task 5: desktop-server `post_draft_render_html_handler` + route + tests

**Files:**
- Modify: `rust/crates/desktop-server/src/handlers/wiki_crud.rs` (new handler + tests in `draft_tests` submodule)
- Modify: `rust/crates/desktop-server/src/routes/wiki.rs` (route registration)
- Modify: `rust/crates/desktop-server/src/lib.rs` (export the handler)

**Step 1: Write the failing tests**

Append to `draft_tests` submodule in `wiki_crud.rs`:

```rust
#[tokio::test]
async fn post_draft_render_html_returns_404_for_missing_draft() {
    let _g = WIKI_ENV_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("CLAWWIKI_HOME", tmp.path());
    wiki_store::init_wiki(tmp.path()).unwrap();

    let err = post_draft_render_html_handler(
        axum::extract::Path("does-not-exist".to_string()),
    )
    .await
    .expect_err("missing draft → 404");
    assert_eq!(err.0, StatusCode::NOT_FOUND);

    std::env::remove_var("CLAWWIKI_HOME");
}

#[tokio::test]
async fn post_draft_render_html_rejects_empty_body() {
    let _g = WIKI_ENV_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("CLAWWIKI_HOME", tmp.path());
    let paths = wiki_store::WikiPaths::resolve(tmp.path());
    wiki_store::init_wiki(tmp.path()).unwrap();
    wiki_store::write_draft(&paths, "p", "T", "xhs", "").unwrap();

    let err = post_draft_render_html_handler(
        axum::extract::Path("p".to_string()),
    )
    .await
    .expect_err("empty body must be rejected before LLM call");
    assert_eq!(err.0, StatusCode::BAD_REQUEST);

    std::env::remove_var("CLAWWIKI_HOME");
}
```

(Note: we deliberately don't test the LLM-success path at the handler level — the maintainer fn covers that with a mock broker. A handler-level success test would require either mocking `BrokerAdapter::from_global` or shipping a stub provider, both of which are over-engineering for this slice.)

**Step 2: Run to confirm failure**

```bash
cd rust && cargo test -p desktop-server post_draft_render_html
```
Expected: FAIL — handler undefined.

**Step 3: Implement the handler**

Append to `rust/crates/desktop-server/src/handlers/wiki_crud.rs` (after `post_draft_export_handler`):

```rust
/// Slice E21 — render a draft as self-contained HTML via the
/// maintainer LLM. Single sync request (no SSE — partial HTML
/// can't render). Persists `wiki/drafts/<slug>.html` AND returns
/// the HTML string in the JSON response so the frontend can
/// open-in-new-tab via blob URL without a second roundtrip.
///
/// Cost note: each call is one LLM completion (8K tokens cap).
/// Gated behind a deliberate user click in the DraftEditor.
pub(crate) async fn post_draft_render_html_handler(
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
    if body.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "draft body is empty — write some markdown first".to_string(),
            }),
        ));
    }

    // Resolve the active LLM provider via the same adapter the
    // absorb pipeline uses. Surfaces the auth-missing case as
    // 503 so the UI can prompt the user to configure a provider.
    let broker = desktop_core::wiki_maintainer_adapter::BrokerAdapter::from_global()
        .await
        .map_err(|e| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse {
                    error: format!("no LLM provider available: {e}"),
                }),
            )
        })?;

    let html = wiki_maintainer::render_draft_html(
        &broker,
        &slug,
        &summary.title,
        &summary.target,
        &body,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("html render failed: {e}"),
            }),
        )
    })?;

    wiki_store::write_draft_html(&paths, &slug, &html).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("write draft html failed: {e}"),
            }),
        )
    })?;

    Ok(Json(serde_json::json!({
        "ok": true,
        "rendered_at": wiki_store::now_iso8601(),
        "html": html,
    })))
}
```

**Step 4: Register route + export**

In `rust/crates/desktop-server/src/routes/wiki.rs`, add to the existing `/api/wiki/drafts/...` route block:

```rust
.route(
    "/api/wiki/drafts/{slug}/render-html",
    post(post_draft_render_html_handler),
)
```

In `rust/crates/desktop-server/src/lib.rs`, extend the `pub(crate) use handlers::wiki_crud::{...}` block:

```rust
post_draft_render_html_handler,
```

**Step 5: Run to confirm pass**

```bash
cd rust && cargo test -p desktop-server post_draft_render_html
```
Expected: PASS (2 tests).

```bash
cd rust && cargo test --workspace --quiet 2>&1 | grep -E "test result|FAILED"
```
Expected: all `ok`. Workspace count up by ~12 (3 prompt + 3 maintainer + 3 wiki_store write_draft_html + 2 init_wiki gitignore + 2 handler — pre-existing tests untouched).

**Step 6: Commit**

```bash
git add rust/crates/desktop-server/src/handlers/wiki_crud.rs rust/crates/desktop-server/src/routes/wiki.rs rust/crates/desktop-server/src/lib.rs
git commit -m "$(cat <<'EOF'
feat(api): POST /api/wiki/drafts/{slug}/render-html (E21)

Sync handler: read draft → resolve broker → render via maintainer
LLM → persist .html alongside .md → return HTML in response. 503
when no provider configured (UI can prompt to open settings); 400
when draft body is empty (don't waste LLM tokens on whitespace);
404 when draft missing. Two handler-level tests cover the early-
exit paths (the LLM-success path is covered by maintainer
mock-broker tests in Task 2).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### E21.1 finishing checklist

```bash
cd rust && cargo test --workspace --quiet 2>&1 | grep -E "test result|FAILED"
```

Expected: all `ok`. Workspace count: ~+12 vs E20.

Ship-or-don't gate: API end-to-end works. A `curl -X POST $base/api/wiki/drafts/post-slug/render-html` would produce a real HTML file on disk + return JSON with the HTML string. No frontend yet.

---

## Slice E21.2 — Frontend: TS API + DraftEditor button + verify + ship

After this slice ships: a button on the DraftEditor opens a styled HTML preview of the current draft in a new browser tab. ROI funnel unchanged (this is a presentation-layer feature, not a content-flow change).

### Task 6: TS API helper `renderDraftHtml`

**Files:**
- Modify: `apps/desktop-shell/src/api/wiki/repository.ts`

**Step 1: Add the helper**

Append after `exportDraft`:

```typescript
/**
 * Slice E21 — POST `/api/wiki/drafts/:slug/render-html`. Triggers
 * a single LLM call to render the draft markdown into a self-
 * contained HTML document. Persists at `wiki/drafts/<slug>.html`
 * AND returns the HTML in the response payload so the UI can
 * open-in-new-tab without a second roundtrip.
 *
 * Expensive — gate behind a deliberate user click. Server returns
 * 503 if no LLM provider is configured (UI should prompt the user
 * to open settings).
 */
export async function renderDraftHtml(slug: string): Promise<{
  ok: boolean;
  rendered_at: string;
  html: string;
}> {
  return fetchJson<{
    ok: boolean;
    rendered_at: string;
    html: string;
  }>(`/api/wiki/drafts/${encodeURIComponent(slug)}/render-html`, {
    method: "POST",
  });
}
```

**Step 2: Type-check**

```bash
cd apps/desktop-shell && npx tsc --noEmit
```
Expected: clean.

**Step 3: Commit**

```bash
git add apps/desktop-shell/src/api/wiki/repository.ts
git commit -m "feat(api): renderDraftHtml TS helper (E21)

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

### Task 7: DraftEditor button + state + new-tab open

**Files:**
- Modify: `apps/desktop-shell/src/features/draft/DraftEditor.tsx` (mutation + button + open-in-tab)

**Step 1: Add mutation + handler**

Add to imports:

```tsx
import { Sparkles } from "lucide-react";
import { renderDraftHtml } from "@/api/wiki/repository";
```

Inside the component, after `exportMutation`:

```tsx
const renderHtmlMutation = useMutation({
  mutationFn: () => renderDraftHtml(slug),
  onSuccess: (data) => {
    // Wrap the HTML in a Blob, mint an object URL, open in a new
    // tab. The blob URL is short-lived (revoked on tab close) but
    // the .html file persists at wiki/drafts/<slug>.html so the
    // user can find it via Vault git status / file system if
    // they want a durable copy.
    const blob = new Blob([data.html], { type: "text/html" });
    const url = URL.createObjectURL(blob);
    window.open(url, "_blank", "noopener,noreferrer");
    // Best-effort cleanup — revoke after the new tab has had a
    // chance to load. Done via setTimeout because there's no
    // direct "tab loaded" callback for window.open.
    setTimeout(() => URL.revokeObjectURL(url), 30_000);
  },
});
```

**Step 2: Add the button to the header (after the Export button)**

Locate the Export button block in DraftEditor.tsx (around line ~213, look for `onClick={() => exportMutation.mutate()}`). After its closing `</button>`, insert:

```tsx
<button
  type="button"
  className="inline-flex items-center gap-1.5 rounded-md border border-border bg-card px-3 py-1.5 text-[13px] hover:bg-muted/50 disabled:opacity-50"
  onClick={() => renderHtmlMutation.mutate()}
  disabled={
    renderHtmlMutation.isPending || dirty || body.trim().length === 0
  }
  title={
    dirty
      ? "请先保存"
      : body.trim().length === 0
        ? "正文为空"
        : "用 LLM 把当前草稿渲染成一个自包含的 HTML 文件，并在新标签页打开预览"
  }
>
  {renderHtmlMutation.isPending ? (
    <Loader2 className="size-3.5 animate-spin" />
  ) : (
    <Sparkles className="size-3.5" />
  )}
  {renderHtmlMutation.isPending ? "渲染中…" : "渲染 HTML 预览"}
</button>
```

**Step 3: Add an inline error banner**

Below the existing error banners block (next to `exportMutation.error` etc.):

```tsx
{renderHtmlMutation.error ? (
  <div className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-[12px] text-destructive">
    渲染失败：{(renderHtmlMutation.error as Error).message}
  </div>
) : null}
```

**Step 4: Verify**

```bash
cd apps/desktop-shell && npx tsc --noEmit && npm run build 2>&1 | tail -3
```
Expected: tsc clean, build succeeds.

**Step 5: Manual smoke** (in `npm run tauri:dev`)

1. Open `/drafts`, create a draft (e.g. title "Test post", target "xhs")
2. Type some markdown body. Save.
3. Click "渲染 HTML 预览". Spinner appears for ~10-30 seconds.
4. New browser tab opens with a phone-frame styled preview.
5. Check `wiki/drafts/test-post.html` exists on disk.
6. Run `git status` in the vault — `.html` should NOT appear (gitignored from Task 4).

**Step 6: Commit**

```bash
git add apps/desktop-shell/src/features/draft/DraftEditor.tsx
git commit -m "$(cat <<'EOF'
feat(draft): 渲染 HTML 预览 button on DraftEditor (E21)

Click → server runs LLM render → response HTML wrapped in Blob
→ object URL → window.open() in new tab. Disabled gates: dirty
(must save first) and empty body (don't waste tokens). Spinner
during the ~10-30s render. Best-effort URL.revokeObjectURL after
30 sec to free the blob.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 8: Verify + bump v0.1.13 + plan link + push

**Files:**
- Modify: `apps/desktop-shell/src-tauri/tauri.conf.json` (version)
- Modify: `apps/desktop-shell/src-tauri/Cargo.toml` (version)
- Modify: `docs/desktop-shell/plans/README.md` (link this plan)

**Step 1: Final verification**

```bash
cd rust && cargo test --workspace --quiet 2>&1 | grep -E "test result|FAILED"
```
Expected: all `ok`. Workspace +~12 tests vs E20 (892 → ~904).

```bash
cd apps/desktop-shell && npx tsc --noEmit && npm run build 2>&1 | tail -3
```
Expected: clean.

**Step 2: Bump version**

```bash
sed -i 's/"version": "0.1.12"/"version": "0.1.13"/' apps/desktop-shell/src-tauri/tauri.conf.json
sed -i 's/^version = "0.1.12"$/version = "0.1.13"/' apps/desktop-shell/src-tauri/Cargo.toml
cd apps/desktop-shell/src-tauri && cargo check 2>&1 | tail -2
```

**Step 3: Update plan index**

In `docs/desktop-shell/plans/README.md`, add the link near other E20-era entries:

```markdown
- [Draft HTML Render Implementation Plan](./2026-05-10-draft-html-render-plan.md)
```

**Step 4: Commit + tag + push**

```bash
git add docs/desktop-shell/plans/README.md docs/desktop-shell/plans/2026-05-10-draft-html-render-plan.md \
  apps/desktop-shell/src-tauri/tauri.conf.json apps/desktop-shell/src-tauri/Cargo.toml \
  apps/desktop-shell/src-tauri/Cargo.lock
git commit -m "$(cat <<'EOF'
chore: bump desktop-shell to v0.1.13 + Draft HTML Render plan

E21 Draft HTML Render shipped. DraftEditor gets a "渲染 HTML 预览"
button: server-side LLM call renders the draft markdown into a
self-contained HTML doc (target-aware: xhs phone-frame, blog
long-form, wechat 公众号, other generic), persists at
wiki/drafts/<slug>.html (gitignored — regeneratable presentation
artifact), and opens in a new tab via blob URL. The MD stays
canonical; HTML is disposable.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git tag -a v0.1.13 -m "v0.1.13: Draft HTML Render (E21)"
git push origin main
git push origin v0.1.13
```

---

## Cross-cutting checklist

- **LR-1 / LR-2**: not relevant (no new wiki page surfaces, no inline font sizes)
- **LR-3**: not relevant (no WikiFileTree changes)
- **LR-4**: button styles use existing tokens (`var(--color-card)`, `var(--color-border)`, `var(--color-muted)`, `var(--color-destructive)`) — all defined in light + dark blocks. No new CSS files.
- **LR-5**: no URL-driven selection introduced.

## Done criteria

- A user with a wiki page → draft → pinned source → saved body can click "渲染 HTML 预览" and see a styled HTML preview open in a new tab within ~30 seconds.
- The HTML file lives at `wiki/drafts/<slug>.html` and `git status` doesn't show it.
- Re-clicking the button regenerates the HTML (overwrites the file, opens a new tab).
- All workspace cargo tests green; tsc clean; vite build clean.
- target=xhs renders phone-frame; target=blog renders long-form column; target=wechat renders 公众号 width; target=other renders generic doc.

## Risks called out

1. **LLM cost**: each render is ~8K output tokens. At Claude Sonnet pricing that's roughly $0.05/render. Surface this in the button tooltip if usage spikes; add a per-day cap in E22 if needed.
2. **LLM ugliness variance**: same MD + same target may produce wildly different layouts across runs. Acceptable trade-off in v1 — regenerate is cheap. Pin temperature to 0.3 (in `MessageRequest.options` if the API supports it) to reduce variance, OR accept variance as feature ("hit regenerate until you like one").
3. **Blob URL lifetime**: revoked after 30s. If the user navigates the new tab after that, the URL becomes invalid (but the .html file on disk is durable). Add a "open from disk" fallback only if users complain.
4. **No streaming**: full LLM response must complete before any UI feedback besides the spinner. 30s feels long. Mitigation = clear loading copy ("LLM 渲染中，约 15-30 秒") via the button label. Don't add SSE streaming in v1 — partial HTML is unusable.

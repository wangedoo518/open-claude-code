---
title: Buddy Desktop File Drag-Drop Ingest (E29) Implementation Plan
doc_type: plan
status: active
owner: desktop-shell
last_verified: 2026-05-12
supersedes:
  - docs/desktop-shell/plans/2026-05-12-wechat-file-ingestion-plan.md
related:
  - apps/desktop-shell/src/features/inbox/InboxPage.tsx
  - rust/crates/desktop-server/src/handlers/wiki_crud.rs
  - rust/crates/wiki_ingest/src/markitdown.rs
  - rust/crates/wiki_store/src/lib.rs
---

# Buddy Desktop File Drag-Drop Ingest (E29) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Buddy desktop Inbox accepts drag-and-drop of any file (PDF / DOCX / PPT / XLSX / images / audio / video / HTML / CSV / JSON / EPUB / ipynb / zip + plain `.txt`/`.md`) and ingests it as a raw entry. WeChat bot's "暂不支持" reply updates to redirect users to drag-drop instead.

**Architecture:** HTML5 drag-drop on a dedicated zone in `InboxPage` → `FormData` POST to a new `POST /api/wiki/raw/upload` multipart endpoint → backend writes the bytes to a temp file → dispatches to `wiki_ingest::markitdown::extract_via_markitdown` (handles 28+ formats via Python sidecar) with a plain-text shortcut for `.txt`/`.md` → reuses the existing `wiki_store::write_raw_entry` + `append_new_raw_task` pipeline → returns the new `RawEntry` so the frontend can refresh the inbox list.

**Tech Stack:** Rust + axum 0.8 (multipart feature), `wiki_ingest::markitdown` (Python sidecar, already implemented), `wiki_store`, React 19 + Tanstack Query, sonner toast, no new dependencies on either side beyond enabling axum's `multipart` feature.

**Why this pivot:** The original plan tried to wire iLink CDN media download for WeChat-sent files. Static analysis + URL probing of `novac2c.cdn.weixin.qq.com` exhausted without finding a working endpoint — the actual download URL is not in the codebase, and the host returns generic 404 for every reasonable URL form. Reverse-engineering would require mitmproxy + cert pinning bypass. Pivoting to a drag-drop ingest in Buddy itself sidesteps the protocol problem entirely AND delivers a feature that's valuable independent of WeChat (today the Inbox is read-only — no way to add files at all). The WeChat bot's reply updates to gently redirect users to this new path. See `2026-05-12-wechat-file-ingestion-plan.md` for the discovery trail.

---

## Pre-flight: working agreement

1. **Branch:** stay on `main`; per-task commits.
2. **Per-task commits** — `git add <specific files>`; never `-A`.
3. **Verification command bank:**
   - Rust: `cd rust && cargo check --workspace 2>&1 | tail -3` after each edit; `cargo test -p desktop-server` after handler tests.
   - Frontend: `cd apps/desktop-shell && npx tsc --noEmit 2>&1 | tail -5` + `npm run build 2>&1 | tail -3`.
4. **No new external deps** — both axum and markitdown are already present; only the `multipart` axum feature flag gets added.

---

## Phase A — Backend Upload Endpoint

### Task A1: Enable axum `multipart` feature

**Files:**
- Modify: `rust/crates/desktop-server/Cargo.toml:18`

**Step 1: Inspect current axum line**
```bash
grep -n '^axum' rust/crates/desktop-server/Cargo.toml
```
Expected: `axum = { version = "0.8", features = ["ws"] }`

**Step 2: Add `"multipart"` feature**

Edit the line to:
```toml
axum = { version = "0.8", features = ["ws", "multipart"] }
```

**Step 3: Verify build**
```bash
cd rust && cargo check -p desktop-server 2>&1 | tail -3
```
Expected: clean build (no new code yet, just feature flag).

**Step 4: Commit**
```bash
git add rust/crates/desktop-server/Cargo.toml
git commit -m "chore(deps): enable axum multipart feature for upload handler (E29.A)"
```

---

### Task A2: `upload_wiki_raw_handler` — multipart receive + dispatch to ingest

**Files:**
- Modify: `rust/crates/desktop-server/src/handlers/wiki_crud.rs` (append new handler at bottom)
- Modify: `rust/crates/desktop-server/src/lib.rs` (re-export the new handler if it's `pub(crate)`)
- Modify: `rust/crates/desktop-server/src/routes/wiki.rs` (register `POST /api/wiki/raw/upload`)

**Step 1: Read existing `ingest_wiki_raw_handler` for style reference**

```bash
grep -n -A 40 "ingest_wiki_raw_handler" rust/crates/desktop-server/src/handlers/wiki_crud.rs | head -50
```
Note: use its error shape (`ApiError = (StatusCode, Json<Value>)`), its frontmatter call pattern, and how it returns the entry.

**Step 2: Add the handler**

Append to `rust/crates/desktop-server/src/handlers/wiki_crud.rs`:

```rust
use axum::extract::Multipart;

/// `POST /api/wiki/raw/upload` — accept a file via multipart-form,
/// extract text via `wiki_ingest::markitdown` (with a fast-path for
/// plain text), and write a raw entry. Returns `{ entry, inbox_entry }`.
///
/// The multipart form must contain exactly one field named `file` whose
/// filename has a recognized extension. Optional fields:
/// - `source` (defaults to `"drag-drop"`)
/// - `origin` (defaults to `"desktop drag-drop"`)
pub(crate) async fn upload_wiki_raw_handler(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, ApiError> {
    // 1. Collect form fields
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut file_name: Option<String> = None;
    let mut source = "drag-drop".to_string();
    let mut origin = "desktop drag-drop".to_string();

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("malformed multipart: {e}") })),
        )
    })? {
        match field.name().unwrap_or("") {
            "file" => {
                file_name = field.file_name().map(|s| s.to_string());
                let bytes = field.bytes().await.map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(json!({ "error": format!("read body: {e}") })),
                    )
                })?;
                file_bytes = Some(bytes.to_vec());
            }
            "source" => {
                if let Ok(s) = field.text().await {
                    if !s.trim().is_empty() {
                        source = s.trim().to_string();
                    }
                }
            }
            "origin" => {
                if let Ok(s) = field.text().await {
                    if !s.trim().is_empty() {
                        origin = s.trim().to_string();
                    }
                }
            }
            _ => {
                // ignore extra fields
                let _ = field.bytes().await;
            }
        }
    }

    let bytes = file_bytes.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "missing 'file' field" })),
        )
    })?;
    let file_name = file_name.unwrap_or_else(|| "unnamed.bin".to_string());

    if bytes.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "uploaded file is empty" })),
        ));
    }

    // 2. Stash to temp file (markitdown needs a Path)
    let tmp_dir = std::env::temp_dir();
    let stash_path = tmp_dir.join(format!(
        "buddy-upload-{}-{}",
        std::process::id(),
        sanitize_filename(&file_name)
    ));
    std::fs::write(&stash_path, &bytes).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("write temp: {e}") })),
        )
    })?;

    // 3. Dispatch: plain text shortcut, otherwise markitdown
    let body = {
        let lower = file_name.to_lowercase();
        let res: Result<String, String> = if lower.ends_with(".txt") || lower.ends_with(".md") {
            String::from_utf8(bytes.clone())
                .map_err(|e| format!("not UTF-8: {e}"))
        } else {
            wiki_ingest::markitdown::extract_via_markitdown(&stash_path)
                .await
                .map(|r| format!("# {}\n\n{}", r.title, r.body))
                .map_err(|e| format!("markitdown: {e:?}"))
        };
        let _ = std::fs::remove_file(&stash_path);
        res.map_err(|e| {
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": e, "file_name": file_name })),
            )
        })?
    };

    // 4. Persist via wiki_store
    let paths = state.desktop.wiki_paths().clone();
    let slug_seed = format!("📎 {}", file_name);
    let frontmatter = wiki_store::RawFrontmatter::for_paste(&source, None);

    let entry = wiki_store::write_raw_entry(&paths, &source, &slug_seed, &body, &frontmatter)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("write_raw_entry: {e}") })),
            )
        })?;

    let inbox_entry = wiki_store::append_new_raw_task(&paths, &entry, &origin).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("append_new_raw_task: {e}") })),
        )
    })?;

    Ok(Json(json!({
        "entry": entry,
        "inbox_entry": inbox_entry,
    })))
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
        .collect()
}
```

**Step 3: Register the route**

In `rust/crates/desktop-server/src/routes/wiki.rs`, find where `POST /api/wiki/raw` is registered (look for `ingest_wiki_raw_handler`) and add a sibling route:

```rust
.route("/api/wiki/raw/upload", post(upload_wiki_raw_handler))
```

Also import `upload_wiki_raw_handler` at the top of the file. Pattern: same `use crate::handlers::wiki_crud::...` block.

**Step 4: Verify build + import sanity**
```bash
cd rust && cargo check -p desktop-server 2>&1 | tail -10
```
Expected: clean build. If `state.desktop.wiki_paths()` is wrong, check the existing `ingest_wiki_raw_handler` for the correct accessor name (might be `paths_snapshot()` or similar).

**Step 5: Commit**
```bash
git add rust/crates/desktop-server/src/handlers/wiki_crud.rs \
        rust/crates/desktop-server/src/routes/wiki.rs \
        rust/crates/desktop-server/src/lib.rs
git commit -m "feat(api): POST /api/wiki/raw/upload multipart file ingest (E29.A)"
```

---

### Task A3: Capability probe — does this endpoint expose markitdown env?

**Files:**
- Modify: `rust/crates/desktop-server/src/handlers/desktop_storage.rs` (the line that returns `supported_formats`) — verify it works alongside the new endpoint
- No code change expected — this is a verification task

**Step 1: Curl the existing storage capability endpoint** (after starting dev)
```bash
curl -s http://127.0.0.1:4357/api/desktop/storage/capabilities | python -m json.tool
```
Expected: a JSON object that already includes `"supported_formats": ["pdf", "docx", ...]`. This is what the frontend will use to set the `accept=` attribute on the drop zone.

**Step 2: If endpoint doesn't exist or doesn't expose `supported_formats`, take notes**
Add a note to this plan's "Implementation Notes" section. Otherwise: nothing to do.

**Step 3: No commit** (verification-only)

---

## Phase B — Frontend Drop Zone

### Task B1: `uploadInboxFile` API client

**Files:**
- Modify: `apps/desktop-shell/src/api/wiki/repository.ts` (add new function near `listInboxEntries`)

**Step 1: Read the existing fetchJson helper used by this file**
```bash
grep -n "fetchJson\|fetch(" apps/desktop-shell/src/api/wiki/repository.ts | head -10
```
Note its signature so the new function reuses the same error-handling pattern.

**Step 2: Add `uploadInboxFile`**

Append near `listInboxEntries`:

```typescript
export interface UploadInboxFileResponse {
  entry: RawEntry;
  inbox_entry: InboxEntry;
}

/**
 * POST /api/wiki/raw/upload — multipart file upload that ingests
 * any markitdown-supported format (PDF/DOCX/PPT/XLSX/images/etc.)
 * plus plain .txt/.md, writes a raw entry, and queues it for inbox
 * review.
 */
export async function uploadInboxFile(
  file: File,
  opts?: { source?: string; origin?: string },
): Promise<UploadInboxFileResponse> {
  const fd = new FormData();
  fd.append("file", file, file.name);
  if (opts?.source) fd.append("source", opts.source);
  if (opts?.origin) fd.append("origin", opts.origin);

  // Use the same base URL the other repo calls use.
  // `fetchJson` is for JSON requests — for multipart we go direct.
  const resp = await fetch("/api/wiki/raw/upload", {
    method: "POST",
    body: fd,
  });
  if (!resp.ok) {
    let detail = "";
    try {
      const j = (await resp.json()) as { error?: string };
      detail = j.error ?? "";
    } catch {
      detail = await resp.text();
    }
    throw new Error(`Upload failed (${resp.status}): ${detail}`);
  }
  return (await resp.json()) as UploadInboxFileResponse;
}
```

**Step 3: TypeScript check**
```bash
cd apps/desktop-shell && npx tsc --noEmit 2>&1 | tail -5
```
Expected: clean. If `RawEntry` / `InboxEntry` types aren't already exported from this file, add the imports.

**Step 4: Commit**
```bash
git add apps/desktop-shell/src/api/wiki/repository.ts
git commit -m "feat(api-client): uploadInboxFile multipart client (E29.B)"
```

---

### Task B2: `InboxDropZone` component

**Files:**
- Create: `apps/desktop-shell/src/features/inbox/InboxDropZone.tsx`

**Step 1: Write the component**

```tsx
import { useState, useCallback } from "react";
import { Upload, Loader2, FileCheck2 } from "lucide-react";
import { toast } from "sonner";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { uploadInboxFile } from "@/api/wiki/repository";
import { inboxKeys } from "./query-keys"; // verify exact key name in Step 1.5

/**
 * Drop zone for ingesting any file into the inbox. Sits at the top
 * of the Inbox list. Accepts ANY file; rejection happens server-side
 * based on extension and markitdown availability.
 *
 * Visual states:
 *   - idle:    dashed border, "拖入文件入库"
 *   - dragover: solid border, primary tint
 *   - uploading: spinner, file name
 *   - success: brief checkmark flash (handled by toast)
 */
export function InboxDropZone() {
  const queryClient = useQueryClient();
  const [isDragOver, setIsDragOver] = useState(false);

  const upload = useMutation({
    mutationFn: (file: File) => uploadInboxFile(file, { origin: "drag-drop" }),
    onSuccess: (data) => {
      toast.success(`已入库 #${String(data.entry.id).padStart(5, "0")}`, {
        description: data.entry.filename,
      });
      void queryClient.invalidateQueries({ queryKey: inboxKeys.list() });
    },
    onError: (err) => {
      toast.error("入库失败", { description: err instanceof Error ? err.message : String(err) });
    },
  });

  const handleDrop = useCallback(
    (e: React.DragEvent<HTMLDivElement>) => {
      e.preventDefault();
      setIsDragOver(false);
      const files = Array.from(e.dataTransfer.files);
      if (files.length === 0) return;
      // Sequential upload — keeps server load and toast UX sane.
      void (async () => {
        for (const file of files) {
          await upload.mutateAsync(file);
        }
      })();
    },
    [upload],
  );

  return (
    <div
      onDragOver={(e) => {
        e.preventDefault();
        setIsDragOver(true);
      }}
      onDragLeave={() => setIsDragOver(false)}
      onDrop={handleDrop}
      className={[
        "flex items-center gap-3 rounded-lg border border-dashed px-4 py-3 transition-colors",
        isDragOver
          ? "border-primary bg-primary/5"
          : "border-border bg-card/50 hover:bg-card",
      ].join(" ")}
    >
      <div className="grid size-9 shrink-0 place-items-center rounded-full bg-muted">
        {upload.isPending ? (
          <Loader2 className="size-4 animate-spin text-muted-foreground" />
        ) : upload.isSuccess ? (
          <FileCheck2 className="size-4 text-emerald-600" />
        ) : (
          <Upload className="size-4 text-muted-foreground" />
        )}
      </div>
      <div className="min-w-0 flex-1">
        <div className="text-[13px] font-medium text-foreground">
          {upload.isPending ? "正在入库…" : "拖入文件入库"}
        </div>
        <div className="mt-0.5 text-[11px] text-muted-foreground">
          {upload.isPending
            ? upload.variables?.name
            : "支持 PDF / DOCX / PPT / XLSX / 图片 / 音视频 / HTML / 文本"}
        </div>
      </div>
    </div>
  );
}
```

**Step 1.5: Verify `inboxKeys` import path**

```bash
grep -rn "export const inboxKeys\|export.*inboxKeys" apps/desktop-shell/src/features/inbox/
```
If `inboxKeys` lives in `InboxPage.tsx` itself rather than a separate `query-keys.ts`, adjust the import — or hoist it out into a shared module. Easiest: if it's inline in `InboxPage.tsx`, export it from there and import `from "./InboxPage"`.

**Step 2: Verify TS compiles**
```bash
cd apps/desktop-shell && npx tsc --noEmit 2>&1 | tail -5
```

**Step 3: Commit**
```bash
git add apps/desktop-shell/src/features/inbox/InboxDropZone.tsx
git commit -m "feat(inbox): InboxDropZone drag-and-drop file ingest component (E29.B)"
```

---

### Task B3: Mount `InboxDropZone` in `InboxPage`

**Files:**
- Modify: `apps/desktop-shell/src/features/inbox/InboxPage.tsx`

**Step 1: Read the area where the inbox list is rendered**

```bash
grep -n "listQuery\.data\|InboxEntry\|<ul\|<div className.*entries" apps/desktop-shell/src/features/inbox/InboxPage.tsx | head -10
```
Pick a spot immediately ABOVE the list — typically inside the page wrapper but before the entries map.

**Step 2: Add import + render**

Add to imports:
```typescript
import { InboxDropZone } from "./InboxDropZone";
```

Render the zone just before the entries list (replace the appropriate line):
```tsx
<div className="space-y-3">
  <InboxDropZone />
  {/* existing list rendering */}
</div>
```

(The exact JSX context depends on the existing layout — keep the change minimal and avoid restructuring.)

**Step 3: Verify build**
```bash
cd apps/desktop-shell && npm run build 2>&1 | tail -3
```
Expected: `✓ built in N.NNs`.

**Step 4: Commit**
```bash
git add apps/desktop-shell/src/features/inbox/InboxPage.tsx
git commit -m "feat(inbox): mount drop zone at top of Inbox list (E29.B)"
```

---

## Phase C — WeChat Bot Redirect Reply + T1.1 Debug Cleanup

### Task C1: Update iLink rejection message + remove debug log

**Files:**
- Modify: `rust/crates/desktop-core/src/wechat_ilink/desktop_handler.rs:284-320`

**Step 1: Replace the entire `_ =>` arm** (which currently contains both the T1.1 debug dump AND the rejection reply)

Find:
```rust
            _ => {
                // ── E29.T1.1 DEBUG CAPTURE — REMOVE AFTER PROTOCOL VERIFIED ──
                // ... (the JSON dump block) ...
                // ── END DEBUG CAPTURE ──

                // Non-text message (image/voice/file). For Phase 2b we don't
                // support these — reply with a hint and move on.
                let reply = build_text_reply(
                    &from_user_id,
                    &context_token,
                    "（暂不支持非文本消息，请发送文字）",
                );
                ...
                return Ok(());
            }
```

Replace with:
```rust
            _ => {
                // Non-text message (file / image / voice / video). We don't
                // attempt the iLink CDN download path — the protocol isn't
                // documented in this codebase (see plan
                // 2026-05-12-wechat-file-ingestion-plan.md for the gate
                // findings). Instead, guide the user toward Buddy's
                // native drag-drop ingest, which handles 28+ file formats
                // via markitdown.
                let reply = build_text_reply(
                    &from_user_id,
                    &context_token,
                    "（暂不支持微信直接发文件 —— 请把文件拖入 Buddy 桌面应用的 Inbox 区域，支持 PDF/DOCX/PPT/XLSX/图片/音视频 等格式）",
                );
                if let Err(e) = send_message_with_retry(client, reply, "non-text-redirect").await {
                    eprintln!("[wechat agent] reply send failed: {e}");
                }
                return Ok(());
            }
```

**Step 2: Verify build**
```bash
cd rust && cargo check -p desktop-core 2>&1 | tail -3
```

**Step 3: Commit**
```bash
git add rust/crates/desktop-core/src/wechat_ilink/desktop_handler.rs
git commit -m "feat(wechat-ilink): redirect non-text messages to drag-drop hint (E29.C)"
```

---

## Phase D — Smoke + Ship

### Task D1: End-to-end smoke

**Step 1: Build + run dev**
```bash
cd /d "D:/Users/111/Desktop/Project/Claude Desktop/buddy/apps/desktop-shell" && npm run tauri:dev
```

**Step 2: Test the drag-drop flow**
1. Open Buddy → Inbox
2. Drag a `.txt` file onto the drop zone → see "正在入库…" → toast "已入库 #NNNNN"
3. New entry appears in the list, click it → confirm body is the file's text
4. Repeat with a small `.pdf` (needs Python + markitdown installed — check via `python -c "import markitdown"`)
5. Repeat with `.docx`
6. Drag a file with weird name `测试文档.pdf` → verify filename gets sanitized for temp file but original name preserved in entry

**Step 3: Test the WeChat redirect**
1. From WeChat, send any non-text message (image / voice / file) to MyAgent
2. Expect new reply: `（暂不支持微信直接发文件 —— 请把文件拖入 Buddy 桌面应用的 Inbox 区域，支持 PDF/DOCX/PPT/XLSX/图片/音视频 等格式）`
3. Console should NOT contain `[E29.T1.1 DEBUG]` markers (the debug log is gone)

**Step 4: Edge cases**
- Drop multiple files at once → sequential upload, multiple toasts
- Drop a file with no extension → server returns 422, toast shows error
- Drop while Python is uninstalled → graceful "markitdown:" error toast (test by setting `PATH=` to exclude python temporarily — or skip if too fiddly)

**Step 5: Record smoke results in plan**
Append findings (which formats worked, any UX issues) to the "Smoke Log" section.

**Step 6: Commit smoke notes if any**
```bash
git add docs/desktop-shell/plans/2026-05-12-buddy-file-drag-drop-ingest-plan.md
git commit -m "docs(plan): record E29 drag-drop smoke results"
```

---

### Task D2: Plan README + v0.1.17 bump + tag + push

**Files:**
- Modify: `docs/desktop-shell/plans/README.md` (add E29 link, mark superseded plan)
- Modify: `apps/desktop-shell/src-tauri/tauri.conf.json` (0.1.16 → 0.1.17)
- Modify: `apps/desktop-shell/src-tauri/Cargo.toml` (0.1.16 → 0.1.17)

**Step 1: README update**

Append to `docs/desktop-shell/plans/README.md` after the E28 entry:
```markdown
- [Buddy Desktop File Drag-Drop Ingest (E29) Implementation Plan](./2026-05-12-buddy-file-drag-drop-ingest-plan.md)
- [WeChat File Ingestion (E29 — superseded) Discovery](./2026-05-12-wechat-file-ingestion-plan.md)
```

Also update the superseded plan's `status:` frontmatter from `draft` to `superseded` (already done in plan file frontmatter `supersedes:` from this plan, but verify the other end is consistent).

**Step 2: Version bump**

Edit both files: `0.1.16` → `0.1.17`.

**Step 3: cargo check to refresh Cargo.lock**
```bash
cd apps/desktop-shell/src-tauri && cargo check 2>&1 | tail -2
```

**Step 4: Commit + tag + push**
```bash
git add apps/desktop-shell/src-tauri/{tauri.conf.json,Cargo.toml,Cargo.lock} \
        docs/desktop-shell/plans/README.md \
        docs/desktop-shell/plans/2026-05-12-wechat-file-ingestion-plan.md
git commit -m "$(cat <<'EOF'
chore: bump desktop-shell to v0.1.17 + Buddy drag-drop ingest plan

E29 — pivoted from iLink CDN file decryption (gate hit: protocol
undocumented in repo, network probing exhausted) to a native
drag-drop ingest in Buddy's Inbox. New POST /api/wiki/raw/upload
multipart endpoint dispatches to wiki_ingest::markitdown for 28+
file formats; the WeChat bot's non-text reply now redirects users to
this path.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git tag -a v0.1.17 -m "v0.1.17 — Buddy File Drag-Drop Ingest (E29)"
git push origin main
git push origin v0.1.17
```

---

## Risk register

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| Python / markitdown not installed on user's machine | High | Plain-text shortcut for `.txt`/`.md` works without Python; for other formats, the 422 error includes the install hint ("`pip install 'markitdown[all]'`"); future task: surface a one-click setup banner |
| `wiki_paths()` accessor name wrong on `state.desktop` | Medium | Task A2 Step 4 catches at `cargo check`; consult existing `ingest_wiki_raw_handler` for the actual accessor |
| `inboxKeys` not exported from a shared location | Low | Task B2 Step 1.5 verifies + hoists if needed |
| Large file (>10 MiB) causes markitdown timeout | Low | Worker has `TIMEOUT_SECS = 120`; UX: toast shows "正在入库…" with file name; future task: progress bar |
| Multipart parser doesn't enforce max body size | Medium | axum 0.8 default 2 MiB body limit — extend with `DefaultBodyLimit::max(50 * 1024 * 1024)` on the route. Add to Task A2. |
| User drops 100 files at once | Low | Sequential upload in component; worst case slow but recoverable |

---

## Implementation Notes
*To be filled in by execution.*

## Smoke Log
*To be filled in by Task D1.*

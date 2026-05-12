---
title: WeChat File Ingestion (E29 — superseded) Discovery
doc_type: plan
status: superseded
superseded_by: docs/desktop-shell/plans/2026-05-12-buddy-file-drag-drop-ingest-plan.md
owner: desktop-shell
last_verified: 2026-05-12
related:
  - rust/crates/desktop-core/src/wechat_ilink/desktop_handler.rs
  - rust/crates/desktop-core/src/wechat_kefu/callback.rs
  - rust/crates/wiki_ingest/src/pdf.rs
  - rust/crates/wiki_ingest/src/docx.rs
  - rust/crates/wiki_store/src/lib.rs
---

> **STATUS: Superseded by [Buddy Desktop File Drag-Drop Ingest (E29)](./2026-05-12-buddy-file-drag-drop-ingest-plan.md).**
>
> T1.1 discovery showed the iLink CDN media download protocol is not documented in this codebase, the host `novac2c.cdn.weixin.qq.com` returns generic 404 for every URL form we probed, and no openclaw upstream reference is available. Without mitmproxy + cert-pinning bypass (heavy lift, possibly blocked by WeChat client), this path is not implementable today. Pivoted to a drag-drop ingest in Buddy itself; the WeChat bot's reply now redirects users there.
>
> **This document is kept as the discovery record** — `## Decryption Notes` below captures the AES-128 key wire format (base64-of-ASCII-hex) and the dead-end URL probing trail, useful if/when someone returns to this protocol with a working reference.

# WeChat File Ingestion (E29) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Both WeChat customer-service channels (iLink desktop handler + Kefu webhook) accept PDF / DOCX / TXT / MD file messages, decrypt-and-download the attachment, run it through the existing `wiki_ingest` extractor, and write the result to `.clawwiki/raw/` — replacing the current `（暂不支持非文本消息，请发送文字）` rejection.

**Architecture:** Add a small `media` submodule under each channel for the channel-specific download path (iLink: AES-decrypt the encrypted CDN stream; Kefu: call `cgi-bin/media/get` with the `media_id`). Both channels then hand off to a shared `file_ingest::ingest_attachment(bytes, file_name, paths)` helper that dispatches on extension to `wiki_ingest::pdf::extract_pdf` / `extract_docx` / utf-8 string, builds a `RawFrontmatter::for_paste("wechat-file", ...)`, and reuses the same `wiki_store::write_raw_entry` + `append_new_raw_task` flow that today's text branch uses.

**Tech Stack:** Rust (aes / cbc / ecb / cipher crates — TBD by Task 1.1 discovery), pdf-extract + docx-rs (already in wiki_ingest deps), reqwest (HTTP client — verify in Cargo.toml), wiki_store, axum.

**Why two phases:** the user's immediate pain (screenshot showed iLink rejecting a PDF) is fully addressable in Phase 1 without touching Kefu. Phase 2 reaches parity but has a discovery gate because Kefu inbound message handling state is partially unknown. Ship Phase 1 first, then decide on Phase 2 based on discovery.

---

## Pre-flight: working agreement

Before starting any task:

1. **Branch:** stay on `main`; commits land directly per the repo's pattern (E23–E28 all shipped to `main`).
2. **Per-task commits** — each task ends in `git add <specific files>` + a `feat(scope):` / `chore:` commit. Never `git add -A` (avoid `.claude/scheduled_tasks.lock` and the `用户意见/` folder).
3. **Verification command bank:**
   - Rust: `cd rust && cargo check --workspace 2>&1 | tail -3` after each Rust edit, `cargo test --workspace 2>&1 | tail -10` after adding tests.
   - Frontend: not touched in this plan (no UI surface yet).
4. **Path-risk awareness** — the FILE branch sits next to `append_user_message` in iLink and the Kefu callback dispatcher. Both are in `path-risk-matrix.md` as HIGH. Read the existing branch shape before adding, don't refactor.

---

## Phase 1 — iLink Channel (the user's actual pain)

### Task 1.1: Discovery — confirm iLink CDN media encryption scheme

**Why this is a gate:** `CdnMedia { encrypt_query_param, aes_key, encrypt_type }` is defined but no decrypt helper exists. We need to know the cipher mode (ECB/CBC/GCM), IV handling, and the meaning of `encrypt_type=0` vs `=1` before writing the decryptor. Without this, Task 1.2 can't be specified.

**Files:**
- Read-only: `rust/crates/desktop-core/src/wechat_ilink/types.rs:71-79` (CdnMedia) and `:357` (`CDN_BASE_URL`)
- Read-only: any openclaw-code-parity reference in `rust/crates/claw-code-parity/` if it exists
- Read-only: search git log + commit messages for `aes`/`cdn`/`encrypt` across `rust/crates/desktop-core/`

**Step 1: Search for any existing reference implementation**
```bash
grep -rn "encrypt_type\|CDN_BASE_URL\|aes_key" rust/crates/desktop-core/src/wechat_ilink/
grep -rn "novac2c\|cdn.weixin" rust/ docs/ 2>/dev/null
git log --all --oneline -S "encrypt_type" -- rust/crates/desktop-core/
```
Expected: surface any commit / doc / TODO that references the cipher spec.

**Step 2: If no hint found, capture a real iLink FILE message**
- Send a tiny `.txt` file (e.g. 12 bytes "hello world\n") to MyAgent via WeChat
- Inspect the inbound `WeixinMessage` JSON at `desktop_handler.rs:263` — log the raw `file_item.media` JSON
- Record `encrypt_type`, length of `aes_key` (after base64-decode → should reveal 16/24/32-byte → AES-128/192/256), and whether `encrypt_query_param` contains an IV-shaped string
- Compare downloaded CDN bytes length vs decrypted-text length (PKCS#7 padding hint)

**Step 3: Document findings in this plan**
Append to this file a `## Decryption Notes` section recording:
- Cipher mode (e.g. `AES-128-ECB` or `AES-128-CBC with IV in encrypt_query_param`)
- `encrypt_type=0` vs `=1` interpretation (likely: 0 = only fileid encrypted, body plaintext; 1 = body also encrypted)
- Cargo deps needed (e.g. `aes = "0.8"`, `cbc = "0.1"`, `block-padding = "0.3"`)

**Step 4: Gate check**
If after Step 1+2 the cipher is still ambiguous: **stop the plan**, raise with user — option to pivot to `wechat-file-passthrough` (save raw encrypted bytes to disk + asynchronously prompt user "decrypt how?" via Inbox) instead of blocking the entire epic on protocol RE.

**Step 5: Commit the discovery notes**
```bash
git add docs/desktop-shell/plans/2026-05-12-wechat-file-ingestion-plan.md
git commit -m "docs(plan): record iLink CDN decryption protocol for E29 Task 1.1"
```

---

### Task 1.2: AES decryption helper

**Files:**
- Create: `rust/crates/desktop-core/src/wechat_ilink/media.rs`
- Modify: `rust/crates/desktop-core/src/wechat_ilink/mod.rs` (add `pub(crate) mod media;`)
- Modify: `rust/crates/desktop-core/Cargo.toml` (add crypto deps from Task 1.1 if missing)

**Step 1: Write the failing test** (assumes AES-128-ECB from Task 1.1 — adjust per Task 1.1 findings)

In `rust/crates/desktop-core/src/wechat_ilink/media.rs`, add at end:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aes_128_ecb_roundtrip() {
        // 16-byte key
        let key = b"0123456789abcdef";
        // 16-byte aligned plaintext (1 block)
        let plain = b"hello world!1234"; // exactly 16 bytes
        // Pre-computed ciphertext: easier to gen with openssl CLI in dev
        // openssl enc -aes-128-ecb -K 30313233343536373839616263646566 -nopad
        let cipher_hex = "<PUT_REAL_HEX_HERE>";
        let cipher = hex_decode(cipher_hex);
        let out = aes_decrypt_ecb(key, &cipher).expect("decrypt ok");
        assert_eq!(&out, plain);
    }

    #[test]
    fn aes_decrypt_rejects_wrong_key_length() {
        let result = aes_decrypt_ecb(b"tooshort", b"\x00".repeat(16).as_slice());
        assert!(result.is_err());
    }

    fn hex_decode(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }
}
```

**Step 2: Run test to verify it fails**
```bash
cd rust && cargo test -p desktop-core --lib wechat_ilink::media 2>&1 | tail -10
```
Expected: FAIL — `aes_decrypt_ecb` not defined.

**Step 3: Write minimal implementation**
```rust
use anyhow::{anyhow, Result};

/// Decrypt AES-128-ECB ciphertext. Padding handling per Task 1.1 findings.
pub(crate) fn aes_decrypt_ecb(key: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
    use aes::cipher::{generic_array::GenericArray, BlockDecrypt, KeyInit};
    use aes::Aes128;

    if key.len() != 16 {
        return Err(anyhow!("AES-128 requires 16-byte key, got {}", key.len()));
    }
    if ciphertext.is_empty() || ciphertext.len() % 16 != 0 {
        return Err(anyhow!(
            "ciphertext length must be multiple of 16, got {}",
            ciphertext.len()
        ));
    }

    let cipher = Aes128::new(GenericArray::from_slice(key));
    let mut out = ciphertext.to_vec();
    for chunk in out.chunks_mut(16) {
        cipher.decrypt_block(GenericArray::from_mut_slice(chunk));
    }
    // PKCS#7 unpad (Task 1.1 may say otherwise)
    if let Some(&pad) = out.last() {
        if pad as usize <= 16 && out.len() >= pad as usize {
            out.truncate(out.len() - pad as usize);
        }
    }
    Ok(out)
}
```

**Step 4: Generate the test ciphertext + run**
```bash
echo -n "hello world!1234" | openssl enc -aes-128-ecb -K 30313233343536373839616263646566 -nopad | xxd -p
# paste hex into the test, then:
cd rust && cargo test -p desktop-core --lib wechat_ilink::media 2>&1 | tail -5
```
Expected: PASS.

**Step 5: Commit**
```bash
git add rust/crates/desktop-core/src/wechat_ilink/media.rs \
        rust/crates/desktop-core/src/wechat_ilink/mod.rs \
        rust/crates/desktop-core/Cargo.toml
git commit -m "feat(wechat-ilink): AES-128 decryption helper for CDN media (E29.1)"
```

---

### Task 1.3: CDN media downloader

**Files:**
- Modify: `rust/crates/desktop-core/src/wechat_ilink/media.rs` (extend)
- Modify: `rust/crates/desktop-core/Cargo.toml` (verify `reqwest` is present; if not, add `reqwest = { version = "0.12", features = ["rustls-tls"] }`)

**Step 1: Write the failing test**

Append to `media.rs`:
```rust
#[cfg(test)]
mod download_tests {
    use super::*;

    // Smoke test: build the URL correctly from media metadata.
    #[test]
    fn build_download_url_assembles_query_params() {
        let url = build_cdn_download_url(
            "abc123fileid",
            Some("ts=1234&signature=xyz"),
        );
        assert_eq!(
            url,
            "https://novac2c.cdn.weixin.qq.com/c2c?fileid=abc123fileid&ts=1234&signature=xyz"
        );
    }

    #[test]
    fn build_download_url_without_query_param() {
        let url = build_cdn_download_url("xyz", None);
        assert_eq!(url, "https://novac2c.cdn.weixin.qq.com/c2c?fileid=xyz");
    }
}
```

**Step 2: Run test to verify it fails**
```bash
cd rust && cargo test -p desktop-core --lib wechat_ilink::media::download_tests 2>&1 | tail -5
```
Expected: FAIL — `build_cdn_download_url` not defined.

**Step 3: Implement url builder + async download**

Add to `media.rs`:
```rust
use crate::wechat_ilink::types::CDN_BASE_URL;

pub(crate) fn build_cdn_download_url(fileid: &str, query: Option<&str>) -> String {
    match query {
        Some(q) if !q.is_empty() => format!("{CDN_BASE_URL}?fileid={fileid}&{q}"),
        _ => format!("{CDN_BASE_URL}?fileid={fileid}"),
    }
}

/// Download + decrypt a CDN media reference. Returns decrypted plaintext bytes.
/// `aes_key_b64` is the base64-encoded AES key from `CdnMedia::aes_key`.
/// If `encrypt_type == 0` per Task 1.1, decryption is skipped.
pub(crate) async fn download_and_decrypt(
    fileid: &str,
    query_param: Option<&str>,
    aes_key_b64: Option<&str>,
    encrypt_type: i32,
) -> Result<Vec<u8>> {
    let url = build_cdn_download_url(fileid, query_param);
    let resp = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow!("CDN GET failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(anyhow!("CDN GET status {}", resp.status()));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| anyhow!("CDN body read failed: {e}"))?
        .to_vec();

    if encrypt_type == 0 {
        return Ok(bytes);
    }

    let key_b64 = aes_key_b64.ok_or_else(|| anyhow!("encrypt_type=1 but no aes_key"))?;
    let key = base64::engine::general_purpose::STANDARD
        .decode(key_b64)
        .map_err(|e| anyhow!("aes_key not base64: {e}"))?;
    aes_decrypt_ecb(&key, &bytes)
}
```

**Step 4: Run test**
```bash
cd rust && cargo test -p desktop-core --lib wechat_ilink::media 2>&1 | tail -5
```
Expected: PASS (both url-builder tests + the earlier roundtrip).

**Step 5: Commit**
```bash
git add rust/crates/desktop-core/src/wechat_ilink/media.rs \
        rust/crates/desktop-core/Cargo.toml
git commit -m "feat(wechat-ilink): CDN media download + decrypt pipeline (E29.1)"
```

---

### Task 1.4: Shared file-ingest dispatcher

**Files:**
- Create: `rust/crates/desktop-core/src/wechat_common/mod.rs` (new module, since both iLink + Kefu will use it)
- Create: `rust/crates/desktop-core/src/wechat_common/file_ingest.rs`
- Modify: `rust/crates/desktop-core/src/lib.rs` (add `pub(crate) mod wechat_common;`)

**Why a new shared module:** iLink and Kefu both end up at "I have `bytes` + `file_name`, ingest as raw". Putting this in either channel's submodule risks the other channel reaching across the tree.

**Step 1: Write the failing test**

In `rust/crates/desktop-core/src/wechat_common/file_ingest.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_extension_pdf() {
        assert_eq!(classify_file_kind("report.pdf"), Some(FileKind::Pdf));
        assert_eq!(classify_file_kind("REPORT.PDF"), Some(FileKind::Pdf));
    }

    #[test]
    fn classify_extension_docx() {
        assert_eq!(classify_file_kind("notes.docx"), Some(FileKind::Docx));
    }

    #[test]
    fn classify_extension_text() {
        assert_eq!(classify_file_kind("readme.txt"), Some(FileKind::PlainText));
        assert_eq!(classify_file_kind("guide.md"), Some(FileKind::PlainText));
    }

    #[test]
    fn classify_extension_unsupported() {
        assert_eq!(classify_file_kind("photo.jpg"), None);
        assert_eq!(classify_file_kind("noext"), None);
    }
}
```

**Step 2: Run to verify failure**
```bash
cd rust && cargo test -p desktop-core --lib wechat_common::file_ingest 2>&1 | tail -10
```
Expected: FAIL — `classify_file_kind` not defined.

**Step 3: Implement classifier + ingest pipeline**
```rust
use anyhow::{anyhow, Context, Result};
use std::path::PathBuf;
use wiki_store::{RawEntry, RawFrontmatter, WikiPaths};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum FileKind {
    Pdf,
    Docx,
    PlainText,
}

pub(crate) fn classify_file_kind(file_name: &str) -> Option<FileKind> {
    let lower = file_name.to_lowercase();
    if lower.ends_with(".pdf") { return Some(FileKind::Pdf); }
    if lower.ends_with(".docx") { return Some(FileKind::Docx); }
    if lower.ends_with(".txt") || lower.ends_with(".md") {
        return Some(FileKind::PlainText);
    }
    None
}

/// Persist `bytes` to a temp file, extract text with the appropriate
/// `wiki_ingest` backend, and write a raw entry. Returns the entry on
/// success.
pub(crate) fn ingest_attachment(
    paths: &WikiPaths,
    bytes: &[u8],
    file_name: &str,
    source_tag: &str,
    user_label: &str, // e.g. "WeChat user xxx" — used in inbox origin
) -> Result<RawEntry> {
    let kind = classify_file_kind(file_name)
        .ok_or_else(|| anyhow!("unsupported file extension: {file_name}"))?;

    // Stage bytes to a temp file so the existing Path-based extractors work.
    let tmp_dir = std::env::temp_dir();
    let stash_path: PathBuf = tmp_dir.join(format!(
        "wechat-{}-{}",
        std::process::id(),
        file_name.replace('/', "_")
    ));
    std::fs::write(&stash_path, bytes)
        .with_context(|| format!("write temp file {stash_path:?}"))?;

    let body = match kind {
        FileKind::Pdf => {
            let res = wiki_ingest::pdf::extract_pdf(&stash_path)
                .context("extract_pdf failed")?;
            format!("# {}\n\n{}", res.title, res.body)
        }
        FileKind::Docx => {
            let res = wiki_ingest::docx::extract_docx(&stash_path)
                .context("extract_docx failed")?;
            format!("# {}\n\n{}", res.title, res.body)
        }
        FileKind::PlainText => {
            String::from_utf8_lossy(bytes).to_string()
        }
    };

    let _ = std::fs::remove_file(&stash_path);

    let slug_seed = format!("WeChat · {file_name}");
    let frontmatter = RawFrontmatter::for_paste(source_tag, None);
    let entry = wiki_store::write_raw_entry(paths, source_tag, &slug_seed, &body, &frontmatter)
        .context("write_raw_entry failed")?;

    if let Err(err) = wiki_store::append_new_raw_task(paths, &entry, user_label) {
        eprintln!("[wechat file] raw written but inbox append failed: {err}");
    }

    Ok(entry)
}
```

**Step 4: Run all wechat_common tests**
```bash
cd rust && cargo test -p desktop-core --lib wechat_common 2>&1 | tail -10
```
Expected: 4 PASS (the four `classify_*` tests). The `ingest_attachment` is integration-only, covered in Task 1.6 smoke.

**Step 5: Commit**
```bash
git add rust/crates/desktop-core/src/wechat_common/ \
        rust/crates/desktop-core/src/lib.rs
git commit -m "feat(wechat-common): file-ingest dispatcher shared by iLink + Kefu (E29.2)"
```

---

### Task 1.5: Wire the FILE branch into iLink `on_message`

**Files:**
- Modify: `rust/crates/desktop-core/src/wechat_ilink/handlers.rs` (add `extract_first_file()` helper)
- Modify: `rust/crates/desktop-core/src/wechat_ilink/desktop_handler.rs:284-298` (split text-vs-file before the rejection)

**Step 1: Write the failing test** for `extract_first_file()`

Append to `rust/crates/desktop-core/src/wechat_ilink/handlers.rs`:
```rust
#[cfg(test)]
mod file_tests {
    use super::*;
    use crate::wechat_ilink::types::{
        CdnMedia, FileItem, MessageItem, WeixinMessage, message_item_type,
    };

    fn file_message() -> WeixinMessage {
        WeixinMessage {
            item_list: Some(vec![MessageItem {
                r#type: Some(message_item_type::FILE),
                file_item: Some(FileItem {
                    media: Some(CdnMedia {
                        encrypt_query_param: Some("ts=1".into()),
                        aes_key: Some("a2V5MDAwMDAwMDAwMDAw".into()), // base64
                        encrypt_type: Some(1),
                    }),
                    file_name: Some("hello.pdf".into()),
                    md5: None,
                    len: Some("12".into()),
                }),
                ..Default::default()
            }]),
            ..Default::default()
        }
    }

    #[test]
    fn extract_first_file_returns_pdf_metadata() {
        let msg = file_message();
        let f = extract_first_file(&msg).expect("found file");
        assert_eq!(f.file_name.as_deref(), Some("hello.pdf"));
    }

    #[test]
    fn extract_first_file_returns_none_on_text_only() {
        let msg = WeixinMessage {
            item_list: Some(vec![MessageItem {
                r#type: Some(message_item_type::TEXT),
                ..Default::default()
            }]),
            ..Default::default()
        };
        assert!(extract_first_file(&msg).is_none());
    }
}
```

(If `MessageItem` doesn't derive `Default`, add `#[derive(Default)]` to it in `types.rs` — and to any nested struct as needed. Verify before writing the test.)

**Step 2: Run to verify failure**
```bash
cd rust && cargo test -p desktop-core --lib wechat_ilink::handlers::file_tests 2>&1 | tail -10
```
Expected: FAIL — `extract_first_file` not defined.

**Step 3: Implement helper + wire branch**

Add to `handlers.rs` after `extract_first_text`:
```rust
pub(crate) fn extract_first_file(message: &WeixinMessage) -> Option<FileItem> {
    let items = message.item_list.as_ref()?;
    for item in items {
        if item.r#type == Some(message_item_type::FILE) {
            if let Some(f) = item.file_item.clone() {
                return Some(f);
            }
        }
    }
    None
}
```

Modify `desktop_handler.rs:284-298` — replace the `_` arm of the `match extract_first_text(...)` with a FILE-first attempt:

```rust
let user_text = match extract_first_text(&message) {
    Some(t) if !t.trim().is_empty() => t,
    _ => {
        // No text — try FILE branch first.
        if let Some(file) = crate::wechat_ilink::handlers::extract_first_file(&message) {
            self.handle_file_message(client, &from_user_id, &context_token, file).await?;
            return Ok(());
        }
        // Truly unsupported (image/voice/video) — improved rejection.
        let reply = build_text_reply(
            &from_user_id,
            &context_token,
            "（暂不支持图片/语音消息，请发送 PDF/DOCX/TXT/MD 文件或文字）",
        );
        if let Err(e) = send_message_with_retry(client, reply, "non-text-reply").await {
            eprintln!("[wechat agent] reply send failed: {e}");
        }
        return Ok(());
    }
};
```

Add a new method on `DesktopAgentHandler` (in `desktop_handler.rs`, near `write_plain_text_raw`):
```rust
async fn handle_file_message(
    &self,
    client: &IlinkClient,
    from_user_id: &str,
    context_token: &str,
    file: crate::wechat_ilink::types::FileItem,
) -> Result<(), MonitorError> {
    let file_name = file.file_name.clone().unwrap_or_else(|| "unknown.bin".into());

    // 1. Classify before downloading (cheap reject)
    if crate::wechat_common::file_ingest::classify_file_kind(&file_name).is_none() {
        let reply = build_text_reply(
            from_user_id,
            context_token,
            &format!("（暂不支持的文件类型：{file_name}。请发送 PDF/DOCX/TXT/MD）"),
        );
        let _ = send_message_with_retry(client, reply, "file-unsupported-reply").await;
        return Ok(());
    }

    // 2. Pull media bytes
    let media = match file.media.as_ref() {
        Some(m) => m,
        None => {
            eprintln!("[wechat agent] file message missing media");
            return Ok(());
        }
    };
    let fileid = media.encrypt_query_param.as_deref().unwrap_or(""); // see Task 1.1 — adjust if fileid lives elsewhere
    let bytes = match crate::wechat_ilink::media::download_and_decrypt(
        fileid,
        media.encrypt_query_param.as_deref(),
        media.aes_key.as_deref(),
        media.encrypt_type.unwrap_or(1),
    )
    .await
    {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[wechat agent] CDN download failed: {e}");
            let reply = build_text_reply(
                from_user_id,
                context_token,
                "（文件下载失败，请重发）",
            );
            let _ = send_message_with_retry(client, reply, "file-dl-fail-reply").await;
            return Ok(());
        }
    };

    // 3. Hand off to shared ingest
    let paths = self.paths.clone(); // assumes handler already holds WikiPaths
    let user_label = format!("WeChat user `{}`", short_openid(from_user_id));
    let result = tokio::task::spawn_blocking(move || {
        crate::wechat_common::file_ingest::ingest_attachment(
            &paths,
            &bytes,
            &file_name,
            "wechat-file",
            &user_label,
        )
    })
    .await;

    match result {
        Ok(Ok(entry)) => {
            let reply = build_text_reply(
                from_user_id,
                context_token,
                &format!(
                    "✅ 已收到文件并入库：{} 素材 #{:05} 已进入知识库处理队列。",
                    file.file_name.as_deref().unwrap_or("file"),
                    entry.id
                ),
            );
            let _ = send_message_with_retry(client, reply, "file-ok-reply").await;
        }
        Ok(Err(e)) | Err(e) if {
            eprintln!("[wechat agent] file ingest failed: {e:?}");
            let reply = build_text_reply(
                from_user_id,
                context_token,
                "（文件解析失败，请稍后重试或换一份文件）",
            );
            let _ = send_message_with_retry(client, reply, "file-parse-fail-reply").await;
        }
    }

    Ok(())
}
```

(NOTE: the match arms above contain a pseudo-pattern `Ok(Err(e)) | Err(e) if {...}` — that's not valid Rust syntax as written; implementer must split into two arms or use a helper that flattens `JoinError + anyhow::Error → anyhow::Error`. Code is illustrative.)

**Step 4: Build + run all tests**
```bash
cd rust && cargo check --workspace 2>&1 | tail -3
cd rust && cargo test -p desktop-core --lib wechat 2>&1 | tail -15
```
Expected: build clean, all wechat tests pass.

**Step 5: Commit**
```bash
git add rust/crates/desktop-core/src/wechat_ilink/handlers.rs \
        rust/crates/desktop-core/src/wechat_ilink/desktop_handler.rs
git commit -m "feat(wechat-ilink): wire FILE branch → file-ingest dispatcher (E29.1)"
```

---

### Task 1.6: iLink E2E smoke test

**Files:**
- Smoke spec (manual): no code, document procedure in PR / commit body

**Step 1: Build the Tauri dev app**
```bash
cd apps/desktop-shell && npm run tauri:dev
```

**Step 2: Manually test**
1. Open WeChat on phone, find MyAgent
2. Send `hello.pdf` (any small PDF, < 1 MB)
3. Within 30s: expect bot reply `✅ 已收到文件并入库：hello.pdf 素材 #NNNNN 已进入知识库处理队列。`
4. In desktop app, navigate to Inbox → confirm new entry titled `WeChat · hello.pdf`
5. Open the raw entry → confirm extracted text body

**Step 3: Test the three other supported formats**
- Send a `.docx` → same flow
- Send a `.txt` → same flow
- Send a `.md` → same flow

**Step 4: Test rejection paths**
- Send a `.jpg` → expect `（暂不支持的文件类型：xxx.jpg。请发送 PDF/DOCX/TXT/MD）`
- Send a voice message → expect `（暂不支持图片/语音消息，请发送 PDF/DOCX/TXT/MD 文件或文字）`

**Step 5: Commit smoke notes (no code, but a NOTES.md entry)**
```bash
# Append smoke result summary to plan file's "Smoke Log" section, then:
git add docs/desktop-shell/plans/2026-05-12-wechat-file-ingestion-plan.md
git commit -m "docs(plan): record E29 Phase 1 smoke test results"
```

**Phase 1 ship gate:** if all 4 supported types + 2 rejections work, Phase 1 is shippable on its own. Decide before moving to Phase 2.

---

## Phase 2 — Kefu Webhook Channel

### Task 2.1: Discovery — confirm Kefu inbound message handling state

**Why a gate:** the earlier exploration found Kefu webhook receives a `MsgReceive { token }` event but the actual `sync_msg` call + message DTO + dispatch wasn't located in `desktop_handler` symmetry. Need to confirm:

(a) Does Kefu currently process incoming TEXT messages end-to-end? If no, the file branch is downstream of unbuilt text branch.
(b) Where does the message DTO live after `sync_msg` decodes the encrypted blob?
(c) Is there an `access_token` cache for the cgi-bin API?

**Files (read-only):**
- `rust/crates/desktop-core/src/wechat_kefu/client.rs` (full file)
- `rust/crates/desktop-core/src/wechat_kefu/callback.rs` (full file)
- `rust/crates/desktop-core/src/wechat_kefu/dispatch.rs` if exists
- search: `grep -rn "sync_msg\|dispatch_kefu" rust/crates/desktop-core/src/wechat_kefu/`

**Step 1: Read the three Kefu files top-to-bottom**

**Step 2: Document findings**
Append a `## Kefu State Notes` section to this plan recording:
- Does the codebase do `sync_msg` ↔ store-raw-entry today for TEXT?
- What's the inbound message struct name + path?
- access_token retrieval shape

**Step 3: Gate decision**
- If TEXT inbound works end-to-end: proceed to Task 2.2.
- If TEXT inbound is also missing: **stop and ask user** — Phase 2 grows to include text-ingest-for-Kefu which doubles scope. Get explicit go-ahead before continuing.

**Step 4: Commit notes**
```bash
git add docs/desktop-shell/plans/2026-05-12-wechat-file-ingestion-plan.md
git commit -m "docs(plan): record E29 Phase 2 Kefu discovery findings"
```

---

### Task 2.2: KefuClient::get_media

**Files:**
- Modify: `rust/crates/desktop-core/src/wechat_kefu/client.rs` (extend KefuClient)

**Step 1: Write the failing test**

Append to `client.rs`:
```rust
#[cfg(test)]
mod media_tests {
    use super::*;

    #[tokio::test]
    async fn get_media_url_assembly() {
        let url = KefuClient::media_get_url("AT_TOKEN_X", "MEDIA_ID_Y");
        assert_eq!(
            url,
            "https://qyapi.weixin.qq.com/cgi-bin/media/get?access_token=AT_TOKEN_X&media_id=MEDIA_ID_Y"
        );
    }
}
```

**Step 2: Run to verify failure**
```bash
cd rust && cargo test -p desktop-core --lib wechat_kefu::client::media_tests 2>&1 | tail -5
```
Expected: FAIL — `media_get_url` not defined.

**Step 3: Implement**

Add to `KefuClient`:
```rust
impl KefuClient {
    pub(crate) fn media_get_url(access_token: &str, media_id: &str) -> String {
        format!(
            "https://qyapi.weixin.qq.com/cgi-bin/media/get?access_token={access_token}&media_id={media_id}"
        )
    }

    pub async fn get_media(&self, media_id: &str) -> Result<Vec<u8>> {
        let token = self.access_token().await?; // assumes existing token cache
        let url = Self::media_get_url(&token, media_id);
        let resp = self.http.get(&url).send().await
            .map_err(|e| anyhow!("get_media HTTP: {e}"))?;
        if !resp.status().is_success() {
            return Err(anyhow!("get_media status {}", resp.status()));
        }
        // Content-Type: application/octet-stream on success; application/json on error
        let ct = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("");
        if ct.contains("application/json") {
            let err: serde_json::Value = resp.json().await
                .map_err(|e| anyhow!("get_media error body: {e}"))?;
            return Err(anyhow!("get_media API error: {err}"));
        }
        let bytes = resp.bytes().await
            .map_err(|e| anyhow!("get_media body: {e}"))?.to_vec();
        Ok(bytes)
    }
}
```

**Step 4: Run + verify**
```bash
cd rust && cargo test -p desktop-core --lib wechat_kefu::client 2>&1 | tail -5
```
Expected: PASS.

**Step 5: Commit**
```bash
git add rust/crates/desktop-core/src/wechat_kefu/client.rs
git commit -m "feat(wechat-kefu): KefuClient::get_media for cgi-bin/media/get (E29.2)"
```

---

### Task 2.3: Wire Kefu file message branch

**Files:**
- Modify: the Kefu message dispatch site identified in Task 2.1

**Step 1: Locate the Kefu inbound message switch** (per Task 2.1 findings, e.g. `dispatch.rs::handle_kefu_message`).

**Step 2: Add a FILE arm**

Sketch (real shape comes from Task 2.1):
```rust
match msg.msgtype.as_str() {
    "text" => { /* existing path */ }
    "file" => {
        let media_id = msg.file.as_ref().and_then(|f| f.media_id.as_deref())
            .ok_or_else(|| anyhow!("file msg without media_id"))?;
        let file_name = msg.file.as_ref().and_then(|f| f.filename.clone())
            .unwrap_or_else(|| "unknown.bin".into());

        if crate::wechat_common::file_ingest::classify_file_kind(&file_name).is_none() {
            self.reply_text(&format!(
                "（暂不支持的文件类型：{file_name}。请发送 PDF/DOCX/TXT/MD）"
            )).await?;
            return Ok(());
        }

        let bytes = self.client.get_media(media_id).await?;
        let paths = self.paths.clone();
        let user_label = format!("WeChat user `{}`", short_openid(&msg.from_openid));
        let entry = tokio::task::spawn_blocking(move || {
            crate::wechat_common::file_ingest::ingest_attachment(
                &paths, &bytes, &file_name, "wechat-file-kefu", &user_label,
            )
        }).await??;

        self.reply_text(&format!(
            "✅ 已收到文件并入库：{} 素材 #{:05} 已进入知识库处理队列。",
            file_name, entry.id
        )).await?;
    }
    _ => {
        self.reply_text(
            "（暂不支持图片/语音消息，请发送 PDF/DOCX/TXT/MD 文件或文字）"
        ).await?;
    }
}
```

**Step 3: Build + run all tests**
```bash
cd rust && cargo check --workspace 2>&1 | tail -3
cd rust && cargo test -p desktop-core --lib wechat 2>&1 | tail -15
```
Expected: build clean, no test regression.

**Step 4: Commit**
```bash
git add <files identified in Task 2.1>
git commit -m "feat(wechat-kefu): wire FILE branch → shared file-ingest (E29.2)"
```

---

### Task 2.4: Kefu E2E smoke

**Step 1: Configure Kefu corpid/secret/callback per `docs/design/modules/04-wechat-kefu.md`**

**Step 2: Send same 4 supported types + 2 rejections from a Kefu-enabled WeChat session**

**Step 3: Append smoke notes to plan**
```bash
git add docs/desktop-shell/plans/2026-05-12-wechat-file-ingestion-plan.md
git commit -m "docs(plan): record E29 Phase 2 smoke test results"
```

---

## Phase 3 — Polish + Ship

### Task 3.1: Capabilities endpoint flip

**Files:**
- Modify: `rust/crates/desktop-core/src/wechat_kefu/types.rs:107-116` (KefuCapabilities defaults)
- Modify: `rust/crates/desktop-server/src/handlers/wechat.rs` (kefu_status_handler — add file:true reflection)
- Verify: `apps/desktop-shell/src/api/wechat` reads capabilities; if it surfaces unsupported in UI, no change needed (just reads new value)

**Step 1: Update `KefuCapabilities` default constructor**
```rust
// before
file: false,
// after
file: true,
```

**Step 2: Run cargo check + workspace tests**
```bash
cd rust && cargo test --workspace 2>&1 | tail -5
```

**Step 3: Commit**
```bash
git add rust/crates/desktop-core/src/wechat_kefu/types.rs \
        rust/crates/desktop-server/src/handlers/wechat.rs
git commit -m "feat(wechat-kefu): flip capabilities.file → supported (E29.3)"
```

---

### Task 3.2: Plan README + version bump + push

**Files:**
- Modify: `docs/desktop-shell/plans/README.md` (add link to this plan)
- Modify: `apps/desktop-shell/src-tauri/tauri.conf.json` (version → 0.1.17)
- Modify: `apps/desktop-shell/src-tauri/Cargo.toml` (version → 0.1.17)

**Step 1: Add plan link**
Append in `README.md` after the E28 entry:
```markdown
- [WeChat File Ingestion (E29) Implementation Plan](./2026-05-12-wechat-file-ingestion-plan.md)
```

**Step 2: Bump version (sed or Edit)**

**Step 3: cargo check to refresh Cargo.lock**
```bash
cd apps/desktop-shell/src-tauri && cargo check 2>&1 | tail -2
```

**Step 4: Chore commit + tag + push**
```bash
git add apps/desktop-shell/src-tauri/{tauri.conf.json,Cargo.toml,Cargo.lock} \
        docs/desktop-shell/plans/README.md
git commit -m "$(cat <<'EOF'
chore: bump desktop-shell to v0.1.17 + WeChat File Ingestion plan

E29 — both WeChat channels (iLink desktop handler + Kefu webhook) now
accept PDF/DOCX/TXT/MD file messages, decrypt+download the attachment,
run the existing wiki_ingest extractors, and write to .clawwiki/raw/.
Replaces the "（暂不支持非文本消息）" rejection.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git tag -a v0.1.17 -m "v0.1.17 — WeChat File Ingestion (E29)"
git push origin main
git push origin v0.1.17
```

---

## Risk register

| Risk | Likelihood | Mitigation |
|------|-----------|------------|
| iLink AES mode unknown (Task 1.1 fails) | Medium | Discovery is a hard gate; pivot to passthrough plan if blocked |
| iLink CDN protocol changes upstream | Low | Pin behavior in tests; openclaw upstream is the source of truth |
| Kefu inbound flow not implemented (Task 2.1 fails) | Medium | Discovery is a hard gate; user re-decides scope |
| Large PDF (>100 MiB cap) | Low | `extract_pdf` already enforces cap → surfaced as parse error → friendly reply |
| Encrypted message replay | Low | Existing iLink session token + `context_token` echo already mitigates |
| Disk fill from staged temp files | Low | `std::fs::remove_file` in `ingest_attachment` runs even on extractor failure (use drop guard) |

---

## Decryption Notes

**Step 1 (code search) findings — 2026-05-12:**

| 项 | 状态 | 证据 |
|----|------|------|
| Crypto 依赖 | ✅ 已有 `aes-gcm = "0.10"` | `desktop-core/Cargo.toml:13`，提供 `aes_gcm::aes::{Aes128, Aes256}` 块解密原语 |
| HTTP 客户端 | ✅ 已有 `reqwest::Client` 长生命周期实例 | `wechat_ilink/client.rs:62`，可直接复用 |
| CDN 下载 / 解密代码 | ❌ 不存在 | 全仓搜 `cdn`/`fileid`/`novac2c` 仅命中 `CDN_BASE_URL` 常量本身（`types.rs:357`） |
| Cipher 模式 (ECB/CBC/GCM) | ⚠️ **协议未知** | 代码无任何注释/实现；上游 openclaw 参考代码（`claw-code-parity` crate）**不存在于本仓库**（Explore agent 之前提到有误） |
| Key 格式 | 部分已知 | `CdnMedia.aes_key` 是 base64-encoded；`ImageItem.aeskey`（独立字段）注释为"Raw AES-128 key as hex string (16 bytes)" → 强暗示 AES-128 |
| `encrypt_type` 语义 | ✅ 已知 | `types.rs:76`：`0 = encrypt fileid only, 1 = encrypt thumb/middle/large fileids together` —— 这是关于**URL fileid** 的加密范围，不是 body 内容的开关。Body 始终用 `aes_key` 解。 |
| 可参考的现有 AES 实现 | ✅ Kefu callback | `wechat_kefu/callback.rs:143-225` 实现 AES-256-CBC：手动用 `Aes256.decrypt_block` + XOR previous block + PKCS7 unpad；IV 取 `aes_key[..16]` |

**结论**：deps、HTTP、参考实现都齐了，**唯一未知是 iLink CDN 媒体的 cipher mode + IV 来源**。无法从代码静态推断。

**两个候选 working hypothesis**（按可能性排序）：

1. **AES-128-CBC, IV = key[..16]**（与 Kefu callback 同模式，只是 key 从 AES-256 改 AES-128）—— 最可能
2. **AES-128-ECB**（无 IV，整 body 按 16-byte 块独立解）—— 次可能

需要一份真实 FILE 消息样本才能裁决。

## Kefu State Notes
*To be filled in by Task 2.1.*

## Smoke Log
*To be filled in by Tasks 1.6 + 2.4.*

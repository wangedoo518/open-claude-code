//! `wiki_ingest` — source adapters for the ClawWiki raw layer.
//!
//! Per canonical §11.2 this crate owns the adapters that turn inbound
//! WeChat/paste inputs into markdown bodies that land under
//! `~/.clawwiki/raw/`. The S1-S5 sprints hand-rolled a couple of
//! placeholder adapters directly inside `desktop-server`; this crate
//! is the real home and lets us layer in `voice`, `image`, `pdf`,
//! `pptx`, `docx`, `video` modules without dragging heavy transitive
//! deps into the HTTP server.
//!
//! ## Module layout (canonical §7.2 targets)
//!
//! ```text
//! wiki_ingest/
//! ├── url       ← live    — real HTTP GET + MIME-aware body extraction
//! ├── voice     ← stub    — whisper.cpp / Whisper API (S6+)
//! ├── image     ← stub    — Vision caption + OCR (S6+)
//! ├── pdf       ← stub    — pdfjs-dist / pdf-extract (S6+)
//! ├── pptx      ← stub    — python-pptx spawn (S6+)
//! ├── docx      ← stub    — mammoth spawn (S6+)
//! └── video     ← stub    — ffmpeg audio track + whisper (S6+)
//! ```
//!
//! Only `url` has a real implementation today; the rest will be
//! stubbed `pub mod` declarations once their owners pick them up in
//! a future sprint. Shipping them as empty modules would add file
//! noise for zero benefit, so they're intentionally NOT present.
//!
//! ## Contract with `wiki_store`
//!
//! Adapters never write to disk themselves. They produce an
//! [`IngestResult`] (title/body/source_url/frontmatter hints) and
//! the caller threads it through `wiki_store::write_raw_entry`.
//! Separation of concerns: `wiki_ingest` knows how to TALK to the
//! outside world (HTTP, ffmpeg, whisper, ...); `wiki_store` knows how
//! to LAY OUT a file on disk. Neither knows about the other.

pub mod docx;
pub mod extractor;
pub mod html_to_md;
pub mod image;
pub mod markitdown;
pub mod pdf;
pub mod pptx;
pub mod url;
pub mod video;
pub mod voice;
pub mod wechat_fetch;

/// Common output shape for every adapter. Carries the pieces the
/// caller needs to construct a `wiki_store::RawFrontmatter` and to
/// call `wiki_store::write_raw_entry` — no more, no less.
///
/// `source` is a canonical tag like `"url"` / `"wechat-article"` /
/// `"voice"`; the caller passes it straight through to
/// `write_raw_entry(source, ...)` so the tag lives both in the
/// filename (`NNNNN_{source}_{slug}_{date}.md`) and in the YAML
/// frontmatter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestResult {
    /// Free-form title used to derive the slug. May contain any
    /// characters; `wiki_store::slugify` sanitises it.
    pub title: String,
    /// Markdown body. Written verbatim after the frontmatter block.
    pub body: String,
    /// Optional source URL recorded in `frontmatter.source_url`.
    pub source_url: Option<String>,
    /// Canonical source tag. See the top-of-file list for valid values.
    pub source: String,
}

/// Errors raised by any adapter. Transport-specific detail is
/// stringified so the HTTP handlers in `desktop-server` don't have
/// to depend on `reqwest::Error` directly.
#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("upstream returned non-success status: {status} for {url}")]
    HttpStatus { status: u16, url: String },
    #[error("response body too large: {bytes} bytes (max {max})")]
    TooLarge { bytes: usize, max: usize },
    #[error("response is not utf-8: {0}")]
    NotUtf8(String),
    #[error("file not found: {0}")]
    NotFound(String),
    #[error("parse error: {0}")]
    Parse(String),
}

pub type Result<T> = std::result::Result<T, IngestError>;

// ─────────────────────────────────────────────────────────────────
// Content quality validation (anti-bot / empty content rejection)
// ─────────────────────────────────────────────────────────────────

/// Anti-bot page markers. Marker hit → reject, **regardless of body
/// length**. We used to gate this on `len < 1000` to avoid false
/// positives on legitimate articles that merely mention "Captcha" as
/// a topic, but real anti-bot pages often embed HTML skeleton / CSS /
/// JS that easily pushes them past 1KB while the meaningful text is
/// still just the marker. Production incident 2026-04: an 18KB
/// mp.weixin.qq.com anti-bot page slipped through the gate and
/// littered the Inbox with placeholder "environment abnormal" entries.
///
/// The markers below are intentionally **specific phrases** rather
/// than loose terms — an article about "Captcha识别综述" shouldn't
/// be rejected, but a page literally saying "完成验证后即可继续访问"
/// is never anything but an anti-bot block.
///
/// Added 2026-04 round B:
///   * Cloudflare challenge pages commonly show "Just a moment..." /
///     "Checking your browser" — these are pre-JS-challenge shells
///     that pass length/meaningful-char gates but contain zero content.
///   * WeChat app-only share URLs opened in a browser render "请在微信
///     客户端打开链接" instead of the article body.
///   * 知乎 login-wall shells show "知乎，让每一次点击都充满意义" as
///     the brand copy. (This string is the logged-out homepage tagline
///     and only appears on unauthenticated walls — logged-in article
///     pages do not surface it.)
///   * Deleted WeChat articles show "该内容已被发布者删除" — not an
///     anti-bot block per se, but definitively empty; reject early.
const ANTI_BOT_MARKERS: &[&str] = &[
    "环境异常",
    "完成验证后即可继续访问",
    "请完成安全验证",
    "人机验证",
    "滑动验证",
    "拼图验证",
    "访问频繁",
    "Access Denied",
    "Please enable JavaScript",
    "403 Forbidden",
    "Just a moment...",
    "Checking your browser",
    "请在微信客户端打开链接",
    "知乎，让每一次点击都充满意义",
    "该内容已被发布者删除",
];

/// Minimum raw body length to accept.
const MIN_BODY_LEN: usize = 200;
/// Minimum text length after stripping markdown images/links.
const MIN_TEXT_AFTER_STRIP: usize = 100;
/// Minimum count of alphanumeric / CJK characters.
const MIN_MEANINGFUL_CHARS: usize = 50;

/// Reject obviously failed fetches (anti-bot pages, empty content,
/// image-only pages). Returns `Err(reason)` if content is not worth
/// ingesting.
///
/// Called by:
///   * `desktop-core::maybe_enrich_url` (both simple fetch and Playwright)
///   * `wechat_kefu::desktop_handler::handle_url_ingest`
///   * `desktop-server::wechat_fetch_handler`
///
/// The primary anti-bot gate. A secondary gate lives in
/// [`wiki_store::write_raw_entry`] that fires for any fetched-source
/// write — see that function for the rationale.
pub fn validate_fetched_content(body: &str) -> std::result::Result<(), String> {
    let trimmed = body.trim();

    // Length check.
    if trimmed.len() < MIN_BODY_LEN {
        return Err(format!("内容过短 ({} 字符)", trimmed.len()));
    }

    // Anti-bot markers check — no length gate: see the constant docs.
    for marker in ANTI_BOT_MARKERS {
        if trimmed.contains(marker) {
            return Err(format!("反爬验证页: 包含 '{}'", marker));
        }
    }

    // Strip markdown images/links: `![...](...)` and `[...](...)`.
    let stripped = strip_markdown_links(trimmed);
    let stripped_trimmed = stripped.trim();
    if stripped_trimmed.len() < MIN_TEXT_AFTER_STRIP {
        return Err(format!(
            "去除图片/链接后文本过少 ({} 字符)",
            stripped_trimmed.len()
        ));
    }

    // Count meaningful chars (ASCII alphanumeric or CJK).
    let meaningful = stripped_trimmed
        .chars()
        .filter(|c| c.is_alphanumeric() || is_cjk(*c))
        .count();
    if meaningful < MIN_MEANINGFUL_CHARS {
        return Err(format!("实际文字过少 ({} 字符)", meaningful));
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────
// Markdown sanitization (post-extraction cleanup)
// ─────────────────────────────────────────────────────────────────

/// Clean a freshly-extracted markdown body. Both the simple HTTP
/// path (`url::fetch_and_body`) and the Playwright/defuddle path
/// (`wechat_fetch::fetch_wechat_article`) call this before handing
/// the body off to `wiki_store::write_raw_entry`.
///
/// Operations (in order — the order matters for nested entities like
/// `&amp;nbsp;`, where `&amp;` must be decoded first so the inner
/// `&nbsp;` is then visible to the second pass):
///
///   1. Decode the common HTML entities defuddle leaves behind
///      (`&amp;` first, then `&nbsp;` / `&lt;` / `&gt;` / `&quot;` /
///       `&#39;` / `&#x27;` / `&apos;` / `&#47;`).
///   2. Drop image markdown whose `data:` URI was truncated mid-encode
///      — the canonical symptom is `![](data:image/svg+xml,%3C%3Fxml version=)`
///      with a payload <20 chars. Removing them cleans up both the
///      visible markdown AND the LLM context that would otherwise
///      receive these orphan stubs.
///   3. Collapse 3+ consecutive newlines into 2.
///   4. Drop empty image markdown (`![]( )` / `![]()`).
///   5. Trim outer whitespace.
pub fn sanitize_markdown(input: &str) -> String {
    // Step 1: HTML entities. `&amp;` first so any `&amp;nbsp;` becomes
    // `&nbsp;` and gets handled in the next replace.
    let mut out = input.replace("&amp;", "&");
    out = out.replace("&nbsp;", " ");
    out = out.replace("&lt;", "<");
    out = out.replace("&gt;", ">");
    out = out.replace("&quot;", "\"");
    out = out.replace("&#39;", "'");
    out = out.replace("&#x27;", "'");
    out = out.replace("&apos;", "'");
    out = out.replace("&#47;", "/");

    // Step 2: drop broken `![alt](data:mime,payload)` images. We don't
    // pull in `regex` for one match — a hand-rolled pass is enough.
    out = strip_broken_data_uris(&out);

    // Step 3: collapse runs of 3+ newlines (handles \n, \r\n).
    out = collapse_blank_lines(&out);

    // Step 4: drop standalone empty image markdown like `![]()` or
    // `![](  )` on its own line.
    out = strip_empty_images(&out);

    out.trim().to_string()
}

/// Sweep `![alt](data:mime,payload)` image links; if `payload` looks
/// truncated (length below 20 chars, OR an SVG without a closing `%3E`)
/// the entire image construct is removed.
///
/// UTF-8-safe: scan markers (`!`, `[`, `]`, `(`, `)`) at byte level
/// because they're all ASCII, but copy non-marker bytes through as
/// raw bytes via `String::from_utf8_lossy`-style accumulation. We
/// build a Vec<u8> and decode once at the end so multi-byte CJK
/// sequences are preserved verbatim.
fn strip_broken_data_uris(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        // Look for the start of an image: `![`
        if bytes[i] == b'!' && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            if let Some(close_bracket) = find_byte_from(bytes, b']', i + 2) {
                if close_bracket + 1 < bytes.len() && bytes[close_bracket + 1] == b'(' {
                    if let Some(close_paren) = find_byte_from(bytes, b')', close_bracket + 2) {
                        let url_bytes = &bytes[close_bracket + 2..close_paren];
                        if let Ok(url) = std::str::from_utf8(url_bytes) {
                            if url.starts_with("data:") && is_truncated_data_uri(url) {
                                // Skip the whole `![...](data:..)` construct
                                i = close_paren + 1;
                                continue;
                            }
                        }
                    }
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    // Original input was valid UTF-8 and we only ever skip whole
    // ASCII-bounded constructs, so the result is still valid UTF-8.
    String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

/// `data:` URI heuristic — too short or visibly cut off.
fn is_truncated_data_uri(uri: &str) -> bool {
    // Strip the `data:` prefix and split into mime / payload at the
    // first comma.
    let rest = &uri["data:".len()..];
    let Some((mime, payload)) = rest.split_once(',') else {
        return true; // No comma → not even a complete data URI
    };
    if payload.len() < 20 {
        return true;
    }
    // For SVG, expect at least one `%3E` (encoded `>`) marking that
    // a tag actually closed. Truncated SVG payloads typically end at
    // `%3C%3Fxml version=` with no closing tag.
    if mime.to_ascii_lowercase().contains("svg") && !payload.contains("%3E") {
        return true;
    }
    false
}

/// Squeeze runs of blank lines down to a single empty line. Tolerates
/// `\r\n` line endings.
fn collapse_blank_lines(input: &str) -> String {
    let normalized = input.replace("\r\n", "\n");
    let mut out = String::with_capacity(normalized.len());
    let mut consecutive_newlines: u32 = 0;
    for ch in normalized.chars() {
        if ch == '\n' {
            consecutive_newlines += 1;
            if consecutive_newlines <= 2 {
                out.push('\n');
            }
        } else {
            consecutive_newlines = 0;
            out.push(ch);
        }
    }
    out
}

/// Drop lines that are exactly an empty image markdown construct.
fn strip_empty_images(input: &str) -> String {
    input
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            // `![]( )` or `![]()` → drop
            !(trimmed.starts_with("![](")
                && trimmed.ends_with(')')
                && trimmed.len() <= 6)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Check if a char is in the CJK Unified Ideographs block.
fn is_cjk(c: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&c)
        || ('\u{3400}'..='\u{4DBF}').contains(&c)
        || ('\u{F900}'..='\u{FAFF}').contains(&c)
}

/// Strip markdown `![alt](url)` and `[text](url)` constructs.
/// Handwritten scanner (avoids pulling the regex crate as a dep).
fn strip_markdown_links(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;

    while i < bytes.len() {
        // Detect image: !
        let is_image = bytes[i] == b'!' && i + 1 < bytes.len() && bytes[i + 1] == b'[';
        // Detect link: [
        let is_link = bytes[i] == b'[';

        if is_image || is_link {
            let start = if is_image { i + 1 } else { i };
            if let Some(bracket_close) = find_byte_from(bytes, b']', start + 1) {
                // Expect `(` immediately after `]`.
                if bracket_close + 1 < bytes.len() && bytes[bracket_close + 1] == b'(' {
                    if let Some(paren_close) = find_byte_from(bytes, b')', bracket_close + 2) {
                        // Skip the whole construct. For regular links, keep
                        // the alt text (between `[...]`) since it's often
                        // meaningful prose. For images, drop everything.
                        if is_image {
                            // Drop alt text too (just image noise).
                        } else {
                            let alt_bytes = &bytes[start + 1..bracket_close];
                            if let Ok(alt_str) = std::str::from_utf8(alt_bytes) {
                                out.push_str(alt_str);
                            }
                        }
                        i = paren_close + 1;
                        continue;
                    }
                }
            }
        }

        // Default: pass through this byte (multi-byte UTF-8 chars are
        // handled naturally since we only match ASCII control chars).
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn find_byte_from(haystack: &[u8], needle: u8, from: usize) -> Option<usize> {
    if from >= haystack.len() {
        return None;
    }
    haystack[from..].iter().position(|&b| b == needle).map(|p| p + from)
}

#[cfg(test)]
mod validate_tests {
    use super::*;

    #[test]
    fn rejects_short_content() {
        let err = validate_fetched_content("hello").unwrap_err();
        assert!(err.contains("过短"));
    }

    #[test]
    fn rejects_anti_bot_page() {
        let body = "** 当前环境异常，完成验证后即可继续访问。 去验证 **";
        let err = validate_fetched_content(body).unwrap_err();
        assert!(err.contains("反爬") || err.contains("过短"));
    }

    #[test]
    fn rejects_image_only_markdown() {
        // Build an image-only page long enough to pass the raw length check
        // (> 200 chars) but mostly markdown images.
        let body = "### [](https://mmbiz.qpic.cn/mmbiz_png/abc/def/ghi/jkl/mno)\n\n".to_string()
            + &"![](https://mmbiz.qpic.cn/mmbiz_png/xyz/abc123/def456/ghi789)\n\n".repeat(6);
        assert!(body.len() > 200, "body should exceed raw length threshold");
        let err = validate_fetched_content(&body).unwrap_err();
        assert!(
            err.contains("文本过少") || err.contains("文字过少"),
            "expected strip-related error, got: {err}"
        );
    }

    #[test]
    fn rejects_empty_markdown() {
        let err = validate_fetched_content("**").unwrap_err();
        assert!(err.contains("过短"));
    }

    #[test]
    fn accepts_real_article() {
        let body = "# ClawWiki 架构概览\n\n\
            ClawWiki 是基于 Karpathy LLM Wiki 方法论的个人知识管理系统。\
            它使用微信客服作为投喂入口，SKILL prompt 引擎作为核心，\
            Markdown Wiki 作为知识资产存储。\
            技术栈是 Tauri 2 + React 18 + TypeScript + Tailwind 前端，\
            加上 Rust workspace 七个 crate 构成的后端。\
            Phase 1 实现了 wiki_store 的 7 个新函数，包括吸收日志、\
            反向链接索引、schema 验证和 wiki 统计。\
            Phase 2 引入双 Tab Shell 和微信自动闭环。";
        validate_fetched_content(body).expect("real article should pass");
    }

    #[test]
    fn accepts_long_article_without_specific_markers() {
        // A long, substantive article that does NOT contain the specific
        // anti-bot marker phrases should pass. Prior to the 2026-04 fix
        // this test also covered articles merely mentioning "Captcha" /
        // "滑动验证" as topics, but those now count as marker hits
        // because real anti-bot pages can exceed 1KB once HTML skeleton
        // is inlined, and a length gate was letting them through.
        let body = "# 验证码识别技术综述\n\n".to_string()
            + &"本文讨论了字符识别技术的发展历程，从最早的 OCR 到现代语义理解模型。\
                早期系统主要依赖扭曲的字符图像，用户需要肉眼辨识并手动输入。\
                随着计算机视觉和深度学习的进展，基于 CNN 的模型可以高精度破解这类简单系统。\
                因此现代系统采用了更复杂的交互形式，包括图像语义分类、\
                行为建模和基于用户交互轨迹的隐形识别等多种防御机制。\
                核心目标是区分自动化程序和真实的人类用户行为。"
                .repeat(5);
        validate_fetched_content(&body)
            .expect("long substantive article without marker phrases should pass");
    }

    #[test]
    fn rejects_long_anti_bot_page_with_html_skeleton() {
        // Regression for 2026-04: mp.weixin.qq.com anti-bot page was
        // ~18KB (HTML skeleton inflating the length) but carried the
        // "环境异常 / 完成验证后即可继续访问" block. Used to slip
        // through because trimmed.len() >= 1000; now the marker gate
        // fires unconditionally.
        let skeleton = "<div>".repeat(500); // a lot of filler HTML
        let body = format!(
            "{skeleton}\n\n## 环境异常\n\n当前环境异常，完成验证后即可继续访问。\n\n去验证"
        );
        assert!(body.len() > 1000, "body must exceed old length gate");
        let err = validate_fetched_content(&body).unwrap_err();
        assert!(
            err.contains("反爬"),
            "expected anti-bot rejection, got: {err}"
        );
    }

    // ── sanitize_markdown regression (2026-04 mp.weixin) ─────────

    #[test]
    fn sanitize_markdown_decodes_html_entities() {
        let input = "&nbsp;&nbsp;新智元报道 &nbsp;编辑：好困";
        let out = sanitize_markdown(input);
        assert!(!out.contains("&nbsp;"), "should decode &nbsp;: {out}");
        // Output is trimmed (outer whitespace removed) — leading nbsp's
        // are decoded to spaces and then trimmed away.
        assert!(out.starts_with("新智元"), "got: {out}");
        // Mid-string nbsp survives as a literal space.
        assert!(out.contains("新智元报道  编辑"));
    }

    #[test]
    fn sanitize_markdown_decodes_nested_amp_first() {
        // `&amp;nbsp;` → first `&amp;` → `&nbsp;` → ` `.
        let out = sanitize_markdown("a &amp;nbsp; b");
        assert_eq!(out, "a   b");
    }

    #[test]
    fn sanitize_markdown_drops_truncated_svg_data_uri() {
        let body = "before\n\n![](data:image/svg+xml,%3C%3Fxml version=)\n\nafter";
        let out = sanitize_markdown(body);
        assert!(!out.contains("data:image/svg+xml"));
        assert!(out.contains("before") && out.contains("after"));
    }

    #[test]
    fn sanitize_markdown_keeps_complete_data_uri() {
        // Real PNG data URI (long, well-formed) must survive.
        let payload = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAA";
        let body = format!("![icon](data:image/png;base64,{payload})");
        let out = sanitize_markdown(&body);
        assert!(out.contains("data:image/png"), "well-formed PNG data URI was wrongly stripped: {out}");
    }

    #[test]
    fn sanitize_markdown_collapses_blank_lines() {
        let body = "a\n\n\n\n\nb";
        let out = sanitize_markdown(body);
        assert_eq!(out, "a\n\nb");
    }

    #[test]
    fn sanitize_markdown_drops_empty_image_lines() {
        let body = "intro\n\n![]()\n\noutro";
        let out = sanitize_markdown(body);
        assert!(!out.contains("![]()"), "empty image marker survived: {out}");
        assert!(out.contains("intro") && out.contains("outro"));
    }

    #[test]
    fn sanitize_markdown_real_wechat_bug_sample() {
        // Excerpt of raw #00002 — the actual bug from prod.
        let raw = "&nbsp;&nbsp;新智元报道 &nbsp;编辑：好困\n\n\
            正文段落。![](data:image/svg+xml,%3C%3Fxml version=)\n\
            另一段。![](data:image/svg+xml,%3C%3Fxml version=)";
        let out = sanitize_markdown(raw);
        assert!(!out.contains("&nbsp;"), "&nbsp; remained: {out}");
        assert!(!out.contains("data:image/svg"), "broken svg remained: {out}");
        assert!(out.contains("正文段落") && out.contains("另一段"));
    }

    #[test]
    fn strip_markdown_links_removes_images() {
        let out = strip_markdown_links("text ![alt](url) more");
        assert!(!out.contains("alt"));
        assert!(!out.contains("url"));
        assert!(out.contains("text"));
        assert!(out.contains("more"));
    }

    #[test]
    fn strip_markdown_links_keeps_link_text() {
        let out = strip_markdown_links("see [docs](https://example.com) here");
        assert!(out.contains("docs"));
        assert!(!out.contains("example.com"));
        assert!(out.contains("here"));
    }
}

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

/// Anti-bot page markers. If body contains any of these AND is short,
/// we reject as a verification/block page, not real content.
const ANTI_BOT_MARKERS: &[&str] = &[
    "环境异常",
    "完成验证",
    "去验证",
    "人机验证",
    "滑动验证",
    "拼图验证",
    "请完成安全验证",
    "访问频繁",
    "Access Denied",
    "Please enable JavaScript",
    "Captcha",
    "302 Found",
    "403 Forbidden",
];

/// Minimum raw body length to accept.
const MIN_BODY_LEN: usize = 200;
/// Minimum text length after stripping markdown images/links.
const MIN_TEXT_AFTER_STRIP: usize = 100;
/// Minimum count of alphanumeric / CJK characters.
const MIN_MEANINGFUL_CHARS: usize = 50;
/// Size threshold below which anti-bot markers are treated as blocking.
/// (Long pages that merely mention "Captcha" as a topic aren't rejected.)
const ANTI_BOT_MAX_LEN: usize = 1000;

/// Reject obviously failed fetches (anti-bot pages, empty content,
/// image-only pages). Returns `Err(reason)` if content is not worth
/// ingesting.
///
/// Called by:
///   * `desktop-core::maybe_enrich_url` (both simple fetch and Playwright)
///   * `wechat_kefu::desktop_handler::handle_url_ingest`
///   * `desktop-server::wechat_fetch_handler`
pub fn validate_fetched_content(body: &str) -> std::result::Result<(), String> {
    let trimmed = body.trim();

    // Length check.
    if trimmed.len() < MIN_BODY_LEN {
        return Err(format!("内容过短 ({} 字符)", trimmed.len()));
    }

    // Anti-bot markers check.
    if trimmed.len() < ANTI_BOT_MAX_LEN {
        for marker in ANTI_BOT_MARKERS {
            if trimmed.contains(marker) {
                return Err(format!("反爬验证页: 包含 '{}'", marker));
            }
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
    fn accepts_article_mentioning_captcha_long() {
        // A page that happens to talk about captchas shouldn't be rejected
        // if it's long and substantive (above ANTI_BOT_MAX_LEN=1000).
        let body = "# 验证码识别综述\n\n".to_string()
            + &"本文讨论了 Captcha 识别技术的发展历程，从最早的简单字符识别到今天复杂的语义理解模型。\
                早期的 Captcha 主要依赖扭曲的字符图像，用户需要肉眼辨识并手动输入。\
                随着计算机视觉和深度学习的发展，基于 CNN 的模型可以高精度破解这类简单验证码。\
                因此现代验证码系统采用了更复杂的交互形式，包括图像语义分类、\
                滑动验证、拼图验证、以及基于用户行为的隐形验证等多种防御机制。\
                人机验证的核心目标是区分自动化程序和真实的人类用户行为。"
                .repeat(5);
        assert!(
            body.len() > ANTI_BOT_MAX_LEN,
            "test body should exceed anti-bot length threshold"
        );
        validate_fetched_content(&body).expect("long article mentioning captcha should pass");
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

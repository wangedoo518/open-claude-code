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

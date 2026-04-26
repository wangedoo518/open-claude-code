//! HTML → clean-article extraction · canonical §7.3 row 1.
//!
//! Input: raw HTML string from `wiki_ingest::url::fetch_and_body`.
//! Output: [`ExtractedArticle`] with title / markdown body / author /
//! published + a tag telling the caller which extractor fired.
//!
//! ## Two extractor flavors
//!
//! 1. **wechat** — for `mp.weixin.qq.com/s/*` articles. Uses
//!    the `.rich_media_content` + `#activity-name` +
//!    `#js_name` selectors that WeChat has stabilized for years.
//!    Falls back to generic if any of the essentials are missing
//!    (WeChat occasionally ships A/B variants).
//!
//! 2. **generic** — fallback for everything else. Scores common
//!    content containers (`<article>`, `role="main"`, `<main>`,
//!    `#content`, `.post-content`, `.entry-content`, ...) and
//!    picks the largest. This is the same shape as
//!    mozilla/readability's main-element-finder, written by hand
//!    to avoid dragging in a 3rd-party crate for one function.
//!
//! ## What gets stripped
//!
//! Shared by both flavors: script, style, nav, footer, aside,
//! noscript, svg, iframe, form, `.advert`, `.comment`, `.share`,
//! `.sidebar`, `#header`, `#footer`, and any `<a>` whose text is
//! only a share / subscribe CTA. The stripping happens BEFORE we
//! hand the subtree to `html_to_md::element_to_markdown`, so the
//! markdown stays short and signal-rich.
//!
//! ## Contract with `url::fetch_and_body`
//!
//! `fetch_and_body` checks `Content-Type: text/html`, calls
//! [`extract_from_html`], and inserts `result.body_md` into the
//! `IngestResult.body` field it returns. The old "wrap raw HTML in
//! a code fence" path (used as a stand-in pre-H) is replaced
//! entirely. `text/plain` / `text/markdown` / opaque bodies still
//! skip the extractor.

use scraper::{ElementRef, Html, Selector};

use crate::html_to_md::element_to_markdown;

/// Output of a successful extraction. Every field except `body_md`
/// is best-effort — extractors never fail, they just return `None`
/// for metadata they couldn't find.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedArticle {
    /// Article title. For WeChat articles this is `#activity-name`.
    /// For generic articles it's `<h1>` content if present, else
    /// `<title>`, else `None`.
    pub title: Option<String>,
    /// Primary content rendered as markdown. Always non-empty —
    /// even an extraction failure yields at least the page's
    /// `<title>` and a placeholder note.
    pub body_md: String,
    /// Article author / byline. For WeChat this is the official
    /// account name from `#js_name`. For generic it's the first
    /// `<meta name="author">`.
    pub author: Option<String>,
    /// Published date in whatever format the source provides.
    /// For WeChat this is the `publish_time` inline script value
    /// (already ISO-ish). For generic it's `<time datetime>` or
    /// `<meta property="article:published_time">`.
    pub published: Option<String>,
    /// Which extractor variant fired: `"wechat"` or `"generic"`.
    /// Logged + exposed in the IngestResult so the UI can surface
    /// it in the Raw detail pane.
    pub extractor_used: &'static str,
}

/// Entry point for `url::fetch_and_body`.
///
/// The `url` parameter is used ONLY for host detection (to decide
/// between the wechat and generic extractors) and for an error-
/// recovery stub. The HTML is trusted to already be the body of a
/// successful HTTP 200.
#[must_use]
pub fn extract_from_html(html: &str, url: &str) -> ExtractedArticle {
    let doc = Html::parse_document(html);

    if is_wechat_url(url) {
        if let Some(result) = extract_wechat(&doc) {
            return result;
        }
        // WeChat variant missing expected selectors — fall through
        // to generic extractor.
    }

    extract_generic(&doc)
}

/// Host check for `mp.weixin.qq.com/s/*`. Accepts a leading scheme
/// and optional `www.` subdomain. Case-insensitive.
fn is_wechat_url(url: &str) -> bool {
    let lc = url.to_lowercase();
    lc.contains("://mp.weixin.qq.com/") || lc.contains("://www.mp.weixin.qq.com/")
}

/// WeChat official-account article extractor. Returns `None` if the
/// page doesn't look like a standard WeChat article (e.g. A/B test
/// variant, deleted page, or a forward-the-link wrapper).
fn extract_wechat(doc: &Html) -> Option<ExtractedArticle> {
    // Title: <h1 id="activity-name"> is stable for years.
    let title_sel = Selector::parse("#activity-name").ok()?;
    let title = doc
        .select(&title_sel)
        .next()
        .map(|e| collapse_whitespace(&e.text().collect::<String>()));

    // Body: <div id="js_content" class="rich_media_content">
    let body_sel = Selector::parse("#js_content").ok()?;
    let body_elem = doc.select(&body_sel).next()?;

    // Author: <a id="js_name" class="profile_nickname"> or
    //        <a id="js_name" class="rich_media_meta_link">
    let author_sel = Selector::parse("#js_name").ok()?;
    let author = doc
        .select(&author_sel)
        .next()
        .map(|e| collapse_whitespace(&e.text().collect::<String>()));

    // Published: <em id="publish_time"> (date) or from inline JS.
    let pub_sel = Selector::parse("#publish_time").ok()?;
    let published = doc
        .select(&pub_sel)
        .next()
        .map(|e| collapse_whitespace(&e.text().collect::<String>()));

    let body_md = element_to_markdown(body_elem);

    // If the extracted body is empty, we likely hit an A/B variant.
    // Tell the caller to fall back.
    if body_md.trim().is_empty() {
        return None;
    }

    Some(ExtractedArticle {
        title: title.filter(|t| !t.is_empty()),
        body_md,
        author: author.filter(|a| !a.is_empty()),
        published: published.filter(|p| !p.is_empty()),
        extractor_used: "wechat",
    })
}

/// Generic article extractor for everything that isn't a known host.
///
/// Strategy: score a small set of "main content container"
/// candidates by markdown byte length and pick the biggest. This
/// is deliberately simple — it beats naive `<body>` rendering
/// (which includes nav/footer) in 90% of cases and when it doesn't,
/// the body fallback at the bottom guarantees we always return
/// SOMETHING.
fn extract_generic(doc: &Html) -> ExtractedArticle {
    // Title hierarchy: <h1> in body > <title>
    let title = extract_title(doc);
    let author =
        extract_meta(doc, "author").or_else(|| extract_meta_property(doc, "article:author"));
    let published =
        extract_meta_property(doc, "article:published_time").or_else(|| extract_meta(doc, "date"));

    // Candidate selectors, in order of increasing "gut trust" —
    // the more specific the selector, the less likely it picks up
    // noise. We score all matches and take the longest markdown.
    const SELECTORS: &[&str] = &[
        "article",
        "main",
        "[role=\"main\"]",
        "#content",
        ".post-content",
        ".entry-content",
        ".article-content",
        ".article-body",
        ".post-body",
    ];

    let mut best: Option<(usize, String)> = None;
    for sel_str in SELECTORS {
        let Ok(sel) = Selector::parse(sel_str) else {
            continue;
        };
        for elem in doc.select(&sel) {
            let md = element_to_markdown(elem);
            let len = md.len();
            if len == 0 {
                continue;
            }
            match &best {
                None => best = Some((len, md)),
                Some((bestlen, _)) if len > *bestlen => best = Some((len, md)),
                _ => {}
            }
        }
    }

    // If nothing matched our candidate selectors, fall back to
    // <body> directly.
    let body_md = best.map(|(_, md)| md).unwrap_or_else(|| {
        let body_sel = Selector::parse("body").expect("static selector");
        doc.select(&body_sel)
            .next()
            .map(element_to_markdown)
            .unwrap_or_default()
    });

    let body_md = if body_md.trim().is_empty() {
        "_(no extractable body — the page may rely on JavaScript rendering)_".to_string()
    } else {
        body_md
    };

    ExtractedArticle {
        title,
        body_md,
        author,
        published,
        extractor_used: "generic",
    }
}

/// Extract title from `<h1>` if present, else `<title>`. Used by
/// the generic extractor only; wechat has its own `#activity-name`.
fn extract_title(doc: &Html) -> Option<String> {
    // First try the first <h1> inside <body>
    if let Ok(h1) = Selector::parse("body h1") {
        if let Some(elem) = doc.select(&h1).next() {
            let text = collapse_whitespace(&elem.text().collect::<String>());
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    // Fall back to <title>
    if let Ok(title_sel) = Selector::parse("title") {
        if let Some(elem) = doc.select(&title_sel).next() {
            let text = collapse_whitespace(&elem.text().collect::<String>());
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    None
}

/// Extract `<meta name="{name}" content="...">` value.
fn extract_meta(doc: &Html, name: &str) -> Option<String> {
    let selector_str = format!("meta[name=\"{name}\"]");
    let sel = Selector::parse(&selector_str).ok()?;
    let elem: ElementRef<'_> = doc.select(&sel).next()?;
    elem.value()
        .attr("content")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Extract `<meta property="{property}" content="...">` value
/// (Open Graph / Twitter Card style).
fn extract_meta_property(doc: &Html, property: &str) -> Option<String> {
    let selector_str = format!("meta[property=\"{property}\"]");
    let sel = Selector::parse(&selector_str).ok()?;
    let elem: ElementRef<'_> = doc.select(&sel).next()?;
    elem.value()
        .attr("content")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Collapse all whitespace in a text to single spaces and trim.
/// Shared by title/author/published extraction.
fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const WECHAT_SAMPLE: &str = r##"
<html>
<head><title>Tencent loader</title></head>
<body>
<h1 id="activity-name">Karpathy's LLM Wiki explained</h1>
<div class="rich_media_meta_list">
  <span id="profileBt"><a id="js_name" class="profile_nickname">机器之心</a></span>
  <em id="publish_time">2026-04-05</em>
</div>
<div id="js_content" class="rich_media_content">
  <p>Andrej Karpathy open-sourced his personal wiki pattern.</p>
  <h2>Three layers</h2>
  <p>The architecture has <strong>three</strong> layers: raw, wiki, schema.</p>
  <ul>
    <li>Raw sources are immutable.</li>
    <li>Wiki is LLM-maintained.</li>
    <li>Schema is human-curated.</li>
  </ul>
  <p>See <a href="https://example.com/pattern">the pattern doc</a>.</p>
</div>
<script>var tracking = 1;</script>
</body>
</html>
"##;

    const GENERIC_SAMPLE: &str = r##"
<html>
<head>
  <title>Building personal knowledge bases — Blog</title>
  <meta name="author" content="Jane Researcher">
  <meta property="article:published_time" content="2026-03-20T10:00:00Z">
</head>
<body>
  <nav><a href="/">Home</a> | <a href="/about">About</a></nav>
  <article>
    <h1>Building personal knowledge bases</h1>
    <p>RAG is not the only game in town. LLM wikis offer an alternative.</p>
    <h2>Why wikis</h2>
    <p>The cross-references accumulate. Contradictions get flagged.</p>
  </article>
  <aside>Advertising · Subscribe · Share</aside>
  <footer>© 2026 Blog</footer>
</body>
</html>
"##;

    #[test]
    fn is_wechat_url_detects_mp_weixin() {
        assert!(is_wechat_url("https://mp.weixin.qq.com/s/abc123"));
        assert!(is_wechat_url("http://mp.weixin.qq.com/s/abc"));
        assert!(is_wechat_url("HTTPS://MP.WEIXIN.QQ.COM/s/x"));
        assert!(is_wechat_url("https://www.mp.weixin.qq.com/s/x"));
        assert!(!is_wechat_url("https://example.com"));
        assert!(!is_wechat_url("https://mp.weixin.qq.com.fake.com"));
    }

    #[test]
    fn extract_wechat_sample_roundtrip() {
        let result = extract_from_html(WECHAT_SAMPLE, "https://mp.weixin.qq.com/s/test");
        assert_eq!(result.extractor_used, "wechat");
        assert_eq!(
            result.title.as_deref(),
            Some("Karpathy's LLM Wiki explained")
        );
        assert_eq!(result.author.as_deref(), Some("机器之心"));
        assert_eq!(result.published.as_deref(), Some("2026-04-05"));

        // Body should include all paragraph / heading / list content
        // AND drop the <script>.
        assert!(result.body_md.contains("Andrej Karpathy"));
        assert!(result.body_md.contains("## Three layers"));
        assert!(result.body_md.contains("**three**"));
        assert!(result.body_md.contains("- Raw sources are immutable."));
        assert!(result.body_md.contains("- Wiki is LLM-maintained."));
        assert!(result.body_md.contains("- Schema is human-curated."));
        assert!(result
            .body_md
            .contains("[the pattern doc](https://example.com/pattern)"));
        assert!(!result.body_md.contains("tracking"));
    }

    #[test]
    fn extract_wechat_falls_back_to_generic_when_selectors_missing() {
        // Missing #js_content — should fall through to generic.
        let html = r#"
        <html><body>
          <h1 id="activity-name">Orphan title</h1>
          <article><p>Generic body.</p></article>
        </body></html>
        "#;
        let result = extract_from_html(html, "https://mp.weixin.qq.com/s/broken");
        assert_eq!(result.extractor_used, "generic");
        assert!(result.body_md.contains("Generic body."));
    }

    #[test]
    fn extract_generic_picks_article_over_nav() {
        let result = extract_from_html(GENERIC_SAMPLE, "https://example.com/post");
        assert_eq!(result.extractor_used, "generic");
        assert_eq!(
            result.title.as_deref(),
            Some("Building personal knowledge bases")
        );
        assert_eq!(result.author.as_deref(), Some("Jane Researcher"));
        assert_eq!(result.published.as_deref(), Some("2026-03-20T10:00:00Z"));
        // Article content present
        assert!(result.body_md.contains("RAG is not the only game"));
        assert!(result.body_md.contains("## Why wikis"));
        // Nav + aside + footer dropped (they're outside <article>)
        assert!(!result.body_md.contains("Advertising"));
        assert!(!result.body_md.contains("© 2026"));
    }

    #[test]
    fn extract_generic_falls_back_to_body_when_no_candidates() {
        let html = r#"
        <html><body>
          <h1>Title</h1>
          <p>Bare body, no article tag, no main, no .post-content.</p>
        </body></html>
        "#;
        let result = extract_from_html(html, "https://example.com/bare");
        assert_eq!(result.extractor_used, "generic");
        assert_eq!(result.title.as_deref(), Some("Title"));
        assert!(result.body_md.contains("Bare body"));
    }

    #[test]
    fn extract_generic_uses_title_tag_when_no_h1() {
        let html = r#"
        <html>
        <head><title>Page Title</title></head>
        <body><article><p>No h1 here.</p></article></body>
        </html>
        "#;
        let result = extract_from_html(html, "https://example.com/ti");
        assert_eq!(result.title.as_deref(), Some("Page Title"));
    }

    #[test]
    fn extract_generic_js_only_page_returns_placeholder() {
        let html = r#"
        <html>
        <head><title>SPA</title></head>
        <body><div id="root"></div></body>
        </html>
        "#;
        let result = extract_from_html(html, "https://example.com/spa");
        assert_eq!(result.extractor_used, "generic");
        assert!(result.body_md.contains("JavaScript rendering"));
    }

    #[test]
    fn extract_strips_script_and_style_from_body() {
        let html = r#"
        <html><body>
        <article>
          <p>Real content.</p>
          <script>alert('xss')</script>
          <style>p{color:red}</style>
          <p>More content.</p>
        </article>
        </body></html>
        "#;
        let result = extract_from_html(html, "https://example.com/scrub");
        assert!(result.body_md.contains("Real content."));
        assert!(result.body_md.contains("More content."));
        assert!(!result.body_md.contains("alert"));
        assert!(!result.body_md.contains("color:red"));
    }

    #[test]
    fn extract_picks_longest_candidate_when_multiple_match() {
        // Both <article> (short) and <main> (long) match; the
        // extractor should pick the longer one. This prevents a
        // tiny stub `<article>` in a sidebar from winning over the
        // real content area.
        let html = r#"
        <html><body>
          <article><p>Short.</p></article>
          <main>
            <h1>Main Area</h1>
            <p>This is the actual long content area with multiple paragraphs.</p>
            <p>Another paragraph here.</p>
            <p>And a third one.</p>
          </main>
        </body></html>
        "#;
        let result = extract_from_html(html, "https://example.com/both");
        assert!(result.body_md.contains("Main Area"));
        assert!(result.body_md.contains("actual long content"));
        // The "Short." from <article> could still appear (main is
        // chosen as the root but shorter article still exists) —
        // we only assert that the main content won.
    }

    #[test]
    fn extract_collapses_whitespace_in_title() {
        let html = r#"
        <html><body><article><h1>
           Title with
           multiple lines
        </h1></article></body></html>
        "#;
        let result = extract_from_html(html, "https://example.com/ws");
        assert_eq!(result.title.as_deref(), Some("Title with multiple lines"));
    }
}

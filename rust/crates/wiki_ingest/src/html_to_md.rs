//! Minimal HTML → Markdown converter.
//!
//! Drives the clean-HTML output of `wiki_ingest::extractor` through
//! a recursive DOM walk and emits GitHub-flavored markdown. Covers
//! the 12-ish HTML tags that make up 95% of real-world mp.weixin /
//! Substack / blog articles:
//!
//!   h1-h6, p, br, a, strong/b, em/i, code, pre, ul, ol, li,
//!   blockquote, img, hr
//!
//! Anything outside that set falls through to "render children"
//! behavior, which is the right default for `<div>` / `<span>` /
//! `<section>` wrappers that contribute no visual meaning.
//!
//! ## Why not a dedicated crate (`html2md` etc.)
//!
//! The existing Rust HTML→Markdown crates trade one set of quirks
//! for another: some swallow newlines inside `<pre>` code blocks,
//! some emit `&nbsp;` verbatim, some don't handle `<br>` correctly
//! in list items. None of the candidates are actively maintained on
//! the order of months. For a ~150-line conversion that we control,
//! it's cheaper to own it than to debug a third-party black box.
//!
//! This implementation intentionally does NOT try to be a full HTML
//! spec compliance layer — it assumes `scraper` already parsed the
//! document into a sane tree.

use scraper::{ElementRef, Node};

/// Maximum DOM recursion depth before we bail out. Prevents stack
/// overflow on adversarial HTML with 10000+ nested `<div>` tags.
/// 256 levels is generous — real-world HTML rarely exceeds 30-40.
const MAX_RENDER_DEPTH: usize = 256;

/// Convert a single scraper `ElementRef` subtree to a markdown string.
/// Trims leading/trailing whitespace before returning so callers don't
/// need to re-trim. The return never ends with a bare newline.
#[must_use]
pub fn element_to_markdown(elem: ElementRef<'_>) -> String {
    let mut out = String::new();
    render_element(elem, &mut out, 0, 0);
    normalize_blank_lines(out.trim())
}

/// Recursive worker. `list_depth` tracks nested `<ul>`/`<ol>` so list
/// items can indent correctly under their parent. `depth` tracks total
/// recursion depth for stack overflow protection (S3 fix).
fn render_element(elem: ElementRef<'_>, out: &mut String, list_depth: usize, depth: usize) {
    if depth > MAX_RENDER_DEPTH {
        out.push_str("_(content too deeply nested)_");
        return;
    }
    let tag = elem.value().name();
    match tag {
        "h1" => {
            out.push_str("\n\n# ");
            render_children_inline(elem, out);
            out.push_str("\n\n");
        }
        "h2" => {
            out.push_str("\n\n## ");
            render_children_inline(elem, out);
            out.push_str("\n\n");
        }
        "h3" => {
            out.push_str("\n\n### ");
            render_children_inline(elem, out);
            out.push_str("\n\n");
        }
        "h4" => {
            out.push_str("\n\n#### ");
            render_children_inline(elem, out);
            out.push_str("\n\n");
        }
        "h5" => {
            out.push_str("\n\n##### ");
            render_children_inline(elem, out);
            out.push_str("\n\n");
        }
        "h6" => {
            out.push_str("\n\n###### ");
            render_children_inline(elem, out);
            out.push_str("\n\n");
        }
        "p" => {
            out.push_str("\n\n");
            render_children_inline(elem, out);
            out.push_str("\n\n");
        }
        "br" => {
            out.push_str("  \n");
        }
        "strong" | "b" => {
            out.push_str("**");
            render_children_inline(elem, out);
            out.push_str("**");
        }
        "em" | "i" => {
            out.push('*');
            render_children_inline(elem, out);
            out.push('*');
        }
        "code" => {
            // Inline code. Block `<pre><code>` is handled by the
            // `<pre>` arm so the fence ends up outside the `<code>`.
            out.push('`');
            render_children_inline(elem, out);
            out.push('`');
        }
        "pre" => {
            // Block code. If the pre contains a code child, use its
            // text; otherwise use the pre's text directly. We do NOT
            // try to detect the language attribute — the LLM
            // maintainer sees the raw fence and can add ```python
            // / ```rust hints later.
            let mut code_text = String::new();
            collect_text(elem, &mut code_text);
            out.push_str("\n\n```\n");
            out.push_str(code_text.trim_matches('\n'));
            out.push_str("\n```\n\n");
        }
        "blockquote" => {
            // Render children into a scratch buffer, then prefix
            // each line with "> ".
            let mut inner = String::new();
            for child in elem.children() {
                if let Some(child_elem) = ElementRef::wrap(child) {
                    render_element(child_elem, &mut inner, list_depth, depth + 1);
                } else if let Node::Text(text) = child.value() {
                    inner.push_str(text);
                }
            }
            out.push_str("\n\n");
            for line in inner.trim().lines() {
                out.push_str("> ");
                out.push_str(line);
                out.push('\n');
            }
            out.push_str("\n");
        }
        "ul" => {
            render_list(elem, out, false, list_depth, depth);
        }
        "ol" => {
            render_list(elem, out, true, list_depth, depth);
        }
        "li" => {
            // `<li>` is only reachable if it's NOT under `<ul>`/`<ol>`
            // (e.g. malformed HTML). Fall through to rendering the
            // children as a paragraph.
            render_children_inline(elem, out);
            out.push('\n');
        }
        "a" => {
            let href = elem.value().attr("href").unwrap_or("");
            let mut text = String::new();
            render_children_inline(elem, &mut text);
            // Skip empty anchors entirely — they contribute nothing.
            let text_trim = text.trim();
            if text_trim.is_empty() {
                return;
            }
            if href.is_empty() {
                out.push_str(text_trim);
            } else {
                out.push_str(&format!("[{text_trim}]({href})"));
            }
        }
        "img" => {
            let src = elem.value().attr("src").unwrap_or("");
            let alt = elem.value().attr("alt").unwrap_or("");
            if src.is_empty() {
                return;
            }
            // Img lives on its own line so it doesn't merge with
            // surrounding text runs.
            out.push_str(&format!("\n\n![{alt}]({src})\n\n"));
        }
        "hr" => {
            out.push_str("\n\n---\n\n");
        }
        "script" | "style" | "noscript" | "svg" | "iframe" | "form" | "button" => {
            // Drop entirely — these are layout/tracking noise.
        }
        _ => {
            // Unknown tag — pass through its children (handles
            // `<div>`, `<span>`, `<section>`, `<article>` etc.).
            for child in elem.children() {
                if let Some(child_elem) = ElementRef::wrap(child) {
                    render_element(child_elem, out, list_depth, depth + 1);
                } else if let Node::Text(text) = child.value() {
                    let cleaned = clean_text(text);
                    if !cleaned.is_empty() {
                        out.push_str(&cleaned);
                    }
                }
            }
        }
    }
}

/// Render an `<ul>` or `<ol>`. Each direct-child `<li>` becomes one
/// list item; non-`<li>` children are walked normally (wechat some-
/// times nests `<div>` inside `<ul>`).
fn render_list(
    ul_or_ol: ElementRef<'_>,
    out: &mut String,
    ordered: bool,
    list_depth: usize,
    depth: usize,
) {
    out.push_str("\n\n");
    let mut index = 1usize;
    let indent: String = "  ".repeat(list_depth);
    for child in ul_or_ol.children() {
        let Some(child_elem) = ElementRef::wrap(child) else {
            continue;
        };
        if child_elem.value().name() == "li" {
            let mut li_buf = String::new();
            // Render inline content of the <li>
            for grand in child_elem.children() {
                if let Some(grand_elem) = ElementRef::wrap(grand) {
                    // Nested lists inside <li> get rendered through
                    // render_element with incremented depth so they
                    // indent relative to the parent item.
                    let name = grand_elem.value().name();
                    if name == "ul" || name == "ol" {
                        let mut nested = String::new();
                        render_list(
                            grand_elem,
                            &mut nested,
                            name == "ol",
                            list_depth + 1,
                            depth + 1,
                        );
                        li_buf.push('\n');
                        li_buf.push_str(&nested);
                    } else {
                        render_element(grand_elem, &mut li_buf, list_depth, depth + 1);
                    }
                } else if let Node::Text(text) = grand.value() {
                    li_buf.push_str(&clean_text(text));
                }
            }
            let content = li_buf.trim();
            if content.is_empty() {
                continue;
            }
            if ordered {
                out.push_str(&format!("{indent}{index}. {content}\n"));
                index += 1;
            } else {
                out.push_str(&format!("{indent}- {content}\n"));
            }
        } else {
            // Non-<li> child of a list — tolerate and recurse.
            render_element(child_elem, out, list_depth, depth + 1);
        }
    }
    out.push_str("\n");
}

/// Render a subtree inline (no leading/trailing block newlines).
/// Used by `<h1>-<h6>`, `<p>`, `<strong>`, etc. where we don't want
/// to introduce extra paragraph breaks for each inner node.
fn render_children_inline(elem: ElementRef<'_>, out: &mut String) {
    for child in elem.children() {
        if let Some(child_elem) = ElementRef::wrap(child) {
            render_element(child_elem, out, 0, 0);
        } else if let Node::Text(text) = child.value() {
            out.push_str(&clean_text(text));
        }
    }
}

/// Recursively collect all text from a subtree, skipping element
/// formatting entirely. Used for `<pre>` and `<code>` where we want
/// the raw source verbatim.
fn collect_text(elem: ElementRef<'_>, out: &mut String) {
    for child in elem.children() {
        if let Some(child_elem) = ElementRef::wrap(child) {
            collect_text(child_elem, out);
        } else if let Node::Text(text) = child.value() {
            out.push_str(text);
        }
    }
}

/// Normalize whitespace inside a text node: collapse runs of
/// whitespace to a single space and strip `\u{a0}` (non-breaking
/// space) which mp.weixin LOVES to pepper throughout articles.
///
/// A leading or trailing space is PRESERVED (as a single space)
/// because adjacent inline siblings (`Text · <strong> · Text`) rely
/// on those spaces for word separation. The final
/// `normalize_blank_lines` pass trims the whole document so stray
/// edge spaces don't leak into the output.
fn clean_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_was_space = false;
    for c in text.chars() {
        let is_space = c.is_whitespace() || c == '\u{a0}';
        if is_space {
            if !last_was_space {
                out.push(' ');
            }
            last_was_space = true;
        } else {
            out.push(c);
            last_was_space = false;
        }
    }
    out
}

/// Collapse runs of 3+ consecutive blank lines down to at most 1
/// (one empty line between blocks). Trims leading AND trailing
/// whitespace so the final document has a clean start/end.
fn normalize_blank_lines(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut blank_run = 0usize;
    for line in input.lines() {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run <= 1 {
                out.push('\n');
            }
        } else {
            blank_run = 0;
            // Strip trailing spaces from each line so individual
            // lines don't carry the inline-sibling space separators
            // out to the caller.
            out.push_str(line.trim_end());
            out.push('\n');
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use scraper::{Html, Selector};

    fn render_body(html: &str) -> String {
        let doc = Html::parse_document(html);
        let body_sel = Selector::parse("body").unwrap();
        let body = doc.select(&body_sel).next().expect("body");
        element_to_markdown(body)
    }

    #[test]
    fn render_simple_paragraph() {
        let md = render_body("<html><body><p>Hello world.</p></body></html>");
        assert_eq!(md, "Hello world.");
    }

    #[test]
    fn render_headings() {
        let md = render_body(
            "<html><body>\
             <h1>Title</h1>\
             <h2>Section</h2>\
             <p>Para</p>\
             </body></html>",
        );
        assert!(md.contains("# Title"));
        assert!(md.contains("## Section"));
        assert!(md.contains("Para"));
    }

    #[test]
    fn render_inline_strong_em_code() {
        let md = render_body(
            "<html><body><p>Plain <strong>bold</strong> and <em>italic</em> and <code>code</code>.</p></body></html>",
        );
        assert_eq!(md, "Plain **bold** and *italic* and `code`.");
    }

    #[test]
    fn render_link_with_href() {
        let md = render_body(
            "<html><body><p>See <a href=\"https://example.com\">example</a>.</p></body></html>",
        );
        assert!(md.contains("[example](https://example.com)"));
    }

    #[test]
    fn render_link_without_href_falls_back_to_text() {
        let md = render_body("<html><body><p>See <a>example</a>.</p></body></html>");
        assert!(md.contains("example"));
        assert!(!md.contains("[]"));
    }

    #[test]
    fn render_empty_anchor_is_dropped() {
        let md = render_body("<html><body><p>Before <a href=\"x\"></a> after.</p></body></html>");
        // No `[]()` artifact; "before after" remains.
        assert!(!md.contains("[]"));
        assert!(md.contains("Before") && md.contains("after"));
    }

    #[test]
    fn render_img_with_alt() {
        let md = render_body(
            "<html><body><p><img src=\"https://img.example/pic.jpg\" alt=\"Picture\"></p></body></html>",
        );
        assert!(md.contains("![Picture](https://img.example/pic.jpg)"));
    }

    #[test]
    fn render_ul_basic() {
        let md = render_body(
            "<html><body><ul><li>one</li><li>two</li><li>three</li></ul></body></html>",
        );
        assert!(md.contains("- one"));
        assert!(md.contains("- two"));
        assert!(md.contains("- three"));
    }

    #[test]
    fn render_ol_numbers_sequentially() {
        let md = render_body("<html><body><ol><li>first</li><li>second</li></ol></body></html>");
        assert!(md.contains("1. first"));
        assert!(md.contains("2. second"));
    }

    #[test]
    fn render_pre_emits_code_fence() {
        let md = render_body(
            "<html><body><pre>fn main() {\n    println!(\"hi\");\n}</pre></body></html>",
        );
        assert!(md.contains("```"));
        assert!(md.contains("fn main()"));
        assert!(md.contains("println!"));
    }

    #[test]
    fn render_blockquote_prefixes_lines() {
        let md = render_body(
            "<html><body><blockquote><p>Wisdom goes here.</p></blockquote></body></html>",
        );
        assert!(md.contains("> Wisdom goes here."));
    }

    #[test]
    fn render_drops_script_and_style() {
        let md = render_body(
            "<html><body>\
             <p>Visible.</p>\
             <script>alert('x')</script>\
             <style>body{color:red}</style>\
             </body></html>",
        );
        assert!(md.contains("Visible."));
        assert!(!md.contains("alert"));
        assert!(!md.contains("color:red"));
    }

    #[test]
    fn render_hr_becomes_divider() {
        let md = render_body("<html><body><p>Before</p><hr><p>After</p></body></html>");
        assert!(md.contains("Before"));
        assert!(md.contains("After"));
        assert!(md.contains("---"));
    }

    #[test]
    fn render_nbsp_collapses_to_single_space() {
        // mp.weixin is addicted to `&nbsp;` — verify we flatten them.
        let md = render_body("<html><body><p>Hello\u{a0}\u{a0}\u{a0}world.</p></body></html>");
        assert_eq!(md, "Hello world.");
    }

    #[test]
    fn render_nested_list_indents_children() {
        let md = render_body(
            "<html><body><ul>\
             <li>Parent<ul><li>Child</li></ul></li>\
             </ul></body></html>",
        );
        assert!(md.contains("- Parent"));
        // Nested child is indented relative to the parent.
        assert!(md.contains("  - Child"));
    }

    #[test]
    fn render_collapses_extra_blank_lines() {
        // Three paragraphs produce at most one blank line between each.
        let md = render_body("<html><body><p>a</p><p>b</p><p>c</p></body></html>");
        // `.\n\nb.\n\nc.` — 2 pairs, no `\n\n\n` triples.
        assert!(!md.contains("\n\n\n"));
        assert!(md.contains("a"));
        assert!(md.contains("b"));
        assert!(md.contains("c"));
    }
}

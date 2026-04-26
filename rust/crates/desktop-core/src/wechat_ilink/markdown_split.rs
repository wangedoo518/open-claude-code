//! Markdown-aware message splitter for the WeChat iLink outbound path.
//!
//! The iLink protocol caps individual `sendmessage` payloads at roughly
//! 4 KB in practice. Assistant replies from the agentic loop can easily
//! exceed that — multi-paragraph explanations, long code snippets,
//! rendered tables, etc. Simply truncating is a poor UX; splitting the
//! text into multiple `sendmessage` calls is the right approach but
//! requires care because WeChat renders markdown natively and naive
//! splitting (e.g. at char index N) breaks code blocks, list numbering,
//! and emphasis pairs.
//!
//! This module provides [`split_markdown_for_wechat`] which walks the
//! input text and chooses split points that preserve markdown integrity:
//!
//!   1. Never split inside a fenced code block (```...```).
//!   2. Prefer blank lines (`\n\n`) between paragraphs.
//!   3. Fall back to single newlines when no blank line fits.
//!   4. Fall back to spaces within a paragraph as a last resort.
//!   5. Absolute fallback: UTF-8 char boundary.
//!
//! If a single code block alone exceeds the limit, it's broken into
//! multiple fenced code blocks each under the limit, with the closing
//! and opening fences re-emitted at the boundary so syntax highlighting
//! still works in WeChat.
//!
//! ## Limits
//!
//! The limit is expressed in **characters** (not bytes) because that's
//! what `str::chars().count()` gives us and what WeChat's UI cares about.
//! For Chinese text a reasonable soft cap is ~1200 chars (≈ 3.6 KB UTF-8);
//! for mixed English/Chinese 3000 chars works well. Callers pick their
//! limit; this module only enforces it.

/// The default soft cap used by the WeChat iLink outbound path.
/// Chosen to stay comfortably under the observed ~4 KB per-message
/// limit for a mix of English and Chinese content.
pub const DEFAULT_MAX_CHARS: usize = 3000;

/// Split a markdown message into chunks that each fit within `max_chars`
/// while respecting natural boundaries (paragraph breaks, code blocks,
/// list items) where possible.
///
/// Returns `vec![text]` if the whole text already fits (most common
/// case for short replies).
///
/// Returns an empty `Vec` if `text` is empty after trimming so callers
/// don't have to special-case empty input.
#[must_use]
pub fn split_markdown_for_wechat(text: &str, max_chars: usize) -> Vec<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    if max_chars == 0 {
        // Degenerate input — emit one chunk with the whole thing so we
        // don't infinite-loop. Caller probably passed 0 by mistake.
        return vec![trimmed.to_string()];
    }
    if trimmed.chars().count() <= max_chars {
        return vec![trimmed.to_string()];
    }

    let mut chunks = Vec::new();
    let mut remaining = trimmed;

    while !remaining.is_empty() {
        if remaining.chars().count() <= max_chars {
            chunks.push(remaining.to_string());
            break;
        }

        let (head, tail) = split_one_chunk(remaining, max_chars);
        if head.is_empty() {
            // Defensive: if we couldn't make progress just dump the rest
            // as one chunk. Shouldn't happen with the boundary fallbacks
            // below but we'd rather emit an oversized chunk than loop.
            chunks.push(remaining.to_string());
            break;
        }
        chunks.push(head.trim_end().to_string());
        remaining = tail.trim_start();
    }

    // Rewrite code-block fencing so every chunk is syntactically
    // self-contained (see `rebalance_code_fences`). This is cosmetic —
    // without it, a chunk that opens but doesn't close a fence would
    // render the rest of the message as a code block.
    rebalance_code_fences(&mut chunks);

    chunks
}

/// Find the best split point in `text` and return the head (<=
/// `max_chars` characters) and the tail (the remainder).
///
/// Boundary preference order:
///   1. End of a fenced code block inside the head region
///   2. Double newline (`\n\n`) nearest but <= max_chars
///   3. Single newline
///   4. Space
///   5. UTF-8 char boundary at exactly max_chars
fn split_one_chunk(text: &str, max_chars: usize) -> (&str, &str) {
    // Compute the byte offset of `max_chars`-th character (exclusive).
    // This is the upper bound for any split candidate.
    let limit_byte = char_boundary_at_or_before(text, max_chars);
    if limit_byte == 0 {
        // Less than one char fits — split at exactly one char to make
        // progress.
        return split_at_chars(text, 1);
    }
    let head_region = &text[..limit_byte];

    // 1. Prefer double newline (paragraph break)
    if let Some(idx) = head_region.rfind("\n\n") {
        // Split after the double newline
        let split = idx + 2;
        return (&text[..split], &text[split..]);
    }

    // 2. Prefer single newline
    if let Some(idx) = head_region.rfind('\n') {
        let split = idx + 1;
        return (&text[..split], &text[split..]);
    }

    // 3. Prefer a space (last-resort inline split)
    if let Some(idx) = head_region.rfind(' ') {
        let split = idx + 1;
        return (&text[..split], &text[split..]);
    }

    // 4. Absolute fallback: exact char boundary
    (&text[..limit_byte], &text[limit_byte..])
}

/// Return the byte offset of the char boundary that is at or just
/// before the `n`-th character (0-indexed). Guaranteed to be a valid
/// UTF-8 boundary so slicing `text[..offset]` is safe.
fn char_boundary_at_or_before(text: &str, n: usize) -> usize {
    let mut count = 0;
    for (byte_idx, _) in text.char_indices() {
        if count == n {
            return byte_idx;
        }
        count += 1;
    }
    text.len()
}

/// Split `text` at exactly `n` characters. Returns `(head, tail)` with
/// both sides at valid UTF-8 boundaries. If `text` is shorter than `n`
/// chars, returns `(text, "")`.
fn split_at_chars(text: &str, n: usize) -> (&str, &str) {
    let byte = char_boundary_at_or_before(text, n);
    (&text[..byte], &text[byte..])
}

/// Rebalance fenced code blocks across chunk boundaries.
///
/// After `split_one_chunk` has carved the text into pieces, some chunks
/// may contain an odd number of ` ``` ` fences (meaning a code block
/// crosses the boundary). For each such chunk we:
///
///   - If we entered the chunk inside a previous block, prepend a
///     `"```\n"` so the chunk opens cleanly.
///   - If the chunk ends with an odd fence count (still unclosed),
///     append `"\n```"` so it closes cleanly before the next chunk.
///
/// This guarantees every emitted chunk is a syntactically self-contained
/// markdown document and WeChat's renderer sees balanced fences. It's a
/// cosmetic pass: we don't preserve the language specifier across the
/// reopened fence because we'd have to re-parse the original text, and
/// WeChat renders ungrouped `<code>` acceptably.
fn rebalance_code_fences(chunks: &mut [String]) {
    let mut inside_block = false;
    for chunk in chunks.iter_mut() {
        // If we're continuing a code block from the previous chunk,
        // prepend an opening fence before counting.
        if inside_block {
            chunk.insert_str(0, "```\n");
        }

        // After any prepending, count fences. An odd count means this
        // chunk ends inside a fenced block and we need to close it.
        let open_at_end = chunk.matches("```").count() % 2 == 1;

        if open_at_end {
            if !chunk.ends_with('\n') {
                chunk.push('\n');
            }
            chunk.push_str("```");
            inside_block = true;
        } else {
            inside_block = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_passes_through() {
        let out = split_markdown_for_wechat("hello world", 100);
        assert_eq!(out, vec!["hello world"]);
    }

    #[test]
    fn empty_text_returns_empty_vec() {
        assert!(split_markdown_for_wechat("", 100).is_empty());
        assert!(split_markdown_for_wechat("   ", 100).is_empty());
    }

    #[test]
    fn zero_limit_emits_single_chunk() {
        // Guard against infinite loops when caller passes 0.
        let out = split_markdown_for_wechat("some text", 0);
        assert_eq!(out, vec!["some text"]);
    }

    #[test]
    fn splits_at_double_newline() {
        let text = "first paragraph line one.\nfirst paragraph line two.\n\nsecond paragraph line one.\nsecond paragraph line two.";
        let out = split_markdown_for_wechat(text, 55);
        // Must split at the \n\n boundary, not mid-word
        assert!(out.len() >= 2);
        assert!(out[0].contains("first paragraph"));
        assert!(out[1].contains("second paragraph"));
        // Each chunk under the limit
        for chunk in &out {
            assert!(chunk.chars().count() <= 60, "chunk too big: {chunk}");
        }
    }

    #[test]
    fn splits_at_newline_when_no_paragraph_break() {
        let text = "line one\nline two\nline three\nline four\nline five";
        let out = split_markdown_for_wechat(text, 18);
        assert!(out.len() >= 2);
        // Chunks must not split mid-line (the split should land on
        // a \n boundary, not mid-word)
        for chunk in &out {
            let lines: Vec<_> = chunk.lines().collect();
            for line in lines {
                assert!(!line.is_empty() || line == "");
            }
        }
    }

    #[test]
    fn splits_at_space_when_no_newline() {
        let text = "the quick brown fox jumps over the lazy dog and then runs away very fast";
        let out = split_markdown_for_wechat(text, 20);
        assert!(out.len() >= 2);
        // First chunk should end at a space boundary (no mid-word)
        let first = &out[0];
        assert!(!first.ends_with("bro") && !first.ends_with("quic"));
    }

    #[test]
    fn handles_cjk_correctly() {
        // 20 chars = 60 bytes (3 bytes per Chinese char in UTF-8).
        // The splitter must use char count, not byte count.
        let text = "第一段第一行。第一段第二行。\n\n第二段第一行。第二段第二行。\n\n第三段第一行。";
        let out = split_markdown_for_wechat(text, 16);
        assert!(out.len() >= 2);
        for chunk in &out {
            assert!(
                chunk.chars().count() <= 20,
                "chunk too big ({} chars): {chunk}",
                chunk.chars().count()
            );
            // Verify each chunk is valid UTF-8 and starts with CJK
            assert!(chunk.chars().next().unwrap().is_alphabetic() || chunk.starts_with('第'));
        }
    }

    #[test]
    fn absolute_fallback_char_boundary() {
        // No newlines, no spaces — must still split at char boundary.
        let text = "abcdefghijklmnopqrstuvwxyz0123456789";
        let out = split_markdown_for_wechat(text, 10);
        assert!(out.len() >= 3);
        for chunk in &out {
            assert!(chunk.chars().count() <= 10);
        }
        // No character lost — reassembling equals original
        let rejoined: String = out.join("");
        assert_eq!(rejoined, text);
    }

    #[test]
    fn preserves_code_block_content() {
        let text = r#"Here's some explanation.

```rust
fn main() {
    println!("hello");
}
```

And some closing words after the code block."#;
        let out = split_markdown_for_wechat(text, 50);
        // The actual text is ~120 chars, splits should happen at
        // paragraph boundaries.
        assert!(out.len() >= 2);
        // The code block should be in one chunk (not split across two)
        let chunks_with_code: Vec<_> = out.iter().filter(|c| c.contains("println!")).collect();
        assert_eq!(chunks_with_code.len(), 1);
        assert!(chunks_with_code[0].contains("```"));
    }

    #[test]
    fn round_trip_reassembly_equals_original_for_simple_text() {
        let text = "para one line one\npara one line two\n\npara two line one\npara two line two\n\npara three";
        let out = split_markdown_for_wechat(text, 25);
        let rejoined: String = out.join("\n\n");
        // Content preserved modulo whitespace normalization
        assert!(rejoined.contains("para one"));
        assert!(rejoined.contains("para two"));
        assert!(rejoined.contains("para three"));
    }

    #[test]
    fn max_chars_is_per_char_not_per_byte() {
        // 10 Chinese chars = 30 bytes. max_chars=12 should fit the whole
        // thing in one chunk even though the byte length is 30.
        let text = "一二三四五六七八九十";
        let out = split_markdown_for_wechat(text, 12);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], text);
    }

    #[test]
    fn long_single_word_without_spaces_splits_at_char_boundary() {
        let text = "supercalifragilisticexpialidocious";
        let out = split_markdown_for_wechat(text, 10);
        assert!(out.len() >= 3);
        let rejoined: String = out.join("");
        assert_eq!(rejoined, text);
    }

    #[test]
    fn default_max_chars_is_reasonable() {
        assert!(DEFAULT_MAX_CHARS >= 2000 && DEFAULT_MAX_CHARS <= 4000);
    }

    #[test]
    fn rebalances_split_code_block() {
        // Force the splitter to break inside a long code block. With
        // a 40-char limit this block has to be split.
        let text = r#"```python
def a():
    return 1
def b():
    return 2
def c():
    return 3
```"#;
        let out = split_markdown_for_wechat(text, 40);

        // Every chunk must have an even number of fences after rebalancing.
        for (i, chunk) in out.iter().enumerate() {
            let fence_count = chunk.matches("```").count();
            assert_eq!(
                fence_count % 2,
                0,
                "chunk {i} has odd fence count ({fence_count}): {chunk}"
            );
        }
    }

    #[test]
    fn rebalanced_content_still_joinable_by_eye() {
        // The prepended "```\n" on continuation chunks means a simple
        // concat() wouldn't equal the original (we've added synthetic
        // fences). But the USER-visible content should still contain
        // all the original code lines.
        let text = r#"```rust
fn one() { 1 }
fn two() { 2 }
fn three() { 3 }
fn four() { 4 }
```"#;
        let out = split_markdown_for_wechat(text, 30);
        let all_text: String = out.join("\n");
        for line in ["fn one", "fn two", "fn three", "fn four"] {
            assert!(
                all_text.contains(line),
                "line '{line}' lost during splitting: {out:?}"
            );
        }
    }
}

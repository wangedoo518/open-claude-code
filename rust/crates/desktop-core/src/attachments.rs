//! File attachment handling for drag-drop uploads.
//!
//! Supports converting common document formats to plain text (or
//! base64 for binary types) so the result can be injected as prefix
//! context into the next user message sent to the LLM.
//!
//! ── Supported types ─────────────────────────────────────────────────
//!
//!   .txt  .md  .log         → UTF-8 as-is
//!   .pdf                    → text extraction via `pdf-extract`
//!   .csv  .tsv              → UTF-8 as-is (agent can parse)
//!   .json .yaml .toml       → UTF-8 as-is
//!   .png .jpg .jpeg .gif .webp → base64 (caller may send as vision block)
//!   other                   → UTF-8 if it decodes cleanly, otherwise
//!                              "[binary file: N bytes]" stub
//!
//! Size limits apply to the EXTRACTED output (not raw bytes) to bound
//! how much LLM context a single attachment can consume. Exceeds limit
//! → truncation with notice.
//!
//! ── Office docs ────────────────────────────────────────────────────
//!
//! .docx / .xlsx / .pptx are NOT extracted in this module (they require
//! heavy dependencies like `docx-rs` / `calamine`). Callers get a
//! "[binary file: N bytes, .docx extension]" stub. This can be
//! extended as a follow-up.

use std::path::Path;

/// Maximum characters of extracted content per attachment.
pub const MAX_ATTACHMENT_CHARS: usize = 50_000;

/// Result of converting an attachment to text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentContent {
    /// Filename provided by the client (no path).
    pub filename: String,
    /// Extracted plaintext or a "[binary: N bytes]" stub.
    pub content: String,
    /// True if `content` was truncated at `MAX_ATTACHMENT_CHARS`.
    pub truncated: bool,
    /// Kind of content (text vs image-base64 vs binary stub) for UI display.
    pub kind: AttachmentKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentKind {
    /// Plain extracted text (most files).
    Text,
    /// Base64-encoded image data (for vision-capable models).
    ImageBase64,
    /// Binary file we couldn't decode — content is a stub message.
    BinaryStub,
}

/// Dispatch by file extension; returns an `AttachmentContent`.
///
/// Never fails — on error, returns a `BinaryStub` describing the
/// failure. Callers should check `kind` to decide whether to inject
/// the content or surface an error.
pub fn process_attachment(filename: &str, bytes: &[u8]) -> AttachmentContent {
    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        // Plain text formats.
        "txt" | "md" | "markdown" | "log" | "csv" | "tsv" | "json" | "yaml" | "yml" | "toml"
        | "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "java" | "c" | "cpp" | "h"
        | "hpp" | "html" | "css" | "xml" | "sh" | "sql" => extract_text(filename, bytes),

        // PDF via pdf-extract.
        "pdf" => extract_pdf(filename, bytes),

        // Images → base64.
        "png" | "jpg" | "jpeg" | "gif" | "webp" => encode_image(filename, bytes, &ext),

        // Office docs — not supported yet, return stub.
        "docx" | "xlsx" | "pptx" | "doc" | "xls" | "ppt" => AttachmentContent {
            filename: filename.to_string(),
            content: format!(
                "[Office document not yet supported: {} ({} bytes). \
                 Please convert to .md or .txt before attaching.]",
                filename,
                bytes.len()
            ),
            truncated: false,
            kind: AttachmentKind::BinaryStub,
        },

        // Unknown extension — try to decode as text, fall back to stub.
        _ => extract_text_or_stub(filename, bytes),
    }
}

fn extract_text(filename: &str, bytes: &[u8]) -> AttachmentContent {
    match std::str::from_utf8(bytes) {
        Ok(text) => {
            let (content, truncated) = truncate_text(text);
            AttachmentContent {
                filename: filename.to_string(),
                content,
                truncated,
                kind: AttachmentKind::Text,
            }
        }
        Err(_) => AttachmentContent {
            filename: filename.to_string(),
            content: format!(
                "[file is not valid UTF-8: {} ({} bytes)]",
                filename,
                bytes.len()
            ),
            truncated: false,
            kind: AttachmentKind::BinaryStub,
        },
    }
}

fn extract_text_or_stub(filename: &str, bytes: &[u8]) -> AttachmentContent {
    // Try UTF-8 first; if that fails, produce a binary stub.
    if let Ok(text) = std::str::from_utf8(bytes) {
        let (content, truncated) = truncate_text(text);
        return AttachmentContent {
            filename: filename.to_string(),
            content,
            truncated,
            kind: AttachmentKind::Text,
        };
    }
    AttachmentContent {
        filename: filename.to_string(),
        content: format!("[binary file: {} ({} bytes)]", filename, bytes.len()),
        truncated: false,
        kind: AttachmentKind::BinaryStub,
    }
}

fn extract_pdf(filename: &str, bytes: &[u8]) -> AttachmentContent {
    match pdf_extract::extract_text_from_mem(bytes) {
        Ok(text) => {
            // Collapse excessive whitespace to save tokens.
            let cleaned: String = text
                .lines()
                .map(|line| line.trim_end())
                .collect::<Vec<_>>()
                .join("\n");
            let (content, truncated) = truncate_text(&cleaned);
            AttachmentContent {
                filename: filename.to_string(),
                content,
                truncated,
                kind: AttachmentKind::Text,
            }
        }
        Err(error) => AttachmentContent {
            filename: filename.to_string(),
            content: format!(
                "[PDF extraction failed for {}: {}]",
                filename, error
            ),
            truncated: false,
            kind: AttachmentKind::BinaryStub,
        },
    }
}

fn encode_image(filename: &str, bytes: &[u8], ext: &str) -> AttachmentContent {
    use base64::Engine;
    let mime = match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    };
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    // Prefix with data URI so callers can use directly.
    let content = format!("data:{mime};base64,{b64}");
    AttachmentContent {
        filename: filename.to_string(),
        content,
        truncated: false,
        kind: AttachmentKind::ImageBase64,
    }
}

/// Truncate at `MAX_ATTACHMENT_CHARS` on a UTF-8 char boundary.
/// Returns `(content, was_truncated)`.
fn truncate_text(text: &str) -> (String, bool) {
    if text.len() <= MAX_ATTACHMENT_CHARS {
        return (text.to_string(), false);
    }
    let mut boundary = MAX_ATTACHMENT_CHARS.min(text.len());
    while boundary > 0 && !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    let truncated = format!(
        "{}\n\n... [attachment truncated at {} bytes; {} more bytes omitted]",
        &text[..boundary],
        MAX_ATTACHMENT_CHARS,
        text.len() - boundary
    );
    (truncated, true)
}

/// Format a list of attachments into a markdown block suitable for
/// prepending to a user message. Each attachment becomes a section
/// with the filename as a header and the content inline.
pub fn format_attachments_as_message_prefix(attachments: &[AttachmentContent]) -> String {
    if attachments.is_empty() {
        return String::new();
    }

    let mut out = String::from("# Attached files\n\n");
    for attachment in attachments {
        out.push_str(&format!("## {}\n\n", attachment.filename));
        match attachment.kind {
            AttachmentKind::ImageBase64 => {
                out.push_str("[Image attached — ");
                out.push_str(&format!("{} bytes]\n\n", attachment.content.len()));
                // Image data itself is not put inline; vision-capable
                // callers can pull it from attachments[].content.
            }
            AttachmentKind::Text => {
                out.push_str("```\n");
                out.push_str(&attachment.content);
                if !attachment.content.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str("```\n\n");
            }
            AttachmentKind::BinaryStub => {
                out.push_str(&attachment.content);
                out.push_str("\n\n");
            }
        }
        if attachment.truncated {
            out.push_str("_(content was truncated)_\n\n");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_file_extracts_as_utf8() {
        let result = process_attachment("notes.md", b"# Hello\n\nWorld");
        assert_eq!(result.kind, AttachmentKind::Text);
        assert_eq!(result.content, "# Hello\n\nWorld");
        assert!(!result.truncated);
    }

    #[test]
    fn text_file_with_chinese() {
        let result = process_attachment("greeting.txt", "你好世界".as_bytes());
        assert_eq!(result.kind, AttachmentKind::Text);
        assert_eq!(result.content, "你好世界");
    }

    #[test]
    fn binary_file_falls_back_to_stub() {
        let bytes = vec![0x00u8, 0xFF, 0xFE, 0xFD, 0xAB, 0xCD];
        let result = process_attachment("unknown.bin", &bytes);
        assert_eq!(result.kind, AttachmentKind::BinaryStub);
        assert!(result.content.contains("binary file"));
    }

    #[test]
    fn image_png_is_base64_encoded() {
        // Minimal PNG header bytes.
        let bytes = vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D,
        ];
        let result = process_attachment("icon.png", &bytes);
        assert_eq!(result.kind, AttachmentKind::ImageBase64);
        assert!(result.content.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn truncate_large_text() {
        let huge = "a".repeat(MAX_ATTACHMENT_CHARS + 1000);
        let result = process_attachment("huge.txt", huge.as_bytes());
        assert_eq!(result.kind, AttachmentKind::Text);
        assert!(result.truncated);
        assert!(result.content.contains("[attachment truncated"));
    }

    #[test]
    fn truncate_at_char_boundary_no_panic() {
        // 20000 Chinese chars = 60000 bytes, exceeds MAX_ATTACHMENT_CHARS.
        // Verifies we don't panic mid-codepoint.
        let huge = "中".repeat(20_000);
        let result = process_attachment("cn.txt", huge.as_bytes());
        assert!(result.truncated);
        // Verify the truncated content is still valid UTF-8.
        assert!(result.content.ends_with("bytes omitted]"));
    }

    #[test]
    fn office_doc_returns_stub() {
        let result = process_attachment("report.docx", b"fake docx bytes");
        assert_eq!(result.kind, AttachmentKind::BinaryStub);
        assert!(result.content.contains("not yet supported"));
    }

    #[test]
    fn format_empty_list_returns_empty_string() {
        let formatted = format_attachments_as_message_prefix(&[]);
        assert!(formatted.is_empty());
    }

    #[test]
    fn format_single_text_attachment() {
        let attachment = AttachmentContent {
            filename: "notes.md".to_string(),
            content: "# Meeting notes\n- item 1\n- item 2".to_string(),
            truncated: false,
            kind: AttachmentKind::Text,
        };
        let formatted = format_attachments_as_message_prefix(&[attachment]);
        assert!(formatted.contains("# Attached files"));
        assert!(formatted.contains("## notes.md"));
        assert!(formatted.contains("# Meeting notes"));
        assert!(formatted.contains("```"));
    }

    #[test]
    fn format_includes_truncation_notice() {
        let attachment = AttachmentContent {
            filename: "big.txt".to_string(),
            content: "content".to_string(),
            truncated: true,
            kind: AttachmentKind::Text,
        };
        let formatted = format_attachments_as_message_prefix(&[attachment]);
        assert!(formatted.contains("(content was truncated)"));
    }

    #[test]
    fn multiple_attachments_all_listed() {
        let attachments = vec![
            AttachmentContent {
                filename: "a.txt".to_string(),
                content: "A".to_string(),
                truncated: false,
                kind: AttachmentKind::Text,
            },
            AttachmentContent {
                filename: "b.txt".to_string(),
                content: "B".to_string(),
                truncated: false,
                kind: AttachmentKind::Text,
            },
        ];
        let formatted = format_attachments_as_message_prefix(&attachments);
        assert!(formatted.contains("## a.txt"));
        assert!(formatted.contains("## b.txt"));
    }
}

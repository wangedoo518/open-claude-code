//! DOCX adapter — extract text from Word documents.
//!
//! Canonical §7.2 row 9: `.docx` files → markdown body.
//!
//! A DOCX file is a ZIP archive containing XML. The main content
//! lives in `word/document.xml`. We parse it with `quick-xml`,
//! extracting text from `<w:t>` elements and using `<w:pPr>` /
//! `<w:pStyle>` hints to detect headings and list items.
//!
//! ## What we extract
//!
//! * Paragraphs → markdown paragraphs (double newline)
//! * Heading styles (Heading1-Heading6) → # through ######
//! * Bold runs (`<w:b/>` in `<w:rPr>`) → **bold**
//! * Italic runs (`<w:i/>` in `<w:rPr>`) → *italic*
//! * List items (rough heuristic on `<w:numPr>`) → - item
//!
//! ## What we DON'T extract
//!
//! * Images (they're in `word/media/` — would need the image adapter)
//! * Tables (rendered as flat text runs, losing structure)
//! * Headers/footers
//! * Track changes / comments
//!
//! ## External dependencies
//!
//! None — `zip` + `quick-xml` are pure Rust.

use std::io::Read;
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::{IngestError, IngestResult, Result};

/// Extract text from a DOCX file at `path`.
pub fn extract_docx(path: &Path) -> Result<IngestResult> {
    let file = std::fs::File::open(path).map_err(|e| {
        IngestError::Invalid(format!("cannot open DOCX at {}: {e}", path.display()))
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| {
        IngestError::Invalid(format!("DOCX is not a valid ZIP: {}: {e}", path.display()))
    })?;

    // Read word/document.xml
    let mut doc_xml = String::new();
    {
        let mut entry = archive.by_name("word/document.xml").map_err(|e| {
            IngestError::Invalid(format!(
                "DOCX missing word/document.xml: {}: {e}",
                path.display()
            ))
        })?;
        entry.read_to_string(&mut doc_xml).map_err(|e| {
            IngestError::Invalid(format!(
                "cannot read word/document.xml: {}: {e}",
                path.display()
            ))
        })?;
    }

    let body_md = parse_document_xml(&doc_xml);
    let title = title_from_path(path);

    let trimmed = body_md.trim();
    if trimmed.is_empty() {
        return Ok(IngestResult {
            title: title.clone(),
            body: format!(
                "# {title}\n\n_DOCX contained no extractable text._\n"
            ),
            source_url: None,
            source: "docx".to_string(),
        });
    }

    let body = format!(
        "# {title}\n\n_Extracted from `{path}`._\n\n{text}\n",
        title = title,
        path = path.display(),
        text = trimmed,
    );

    Ok(IngestResult {
        title,
        body,
        source_url: None,
        source: "docx".to_string(),
    })
}

/// Parse `word/document.xml` and produce markdown text.
///
/// Walks the XML event stream looking for:
/// - `<w:p>` (paragraph start/end)
/// - `<w:pStyle w:val="HeadingN"/>` (heading detection)
/// - `<w:b/>` / `<w:i/>` (bold/italic in run properties)
/// - `<w:t>` (text content)
/// - `<w:numPr>` (list item hint)
fn parse_document_xml(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut output = String::new();
    let mut in_paragraph = false;
    let mut in_run_props = false;
    let mut in_para_props = false;
    let mut in_text = false;
    let mut current_heading: Option<u8> = None;
    let mut is_bold = false;
    let mut is_italic = false;
    let mut is_list_item = false;
    let mut paragraph_text = String::new();

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let name_bytes = e.name().as_ref().to_vec();
                let local = local_name(&name_bytes);
                match local {
                    b"p" => {
                        in_paragraph = true;
                        paragraph_text.clear();
                        current_heading = None;
                        is_list_item = false;
                    }
                    b"pPr" => {
                        in_para_props = true;
                    }
                    b"pStyle" if in_para_props => {
                        // Detect heading level from style val attribute
                        for attr in e.attributes().flatten() {
                            if local_name(attr.key.as_ref()) == b"val" {
                                let val = String::from_utf8_lossy(&attr.value);
                                if let Some(level) = parse_heading_level(&val) {
                                    current_heading = Some(level);
                                }
                            }
                        }
                    }
                    b"numPr" if in_para_props => {
                        is_list_item = true;
                    }
                    b"rPr" => {
                        in_run_props = true;
                    }
                    b"b" if in_run_props => {
                        is_bold = true;
                    }
                    b"i" if in_run_props => {
                        is_italic = true;
                    }
                    b"t" => {
                        in_text = true;
                    }
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let name_bytes = e.name().as_ref().to_vec();
                let local = local_name(&name_bytes);
                match local {
                    b"p" => {
                        // Flush paragraph
                        let text = paragraph_text.trim().to_string();
                        if !text.is_empty() {
                            if let Some(level) = current_heading {
                                let hashes = "#".repeat(level as usize);
                                output.push_str(&format!("\n{hashes} {text}\n\n"));
                            } else if is_list_item {
                                output.push_str(&format!("- {text}\n"));
                            } else {
                                output.push_str(&text);
                                output.push_str("\n\n");
                            }
                        }
                        in_paragraph = false;
                        paragraph_text.clear();
                        is_bold = false;
                        is_italic = false;
                    }
                    b"pPr" => {
                        in_para_props = false;
                    }
                    b"rPr" => {
                        in_run_props = false;
                    }
                    b"t" => {
                        in_text = false;
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) if in_text && in_paragraph => {
                let text = e.unescape().unwrap_or_default();
                if is_bold {
                    paragraph_text.push_str("**");
                    paragraph_text.push_str(&text);
                    paragraph_text.push_str("**");
                } else if is_italic {
                    paragraph_text.push_str("*");
                    paragraph_text.push_str(&text);
                    paragraph_text.push_str("*");
                } else {
                    paragraph_text.push_str(&text);
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    output
}

/// Extract the local name from a potentially namespaced XML tag.
/// `w:p` → `p`, `w:t` → `t`, `p` → `p`.
fn local_name(full: &[u8]) -> &[u8] {
    full.iter()
        .position(|&b| b == b':')
        .map(|i| &full[i + 1..])
        .unwrap_or(full)
}

/// Try to parse "Heading1" .. "Heading6" or "heading 1" .. "heading 6"
/// style names into a heading level. Returns None for non-heading styles.
fn parse_heading_level(style_val: &str) -> Option<u8> {
    let lower = style_val.to_lowercase();
    // Common patterns: "Heading1", "heading1", "Heading 1", "heading 1"
    let stripped = lower
        .strip_prefix("heading")
        .map(|rest| rest.trim())?;
    stripped.parse::<u8>().ok().filter(|&n| (1..=6).contains(&n))
}

fn title_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("untitled")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_name_strips_namespace() {
        assert_eq!(local_name(b"w:p"), b"p");
        assert_eq!(local_name(b"w:t"), b"t");
        assert_eq!(local_name(b"p"), b"p");
    }

    #[test]
    fn parse_heading_level_works() {
        assert_eq!(parse_heading_level("Heading1"), Some(1));
        assert_eq!(parse_heading_level("Heading 2"), Some(2));
        assert_eq!(parse_heading_level("heading3"), Some(3));
        assert_eq!(parse_heading_level("heading 6"), Some(6));
        assert_eq!(parse_heading_level("heading7"), None);
        assert_eq!(parse_heading_level("Normal"), None);
    }

    #[test]
    fn parse_document_xml_extracts_paragraphs() {
        let xml = r#"<?xml version="1.0"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>Hello world.</w:t></w:r>
    </w:p>
    <w:p>
      <w:r><w:t>Second paragraph.</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
        let md = parse_document_xml(xml);
        assert!(md.contains("Hello world."));
        assert!(md.contains("Second paragraph."));
    }

    #[test]
    fn parse_document_xml_detects_headings() {
        let xml = r#"<?xml version="1.0"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr><w:pStyle w:val="Heading1"/></w:pPr>
      <w:r><w:t>Main Title</w:t></w:r>
    </w:p>
    <w:p>
      <w:pPr><w:pStyle w:val="Heading2"/></w:pPr>
      <w:r><w:t>Section</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
        let md = parse_document_xml(xml);
        assert!(md.contains("# Main Title"));
        assert!(md.contains("## Section"));
    }

    #[test]
    fn parse_document_xml_detects_bold_and_italic() {
        let xml = r#"<?xml version="1.0"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>Plain </w:t></w:r>
      <w:r><w:rPr><w:b/></w:rPr><w:t>bold</w:t></w:r>
      <w:r><w:t> and </w:t></w:r>
      <w:r><w:rPr><w:i/></w:rPr><w:t>italic</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
        let md = parse_document_xml(xml);
        assert!(md.contains("**bold**"));
        assert!(md.contains("*italic*"));
    }

    #[test]
    fn extract_docx_returns_error_for_nonexistent_file() {
        let err = extract_docx(Path::new("/tmp/nonexistent.docx")).unwrap_err();
        assert!(matches!(err, IngestError::Invalid(_)));
    }

    #[test]
    fn extract_docx_returns_error_for_non_zip() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"not a zip").unwrap();
        let err = extract_docx(tmp.path()).unwrap_err();
        assert!(matches!(err, IngestError::Invalid(_)));
    }
}

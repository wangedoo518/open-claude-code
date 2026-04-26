//! PPTX adapter — extract slide text from PowerPoint files.
//!
//! Canonical §7.2 row 7: `.pptx` files → per-slide section markdown.
//!
//! A PPTX file is a ZIP archive. Each slide lives at
//! `ppt/slides/slide{N}.xml`. Text content is in `<a:t>` elements
//! (DrawingML namespace). We extract text runs per slide, emit
//! them as markdown sections `## Slide N`, and join with blank lines.
//!
//! ## What we extract
//!
//! * Text from every `<a:t>` element on every slide
//! * Slide ordering by filename (slide1.xml, slide2.xml, ...)
//! * Each slide becomes a `## Slide N` section
//!
//! ## What we DON'T extract
//!
//! * Images, charts, SmartArt (they're in `ppt/media/`)
//! * Speaker notes (in `ppt/notesSlides/`)
//! * Animations, transitions
//! * Table structure (rendered as flat text)
//!
//! ## External dependencies
//!
//! None — reuses `zip` + `quick-xml` from the docx adapter.

use std::io::Read;
use std::path::Path;

use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::{IngestError, IngestResult, Result};

/// Extract text from a PPTX file at `path`, one section per slide.
pub fn extract_pptx(path: &Path) -> Result<IngestResult> {
    let file = std::fs::File::open(path).map_err(|e| {
        IngestError::Invalid(format!("cannot open PPTX at {}: {e}", path.display()))
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| {
        IngestError::Invalid(format!("PPTX is not a valid ZIP: {}: {e}", path.display()))
    })?;

    // Collect slide entry names and sort by slide number.
    let mut slide_names: Vec<String> = (0..archive.len())
        .filter_map(|i| {
            let entry = archive.by_index(i).ok()?;
            let name = entry.name().to_string();
            if name.starts_with("ppt/slides/slide") && name.ends_with(".xml") {
                Some(name)
            } else {
                None
            }
        })
        .collect();
    slide_names.sort_by(|a, b| slide_number(a).cmp(&slide_number(b)));

    if slide_names.is_empty() {
        return Ok(IngestResult {
            title: title_from_path(path),
            body: format!(
                "# {}\n\n_PPTX contained no slides._\n",
                title_from_path(path)
            ),
            source_url: None,
            source: "pptx".to_string(),
        });
    }

    let title = title_from_path(path);
    let mut body = format!(
        "# {title}\n\n_Extracted from `{path}` ({count} slides)._\n\n",
        title = title,
        path = path.display(),
        count = slide_names.len(),
    );

    // S2 fix: ZIP bomb defense — cap per-slide XML size.
    const MAX_SLIDE_XML_BYTES: u64 = 20 * 1024 * 1024; // 20 MiB per slide

    for (idx, slide_name) in slide_names.iter().enumerate() {
        let mut xml_content = String::new();
        // S2 fix: check uncompressed size before reading.
        let too_large = archive
            .by_name(slide_name)
            .map(|e| e.size() > MAX_SLIDE_XML_BYTES)
            .unwrap_or(false);
        if too_large {
            xml_content = format!("(slide too large, skipped)");
        } else if let Ok(mut entry) = archive.by_name(slide_name) {
            let _ = entry.read_to_string(&mut xml_content);
        }
        let text = extract_slide_text(&xml_content);
        let trimmed = text.trim();
        body.push_str(&format!("## Slide {}\n\n", idx + 1));
        if trimmed.is_empty() {
            body.push_str("_(empty slide)_\n\n");
        } else {
            body.push_str(trimmed);
            body.push_str("\n\n");
        }
    }

    Ok(IngestResult {
        title,
        body,
        source_url: None,
        source: "pptx".to_string(),
    })
}

/// Extract text from a single slide XML. Walks `<a:t>` elements
/// and joins text runs with spaces. Paragraph breaks (`<a:p>`) get
/// newlines.
fn extract_slide_text(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut output = String::new();
    let mut in_text = false;
    let mut in_paragraph = false;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let name_bytes = e.name().as_ref().to_vec();
                let local = local_name(&name_bytes);
                match local {
                    b"p" => {
                        if in_paragraph && !output.ends_with('\n') {
                            output.push('\n');
                        }
                        in_paragraph = true;
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
                        in_paragraph = false;
                    }
                    b"t" => {
                        in_text = false;
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) if in_text => {
                let text = e.unescape().unwrap_or_default();
                if !text.is_empty() {
                    if !output.is_empty() && !output.ends_with('\n') && !output.ends_with(' ') {
                        output.push(' ');
                    }
                    output.push_str(&text);
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

/// Extract slide number from filename like "ppt/slides/slide3.xml" → 3.
fn slide_number(name: &str) -> u32 {
    name.strip_prefix("ppt/slides/slide")
        .and_then(|rest| rest.strip_suffix(".xml"))
        .and_then(|num| num.parse().ok())
        .unwrap_or(0)
}

/// Extract local name from namespaced XML tag (a:t → t, a:p → p).
fn local_name(full: &[u8]) -> &[u8] {
    full.iter()
        .position(|&b| b == b':')
        .map(|i| &full[i + 1..])
        .unwrap_or(full)
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
    fn slide_number_parses_correctly() {
        assert_eq!(slide_number("ppt/slides/slide1.xml"), 1);
        assert_eq!(slide_number("ppt/slides/slide12.xml"), 12);
        assert_eq!(slide_number("ppt/slides/slide0.xml"), 0);
        assert_eq!(slide_number("other/path.xml"), 0);
    }

    #[test]
    fn extract_slide_text_parses_drawingml() {
        let xml = r#"<?xml version="1.0"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
       xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld>
    <p:spTree>
      <p:sp>
        <p:txBody>
          <a:p><a:r><a:t>Title Text</a:t></a:r></a:p>
          <a:p><a:r><a:t>Bullet point one</a:t></a:r></a:p>
          <a:p><a:r><a:t>Bullet point two</a:t></a:r></a:p>
        </p:txBody>
      </p:sp>
    </p:spTree>
  </p:cSld>
</p:sld>"#;
        let text = extract_slide_text(xml);
        assert!(text.contains("Title Text"));
        assert!(text.contains("Bullet point one"));
        assert!(text.contains("Bullet point two"));
    }

    #[test]
    fn extract_slide_text_handles_empty_slide() {
        let xml = r#"<?xml version="1.0"?>
<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main">
  <p:cSld><p:spTree/></p:cSld>
</p:sld>"#;
        let text = extract_slide_text(xml);
        assert!(text.trim().is_empty());
    }

    #[test]
    fn extract_pptx_returns_error_for_nonexistent_file() {
        let err = extract_pptx(Path::new("/tmp/nonexistent.pptx")).unwrap_err();
        assert!(matches!(err, IngestError::Invalid(_)));
    }

    #[test]
    fn extract_pptx_returns_error_for_non_zip() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"not a zip").unwrap();
        let err = extract_pptx(tmp.path()).unwrap_err();
        assert!(matches!(err, IngestError::Invalid(_)));
    }

    #[test]
    fn local_name_strips_namespace() {
        assert_eq!(local_name(b"a:t"), b"t");
        assert_eq!(local_name(b"a:p"), b"p");
        assert_eq!(local_name(b"t"), b"t");
    }
}

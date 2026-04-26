//! PDF adapter — extract text from PDF documents.
//!
//! Canonical §7.2 row 8: `.pdf` files → text extraction → markdown.
//!
//! Uses the `pdf-extract` crate (pure Rust, zero external binaries).
//! The extraction is text-only — images, tables drawn as vector
//! graphics, and scanned pages are NOT handled (they'd need OCR
//! which requires the image adapter or an external Tesseract binary).
//!
//! ## Limitations
//!
//! * Scanned PDFs (image-only): returns empty or near-empty body
//!   with a note suggesting the image adapter.
//! * Complex tables: extracted as raw text runs without column
//!   alignment — readable but ugly.
//! * CJK fonts with CID encoding: works for most fonts; some rare
//!   custom-embedded fonts may produce garbled output.
//! * Encrypted/DRM PDFs: pdf-extract returns an error which we
//!   map to IngestError.

use std::path::Path;

use crate::{IngestError, IngestResult, Result};

/// Extract text from a PDF file at `path` and return it as an
/// `IngestResult` with the text body formatted as markdown.
///
/// The title is derived from the filename (sans `.pdf` extension)
/// since PDF metadata titles are often empty or useless ("Microsoft
/// Word - Document1").
/// Hard cap on PDF file size (100 MiB). Larger PDFs need to be
/// split before ingestion.
pub const MAX_PDF_BYTES: usize = 100 * 1024 * 1024;

pub fn extract_pdf(path: &Path) -> Result<IngestResult> {
    // I4 fix: check file size BEFORE reading to prevent OOM.
    let metadata = std::fs::metadata(path)
        .map_err(|e| IngestError::Invalid(format!("cannot stat PDF at {}: {e}", path.display())))?;
    if metadata.len() > MAX_PDF_BYTES as u64 {
        return Err(IngestError::TooLarge {
            bytes: metadata.len() as usize,
            max: MAX_PDF_BYTES,
        });
    }
    let bytes = std::fs::read(path)
        .map_err(|e| IngestError::Invalid(format!("cannot read PDF at {}: {e}", path.display())))?;

    let text = pdf_extract::extract_text_from_mem(&bytes).map_err(|e| {
        IngestError::Invalid(format!("PDF extraction failed for {}: {e}", path.display()))
    })?;

    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(IngestResult {
            title: title_from_path(path),
            body: format!(
                "# {}\n\n\
                 _PDF contained no extractable text. This may be a \
                 scanned document — try re-ingesting through the image \
                 adapter for OCR._\n",
                title_from_path(path)
            ),
            source_url: None,
            source: "pdf".to_string(),
        });
    }

    // Format: title header + source path + body text.
    // We collapse excessive whitespace and normalize line endings
    // but do NOT try to reconstruct paragraphs — the raw text runs
    // from pdf-extract are already roughly paragraph-shaped.
    let title = title_from_path(path);
    let body = format!(
        "# {title}\n\n\
         _Extracted from `{path}` ({size} bytes, {pages} chars)._\n\n\
         {text}\n",
        title = title,
        path = path.display(),
        size = bytes.len(),
        pages = trimmed.len(),
        text = trimmed,
    );

    Ok(IngestResult {
        title,
        body,
        source_url: None,
        source: "pdf".to_string(),
    })
}

/// Derive a human-readable title from the file path.
fn title_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("untitled")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn extract_pdf_returns_error_for_nonexistent_file() {
        let err = extract_pdf(Path::new("/tmp/nonexistent-abc123.pdf")).unwrap_err();
        assert!(matches!(err, IngestError::Invalid(_)));
    }

    #[test]
    fn extract_pdf_returns_error_for_invalid_pdf() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"this is not a PDF").unwrap();
        let err = extract_pdf(tmp.path()).unwrap_err();
        assert!(matches!(err, IngestError::Invalid(_)));
    }

    #[test]
    fn title_from_path_extracts_stem() {
        assert_eq!(
            title_from_path(Path::new("/docs/my-report.pdf")),
            "my-report"
        );
        assert_eq!(title_from_path(Path::new("单页.pdf")), "单页");
    }

    // NOTE: We don't include a real PDF fixture in the repo to keep
    // the test suite lightweight. The two tests above verify error
    // paths. A real PDF extraction test would need a tiny valid PDF
    // which adds ~2 KB of binary fixture — acceptable but deferred
    // to a dedicated test-fixtures commit.
}

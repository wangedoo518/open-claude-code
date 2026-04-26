//! Image adapter — describe images using base64 encoding for LLM Vision.
//!
//! Canonical §7.2 row 6: `.jpg` / `.png` / `.webp` images →
//! read file → base64 encode → prepare for Vision LLM caption.
//!
//! The actual LLM call happens in the caller (desktop-server handler
//! or wiki_maintainer) because this crate doesn't depend on
//! desktop-core / codex_broker. This adapter just prepares the image
//! data and returns it in a format ready for the Vision API.
//!
//! ## Flow
//!
//! 1. Read image bytes from disk
//! 2. Detect MIME type from extension
//! 3. Base64-encode the image
//! 4. Return IngestResult with the base64 data URI + a markdown
//!    placeholder body that the caller replaces with the LLM caption
//!
//! ## External dependencies
//!
//! None for this adapter. The caller needs a Vision-capable model
//! in the Codex pool (e.g. GPT-5.4 Vision).

use std::path::Path;

use crate::{IngestError, IngestResult, Result};

/// Maximum image file size we'll load (10 MiB). Larger images should
/// be resized before ingestion.
pub const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;

/// Supported image extensions and their MIME types.
const MIME_MAP: &[(&str, &str)] = &[
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("png", "image/png"),
    ("webp", "image/webp"),
    ("gif", "image/gif"),
    ("bmp", "image/bmp"),
    ("svg", "image/svg+xml"),
];

/// Prepare an image file for Vision API captioning.
///
/// Returns an `IngestResult` where:
/// - `title` is the filename stem
/// - `body` contains a markdown image reference + base64 data URI
///   metadata that the caller can pass to the Vision API
/// - `source` is "image"
///
/// The body includes a `<image-base64>` block that the desktop-server
/// handler or wiki_maintainer can extract and send to the Vision LLM.
/// If no Vision model is available, the body still has the image
/// reference (just without the caption).
pub fn prepare_image(path: &Path) -> Result<IngestResult> {
    let bytes = std::fs::read(path).map_err(|e| {
        IngestError::Invalid(format!("cannot read image at {}: {e}", path.display()))
    })?;

    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(IngestError::TooLarge {
            bytes: bytes.len(),
            max: MAX_IMAGE_BYTES,
        });
    }

    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    let mime = MIME_MAP
        .iter()
        .find(|(e, _)| *e == ext)
        .map(|(_, m)| *m)
        .unwrap_or("application/octet-stream");

    let b64 = base64_encode(&bytes);
    let data_uri = format!("data:{mime};base64,{b64}");
    let title = title_from_path(path);

    let body = format!(
        "# {title}\n\n\
         _Image: `{path}` ({size} bytes, {mime})_\n\n\
         ![{title}]({data_uri_short})\n\n\
         <image-base64 mime=\"{mime}\" size=\"{size}\">\n\
         {b64_preview}...\n\
         </image-base64>\n\n\
         _To generate a caption, run this image through a Vision-capable \
         model in the Codex pool. The full base64 data ({b64_len} chars) \
         is available in the raw entry file._\n",
        title = title,
        path = path.display(),
        size = bytes.len(),
        mime = mime,
        data_uri_short = if data_uri.len() > 80 {
            format!("{}...", &data_uri[..80])
        } else {
            data_uri.clone()
        },
        b64_preview = &b64[..b64.len().min(100)],
        b64_len = b64.len(),
    );

    Ok(IngestResult {
        title,
        body,
        source_url: None,
        source: "image".to_string(),
    })
}

/// Return the full base64-encoded image as a data URI for Vision API.
/// Used by the desktop-server handler when it has a Vision-capable
/// broker to generate the actual caption.
pub fn image_to_data_uri(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).map_err(|e| {
        IngestError::Invalid(format!("cannot read image at {}: {e}", path.display()))
    })?;
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(IngestError::TooLarge {
            bytes: bytes.len(),
            max: MAX_IMAGE_BYTES,
        });
    }
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    let mime = MIME_MAP
        .iter()
        .find(|(e, _)| *e == ext)
        .map(|(_, m)| *m)
        .unwrap_or("application/octet-stream");
    let b64 = base64_encode(&bytes);
    Ok(format!("data:{mime};base64,{b64}"))
}

/// Simple base64 encoder (no padding). We avoid pulling in the
/// `base64` crate for one function — desktop-server already has it
/// but wiki_ingest doesn't depend on desktop-server.
fn base64_encode(bytes: &[u8]) -> String {
    const CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        out.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            out.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
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
    use std::io::Write;

    #[test]
    fn base64_encode_basic() {
        assert_eq!(base64_encode(b"Hello"), "SGVsbG8=");
        assert_eq!(base64_encode(b"Hi"), "SGk=");
        assert_eq!(base64_encode(b"A"), "QQ==");
        assert_eq!(base64_encode(b""), "");
    }

    #[test]
    fn prepare_image_reads_small_file() {
        let mut tmp = tempfile::NamedTempFile::with_suffix(".png").unwrap();
        // Write a tiny "PNG" (not valid, but prepare_image only reads bytes)
        tmp.write_all(b"\x89PNG\r\n\x1a\n fake png data").unwrap();
        tmp.flush().unwrap();

        let result = prepare_image(tmp.path()).unwrap();
        assert_eq!(result.source, "image");
        assert!(result.body.contains("image/png"));
        assert!(result.body.contains("image-base64"));
    }

    #[test]
    fn prepare_image_rejects_oversize() {
        let mut tmp = tempfile::NamedTempFile::with_suffix(".jpg").unwrap();
        let big = vec![0u8; MAX_IMAGE_BYTES + 1];
        tmp.write_all(&big).unwrap();
        tmp.flush().unwrap();

        let err = prepare_image(tmp.path()).unwrap_err();
        assert!(matches!(err, IngestError::TooLarge { .. }));
    }

    #[test]
    fn prepare_image_detects_mime_from_extension() {
        for (ext, expected_mime) in [
            ("jpg", "image/jpeg"),
            ("png", "image/png"),
            ("webp", "image/webp"),
        ] {
            let mut tmp = tempfile::Builder::new()
                .suffix(&format!(".{ext}"))
                .tempfile()
                .unwrap();
            tmp.write_all(b"fake").unwrap();
            tmp.flush().unwrap();
            let result = prepare_image(tmp.path()).unwrap();
            assert!(
                result.body.contains(expected_mime),
                "ext={ext} should produce {expected_mime}"
            );
        }
    }

    #[test]
    fn image_to_data_uri_produces_valid_uri() {
        let mut tmp = tempfile::NamedTempFile::with_suffix(".png").unwrap();
        tmp.write_all(b"test data").unwrap();
        tmp.flush().unwrap();

        let uri = image_to_data_uri(tmp.path()).unwrap();
        assert!(uri.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn prepare_image_returns_error_for_nonexistent() {
        let err = prepare_image(Path::new("/tmp/no-such-image.png")).unwrap_err();
        assert!(matches!(err, IngestError::Invalid(_)));
    }
}

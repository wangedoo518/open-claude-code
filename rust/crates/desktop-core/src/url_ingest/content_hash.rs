//! Content identity: SHA-256 of the cleaned markdown body.
//!
//! Used as the secondary dedupe signal ("content identity") alongside
//! canonical URL ("URL identity"). The hash is computed against
//! `wiki_ingest::sanitize_markdown` output, which strips data URIs and
//! decodes HTML entities — so trivial noise (data:image/svg+xml,%3C...,
//! &amp;nbsp;) doesn't break the hash, but any actual text change does.
//!
//! Empty / whitespace-only bodies return None (caller skips content
//! dedupe for those).

use sha2::{Digest, Sha256};

/// Compute SHA-256 of the cleaned body. Returns hex string (64 chars).
/// Returns None when body is empty or whitespace-only.
#[must_use]
pub fn compute_content_hash(cleaned_body: &str) -> Option<String> {
    let trimmed = cleaned_body.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut hasher = Sha256::new();
    hasher.update(trimmed.as_bytes());
    Some(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_hash() {
        let a = compute_content_hash("hello world").unwrap();
        let b = compute_content_hash("hello world").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn different_content_different_hash() {
        let a = compute_content_hash("hello world").unwrap();
        let b = compute_content_hash("hello universe").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn whitespace_sensitive_but_edges_trimmed() {
        let a = compute_content_hash("  hello\n").unwrap();
        let b = compute_content_hash("hello").unwrap();
        assert_eq!(a, b); // trimmed
    }

    #[test]
    fn empty_returns_none() {
        assert!(compute_content_hash("").is_none());
        assert!(compute_content_hash("   \n\t").is_none());
    }

    #[test]
    fn hash_is_lowercase_hex() {
        let hash = compute_content_hash("some content").unwrap();
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(hash.chars().all(|c| !c.is_ascii_uppercase()));
    }
}

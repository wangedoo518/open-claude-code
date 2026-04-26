//! Canonical URL normalization: strip tracking/noise while preserving
//! content identity. Used exclusively by the ingest orchestrator to
//! derive dedupe keys and canonical `source_url` landings so that a
//! single article reached through multiple share links collapses to
//! one raw entry rather than spawning a flood of duplicates.
//!
//! Why this lives in `desktop-core/url_ingest/` rather than the
//! `url` adapter in `wiki_ingest`: the adapter's job is network I/O,
//! which stays oblivious to how callers dedupe upstream. The
//! orchestrator owns the dedupe key derivation, so the normalization
//! logic is co-located with the `dedupe` module that consumes it.
//!
//! # Rules (Explorer A §A-3)
//!
//!   * Lowercase the scheme + host (path preserves case — GitHub
//!     URLs like `/AbC` and `/abc` are different resources).
//!   * Strip the default port for the given scheme (`:80` for http,
//!     `:443` for https).
//!   * Strip any URL fragment (`#anchor`) — dedupe should not split
//!     an article just because two shares pointed at different
//!     headings.
//!   * Strip a vetted blacklist of tracking + sharing-noise query
//!     params. Preserve everything else (safer than a whitelist:
//!     unknown params are far more likely to be content-selecting
//!     than tracking).
//!   * Sort the remaining query params by key so permuted shares
//!     (`?a=1&b=2` vs `?b=2&a=1`) collapse to the same canonical.
//!
//! # Parse failures
//!
//! If `url::Url::parse` rejects the input, this function falls back
//! to returning the trimmed original. The orchestrator's upstream
//! `ingest_url` already enforces a scheme check, so the realistic
//! failure mode here is an exotic percent-encoded payload — returning
//! it verbatim keeps the dedupe key stable for the re-submission
//! case rather than panicking.

use std::collections::BTreeMap;

/// Tracking / sharing-noise query keys stripped during canonicalization.
///
/// Grouped by provenance for auditability. Adding a new key is a
/// conservative operation; removing one risks re-surfacing duplicates
/// across share paths.
const TRACKING_PARAMS: &[&str] = &[
    // ── Google / Facebook / Microsoft / Yandex ads ──
    "utm_source",
    "utm_medium",
    "utm_campaign",
    "utm_term",
    "utm_content",
    "fbclid",
    "gclid",
    "msclkid",
    "dclid",
    "yclid",
    "_ga",
    "_gid",
    "mc_cid",
    "mc_eid",
    "ref_src",
    "ref_source",
    // ── WeChat (mp.weixin.qq.com share / forward noise) ──
    "scene",
    "clicktime",
    "chksm",
    "mpshare",
    "srcid",
    "sharer_sharetime",
    "sharer_shareid",
    "enterid",
    "nettype",
    "version",
    "lang",
    "exportkey",
    "ascene",
    "devicetype",
    "pass_ticket",
    "wx_header",
    "session_us",
    "subscene",
    "sharer_scene",
    "sharer_from",
    // ── Alibaba / TikTok ──
    "spm",
    "scm",
    "tt_from",
    // ── Ambiguous but empirically affiliate / share noise ──
    "from",
    "source",
    "ref",
];

/// Returns the canonicalized form of `url`. Parse failures fall back
/// to the trimmed original so upstream never panics.
///
/// See module-level docs for the full rule list.
#[must_use]
pub fn canonicalize(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let Ok(mut parsed) = url::Url::parse(trimmed) else {
        // Parse rejection: return the trimmed input verbatim. The
        // orchestrator already validated the scheme before we got
        // here, so this branch is mostly defensive against exotic
        // percent-encoded payloads.
        return trimmed.to_string();
    };

    // ── Lowercase scheme ──────────────────────────────────────────
    // `url::Url` stores scheme lowercased already (RFC 3986), but
    // re-normalize defensively in case the parser behavior ever
    // changes.
    let scheme = parsed.scheme().to_ascii_lowercase();
    let _ = parsed.set_scheme(&scheme);

    // ── Lowercase host ────────────────────────────────────────────
    // Host matching is case-insensitive by RFC 3986 §3.2.2.
    if let Some(host) = parsed.host_str().map(|h| h.to_ascii_lowercase()) {
        // `set_host` can fail for URLs that don't allow a host (e.g.
        // `data:`), but we've already filtered those via the scheme
        // check upstream. Ignore the error defensively.
        let _ = parsed.set_host(Some(&host));
    }

    // ── Strip default port ────────────────────────────────────────
    // `url::Url::port_or_known_default()` returns the default port
    // for known schemes even when the URL did not specify one.
    // `port()` returns `Some(n)` only when the URL wrote `:n`
    // explicitly. Strip when the explicit port matches the default.
    if let (Some(explicit), Some(default)) =
        (parsed.port(), default_port_for_scheme(parsed.scheme()))
    {
        if explicit == default {
            let _ = parsed.set_port(None);
        }
    }

    // ── Strip fragment ────────────────────────────────────────────
    parsed.set_fragment(None);

    // ── Filter + sort query params ────────────────────────────────
    // Collect every pair into a `BTreeMap<String, Vec<String>>` so
    // we get deterministic ordering on the output. Using a multi-map
    // shape (Vec values) preserves `?tag=a&tag=b`-style repeats that
    // some APIs meaningfully rely on.
    let pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();

    if pairs.is_empty() {
        // No query string at all — make sure we don't emit a trailing
        // `?` from a URL that had `?` with no pairs.
        parsed.set_query(None);
        return parsed.into();
    }

    let mut filtered: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (key, value) in pairs {
        if is_tracking_param(&key) {
            continue;
        }
        filtered.entry(key).or_default().push(value);
    }

    if filtered.is_empty() {
        parsed.set_query(None);
    } else {
        // Re-serialize in BTreeMap order (keys sorted). For multi-
        // valued keys, preserve insertion order (original share URL
        // order) within the key group. `url::Url::query_pairs_mut`
        // handles percent-encoding for us.
        let mut serializer = parsed.query_pairs_mut();
        serializer.clear();
        for (key, values) in &filtered {
            for value in values {
                serializer.append_pair(key, value);
            }
        }
        drop(serializer);
    }

    parsed.into()
}

/// True if `key` matches a known tracking / sharing-noise parameter.
/// Case-insensitive match — some WeChat links emit `SCENE` etc.
fn is_tracking_param(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    TRACKING_PARAMS.iter().any(|p| *p == lower)
}

/// Default port for known schemes, used for stripping redundant
/// `:80` / `:443` from canonicalized URLs.
fn default_port_for_scheme(scheme: &str) -> Option<u16> {
    match scheme {
        "http" | "ws" => Some(80),
        "https" | "wss" => Some(443),
        "ftp" => Some(21),
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_wechat_sharing_noise() {
        // WeChat share URLs are the primary consumer. Every one of
        // these params must be stripped so that the same article
        // forwarded through three different groups collapses to one
        // canonical entry.
        let input =
            "https://mp.weixin.qq.com/s/AbC123?scene=1&clicktime=1234567&chksm=xyz&mpshare=1";
        let expected = "https://mp.weixin.qq.com/s/AbC123";
        assert_eq!(canonicalize(input), expected);
    }

    #[test]
    fn strips_generic_tracking_params() {
        // Stock marketing / ads pipeline params. `id=42` is content-
        // selecting and must survive.
        let input = "https://example.com/post?utm_source=tw&fbclid=x&id=42";
        let out = canonicalize(input);
        assert!(out.contains("id=42"), "expected id=42 preserved, got {out}");
        assert!(!out.contains("utm_source"), "utm_source leaked: {out}");
        assert!(!out.contains("fbclid"), "fbclid leaked: {out}");
    }

    #[test]
    fn preserves_path_case() {
        // GitHub + many CMSs treat `/AbC` and `/abc` as distinct
        // resources. Lowercasing the path would collapse them into
        // one dedupe bucket — a correctness bug, not a convenience.
        let upper = canonicalize("https://example.com/AbC");
        let lower = canonicalize("https://example.com/abc");
        assert_ne!(
            upper, lower,
            "path case was folded (upper={upper}, lower={lower})"
        );
    }

    #[test]
    fn lowercases_host() {
        // RFC 3986 §3.2.2 — host is case-insensitive.
        let out = canonicalize("https://Example.COM/x");
        assert!(out.starts_with("https://example.com"), "got {out}");
    }

    #[test]
    fn strips_fragment() {
        // Fragments are client-side only; a share link with `#h2`
        // must not create a second dedupe bucket.
        let out = canonicalize("https://example.com/a#section");
        assert!(!out.contains('#'), "fragment leaked: {out}");
    }

    #[test]
    fn strips_default_port() {
        // `:443` is redundant on https and vice versa for http.
        let https = canonicalize("https://example.com:443/a");
        let http = canonicalize("http://example.com:80/a");
        assert_eq!(https, "https://example.com/a");
        assert_eq!(http, "http://example.com/a");
    }

    #[test]
    fn preserves_unknown_params() {
        // Whitelist-only would drop legitimate content keys we've
        // never heard of. The rule is: strip known noise, keep
        // everything else.
        let out = canonicalize("https://example.com/post?custom_key=v");
        assert!(out.contains("custom_key=v"), "unknown param dropped: {out}");
    }

    #[test]
    fn fallback_for_unparseable_input() {
        // Garbage in → trimmed garbage out. The orchestrator's scheme
        // check upstream catches `not-a-url` before this ever runs,
        // but we defend against exotic percent-encoded payloads that
        // the parser may reject.
        assert_eq!(canonicalize("not-a-url"), "not-a-url");
        assert_eq!(canonicalize("   not-a-url   "), "not-a-url");
    }

    #[test]
    fn sorts_query_params_by_key() {
        // Permuted share orderings must collapse.
        let a = canonicalize("https://example.com/p?b=2&a=1");
        let b = canonicalize("https://example.com/p?a=1&b=2");
        assert_eq!(a, b, "query param order not normalized: a={a} b={b}");
    }

    #[test]
    fn strips_non_default_port_leaves_intact() {
        // Non-default ports are significant and must survive.
        let out = canonicalize("https://example.com:8443/a");
        assert_eq!(out, "https://example.com:8443/a");
    }

    #[test]
    fn case_insensitive_tracking_key_match() {
        // WeChat sometimes emits uppercase share keys from older
        // clients — normalization must still catch them.
        let out = canonicalize("https://example.com/p?UTM_SOURCE=x&id=1");
        assert!(!out.contains("UTM_SOURCE"), "UTM_SOURCE leaked: {out}");
        assert!(!out.contains("utm_source"), "utm_source leaked: {out}");
        assert!(out.contains("id=1"), "id leaked: {out}");
    }

    #[test]
    fn empty_input_returns_empty_string() {
        assert_eq!(canonicalize(""), "");
        assert_eq!(canonicalize("   "), "");
    }

    #[test]
    fn idempotent() {
        // Running canonicalize twice must be a no-op.
        let once = canonicalize("https://mp.weixin.qq.com/s/Abc?scene=1&utm_source=x&id=42");
        let twice = canonicalize(&once);
        assert_eq!(once, twice, "canonicalize is not idempotent");
    }
}

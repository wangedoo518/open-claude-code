//! `wiki_store` — on-disk layout and lifecycle for `~/.clawwiki/`.
//!
//! ClawWiki canonical (`docs/clawwiki/product-design.md` §10) defines a
//! three-layer file system rooted at `~/.clawwiki/`:
//!
//! ```text
//! ~/.clawwiki/
//! ├── raw/        # immutable facts ingested from WeChat (forever read-only)
//! ├── wiki/       # LLM-maintained pages (concept/people/topic/compare/...)
//! ├── schema/     # human-curated rules (CLAUDE.md, AGENTS.md, templates)
//! └── .clawwiki/  # machine-readable metadata (manifest, sessions DB, ...)
//! ```
//!
//! This crate owns:
//!
//! 1. Resolving the root path (env override → platform default).
//! 2. Bootstrapping the four subdirectories on first run.
//! 3. Seeding `schema/CLAUDE.md` from the canonical §8 template the
//!    first time the user opens ClawWiki — if the user has hand-edited
//!    it later, we never overwrite their edits.
//!
//! Higher-level CRUD on raw / wiki pages lives in sibling crates
//! (`wiki_ingest`, `wiki_maintainer`) that depend on this one. Keeping
//! the layout knowledge here lets all of them agree on path semantics
//! without circular deps on `desktop-core`.
//!
//! ## D2 override note
//!
//! The canonical doc originally framed WeChat ingestion as "enterprise
//! WeChat outbound bot + cloud microservice". The user's D2 override
//! (commit `6617945` "docs(clawwiki): override D2") swaps that for the
//! existing personal-WeChat iLink path (Phase 1-2 of (8)). This crate
//! is channel-agnostic: whichever crate ends up writing into `raw/`
//! sees the same on-disk shape.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Subdirectory under the wiki root that holds immutable WeChat-ingested
/// facts. Files here MUST NOT be mutated by any tool.
pub const RAW_DIR: &str = "raw";

/// Subdirectory under the wiki root that holds LLM-maintained pages.
/// The maintainer agent has write access; the user has audit access via
/// the Inbox UI.
pub const WIKI_DIR: &str = "wiki";

/// Subdirectory under the wiki root that holds human-curated rules
/// (`CLAUDE.md`, `AGENTS.md`, templates, policies). The maintainer
/// agent may PROPOSE changes via Inbox but never writes here directly.
pub const SCHEMA_DIR: &str = "schema";

/// Subdirectory under the wiki root for machine-readable metadata
/// (manifest.json, compile-state.json, ask-sessions.db, ...). Hidden
/// from the wiki root listing because users have no reason to touch it.
pub const META_DIR: &str = ".clawwiki";

/// Filename for the maintainer-rules document inside `schema/`. Bootstrap
/// writes the canonical `templates/CLAUDE.md` content here on first run.
pub const CLAUDE_MD_FILENAME: &str = "CLAUDE.md";

/// Environment variable that overrides the default `~/.clawwiki/` path.
/// Useful for tests, CI, and per-project sandboxes.
pub const ENV_OVERRIDE: &str = "CLAWWIKI_HOME";

/// Default folder name appended to the user's HOME directory when no
/// override is set: `$HOME/.clawwiki/`.
pub const DEFAULT_DIRNAME: &str = ".clawwiki";

/// Embedded canonical CLAUDE.md template (kept in `templates/CLAUDE.md`
/// to avoid escaping a 60-line markdown blob inside Rust source).
const CLAUDE_MD_TEMPLATE: &str = include_str!("../templates/CLAUDE.md");

/// Errors raised by `wiki_store` filesystem operations.
#[derive(Debug, thiserror::Error)]
pub enum WikiStoreError {
    #[error("filesystem error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// A raw entry id was requested but no matching file exists.
    #[error("raw entry not found: id={0}")]
    NotFound(u32),
    /// An input violated a wiki_store invariant (empty slug, malformed
    /// filename, etc.). The string carries the detail.
    #[error("invalid input: {0}")]
    Invalid(String),
}

impl WikiStoreError {
    fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

pub type Result<T> = std::result::Result<T, WikiStoreError>;

/// Resolved absolute paths for every well-known location inside a
/// ClawWiki root. Returned by [`WikiPaths::resolve`] and consumed by
/// downstream crates that need to read or write specific subtrees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiPaths {
    pub root: PathBuf,
    pub raw: PathBuf,
    pub wiki: PathBuf,
    pub schema: PathBuf,
    pub meta: PathBuf,
    pub schema_claude_md: PathBuf,
}

impl WikiPaths {
    /// Resolve the layout for a given root. Pure function — does not
    /// touch the filesystem and does not validate that any path exists.
    #[must_use]
    pub fn resolve(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            raw: root.join(RAW_DIR),
            wiki: root.join(WIKI_DIR),
            schema: root.join(SCHEMA_DIR),
            meta: root.join(META_DIR),
            schema_claude_md: root.join(SCHEMA_DIR).join(CLAUDE_MD_FILENAME),
        }
    }
}

/// Resolve the default `~/.clawwiki/` root for the current process.
///
/// Resolution order:
///   1. `$CLAWWIKI_HOME` if set (any value, even empty string is honored
///      verbatim — same convention as `XDG_*` vars).
///   2. `$HOME/.clawwiki/` on Unix.
///   3. `%USERPROFILE%/.clawwiki/` on Windows.
///   4. Falls back to a relative `./.clawwiki/` if neither HOME nor
///      USERPROFILE are set (very rare; only happens in CI sandboxes
///      that strip the environment).
#[must_use]
pub fn default_root() -> PathBuf {
    let override_var = std::env::var_os(ENV_OVERRIDE);
    let home = home_dir();
    default_root_from(override_var, home.as_deref())
}

/// Pure version of [`default_root`] that takes its inputs explicitly so
/// the resolution rules can be unit-tested without mutating the
/// process-wide environment (which would race with parallel tests).
#[must_use]
pub fn default_root_from(override_var: Option<OsString>, home: Option<&Path>) -> PathBuf {
    if let Some(v) = override_var {
        return PathBuf::from(v);
    }
    match home {
        Some(h) => h.join(DEFAULT_DIRNAME),
        None => PathBuf::from(".").join(DEFAULT_DIRNAME),
    }
}

/// Best-effort home directory lookup without pulling in the `dirs`
/// crate (which adds 6 transitive deps just for one path). Matches the
/// convention used elsewhere in the workspace.
fn home_dir() -> Option<PathBuf> {
    if let Some(v) = std::env::var_os("HOME") {
        if !v.is_empty() {
            return Some(PathBuf::from(v));
        }
    }
    if let Some(v) = std::env::var_os("USERPROFILE") {
        if !v.is_empty() {
            return Some(PathBuf::from(v));
        }
    }
    None
}

/// Bootstrap the wiki layout at `root`. Idempotent: safe to call on
/// every desktop-server startup.
///
/// Behavior:
/// * Creates `root/`, `raw/`, `wiki/`, `schema/`, `.clawwiki/` if any
///   are missing. Existing directories are left untouched.
/// * Seeds `schema/CLAUDE.md` from the canonical template ONLY when the
///   file does not exist yet. Once the user (or the maintainer Inbox
///   flow) has touched it, future calls are a no-op for that file.
///
/// This is the only mutating function in the crate; everything else is
/// either a pure resolver (`WikiPaths::resolve`, `default_root_from`) or
/// a getter for the canonical template content.
pub fn init_wiki(root: &Path) -> Result<()> {
    let paths = WikiPaths::resolve(root);

    // mkdir -p the four subdirectories. `create_dir_all` is idempotent
    // and creates intermediate components, so even a fresh root works.
    for dir in [&paths.root, &paths.raw, &paths.wiki, &paths.schema, &paths.meta] {
        fs::create_dir_all(dir).map_err(|e| WikiStoreError::io(dir.clone(), e))?;
    }

    // Seed CLAUDE.md only if absent. We deliberately do NOT compare
    // contents — once the file exists we treat it as user-owned and
    // never overwrite. The user can `rm schema/CLAUDE.md` to re-seed.
    if !paths.schema_claude_md.exists() {
        fs::write(&paths.schema_claude_md, CLAUDE_MD_TEMPLATE)
            .map_err(|e| WikiStoreError::io(paths.schema_claude_md.clone(), e))?;
    }

    Ok(())
}

/// Returns the bytes of the canonical `schema/CLAUDE.md` template that
/// `init_wiki` writes on first run. Exposed so other crates can show it
/// in onboarding screens or "reset to defaults" actions without having
/// to read from disk.
#[must_use]
pub fn canonical_claude_md_template() -> &'static str {
    CLAUDE_MD_TEMPLATE
}

// ─────────────────────────────────────────────────────────────────────
// Raw layer CRUD (S1.1)
// ─────────────────────────────────────────────────────────────────────
//
// `raw/` is the immutable facts layer per ClawWiki canonical §10.
// Files here are written exactly once at ingest time and never mutated;
// the wiki_maintainer agent reads them and produces wiki/ pages on top.
//
// Filenames follow `NNNNN_{source}_{slug}_{date}.md` where:
//   - NNNNN     monotonically-increasing 5-digit id, scanned from disk
//   - source    `paste`, `wechat-text`, `wechat-article`, `voice`, ...
//   - slug      kebab-case short identifier (sanitized from title/url)
//   - date      ISO date `YYYY-MM-DD` of ingestion
//
// Each file starts with a YAML frontmatter block matching schema v1
// (the `Frontmatter` struct below). The body is the raw markdown.

/// YAML frontmatter for raw entries. Matches `schema v1` from
/// `templates/CLAUDE.md` §"Frontmatter (schema v1, required)".
///
/// Serialized via `serde_yaml`-style hand-rolled writer (no extra dep)
/// because the field order is fixed and no escaping is needed for our
/// inputs (slugs and ISO dates).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RawFrontmatter {
    /// Always `"raw"` for entries in this layer.
    pub kind: String,
    /// `"ingested"` while waiting for maintainer; promoted later.
    pub status: String,
    /// Always `"user"` (raw layer is user-curated, not maintainer-owned).
    pub owner: String,
    /// Schema version pin: always `"v1"` until we bump.
    pub schema: String,
    /// Where it came from: `paste`, `wechat-text`, `wechat-article`, etc.
    pub source: String,
    /// Optional source URL when ingesting a URL or web article.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    /// ISO-8601 datetime when the file was written.
    pub ingested_at: String,
}

impl RawFrontmatter {
    /// Build a frontmatter for a `paste`-source text entry. The
    /// `ingested_at` field is filled with the current UTC datetime
    /// (ISO-8601, second precision).
    #[must_use]
    pub fn for_paste(source: &str, source_url: Option<String>) -> Self {
        Self {
            kind: "raw".to_string(),
            status: "ingested".to_string(),
            owner: "user".to_string(),
            schema: "v1".to_string(),
            source: source.to_string(),
            source_url,
            ingested_at: now_iso8601(),
        }
    }

    /// Render the frontmatter as a YAML block delimited by `---` lines,
    /// suitable for prepending to the markdown body.
    ///
    /// We hand-write the serializer rather than pull `serde_yaml` because:
    ///   1. The field set is small and fixed.
    ///   2. None of our values need escaping (controlled inputs only).
    ///   3. Avoiding the dep keeps `wiki_store` at 2 deps total.
    #[must_use]
    pub fn to_yaml_block(&self) -> String {
        let mut s = String::from("---\n");
        s.push_str(&format!("kind: {}\n", self.kind));
        s.push_str(&format!("status: {}\n", self.status));
        s.push_str(&format!("owner: {}\n", self.owner));
        s.push_str(&format!("schema: {}\n", self.schema));
        s.push_str(&format!("source: {}\n", self.source));
        if let Some(url) = &self.source_url {
            s.push_str(&format!("source_url: {url}\n"));
        }
        s.push_str(&format!("ingested_at: {}\n", self.ingested_at));
        s.push_str("---\n");
        s
    }
}

/// On-disk metadata for a single raw entry, returned by [`list_raw_entries`]
/// and [`read_raw_entry`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RawEntry {
    /// Numeric id (the leading 5-digit prefix on the filename).
    pub id: u32,
    /// Just the basename (no path), e.g. `00001_paste_hello_2026-04-09.md`.
    pub filename: String,
    /// Source identifier from the frontmatter (`paste`, `wechat-text`, ...).
    pub source: String,
    /// Slug part of the filename (between source and date).
    pub slug: String,
    /// ISO date from the filename `YYYY-MM-DD`.
    pub date: String,
    /// Optional `source_url` from the frontmatter.
    pub source_url: Option<String>,
    /// ISO-8601 datetime from the frontmatter.
    pub ingested_at: String,
    /// File size in bytes (for the listing UI).
    pub byte_size: u64,
}

/// Compute the next available numeric id by scanning `raw/` for
/// existing `NNNNN_*.md` files and returning `max + 1`. Returns `1`
/// when the directory is empty or absent.
///
/// Pure read — does not create the directory or any file.
pub fn next_raw_id(paths: &WikiPaths) -> Result<u32> {
    if !paths.raw.is_dir() {
        return Ok(1);
    }
    let mut max_id: u32 = 0;
    let dir = fs::read_dir(&paths.raw).map_err(|e| WikiStoreError::io(paths.raw.clone(), e))?;
    for entry in dir.flatten() {
        let name = entry.file_name();
        let name_str = match name.to_str() {
            Some(s) => s,
            None => continue,
        };
        if let Some(id) = parse_id_prefix(name_str) {
            if id > max_id {
                max_id = id;
            }
        }
    }
    Ok(max_id + 1)
}

/// Sanitize a free-form string into a filesystem-safe kebab-case slug.
///
/// * Lowercases ASCII letters
/// * Replaces any run of non-alphanumeric chars with a single `-`
/// * Trims leading/trailing `-`
/// * Caps at 64 chars to keep filenames sane
/// * Returns `"untitled"` if the result is empty
#[must_use]
pub fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut last_dash = true;
    for c in input.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
        if out.len() >= 64 {
            break;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "untitled".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Write a new raw entry under `raw/`. Returns the resolved absolute
/// path (for the HTTP response) so the caller can echo it back to the
/// frontend if useful.
///
/// Filename format: `{NNNNN}_{source}_{slug}_{date}.md` (see module
/// docs above for the full convention).
///
/// The body is wrapped with the YAML frontmatter from `frontmatter`
/// followed by a blank line and then `body` verbatim. We never escape
/// or rewrite `body` — it lands on disk byte-for-byte after the
/// frontmatter prefix.
///
/// Errors:
///   * I/O errors (filesystem full, permission denied, etc.)
///   * `WikiStoreError::Invalid` if `slug` is empty after sanitization
pub fn write_raw_entry(
    paths: &WikiPaths,
    source: &str,
    slug: &str,
    body: &str,
    frontmatter: &RawFrontmatter,
) -> Result<RawEntry> {
    // Resolve next id and ensure raw/ exists.
    fs::create_dir_all(&paths.raw).map_err(|e| WikiStoreError::io(paths.raw.clone(), e))?;
    let id = next_raw_id(paths)?;
    let safe_slug = slugify(slug);
    let date = frontmatter
        .ingested_at
        .split('T')
        .next()
        .unwrap_or("0000-00-00")
        .to_string();
    let filename = format!("{id:05}_{source}_{safe_slug}_{date}.md");
    let path = paths.raw.join(&filename);

    // Compose the full file content: YAML frontmatter + blank line + body.
    let mut content = frontmatter.to_yaml_block();
    content.push('\n');
    content.push_str(body);
    if !body.ends_with('\n') {
        content.push('\n');
    }

    fs::write(&path, &content).map_err(|e| WikiStoreError::io(path.clone(), e))?;

    let metadata = fs::metadata(&path).map_err(|e| WikiStoreError::io(path.clone(), e))?;
    Ok(RawEntry {
        id,
        filename,
        source: source.to_string(),
        slug: safe_slug,
        date,
        source_url: frontmatter.source_url.clone(),
        ingested_at: frontmatter.ingested_at.clone(),
        byte_size: metadata.len(),
    })
}

/// List every raw entry under `raw/`, sorted by id ascending. Empty
/// directory or missing directory both return an empty `Vec`.
///
/// This intentionally re-parses each file's frontmatter on every call
/// rather than maintaining an index. The directory rarely exceeds a
/// few hundred entries during MVP, and re-parsing avoids any
/// drift-vs-disk bug. S6 may add a `manifest.json` cache when needed.
pub fn list_raw_entries(paths: &WikiPaths) -> Result<Vec<RawEntry>> {
    if !paths.raw.is_dir() {
        return Ok(Vec::new());
    }
    let mut entries: Vec<RawEntry> = Vec::new();
    let dir = fs::read_dir(&paths.raw).map_err(|e| WikiStoreError::io(paths.raw.clone(), e))?;
    for entry in dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        if let Ok(parsed) = parse_raw_file(&path) {
            entries.push(parsed);
        }
    }
    entries.sort_by_key(|e| e.id);
    Ok(entries)
}

/// Read a single raw entry by its numeric id. Returns the parsed
/// metadata along with the body text (everything after the closing
/// `---` line of the frontmatter, leading newline trimmed).
pub fn read_raw_entry(paths: &WikiPaths, id: u32) -> Result<(RawEntry, String)> {
    let entries = list_raw_entries(paths)?;
    let entry = entries
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| WikiStoreError::NotFound(id))?;
    let path = paths.raw.join(&entry.filename);
    let content = fs::read_to_string(&path).map_err(|e| WikiStoreError::io(path.clone(), e))?;
    let body = strip_frontmatter(&content).to_string();
    Ok((entry, body))
}

// ── internal helpers ──────────────────────────────────────────────

/// Pull the leading 5-digit id off a filename, returning `None` for
/// names that do not match the convention.
fn parse_id_prefix(name: &str) -> Option<u32> {
    let mut chars = name.chars();
    let mut digits = String::new();
    for _ in 0..5 {
        match chars.next() {
            Some(c) if c.is_ascii_digit() => digits.push(c),
            _ => return None,
        }
    }
    if chars.next() != Some('_') {
        return None;
    }
    digits.parse::<u32>().ok()
}

/// Parse a single raw file: extract id from the filename, parse the
/// frontmatter to read source / source_url / ingested_at, and stat()
/// the file for its byte size. The body itself is not loaded — use
/// [`read_raw_entry`] for that.
fn parse_raw_file(path: &Path) -> Result<RawEntry> {
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| WikiStoreError::Invalid("filename is not utf-8".to_string()))?
        .to_string();
    let id = parse_id_prefix(&filename)
        .ok_or_else(|| WikiStoreError::Invalid(format!("filename missing id prefix: {filename}")))?;

    // Strip the `NNNNN_` prefix and `.md` suffix, then split on `_` to
    // recover source / slug / date. The slug itself may contain `-` but
    // never `_` (slugify converts `_` to `-`), so this is unambiguous.
    let stem = filename
        .strip_suffix(".md")
        .unwrap_or(&filename)
        .splitn(2, '_')
        .nth(1)
        .unwrap_or("");
    let parts: Vec<&str> = stem.rsplitn(2, '_').collect();
    let date = parts.first().copied().unwrap_or("").to_string();
    let source_and_slug = parts.get(1).copied().unwrap_or("");
    let mut sas = source_and_slug.splitn(2, '_');
    let source = sas.next().unwrap_or("").to_string();
    let slug = sas.next().unwrap_or("").to_string();

    let content = fs::read_to_string(path).map_err(|e| WikiStoreError::io(path.to_path_buf(), e))?;
    let (source_url, ingested_at) = parse_frontmatter_fields(&content);
    let metadata = fs::metadata(path).map_err(|e| WikiStoreError::io(path.to_path_buf(), e))?;

    Ok(RawEntry {
        id,
        filename,
        source,
        slug,
        date,
        source_url,
        ingested_at,
        byte_size: metadata.len(),
    })
}

/// Pull `source_url` and `ingested_at` out of the YAML frontmatter
/// block at the top of a file. Tolerant: missing fields are returned
/// as `None` / empty string rather than erroring.
fn parse_frontmatter_fields(content: &str) -> (Option<String>, String) {
    let mut source_url: Option<String> = None;
    let mut ingested_at = String::new();
    let mut in_frontmatter = false;
    for line in content.lines() {
        if line == "---" {
            if in_frontmatter {
                break;
            }
            in_frontmatter = true;
            continue;
        }
        if !in_frontmatter {
            continue;
        }
        if let Some(rest) = line.strip_prefix("source_url: ") {
            source_url = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("ingested_at: ") {
            ingested_at = rest.to_string();
        }
    }
    (source_url, ingested_at)
}

/// Return the body text after the closing `---` of a frontmatter
/// block. If no frontmatter is found, the entire content is returned.
fn strip_frontmatter(content: &str) -> &str {
    let mut lines = content.lines();
    if lines.next() != Some("---") {
        return content;
    }
    let mut byte_offset = "---\n".len();
    for line in lines {
        byte_offset += line.len() + 1;
        if line == "---" {
            return content[byte_offset..].trim_start_matches('\n');
        }
    }
    content
}

// ─────────────────────────────────────────────────────────────────────
// Inbox layer (S4.1)
// ─────────────────────────────────────────────────────────────────────
//
// The Inbox is the wiki_maintainer's queue of pending human-review
// decisions. Per ClawWiki canonical §6.1 and §11.2, it holds every
// maintainer-proposed action that needs user approval: "new raw
// entry → propose concept page", "stale page → re-verify",
// "conflict → pick canonical", etc.
//
// S4 MVP: only the `new-raw` kind is produced, and only by the
// `ingest_wiki_raw_handler` side-channel in desktop-server. Real
// maintainer proposals (conflict / stale / deprecate) land once
// codex_broker::chat_completion is wired in a future sprint.
//
// Persistence: a plaintext JSON array at `{meta_dir}/inbox.json`.
// Unlike the Codex account pool this is NOT encrypted — the records
// contain no secrets, only public wiki metadata (ids, slugs, dates).
// Plaintext lets the user open the file directly if they want to
// audit or migrate.

/// One line item in the Inbox. Discriminated by `kind`; `source_raw_id`
/// points at the raw layer entry that caused this task (e.g. the
/// `NNNNN_paste_hello_2026-04-09.md` file that just landed).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InboxEntry {
    /// Monotonically-increasing id, scoped to the inbox file. Never
    /// reused even after resolve — we just append.
    pub id: u32,
    /// What kind of maintainer task this is. For S4 always `"new-raw"`.
    pub kind: String,
    /// Current status. Starts as `"pending"`, moves to `"approved"`
    /// or `"rejected"` after the user resolves it.
    pub status: String,
    /// Short human-readable title shown in the Inbox list.
    pub title: String,
    /// Longer description shown in the detail pane.
    pub description: String,
    /// The raw layer entry id that caused this task, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_raw_id: Option<u32>,
    /// ISO-8601 datetime the task was created.
    pub created_at: String,
    /// ISO-8601 datetime the task was resolved, or `None` while pending.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<String>,
}

/// Filename for the inbox persistence file under `{meta}/`.
pub const INBOX_FILENAME: &str = "inbox.json";

fn inbox_path(paths: &WikiPaths) -> PathBuf {
    paths.meta.join(INBOX_FILENAME)
}

fn load_inbox_file(paths: &WikiPaths) -> Result<Vec<InboxEntry>> {
    let path = inbox_path(paths);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let bytes = fs::read(&path).map_err(|e| WikiStoreError::io(path.clone(), e))?;
    let parsed: Vec<InboxEntry> = serde_json::from_slice(&bytes)
        .map_err(|e| WikiStoreError::Invalid(format!("inbox.json parse error: {e}")))?;
    Ok(parsed)
}

fn save_inbox_file(paths: &WikiPaths, entries: &[InboxEntry]) -> Result<()> {
    fs::create_dir_all(&paths.meta).map_err(|e| WikiStoreError::io(paths.meta.clone(), e))?;
    let bytes = serde_json::to_vec_pretty(entries)
        .map_err(|e| WikiStoreError::Invalid(format!("inbox.json serialize error: {e}")))?;
    let path = inbox_path(paths);
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &bytes).map_err(|e| WikiStoreError::io(tmp.clone(), e))?;
    fs::rename(&tmp, &path).map_err(|e| WikiStoreError::io(path.clone(), e))?;
    Ok(())
}

/// Append a new pending inbox entry and return the stored record.
/// Assigns the next id (max+1, or 1 if empty), timestamps with
/// `now_iso8601`, writes atomically to disk.
pub fn append_inbox_pending(
    paths: &WikiPaths,
    kind: &str,
    title: &str,
    description: &str,
    source_raw_id: Option<u32>,
) -> Result<InboxEntry> {
    let mut entries = load_inbox_file(paths)?;
    let next_id = entries.iter().map(|e| e.id).max().unwrap_or(0) + 1;
    let entry = InboxEntry {
        id: next_id,
        kind: kind.to_string(),
        status: "pending".to_string(),
        title: title.to_string(),
        description: description.to_string(),
        source_raw_id,
        created_at: now_iso8601(),
        resolved_at: None,
    };
    entries.push(entry.clone());
    save_inbox_file(paths, &entries)?;
    Ok(entry)
}

/// List every inbox entry, oldest first. Missing file returns empty.
pub fn list_inbox_entries(paths: &WikiPaths) -> Result<Vec<InboxEntry>> {
    load_inbox_file(paths)
}

/// Resolve a pending entry by id. `action` must be `"approve"` or
/// `"reject"`; any other value fails with `WikiStoreError::Invalid`.
/// Returns the updated record.
pub fn resolve_inbox_entry(
    paths: &WikiPaths,
    id: u32,
    action: &str,
) -> Result<InboxEntry> {
    let new_status = match action {
        "approve" => "approved",
        "reject" => "rejected",
        other => {
            return Err(WikiStoreError::Invalid(format!(
                "unknown inbox action: {other}"
            )))
        }
    };
    let mut entries = load_inbox_file(paths)?;
    let found = entries
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or(WikiStoreError::NotFound(id))?;
    found.status = new_status.to_string();
    found.resolved_at = Some(now_iso8601());
    let updated = found.clone();
    save_inbox_file(paths, &entries)?;
    Ok(updated)
}

/// Count pending inbox entries. Used by the Dashboard and Sidebar
/// badge. O(n) in the inbox size — fine for the MVP since the inbox
/// never exceeds a few hundred entries during normal use.
pub fn count_pending_inbox(paths: &WikiPaths) -> Result<usize> {
    Ok(load_inbox_file(paths)?
        .iter()
        .filter(|e| e.status == "pending")
        .count())
}

/// Hand-rolled UTC ISO-8601 timestamp formatter, second precision.
/// We use `std::time` rather than pulling `chrono` for one function.
fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_iso8601(secs)
}

/// Format an `i64` epoch-seconds value as `YYYY-MM-DDTHH:MM:SSZ`.
/// Pure function with no globals — exposed `pub(crate)` so tests can
/// pin specific timestamps without polluting the system clock.
fn format_iso8601(epoch_secs: u64) -> String {
    // Days from 1970-01-01 (Thursday) to the start of `epoch_secs`'s day.
    let days = (epoch_secs / 86_400) as i64;
    let secs_of_day = epoch_secs % 86_400;
    let hours = secs_of_day / 3600;
    let minutes = (secs_of_day % 3600) / 60;
    let seconds = secs_of_day % 60;

    // Convert `days since 1970-01-01` to (year, month, day) using
    // a Howard Hinnant-style civil-from-days formula.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };

    format!(
        "{year:04}-{m:02}-{d:02}T{hours:02}:{minutes:02}:{seconds:02}Z"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use tempfile::tempdir;

    #[test]
    fn wiki_paths_resolve_is_pure() {
        let root = Path::new("/tmp/clawwiki-test");
        let paths = WikiPaths::resolve(root);
        assert_eq!(paths.root, root);
        assert_eq!(paths.raw, root.join("raw"));
        assert_eq!(paths.wiki, root.join("wiki"));
        assert_eq!(paths.schema, root.join("schema"));
        assert_eq!(paths.meta, root.join(".clawwiki"));
        assert_eq!(paths.schema_claude_md, root.join("schema").join("CLAUDE.md"));
    }

    #[test]
    fn init_wiki_creates_all_subdirs() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        assert!(tmp.path().join(RAW_DIR).is_dir(), "raw/ not created");
        assert!(tmp.path().join(WIKI_DIR).is_dir(), "wiki/ not created");
        assert!(tmp.path().join(SCHEMA_DIR).is_dir(), "schema/ not created");
        assert!(tmp.path().join(META_DIR).is_dir(), ".clawwiki/ not created");
    }

    #[test]
    fn init_wiki_seeds_canonical_claude_md() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let claude_md_path = tmp.path().join(SCHEMA_DIR).join(CLAUDE_MD_FILENAME);
        assert!(claude_md_path.is_file(), "schema/CLAUDE.md not seeded");

        let content = fs::read_to_string(&claude_md_path).unwrap();
        // Hard-pin a few canonical landmarks. If the template ever
        // drifts away from §8 of product-design.md, these break loudly.
        assert!(
            content.starts_with("# CLAUDE.md · wiki-maintainer agent rules"),
            "header missing or changed"
        );
        assert!(content.contains("## Layer contract"), "Layer contract section missing");
        assert!(content.contains("## Triggers"), "Triggers section missing");
        assert!(
            content.contains("## Frontmatter (schema v1, required)"),
            "frontmatter section missing"
        );
        assert!(
            content.contains("## Tool permissions (WikiPermissionDialog enforces)"),
            "tool permissions section missing"
        );
        assert!(content.contains("## Never do"), "never-do section missing");
        // The 5 maintenance actions are the most operationally important
        // promise of CLAUDE.md — pin all five.
        assert!(content.contains("summarise"));
        assert!(content.contains("update affected concept"));
        assert!(content.contains("backlinks"));
        assert!(content.contains("mark_conflict"));
        assert!(content.contains("changelog/YYYY-MM-DD"));
    }

    #[test]
    fn init_wiki_is_idempotent() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        // Second call must not error.
        init_wiki(tmp.path()).unwrap();
        // Third call must not error.
        init_wiki(tmp.path()).unwrap();
        assert!(tmp.path().join(RAW_DIR).is_dir());
        assert!(tmp.path().join(SCHEMA_DIR).join(CLAUDE_MD_FILENAME).is_file());
    }

    #[test]
    fn init_wiki_preserves_user_edited_claude_md() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let claude_md = tmp.path().join(SCHEMA_DIR).join(CLAUDE_MD_FILENAME);

        // User hand-edits the file (or runs the Inbox proposal flow).
        let user_content = "# MY CUSTOM RULES\n\nDo whatever I say.\n";
        fs::write(&claude_md, user_content).unwrap();

        // A second init_wiki on next desktop-server start MUST NOT
        // overwrite the user's edits. This is a hard contract.
        init_wiki(tmp.path()).unwrap();

        let after = fs::read_to_string(&claude_md).unwrap();
        assert_eq!(after, user_content, "user edits were clobbered");
    }

    #[test]
    fn init_wiki_works_on_existing_root_with_extra_files() {
        let tmp = tempdir().unwrap();
        // Pre-create the root and put a stray file in it (e.g. a
        // pre-existing .git/, README.md, etc).
        fs::create_dir_all(tmp.path()).unwrap();
        fs::write(tmp.path().join("README.md"), "stray").unwrap();

        init_wiki(tmp.path()).unwrap();

        // The stray file is untouched, the layout is in place.
        assert_eq!(
            fs::read_to_string(tmp.path().join("README.md")).unwrap(),
            "stray"
        );
        assert!(tmp.path().join(RAW_DIR).is_dir());
        assert!(tmp.path().join(WIKI_DIR).is_dir());
    }

    #[test]
    fn default_root_from_env_override_wins_over_home() {
        let resolved = default_root_from(
            Some(OsString::from("/explicit/override/path")),
            Some(Path::new("/home/user")),
        );
        assert_eq!(resolved, PathBuf::from("/explicit/override/path"));
    }

    #[test]
    fn default_root_from_falls_back_to_home_clawwiki() {
        let resolved = default_root_from(None, Some(Path::new("/home/user")));
        assert_eq!(resolved, PathBuf::from("/home/user").join(".clawwiki"));
    }

    #[test]
    fn default_root_from_handles_missing_home_with_relative_fallback() {
        let resolved = default_root_from(None, None);
        assert_eq!(resolved, PathBuf::from(".").join(".clawwiki"));
    }

    #[test]
    fn canonical_claude_md_template_is_non_empty_and_versioned() {
        let template = canonical_claude_md_template();
        assert!(template.len() > 1000, "template suspiciously short");
        assert!(template.contains("schema:        v1"));
    }

    // ── S1.1 raw layer CRUD tests ─────────────────────────────────

    #[test]
    fn slugify_handles_chinese_and_symbols() {
        // ASCII passthrough
        assert_eq!(slugify("Hello World"), "hello-world");
        // Symbols collapse to single dash
        assert_eq!(slugify("foo_bar.baz!!qux"), "foo-bar-baz-qux");
        // Pure non-ascii falls back to "untitled"
        assert_eq!(slugify("中文标题"), "untitled");
        // Empty
        assert_eq!(slugify(""), "untitled");
        // Leading / trailing junk trimmed
        assert_eq!(slugify("  ---hello---  "), "hello");
    }

    #[test]
    fn next_raw_id_starts_at_1_for_empty() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        assert_eq!(next_raw_id(&paths).unwrap(), 1);
    }

    #[test]
    fn next_raw_id_handles_missing_raw_dir() {
        let tmp = tempdir().unwrap();
        // Do NOT call init_wiki — raw/ doesn't exist
        let paths = WikiPaths::resolve(tmp.path());
        assert_eq!(next_raw_id(&paths).unwrap(), 1);
    }

    #[test]
    fn write_then_list_then_read_round_trip() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let frontmatter = RawFrontmatter::for_paste("paste", None);
        let entry = write_raw_entry(
            &paths,
            "paste",
            "Hello World",
            "This is the body.\n",
            &frontmatter,
        )
        .unwrap();

        assert_eq!(entry.id, 1);
        assert!(entry.filename.starts_with("00001_paste_hello-world_"));
        assert!(entry.filename.ends_with(".md"));
        assert_eq!(entry.source, "paste");
        assert_eq!(entry.slug, "hello-world");

        let listed = list_raw_entries(&paths).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, 1);
        assert_eq!(listed[0].filename, entry.filename);

        let (read, body) = read_raw_entry(&paths, 1).unwrap();
        assert_eq!(read.id, 1);
        assert_eq!(body, "This is the body.\n");
    }

    #[test]
    fn write_assigns_monotonically_increasing_ids() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        for i in 1..=5 {
            let fm = RawFrontmatter::for_paste("paste", None);
            let entry =
                write_raw_entry(&paths, "paste", &format!("entry {i}"), "body", &fm).unwrap();
            assert_eq!(entry.id, i);
        }

        // After deleting #3 we should still get id 6, NOT id 3.
        let target = paths.raw.join(format!(
            "00003_paste_entry-3_{}",
            now_iso8601().split('T').next().unwrap()
        ));
        // We don't know the exact filename — discover it.
        let listed = list_raw_entries(&paths).unwrap();
        let third = &listed[2];
        let _ = target; // appease unused
        fs::remove_file(paths.raw.join(&third.filename)).unwrap();

        let next = next_raw_id(&paths).unwrap();
        assert_eq!(next, 6, "next id should reuse 6, not back-fill 3");
    }

    #[test]
    fn read_raw_entry_returns_not_found_for_missing_id() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let err = read_raw_entry(&paths, 999).unwrap_err();
        assert!(matches!(err, WikiStoreError::NotFound(999)));
    }

    #[test]
    fn frontmatter_yaml_block_round_trip() {
        let fm = RawFrontmatter {
            kind: "raw".to_string(),
            status: "ingested".to_string(),
            owner: "user".to_string(),
            schema: "v1".to_string(),
            source: "paste".to_string(),
            source_url: Some("https://example.com/article".to_string()),
            ingested_at: "2026-04-09T14:22:00Z".to_string(),
        };
        let yaml = fm.to_yaml_block();
        assert!(yaml.starts_with("---\n"));
        assert!(yaml.ends_with("---\n"));
        assert!(yaml.contains("kind: raw\n"));
        assert!(yaml.contains("status: ingested\n"));
        assert!(yaml.contains("owner: user\n"));
        assert!(yaml.contains("schema: v1\n"));
        assert!(yaml.contains("source: paste\n"));
        assert!(yaml.contains("source_url: https://example.com/article\n"));
        assert!(yaml.contains("ingested_at: 2026-04-09T14:22:00Z\n"));
    }

    #[test]
    fn frontmatter_omits_source_url_when_absent() {
        let fm = RawFrontmatter::for_paste("paste", None);
        let yaml = fm.to_yaml_block();
        assert!(!yaml.contains("source_url:"));
    }

    #[test]
    fn list_raw_entries_skips_non_md_files() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        // Real entry
        let fm = RawFrontmatter::for_paste("paste", None);
        write_raw_entry(&paths, "paste", "real", "body", &fm).unwrap();

        // Decoy file: looks like an id-prefixed name but `.txt` extension
        fs::write(paths.raw.join("00099_decoy_x_2026-01-01.txt"), "junk").unwrap();
        // Decoy file: `.md` but no id prefix
        fs::write(paths.raw.join("README.md"), "this is not a raw entry").unwrap();

        let listed = list_raw_entries(&paths).unwrap();
        assert_eq!(listed.len(), 1, "only the real entry should be listed");
        assert_eq!(listed[0].id, 1);
    }

    #[test]
    fn read_raw_entry_strips_frontmatter_from_body() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let fm = RawFrontmatter::for_paste("paste", None);
        write_raw_entry(
            &paths,
            "paste",
            "test",
            "Line one.\nLine two.\n",
            &fm,
        )
        .unwrap();

        let (_entry, body) = read_raw_entry(&paths, 1).unwrap();
        assert_eq!(body, "Line one.\nLine two.\n");
        assert!(!body.starts_with("---"));
    }

    #[test]
    fn format_iso8601_known_epoch() {
        // 1700000000 = 2023-11-14T22:13:20Z
        assert_eq!(format_iso8601(1_700_000_000), "2023-11-14T22:13:20Z");
        // Epoch start
        assert_eq!(format_iso8601(0), "1970-01-01T00:00:00Z");
        // Y2K
        assert_eq!(format_iso8601(946_684_800), "2000-01-01T00:00:00Z");
    }

    // ── S4.1 Inbox layer tests ───────────────────────────────────

    #[test]
    fn inbox_list_empty_when_missing() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        assert_eq!(list_inbox_entries(&paths).unwrap().len(), 0);
        assert_eq!(count_pending_inbox(&paths).unwrap(), 0);
    }

    #[test]
    fn inbox_append_assigns_sequential_ids() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let e1 =
            append_inbox_pending(&paths, "new-raw", "first", "desc", Some(1)).unwrap();
        let e2 =
            append_inbox_pending(&paths, "new-raw", "second", "desc", Some(2)).unwrap();
        let e3 =
            append_inbox_pending(&paths, "new-raw", "third", "desc", None).unwrap();

        assert_eq!(e1.id, 1);
        assert_eq!(e2.id, 2);
        assert_eq!(e3.id, 3);
        assert_eq!(e1.status, "pending");
        assert_eq!(e1.source_raw_id, Some(1));
        assert_eq!(e3.source_raw_id, None);

        let listed = list_inbox_entries(&paths).unwrap();
        assert_eq!(listed.len(), 3);
        assert_eq!(count_pending_inbox(&paths).unwrap(), 3);
    }

    #[test]
    fn inbox_resolve_approve_flips_status() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let entry =
            append_inbox_pending(&paths, "new-raw", "test", "desc", Some(1)).unwrap();
        let resolved = resolve_inbox_entry(&paths, entry.id, "approve").unwrap();

        assert_eq!(resolved.id, entry.id);
        assert_eq!(resolved.status, "approved");
        assert!(resolved.resolved_at.is_some());

        // Re-reading from disk must reflect the change.
        let listed = list_inbox_entries(&paths).unwrap();
        assert_eq!(listed[0].status, "approved");
        assert_eq!(count_pending_inbox(&paths).unwrap(), 0);
    }

    #[test]
    fn inbox_resolve_reject_marks_rejected() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        append_inbox_pending(&paths, "new-raw", "keep", "", Some(1)).unwrap();
        let bad = append_inbox_pending(&paths, "new-raw", "drop", "", Some(2)).unwrap();

        resolve_inbox_entry(&paths, bad.id, "reject").unwrap();

        let entries = list_inbox_entries(&paths).unwrap();
        assert_eq!(entries[0].status, "pending");
        assert_eq!(entries[1].status, "rejected");
        assert_eq!(count_pending_inbox(&paths).unwrap(), 1);
    }

    #[test]
    fn inbox_resolve_unknown_id_returns_not_found() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        let err = resolve_inbox_entry(&paths, 999, "approve").unwrap_err();
        assert!(matches!(err, WikiStoreError::NotFound(999)));
    }

    #[test]
    fn inbox_resolve_unknown_action_is_invalid() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        let e = append_inbox_pending(&paths, "new-raw", "t", "", None).unwrap();
        let err = resolve_inbox_entry(&paths, e.id, "banana").unwrap_err();
        assert!(matches!(err, WikiStoreError::Invalid(_)));
    }

    #[test]
    fn inbox_persists_across_calls() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        append_inbox_pending(&paths, "new-raw", "task-a", "", Some(1)).unwrap();
        append_inbox_pending(&paths, "new-raw", "task-b", "", Some(2)).unwrap();

        // Simulate process restart by re-resolving paths and reading.
        let paths_fresh = WikiPaths::resolve(tmp.path());
        let listed = list_inbox_entries(&paths_fresh).unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].title, "task-a");
        assert_eq!(listed[1].title, "task-b");
    }
}

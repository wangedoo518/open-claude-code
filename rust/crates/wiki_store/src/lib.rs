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
use std::sync::{Mutex, MutexGuard, OnceLock};

use serde::{Deserialize, Serialize};

/// Process-global guard that serializes read-modify-write access to
/// `{meta}/inbox.json`. Addresses the TOCTOU race found by the
/// S4/S5/S6 code review: without this guard, two concurrent callers
/// of `append_inbox_pending` can both read the same `max(id)+1`,
/// producing a pair of entries with identical ids and losing one on
/// the next save. Same story for two concurrent `resolve_inbox_entry`
/// calls racing on a single id.
///
/// We use a single process-global mutex (rather than per-path) because
/// desktop-server runs as a single process with exactly one wiki root
/// per `$CLAWWIKI_HOME`. Workspace-wide serialization of inbox mutations
/// costs ~microseconds per append and is completely dwarfed by the
/// subsequent file I/O. If a future ClawWiki daemon ever needs to
/// manage multiple wiki roots concurrently, we'd switch to a
/// `DashMap<PathBuf, Mutex<()>>` keyed by the inbox file path.
///
/// Poison recovery: any panic inside the critical section leaves the
/// on-disk file in one of two well-defined states (untouched if the
/// write failed before `rename`, or committed if the write succeeded),
/// so `into_inner()` on a poisoned lock is safe.
static INBOX_WRITE_GUARD: OnceLock<Mutex<()>> = OnceLock::new();

fn lock_inbox_writes() -> MutexGuard<'static, ()> {
    INBOX_WRITE_GUARD
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Process-global guard that serializes `append_wiki_log` and
/// `rebuild_wiki_index` calls. Same reasoning as `INBOX_WRITE_GUARD`:
/// two concurrent appenders can race on read-modify-write (load file,
/// append, save) and lose entries. A single wiki root per process
/// makes one global mutex the simplest correct design.
static WIKI_WRITE_GUARD: OnceLock<Mutex<()>> = OnceLock::new();

fn lock_wiki_writes() -> MutexGuard<'static, ()> {
    WIKI_WRITE_GUARD
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Subdirectory under the wiki root that holds immutable WeChat-ingested
/// facts. Files here MUST NOT be mutated by any tool.
pub const RAW_DIR: &str = "raw";

/// Subdirectory under the wiki root that holds LLM-maintained pages.
/// The maintainer agent has write access; the user has audit access via
/// the Inbox UI.
pub const WIKI_DIR: &str = "wiki";

/// Subdirectory under `wiki/` for concept pages specifically. Other
/// canonical categories (people, topics, compare, changelog) get
/// their own subdirs as future sprints add them; for S4 maintainer
/// MVP only `concepts/` exists.
pub const WIKI_CONCEPTS_SUBDIR: &str = "concepts";

/// Filename for the content-oriented catalog at the top of `wiki/`.
/// Karpathy's llm-wiki.md calls out `index.md` as the "first entry
/// point for query-time retrieval — works at moderate scale without
/// needing embedding-based RAG". We rebuild it after every
/// maintainer write so it stays in sync with `concepts/*.md`.
pub const WIKI_INDEX_FILENAME: &str = "index.md";

/// Filename for the chronological append-only audit log at the top
/// of `wiki/`. Canonical §8 Triggers pins the entry prefix to
/// `## [YYYY-MM-DD HH:MM] <verb> | <title>` so simple grep tools
/// (per Karpathy's llm-wiki.md §"Indexing and logging") can parse
/// the history without any structured store.
pub const WIKI_LOG_FILENAME: &str = "log.md";

/// Subdirectory under `wiki/` for per-day changelog files.
/// Canonical §8 Triggers row 5 + §10 layout: every maintainer
/// action also gets appended to `wiki/changelog/YYYY-MM-DD.md`
/// (one file per day) so users can see "what happened today" in a
/// single view without scrolling through the global log. The day
/// file shares the same `## [HH:MM] verb | title` line format as
/// the global log so a simple `cat changelog/2026-04-10.md` reads
/// naturally.
pub const WIKI_CHANGELOG_SUBDIR: &str = "changelog";

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

/// Default `.gitignore` content seeded alongside `git init`.
///
/// Excludes:
/// * `*.tmp` — the atomic-write sidecars from `write_wiki_page` /
///   `save_inbox_file` / `append_wiki_log`. These exist for < 1ms
///   during normal operation but would churn the git working tree
///   if a write crashed mid-rename.
/// * `.clawwiki/cloud-accounts.enc` — AES-GCM encrypted Codex pool
///   storage from `codex_broker`. Still encrypted, but committing
///   a ciphertext + a stable key file is a recipe for key-leak
///   regret. Better to keep the whole thing out of history.
/// * `.clawwiki/*.db` — SQLite sessions, ephemeral caches.
/// * `Thumbs.db` / `.DS_Store` — OS cruft.
const GITIGNORE_TEMPLATE: &str = r#"# ClawWiki autogenerated .gitignore
# Safe to edit — re-running init_wiki will NOT overwrite user edits.

# Atomic-write sidecars from wiki_store (tmp + rename pattern)
*.tmp

# Encrypted Codex account pool (never commit even encrypted)
.clawwiki/cloud-accounts.enc

# SQLite session stores and other ephemeral caches
.clawwiki/*.db
.clawwiki/*.db-journal
.clawwiki/*.db-wal
.clawwiki/*.db-shm

# OS cruft
Thumbs.db
.DS_Store
"#;

/// Bootstrap the wiki layout at `root`. Idempotent: safe to call on
/// every desktop-server startup.
///
/// Behavior:
/// * Creates `root/`, `raw/`, `wiki/`, `schema/`, `.clawwiki/` if any
///   are missing. Existing directories are left untouched.
/// * Seeds `schema/CLAUDE.md` from the canonical template ONLY when the
///   file does not exist yet. Once the user (or the maintainer Inbox
///   flow) has touched it, future calls are a no-op for that file.
/// * Seeds `.gitignore` and runs `git init` on the root if `git` is
///   available and no repo exists yet (canonical §10: "white-free
///   version history"). All git operations are **soft-fail** — if
///   `git` is not in PATH, or init fails for any reason, `init_wiki`
///   still returns Ok and the wiki continues to work without version
///   control. A warning lands on stderr.
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

    // Seed .gitignore only if absent.
    let gitignore_path = paths.root.join(".gitignore");
    if !gitignore_path.exists() {
        fs::write(&gitignore_path, GITIGNORE_TEMPLATE)
            .map_err(|e| WikiStoreError::io(gitignore_path.clone(), e))?;
    }

    // Initialize a git repo on the root if we haven't already AND
    // `git` is available. Soft-fail: any error is logged but not
    // propagated. Canonical §10 "white-free version history".
    let git_dir = paths.root.join(".git");
    if !git_dir.exists() {
        if let Err(e) = try_git_init(&paths.root) {
            eprintln!(
                "wiki_store: git init failed for {:?}: {e}. Version history will be off.",
                paths.root
            );
        }
    }

    Ok(())
}

/// Attempt to run `git init` on the given directory. Returns Ok(())
/// on success or if git is not installed (the "no git" case is the
/// happy path — we silently disable version history). Returns
/// `WikiStoreError::Io` only for genuine filesystem errors.
///
/// Separated from `init_wiki` so tests can exercise the git-available
/// path in isolation.
fn try_git_init(root: &Path) -> Result<()> {
    use std::process::Command;

    // Check if git binary is on PATH. If not, silently skip —
    // canonical §10 "white-free version history" is a nice-to-have,
    // not a hard requirement.
    let probe = Command::new("git").arg("--version").output();
    let probe_ok = match probe {
        Ok(output) => output.status.success(),
        Err(_) => false,
    };
    if !probe_ok {
        return Ok(());
    }

    // Run `git init --initial-branch=main` (or just `git init` if
    // the git version doesn't support the flag — falls back silently).
    // We use Command's stdin/stdout null-piping to avoid any
    // interactive prompts bleeding onto the server's stderr.
    let output = Command::new("git")
        .arg("init")
        .arg("--initial-branch=main")
        .arg(root)
        .output()
        .map_err(|e| WikiStoreError::io(root.to_path_buf(), e))?;
    if !output.status.success() {
        // Try again without --initial-branch for older git versions.
        let output2 = Command::new("git")
            .arg("init")
            .arg(root)
            .output()
            .map_err(|e| WikiStoreError::io(root.to_path_buf(), e))?;
        if !output2.status.success() {
            return Err(WikiStoreError::Invalid(format!(
                "git init exited non-zero: {}",
                String::from_utf8_lossy(&output2.stderr)
            )));
        }
    }
    Ok(())
}

/// Overwrite `schema/CLAUDE.md` with new user-supplied content.
/// Canonical §8: schema is human-curated; this is the human write
/// path (the maintainer agent never calls this — it goes through
/// the Inbox proposal flow instead).
///
/// Atomic via tmp + rename. Idempotent — calling twice with the
/// same content produces the same on-disk result. Empty content
/// is REJECTED to prevent accidental schema truncation; callers
/// should validate before calling.
pub fn overwrite_schema_claude_md(paths: &WikiPaths, content: &str) -> Result<()> {
    if content.trim().is_empty() {
        return Err(WikiStoreError::Invalid(
            "schema content must not be empty".to_string(),
        ));
    }
    fs::create_dir_all(&paths.schema)
        .map_err(|e| WikiStoreError::io(paths.schema.clone(), e))?;
    let target = paths.schema_claude_md.clone();
    let tmp = target.with_extension("md.tmp");
    fs::write(&tmp, content.as_bytes()).map_err(|e| WikiStoreError::io(tmp.clone(), e))?;
    fs::rename(&tmp, &target).map_err(|e| WikiStoreError::io(target.clone(), e))?;
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
// Wiki layer CRUD (S4.2 — engram-style maintainer output target)
// ─────────────────────────────────────────────────────────────────────
//
// `wiki/concepts/` is where the maintainer agent writes its output
// per ClawWiki canonical §10 layer contract and §4 blade 3. Every
// file is a human-reviewable concept page produced by
// `wiki_maintainer::propose_for_raw_entry` followed by a human
// "Approve & Write" click on the Inbox detail pane.
//
// Filename convention: `{slug}.md` (no numeric prefix — the slug IS
// the primary key). A second write with the same slug OVERWRITES
// the existing file; that's the MVP contract. A future sprint can
// add an "already exists" warning path to the approve flow.
//
// Frontmatter shape matches canonical schema v1 (`type: concept`,
// `status: draft`, `owner: maintainer`, `schema: v1`, plus
// `title`, `summary`, optional `source_raw_id`, `created_at`).

/// YAML frontmatter for concept wiki pages. Matches the canonical
/// schema v1 "type: concept" shape from `templates/CLAUDE.md`
/// §"Frontmatter (schema v1, required)".
///
/// Rust's `type` is a reserved keyword, so the discriminator field
/// is stored as `kind` in Rust and emitted as `type` in YAML.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WikiFrontmatter {
    /// Always `"concept"` for entries in `wiki/concepts/`. Future
    /// sprints add `"people"`, `"topic"`, `"compare"`, `"changelog"`.
    pub kind: String,
    /// `"draft"` for newly-approved maintainer pages. The user can
    /// promote to `"canonical"` via a future edit flow.
    pub status: String,
    /// Always `"maintainer"` for pages written via the engram flow.
    pub owner: String,
    /// Schema version pin: always `"v1"` until we bump.
    pub schema: String,
    /// Human-readable display title. May contain CJK, spaces, etc.
    pub title: String,
    /// Short one-line summary (≤ 200 chars per CLAUDE.md §Triggers).
    pub summary: String,
    /// The raw/ entry id that seeded this proposal. `None` when the
    /// page was hand-written outside the maintainer flow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_raw_id: Option<u32>,
    /// ISO-8601 datetime when the page was first written.
    pub created_at: String,
}

impl WikiFrontmatter {
    /// Build a frontmatter for a freshly-approved maintainer proposal.
    /// `created_at` is filled with the current UTC datetime.
    #[must_use]
    pub fn for_concept(title: &str, summary: &str, source_raw_id: Option<u32>) -> Self {
        Self {
            kind: "concept".to_string(),
            status: "draft".to_string(),
            owner: "maintainer".to_string(),
            schema: "v1".to_string(),
            title: title.to_string(),
            summary: summary.to_string(),
            source_raw_id,
            created_at: now_iso8601(),
        }
    }

    /// Render the frontmatter as a YAML block delimited by `---`
    /// lines, suitable for prepending to the markdown body.
    ///
    /// Hand-written (same reasoning as `RawFrontmatter::to_yaml_block`)
    /// — field set is small, values are controlled, and we keep the
    /// `wiki_store` crate at 2 deps total. The Rust `kind` field is
    /// emitted as `type:` per canonical schema v1.
    #[must_use]
    pub fn to_yaml_block(&self) -> String {
        let mut s = String::from("---\n");
        s.push_str(&format!("type: {}\n", self.kind));
        s.push_str(&format!("status: {}\n", self.status));
        s.push_str(&format!("owner: {}\n", self.owner));
        s.push_str(&format!("schema: {}\n", self.schema));
        s.push_str(&format!("title: {}\n", self.title));
        s.push_str(&format!("summary: {}\n", self.summary));
        if let Some(raw_id) = self.source_raw_id {
            s.push_str(&format!("source_raw_id: {raw_id}\n"));
        }
        s.push_str(&format!("created_at: {}\n", self.created_at));
        s.push_str("---\n");
        s
    }
}

/// On-disk metadata for a single wiki concept page, returned by
/// [`list_wiki_pages`] and [`read_wiki_page`]. Mirrors `RawEntry`'s
/// shape for the raw layer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WikiPageSummary {
    /// Slug (kebab-case ASCII). Primary key and filename stem.
    pub slug: String,
    /// Display title from the frontmatter.
    pub title: String,
    /// Short one-line summary from the frontmatter.
    pub summary: String,
    /// Optional raw/ entry id that seeded this page.
    pub source_raw_id: Option<u32>,
    /// ISO-8601 datetime from the frontmatter.
    pub created_at: String,
    /// File size in bytes (for the listing UI).
    pub byte_size: u64,
}

/// Validate a wiki page slug. Rules match `slugify` output plus the
/// subset of RFC 3986 unreserved chars the maintainer's
/// `parse_proposal()` accepts (ASCII alphanumeric + `-`, `_`, `.`).
///
/// * Non-empty
/// * ≤ 64 chars
/// * ASCII alphanumeric or one of `-`, `_`, `.`
///
/// Used by all three wiki-layer CRUD entry points so bogus slugs
/// from hand-edited API calls never touch the filesystem.
fn validate_wiki_slug(slug: &str) -> Result<()> {
    if slug.is_empty() {
        return Err(WikiStoreError::Invalid("slug is empty".to_string()));
    }
    if slug.len() > 64 {
        return Err(WikiStoreError::Invalid(format!(
            "slug longer than 64 chars: {slug}"
        )));
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(WikiStoreError::Invalid(format!(
            "slug contains invalid chars: {slug}"
        )));
    }
    Ok(())
}

/// Resolve the absolute filesystem path for a wiki concept page
/// given its slug. Pure — does not touch the filesystem, does not
/// validate the slug (callers that hit disk must validate first).
#[must_use]
pub fn wiki_concept_path(paths: &WikiPaths, slug: &str) -> PathBuf {
    paths.wiki.join(WIKI_CONCEPTS_SUBDIR).join(format!("{slug}.md"))
}

/// Write a concept wiki page to `wiki/concepts/{slug}.md`.
///
/// Idempotent by slug: a second call with the same slug
/// OVERWRITES the existing file (canonical §4 blade 3 explicitly
/// allows this — the maintainer is allowed to re-summarise with
/// newer information, and the Inbox approve flow gates every write
/// through a human decision anyway).
///
/// Atomic: writes to a sibling `.tmp` file then renames into place
/// so a crash mid-write can't leave a half-written concept page on
/// disk. Same technique as `save_inbox_file`.
///
/// Errors:
///   * `WikiStoreError::Invalid` if `slug` fails validation
///   * I/O errors (filesystem full, permission denied, ...)
pub fn write_wiki_page(
    paths: &WikiPaths,
    slug: &str,
    title: &str,
    summary: &str,
    body: &str,
    source_raw_id: Option<u32>,
) -> Result<PathBuf> {
    validate_wiki_slug(slug)?;
    let concepts_dir = paths.wiki.join(WIKI_CONCEPTS_SUBDIR);
    fs::create_dir_all(&concepts_dir).map_err(|e| WikiStoreError::io(concepts_dir.clone(), e))?;
    let path = wiki_concept_path(paths, slug);

    // Compose the full file content: YAML frontmatter + blank line + body.
    let frontmatter = WikiFrontmatter::for_concept(title, summary, source_raw_id);
    let mut content = frontmatter.to_yaml_block();
    content.push('\n');
    content.push_str(body);
    if !body.ends_with('\n') {
        content.push('\n');
    }

    // Atomic write: tmp + rename.
    let tmp = path.with_extension("md.tmp");
    fs::write(&tmp, content.as_bytes()).map_err(|e| WikiStoreError::io(tmp.clone(), e))?;
    fs::rename(&tmp, &path).map_err(|e| WikiStoreError::io(path.clone(), e))?;
    Ok(path)
}

/// List every wiki concept page under `wiki/concepts/`, sorted by
/// slug ascending. Missing or empty directory both return an empty
/// `Vec` (consistent with `list_raw_entries`).
///
/// Same "no index cache" philosophy as `list_raw_entries`: we
/// re-parse the frontmatter of every `.md` file on each call. The
/// directory rarely exceeds a few hundred entries during MVP and
/// re-parsing avoids any drift-vs-disk bugs.
pub fn list_wiki_pages(paths: &WikiPaths) -> Result<Vec<WikiPageSummary>> {
    let concepts_dir = paths.wiki.join(WIKI_CONCEPTS_SUBDIR);
    if !concepts_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut summaries: Vec<WikiPageSummary> = Vec::new();
    let dir = fs::read_dir(&concepts_dir)
        .map_err(|e| WikiStoreError::io(concepts_dir.clone(), e))?;
    for entry in dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let slug = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        // Silently skip filenames that don't match the slug contract
        // (e.g. a stray README.md a user drops into the dir). Don't
        // error the whole list — the frontend still wants to show
        // the rest.
        if validate_wiki_slug(&slug).is_err() {
            continue;
        }
        if let Ok(summary) = parse_wiki_file(&path, &slug) {
            summaries.push(summary);
        }
    }
    summaries.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(summaries)
}

/// Read a single wiki concept page by slug. Returns the parsed
/// summary plus the body text (everything after the closing `---`
/// of the frontmatter, leading newline trimmed — same convention
/// as `read_raw_entry`).
///
/// Errors with `WikiStoreError::Invalid` when the file doesn't
/// exist; the `NotFound` variant currently carries only `u32` ids
/// for the raw layer. A future sprint can switch it to an enum
/// carrying either ids or slugs.
pub fn read_wiki_page(paths: &WikiPaths, slug: &str) -> Result<(WikiPageSummary, String)> {
    validate_wiki_slug(slug)?;
    let path = wiki_concept_path(paths, slug);
    if !path.is_file() {
        return Err(WikiStoreError::Invalid(format!(
            "wiki page not found: {slug}"
        )));
    }
    let summary = parse_wiki_file(&path, slug)?;
    let content = fs::read_to_string(&path).map_err(|e| WikiStoreError::io(path.clone(), e))?;
    let body = strip_frontmatter(&content).to_string();
    Ok((summary, body))
}

/// A node in the wiki graph (`build_wiki_graph` output).
/// Carries enough metadata for the frontend to render labels and
/// distinguish raw entries from wiki pages by color/shape.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WikiGraphNode {
    /// Stable identifier within the graph: `raw-{id}` for raw
    /// entries, `wiki-{slug}` for concept pages.
    pub id: String,
    /// Display label — title for raw entries (or filename if title
    /// is missing), title or slug for concept pages.
    pub label: String,
    /// Node category: "raw" or "concept". Drives node coloring on
    /// the frontend (Karpathy three-layer visualization).
    pub kind: String,
}

/// A directed edge in the wiki graph: `from` references `to`.
/// Today the only edge type is `derived-from` (a concept page that
/// was generated from a raw entry via the maintainer flow). Future
/// edge types: `references` (concept-to-concept link in body),
/// `mentions` (named entity overlap), `conflicts-with` (conflict
/// detection from canonical §8 Triggers row 4).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WikiGraphEdge {
    pub from: String,
    pub to: String,
    /// Edge category: "derived-from" for raw->concept,
    /// "references" for concept->concept (future), etc.
    pub kind: String,
}

/// Full wiki graph: nodes + edges + summary counts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WikiGraph {
    pub nodes: Vec<WikiGraphNode>,
    pub edges: Vec<WikiGraphEdge>,
    pub raw_count: usize,
    pub concept_count: usize,
    pub edge_count: usize,
}

/// Build a graph view over the current `~/.clawwiki/raw` and
/// `~/.clawwiki/wiki/concepts` directories. Designed for the
/// frontend Graph page (`GET /api/wiki/graph`) to render a node-
/// edge force layout without doing its own filesystem walks.
///
/// Edge construction (today):
///   * For every concept page with a `source_raw_id` in its
///     frontmatter, emit a `derived-from` edge from
///     `wiki-{slug}` to `raw-{id}`. This is the only edge kind
///     in the MVP — future feat(Q) backlinks pass adds
///     `references` edges between concept pages.
///
/// Empty wiki returns an empty graph (`raw_count = concept_count
/// = edge_count = 0`). Missing wiki dir returns the same empty
/// graph (not an error).
pub fn build_wiki_graph(paths: &WikiPaths) -> Result<WikiGraph> {
    let raws = list_raw_entries(paths).unwrap_or_default();
    let concepts = list_wiki_pages(paths).unwrap_or_default();

    let raw_count = raws.len();
    let concept_count = concepts.len();
    let mut nodes: Vec<WikiGraphNode> =
        Vec::with_capacity(raw_count + concept_count);
    let mut edges: Vec<WikiGraphEdge> = Vec::new();

    // Raw nodes — id is `raw-{id}`, label is slug (with source as
    // a prefix hint for users). RawEntry doesn't carry a separate
    // title field; the slug is the closest human-readable handle
    // and matches what the Raw Library shows.
    for raw in &raws {
        let label = if raw.slug.is_empty() {
            raw.filename.clone()
        } else {
            format!("{}: {}", raw.source, raw.slug)
        };
        nodes.push(WikiGraphNode {
            id: format!("raw-{}", raw.id),
            label,
            kind: "raw".to_string(),
        });
    }

    // Concept nodes — id is `wiki-{slug}`, label is title || slug.
    // Also collect internal links from each body for `references` edges.
    for concept in &concepts {
        let label = if concept.title.is_empty() {
            concept.slug.clone()
        } else {
            concept.title.clone()
        };
        nodes.push(WikiGraphNode {
            id: format!("wiki-{}", concept.slug),
            label,
            kind: "concept".to_string(),
        });

        // Edge: concept → raw via source_raw_id frontmatter.
        if let Some(raw_id) = concept.source_raw_id {
            edges.push(WikiGraphEdge {
                from: format!("wiki-{}", concept.slug),
                to: format!("raw-{raw_id}"),
                kind: "derived-from".to_string(),
            });
        }

        // feat(Q): backlink edges — parse internal markdown links
        // in the body. For each `[...](concepts/target.md)` found,
        // emit a `references` edge from this concept to the target.
        let concept_file =
            paths.wiki.join(WIKI_CONCEPTS_SUBDIR).join(format!("{}.md", concept.slug));
        if let Ok(content) = fs::read_to_string(&concept_file) {
            let body = strip_frontmatter(&content);
            for target_slug in extract_internal_links(body) {
                edges.push(WikiGraphEdge {
                    from: format!("wiki-{}", concept.slug),
                    to: format!("wiki-{target_slug}"),
                    kind: "references".to_string(),
                });
            }
        }
    }

    let edge_count = edges.len();
    Ok(WikiGraph {
        nodes,
        edges,
        raw_count,
        concept_count,
        edge_count,
    })
}

/// Extract internal wiki links from a concept page body. Looks for
/// markdown links of the form `[anything](concepts/{slug}.md)` and
/// returns the unique slugs referenced. Case-insensitive path match.
///
/// This is the parser that feeds the backlinks system: if page A
/// mentions `[LLM Wiki](concepts/llm-wiki.md)` in its body, then
/// `extract_internal_links(body_a)` returns `["llm-wiki"]`, and
/// `build_wiki_graph` emits a `references` edge from A to B.
///
/// The regex is intentionally simple — we only look for
/// `](concepts/slug.md)` suffixes. Future sprints can extend this
/// to detect bare `[[slug]]` wiki-link syntax if we add that.
pub fn extract_internal_links(body: &str) -> Vec<String> {
    let mut slugs: Vec<String> = Vec::new();
    // Look for `](concepts/SLUG.md)` patterns.
    // We search for the closing paren → walk backward to find `](concepts/`
    // prefix. This avoids pulling in the `regex` crate for one function.
    let prefix = "](concepts/";
    let suffix = ".md)";
    let lower = body.to_lowercase();
    let mut search_from = 0usize;
    while let Some(start) = lower[search_from..].find(prefix) {
        let abs_start = search_from + start + prefix.len();
        if let Some(end_rel) = lower[abs_start..].find(suffix) {
            let slug = &body[abs_start..abs_start + end_rel];
            // Validate: slug must be non-empty and ASCII-safe.
            let slug_lower = slug.to_lowercase();
            if !slug_lower.is_empty()
                && slug_lower
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
                && !slugs.contains(&slug_lower)
            {
                slugs.push(slug_lower);
            }
            search_from = abs_start + end_rel + suffix.len();
        } else {
            break;
        }
    }
    slugs
}

/// List all concept pages that link to `target_slug` in their body.
/// This is the "reverse" lookup for backlinks: given slug B, which
/// pages A have a `[...](concepts/B.md)` link?
///
/// Returns a list of `WikiPageSummary` for each referring page.
/// Empty when no page links to `target_slug` (including when the
/// target itself doesn't exist — that's not an error, it just
/// means nothing links to a nonexistent page).
///
/// Performance: re-reads every concept page body on each call.
/// Same "no index cache" philosophy as `list_wiki_pages`. When
/// the concept count outgrows a few hundred, we can add a manifest
/// or SQLite FTS index. For now, the simplicity is worth it.
pub fn list_backlinks(paths: &WikiPaths, target_slug: &str) -> Result<Vec<WikiPageSummary>> {
    let target = target_slug.to_lowercase();
    let concepts_dir = paths.wiki.join(WIKI_CONCEPTS_SUBDIR);
    if !concepts_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut results: Vec<WikiPageSummary> = Vec::new();
    let dir =
        fs::read_dir(&concepts_dir).map_err(|e| WikiStoreError::io(concepts_dir.clone(), e))?;
    for entry in dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let slug = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        // Don't include the target page itself in its own backlinks.
        if slug.to_lowercase() == target {
            continue;
        }
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let body = strip_frontmatter(&content);
        let links = extract_internal_links(body);
        if links.iter().any(|s| *s == target) {
            let metadata = match fs::metadata(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let (title, summary, source_raw_id, created_at) =
                parse_wiki_frontmatter_fields(&content);
            results.push(WikiPageSummary {
                slug,
                title,
                summary,
                source_raw_id,
                created_at,
                byte_size: metadata.len(),
            });
        }
    }
    results.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(results)
}

/// One hit in a [`search_wiki_pages`] result. Carries the matching
/// page's summary, the computed relevance score, and a short text
/// snippet centered on the first body-level match (if any). The
/// frontend uses `snippet` to render the search result card's
/// "excerpt" line without re-fetching the full body.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WikiSearchHit {
    /// The matched page's summary (slug / title / summary / etc.).
    pub page: WikiPageSummary,
    /// Relevance score — higher is better. See `search_wiki_pages`
    /// for the scoring rubric.
    pub score: u32,
    /// Short excerpt around the first body-level hit, capped at
    /// ~200 chars. Empty when the match was only in frontmatter.
    pub snippet: String,
}

/// Search every concept page under `wiki/concepts/` and return the
/// pages that match `query`, sorted by relevance descending.
///
/// ## Scoring
///
/// Matches are case-insensitive substring matches, with fixed
/// weights per field so the most "entity-level" signals outrank
/// body noise:
///
///   * slug match       → +8
///   * title match      → +5
///   * summary match    → +3
///   * body match       → +1 per occurrence, up to +10
///
/// A page with query in its slug and 3 body occurrences scores
/// `8 + 3 = 11`; a page with only body occurrences can cap at 10.
/// This keeps "Karpathy"-slug pages above "mentions Karpathy once"
/// pages without needing full BM25 / embeddings.
///
/// ## Rationale (no tantivy, no embeddings)
///
/// Karpathy's llm-wiki.md §"Optional CLI tools" explicitly says
/// "at small scale the index file is enough". Until we hit the
/// low-hundreds-of-pages regime, a substring match with weighted
/// fields gives us: zero new deps, deterministic results, and
/// sub-millisecond query time on a few hundred pages. When we
/// outgrow it, this function is a clean swap point — same
/// signature, same return type, different implementation.
///
/// Empty query returns an empty result (not all pages).
/// Missing wiki dir returns an empty result (not an error).
pub fn search_wiki_pages(paths: &WikiPaths, query: &str) -> Result<Vec<WikiSearchHit>> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Ok(Vec::new());
    }

    let concepts_dir = paths.wiki.join(WIKI_CONCEPTS_SUBDIR);
    if !concepts_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut hits: Vec<WikiSearchHit> = Vec::new();
    let dir =
        fs::read_dir(&concepts_dir).map_err(|e| WikiStoreError::io(concepts_dir.clone(), e))?;
    for entry in dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let slug = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        if validate_wiki_slug(&slug).is_err() {
            continue;
        }

        // Read the full file once — cheaper than two separate reads
        // via parse_wiki_file + read_wiki_page.
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let metadata = match fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let (title, summary, source_raw_id, created_at) =
            parse_wiki_frontmatter_fields(&content);
        let body = strip_frontmatter(&content);

        // Compute scores per field.
        let slug_lc = slug.to_lowercase();
        let title_lc = title.to_lowercase();
        let summary_lc = summary.to_lowercase();
        let body_lc = body.to_lowercase();

        let mut score: u32 = 0;
        if slug_lc.contains(&q) {
            score += 8;
        }
        if title_lc.contains(&q) {
            score += 5;
        }
        if summary_lc.contains(&q) {
            score += 3;
        }
        // Body: +1 per occurrence, capped at +10.
        let body_occurrences = body_lc.matches(&q).count().min(10) as u32;
        score += body_occurrences;

        if score == 0 {
            continue;
        }

        let snippet = extract_snippet(body, &q);

        hits.push(WikiSearchHit {
            page: WikiPageSummary {
                slug,
                title,
                summary,
                source_raw_id,
                created_at,
                byte_size: metadata.len(),
            },
            score,
            snippet,
        });
    }

    // Sort by score descending, then by slug ascending for stable
    // tie-breaking (so repeated queries return the same order).
    hits.sort_by(|a, b| b.score.cmp(&a.score).then(a.page.slug.cmp(&b.page.slug)));
    Ok(hits)
}

/// Extract a ~200-char snippet centered on the first body occurrence
/// of `query_lc` (expected lowercased). Returns an empty string when
/// the query doesn't appear in the body (frontmatter-only match).
///
/// The snippet is taken from `body` (the original-case body) so the
/// user sees the actual text; we only use the lowercased copy for
/// locating the match position.
fn extract_snippet(body: &str, query_lc: &str) -> String {
    let body_lc = body.to_lowercase();
    let Some(mut pos) = body_lc.find(query_lc) else {
        return String::new();
    };

    // Walk `pos` back to a char boundary in the original (case-sensitive)
    // body. `find` on the lowercased string can land on a position that
    // isn't a valid char boundary in the original (CJK / ligatures /
    // Unicode case folding change byte widths). We fix this by stepping
    // backward until we find a boundary.
    while pos > 0 && !body.is_char_boundary(pos) {
        pos -= 1;
    }

    // Take ~80 chars before the hit and ~120 after for a total of ~200.
    // Walk backward char-by-char to avoid slicing mid-codepoint.
    const BEFORE: usize = 80;
    const AFTER: usize = 120;
    let start = nth_char_boundary_back(body, pos, BEFORE);
    let end = nth_char_boundary_forward(body, pos, AFTER + query_lc.chars().count());
    let raw = &body[start..end];

    // Collapse internal whitespace runs so the snippet is one line
    // and won't break the result card layout.
    let collapsed: String = raw
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    // Prepend / append ellipsis hints when we clipped the body.
    let mut out = String::new();
    if start > 0 {
        out.push_str("… ");
    }
    out.push_str(&collapsed);
    if end < body.len() {
        out.push_str(" …");
    }
    out
}

/// Step backward from `from` in `body` by at most `max_chars`
/// characters, landing on a valid char boundary. Returns 0 if we hit
/// the start.
fn nth_char_boundary_back(body: &str, from: usize, max_chars: usize) -> usize {
    let mut pos = from;
    let mut count = 0usize;
    while pos > 0 && count < max_chars {
        pos -= 1;
        while pos > 0 && !body.is_char_boundary(pos) {
            pos -= 1;
        }
        count += 1;
    }
    pos
}

/// Step forward from `from` in `body` by at most `max_chars`
/// characters, landing on a valid char boundary. Returns `body.len()`
/// if we hit the end.
fn nth_char_boundary_forward(body: &str, from: usize, max_chars: usize) -> usize {
    let mut pos = from;
    let mut count = 0usize;
    let len = body.len();
    while pos < len && count < max_chars {
        pos += 1;
        while pos < len && !body.is_char_boundary(pos) {
            pos += 1;
        }
        count += 1;
    }
    pos
}

/// Parse a single wiki file into a `WikiPageSummary`. The body
/// itself is not returned — use [`read_wiki_page`] for that.
fn parse_wiki_file(path: &Path, slug: &str) -> Result<WikiPageSummary> {
    let content =
        fs::read_to_string(path).map_err(|e| WikiStoreError::io(path.to_path_buf(), e))?;
    let metadata = fs::metadata(path).map_err(|e| WikiStoreError::io(path.to_path_buf(), e))?;
    let (title, summary, source_raw_id, created_at) = parse_wiki_frontmatter_fields(&content);
    Ok(WikiPageSummary {
        slug: slug.to_string(),
        title,
        summary,
        source_raw_id,
        created_at,
        byte_size: metadata.len(),
    })
}

/// Pull `title`, `summary`, `source_raw_id`, `created_at` out of the
/// YAML frontmatter of a wiki page. Tolerant of missing fields —
/// returns empty strings / `None` rather than erroring. Same
/// defensive posture as `parse_frontmatter_fields` for raw entries.
fn parse_wiki_frontmatter_fields(content: &str) -> (String, String, Option<u32>, String) {
    let mut title = String::new();
    let mut summary = String::new();
    let mut source_raw_id: Option<u32> = None;
    let mut created_at = String::new();
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
        if let Some(rest) = line.strip_prefix("title: ") {
            title = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("summary: ") {
            summary = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("source_raw_id: ") {
            source_raw_id = rest.trim().parse().ok();
        } else if let Some(rest) = line.strip_prefix("created_at: ") {
            created_at = rest.to_string();
        }
    }
    (title, summary, source_raw_id, created_at)
}

// ─────────────────────────────────────────────────────────────────────
// Wiki index + log (canonical §8 Triggers · Karpathy §"Indexing and logging")
// ─────────────────────────────────────────────────────────────────────
//
// Karpathy's llm-wiki.md calls out two special files at the top of
// `wiki/` that the maintainer must keep in sync with every write:
//
//   * `wiki/index.md` — content-oriented catalog. Lists every page
//     with title + summary + link. Rebuilt in full after each write
//     (the content is a projection of the filesystem state, so a
//     rebuild is always cheaper than a patch). LLM reads this first
//     at query time and drills into specific pages from there.
//   * `wiki/log.md` — chronological append-only audit log. Canonical
//     §8 Triggers pins the entry prefix to
//     `## [YYYY-MM-DD HH:MM] <verb> | <title>` so simple grep tools
//     can walk the history.
//
// Both files are under `WIKI_WRITE_GUARD` so concurrent writers
// never lose appends. The rebuild is also serialized because the
// list→format→write race could otherwise miss a concurrent append.

/// Absolute path to `wiki/index.md`. Pure — does not touch the
/// filesystem.
#[must_use]
pub fn wiki_index_path(paths: &WikiPaths) -> PathBuf {
    paths.wiki.join(WIKI_INDEX_FILENAME)
}

/// Absolute path to `wiki/log.md`. Pure — does not touch the
/// filesystem.
#[must_use]
pub fn wiki_log_path(paths: &WikiPaths) -> PathBuf {
    paths.wiki.join(WIKI_LOG_FILENAME)
}

/// Append a single line to `wiki/log.md`. Format matches canonical
/// §8 Triggers:
///
/// ```text
/// ## [YYYY-MM-DD HH:MM] <verb> | <title>
/// ```
///
/// `verb` is a short lower-case action like `write-concept`,
/// `resolve-inbox`, `ingest-raw`. `title` is a human-readable
/// one-liner. Both are trimmed before writing.
///
/// Behavior:
///   * Creates `wiki/` and the log file if missing.
///   * Seeds a `# ClawWiki log` header on the very first call.
///   * Writes "atomically" via read-append-rename so a crash mid-
///     write never leaves a half-written line.
///   * Thread-safe via [`WIKI_WRITE_GUARD`].
///
/// Returns the resolved path so callers can echo it.
pub fn append_wiki_log(
    paths: &WikiPaths,
    verb: &str,
    title: &str,
) -> Result<PathBuf> {
    let _guard = lock_wiki_writes();
    fs::create_dir_all(&paths.wiki).map_err(|e| WikiStoreError::io(paths.wiki.clone(), e))?;
    let path = wiki_log_path(paths);

    // Load existing content (empty when the file doesn't exist yet)
    // and prepend the canonical header on first write.
    let existing = if path.is_file() {
        fs::read_to_string(&path).map_err(|e| WikiStoreError::io(path.clone(), e))?
    } else {
        String::new()
    };

    let header = "# ClawWiki log\n\nAppend-only timeline of maintainer and ingest actions. \
                  Entry format: `## [YYYY-MM-DD HH:MM] verb | title`. Canonical §8.\n\n";

    let timestamp = log_timestamp_now();
    let verb_clean = verb.trim();
    let title_clean = title.trim();
    let entry = format!("## [{timestamp}] {verb_clean} | {title_clean}\n");

    let mut new_content = if existing.is_empty() {
        header.to_string()
    } else {
        existing
    };
    if !new_content.ends_with('\n') {
        new_content.push('\n');
    }
    new_content.push_str(&entry);

    // Atomic write: tmp + rename.
    let tmp = path.with_extension("md.tmp");
    fs::write(&tmp, new_content.as_bytes()).map_err(|e| WikiStoreError::io(tmp.clone(), e))?;
    fs::rename(&tmp, &path).map_err(|e| WikiStoreError::io(path.clone(), e))?;
    Ok(path)
}

/// Rebuild `wiki/index.md` from scratch using the current
/// `wiki/concepts/*.md` contents. Overwrites any existing index
/// file. Called by the desktop-server `approve-with-write` handler
/// immediately after `append_wiki_log` so the two stay consistent.
///
/// The generated content is deterministic: for the same set of
/// wiki pages, calling `rebuild_wiki_index` twice in a row produces
/// byte-identical output. This means concurrent rebuilds converge
/// on the same final state regardless of interleaving.
///
/// Thread-safe via [`WIKI_WRITE_GUARD`] — shares the same mutex as
/// `append_wiki_log` so a rebuild can't race against an append that
/// adds a page after `list_wiki_pages` but before the write.
///
/// Layout:
///
/// ```text
/// # ClawWiki index
///
/// Auto-generated catalog ...
///
/// ## Concepts (N)
/// - [{title}](concepts/{slug}.md) - {summary}
/// - ...
/// ```
pub fn rebuild_wiki_index(paths: &WikiPaths) -> Result<PathBuf> {
    let _guard = lock_wiki_writes();
    fs::create_dir_all(&paths.wiki).map_err(|e| WikiStoreError::io(paths.wiki.clone(), e))?;
    let path = wiki_index_path(paths);

    // list_wiki_pages is already sorted alphabetically by slug, so
    // the generated listing is deterministic. We ignore the guard
    // inside list_wiki_pages (it doesn't take one) — we're holding
    // the WIKI_WRITE_GUARD ourselves so no one else can append.
    let concepts = list_wiki_pages(paths)?;

    let mut content = String::new();
    content.push_str("# ClawWiki index\n\n");
    content.push_str(
        "Auto-generated catalog of every wiki page. Updated after every \
         maintainer write. Canonical §10, Karpathy llm-wiki §\"Indexing and logging\".\n\n",
    );
    content.push_str(&format!("## Concepts ({})\n\n", concepts.len()));
    if concepts.is_empty() {
        content.push_str("_No concept pages yet. Ingest something and approve a maintainer proposal from the Inbox._\n");
    } else {
        for page in &concepts {
            let title = if page.title.is_empty() {
                page.slug.clone()
            } else {
                page.title.clone()
            };
            let summary = if page.summary.is_empty() {
                String::new()
            } else {
                format!(" — {}", page.summary)
            };
            content.push_str(&format!(
                "- [{title}]({subdir}/{slug}.md){summary}\n",
                title = title,
                subdir = WIKI_CONCEPTS_SUBDIR,
                slug = page.slug,
                summary = summary,
            ));
        }
    }

    // Atomic write: tmp + rename.
    let tmp = path.with_extension("md.tmp");
    fs::write(&tmp, content.as_bytes()).map_err(|e| WikiStoreError::io(tmp.clone(), e))?;
    fs::rename(&tmp, &path).map_err(|e| WikiStoreError::io(path.clone(), e))?;
    Ok(path)
}

/// Format `now()` as `YYYY-MM-DD HH:MM` for wiki log entries.
/// Distinct from `now_iso8601` because the log format is explicitly
/// space-separated (per canonical §8 Triggers example) so greps
/// look clean in shell output.
fn log_timestamp_now() -> String {
    let (date, hhmm) = current_date_and_hhmm();
    format!("{date} {hhmm}")
}

/// Returns `(date, hhmm)` for the current UTC time, e.g.
/// `("2026-04-10", "14:22")`. Shared by `log_timestamp_now` (which
/// joins them with a space) and `append_changelog_entry` (which uses
/// `date` to pick the destination file and `hhmm` for the entry
/// prefix).
fn current_date_and_hhmm() -> (String, String) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let iso = format_iso8601(secs);
    let (date, rest) = iso.split_once('T').unwrap_or((&iso, ""));
    let hhmm = rest.get(..5).unwrap_or("00:00");
    (date.to_string(), hhmm.to_string())
}

/// Absolute path to `wiki/changelog/{date}.md`. Pure — does not
/// touch the filesystem.
#[must_use]
pub fn changelog_path_for_date(paths: &WikiPaths, date: &str) -> PathBuf {
    paths
        .wiki
        .join(WIKI_CHANGELOG_SUBDIR)
        .join(format!("{date}.md"))
}

/// Append a single line to today's `wiki/changelog/YYYY-MM-DD.md`.
/// Canonical §8 Triggers row 5: every maintainer action also lands
/// in a per-day file so users get a "today's activity" view without
/// scrolling through the all-time `log.md`.
///
/// Format matches `append_wiki_log` but with HH:MM instead of full
/// `YYYY-MM-DD HH:MM` (the date is already in the filename):
///
/// ```text
/// ## [HH:MM] <verb> | <title>
/// ```
///
/// Behavior:
///   * Creates `wiki/changelog/` and the day file if missing.
///   * Seeds a header on first write of the day:
///     `# Changelog YYYY-MM-DD`.
///   * Atomic write via tmp + rename.
///   * Thread-safe via the same `WIKI_WRITE_GUARD` as
///     `append_wiki_log` and `rebuild_wiki_index`.
///
/// Returns the resolved path so callers can echo it.
pub fn append_changelog_entry(
    paths: &WikiPaths,
    verb: &str,
    title: &str,
) -> Result<PathBuf> {
    let _guard = lock_wiki_writes();
    let (date, hhmm) = current_date_and_hhmm();

    let changelog_dir = paths.wiki.join(WIKI_CHANGELOG_SUBDIR);
    fs::create_dir_all(&changelog_dir)
        .map_err(|e| WikiStoreError::io(changelog_dir.clone(), e))?;
    let path = changelog_path_for_date(paths, &date);

    let existing = if path.is_file() {
        fs::read_to_string(&path).map_err(|e| WikiStoreError::io(path.clone(), e))?
    } else {
        String::new()
    };

    let header = format!(
        "# Changelog {date}\n\n\
         Per-day audit trail of maintainer and ingest actions. \
         See `../log.md` for the all-time log. Canonical §8 row 5.\n\n"
    );

    let verb_clean = verb.trim();
    let title_clean = title.trim();
    let entry = format!("## [{hhmm}] {verb_clean} | {title_clean}\n");

    let mut new_content = if existing.is_empty() {
        header
    } else {
        existing
    };
    if !new_content.ends_with('\n') {
        new_content.push('\n');
    }
    new_content.push_str(&entry);

    let tmp = path.with_extension("md.tmp");
    fs::write(&tmp, new_content.as_bytes()).map_err(|e| WikiStoreError::io(tmp.clone(), e))?;
    fs::rename(&tmp, &path).map_err(|e| WikiStoreError::io(path.clone(), e))?;
    Ok(path)
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

/// Kind of maintainer task. Strongly-typed so TypeScript's union type
/// and the Rust variants stay in lockstep: if we add a variant here,
/// the `match` statements below force the TS client to handle it too
/// via a compile-time `unimplemented!` tripwire elsewhere.
///
/// S4 only emits `NewRaw`; the other variants land once
/// `wiki_maintainer` starts producing proposals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InboxKind {
    /// A new raw file landed and needs a concept/page proposal.
    NewRaw,
    /// An existing page conflicts with incoming info.
    Conflict,
    /// An existing page hasn't been verified in the canonical window.
    Stale,
    /// Proposed deprecation of an existing page.
    Deprecate,
}

/// Resolution status of an inbox task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InboxStatus {
    /// Waiting for a user decision.
    Pending,
    /// User approved the proposed action.
    Approved,
    /// User rejected the proposed action.
    Rejected,
}

impl InboxStatus {
    /// Wire tag used by the resolve HTTP handler. Kept as a method
    /// rather than `rename_all` so the `action` query parameter
    /// accepts the two historical strings and the serde tag stays
    /// intact regardless of future variant additions.
    #[must_use]
    pub fn from_action(action: &str) -> Option<Self> {
        match action {
            "approve" => Some(Self::Approved),
            "reject" => Some(Self::Rejected),
            _ => None,
        }
    }
}

/// One line item in the Inbox. Discriminated by `kind`; `source_raw_id`
/// points at the raw layer entry that caused this task (e.g. the
/// `NNNNN_paste_hello_2026-04-09.md` file that just landed).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InboxEntry {
    /// Monotonically-increasing id, scoped to the inbox file. Never
    /// reused even after resolve — we just append.
    pub id: u32,
    /// What kind of maintainer task this is. For S4 always `NewRaw`.
    pub kind: InboxKind,
    /// Current status. Starts as `Pending`, moves to `Approved`
    /// or `Rejected` after the user resolves it.
    pub status: InboxStatus,
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
///
/// Thread-safe: serialized through [`INBOX_WRITE_GUARD`] so concurrent
/// callers from the HTTP `ingest_wiki_raw_handler` side-channel and
/// the WeChat long-poll monitor cannot produce duplicate ids (see
/// the TDD test `inbox_append_is_thread_safe`).
pub fn append_inbox_pending(
    paths: &WikiPaths,
    kind: InboxKind,
    title: &str,
    description: &str,
    source_raw_id: Option<u32>,
) -> Result<InboxEntry> {
    let _guard = lock_inbox_writes();
    let mut entries = load_inbox_file(paths)?;
    let next_id = entries.iter().map(|e| e.id).max().unwrap_or(0) + 1;
    let entry = InboxEntry {
        id: next_id,
        kind,
        status: InboxStatus::Pending,
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
///
/// Thread-safe: shares [`INBOX_WRITE_GUARD`] with `append_inbox_pending`
/// so a resolve can't lose to an append that races the read.
pub fn resolve_inbox_entry(
    paths: &WikiPaths,
    id: u32,
    action: &str,
) -> Result<InboxEntry> {
    let new_status = InboxStatus::from_action(action).ok_or_else(|| {
        WikiStoreError::Invalid(format!("unknown inbox action: {action}"))
    })?;
    let _guard = lock_inbox_writes();
    let mut entries = load_inbox_file(paths)?;
    let found = entries
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or(WikiStoreError::NotFound(id))?;
    found.status = new_status;
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
        .filter(|e| e.status == InboxStatus::Pending)
        .count())
}

/// Convenience wrapper for the two callers that currently append a
/// `NewRaw` inbox task after a raw entry lands (desktop-server's
/// HTTP ingest handler and wechat_ilink's on_message hook). Review
/// nit #15: consolidates the title/description formatting so schema
/// changes (e.g. adding an origin tag) don't need to be made in two
/// call sites in lockstep.
///
/// `origin` is a short, human-readable tag such as `"paste"` or
/// `"WeChat user abcd1234"` that lands inside the description.
pub fn append_new_raw_task(
    paths: &WikiPaths,
    entry: &RawEntry,
    origin: &str,
) -> Result<InboxEntry> {
    let title = format!("New raw entry #{:05}", entry.id);
    let description = format!(
        "Raw entry `{}` was ingested from {origin}. \
         Proposed action: summarise into a concept page.",
        entry.filename
    );
    append_inbox_pending(
        paths,
        InboxKind::NewRaw,
        &title,
        &description,
        Some(entry.id),
    )
}

/// Append a `Conflict` inbox task. Canonical §8 Triggers row 4:
/// when the maintainer LLM detects that a new raw source contradicts
/// or significantly diverges from an existing concept page, it must
/// emit `mark_conflict` (instead of silently rewriting the page),
/// queueing a human-review task in the Inbox.
///
/// MVP: this function is called by future feat(P/Q) update-affected
/// passes once the maintainer can compare two pages. The wiring
/// already exists for the human side: list_inbox_entries returns
/// these entries, the resolve flow accepts them. The detection
/// half is intentionally NOT here — wiki_maintainer is the only
/// component that knows when two semantic units conflict.
///
/// `affected_slugs` are the wiki page slugs that the new source
/// might contradict. They are joined into the description so the
/// frontend (and the user) can see at a glance which pages need
/// review.
pub fn mark_conflict(
    paths: &WikiPaths,
    title: &str,
    affected_slugs: &[String],
    source_raw_id: Option<u32>,
    reason: &str,
) -> Result<InboxEntry> {
    let slugs_str = if affected_slugs.is_empty() {
        "no specific page".to_string()
    } else {
        affected_slugs
            .iter()
            .map(|s| format!("`{s}`"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let description = format!(
        "Conflict detected with {slugs_str}.\n\nReason: {reason}\n\n\
         Proposed action: human review required to pick the canonical \
         version or mark one of the pages as deprecated."
    );
    append_inbox_pending(
        paths,
        InboxKind::Conflict,
        title,
        &description,
        source_raw_id,
    )
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

    // ── S4.2 Wiki layer CRUD tests ───────────────────────────────

    #[test]
    fn write_wiki_page_creates_concepts_dir_and_file() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        // Precondition: wiki/concepts/ does NOT exist after init_wiki
        // (init creates wiki/ but not its subdirs).
        let concepts_dir = paths.wiki.join(WIKI_CONCEPTS_SUBDIR);
        assert!(!concepts_dir.exists(), "concepts/ must not exist pre-write");

        let written = write_wiki_page(
            &paths,
            "llm-wiki",
            "LLM Wiki",
            "Karpathy three-layer cognitive asset architecture.",
            "# LLM Wiki\n\nThree layers: raw/ wiki/ schema/.\n",
            Some(42),
        )
        .unwrap();

        // Directory got created and the file lives at the expected path.
        assert!(concepts_dir.is_dir(), "concepts/ not created by write");
        assert!(written.ends_with("wiki/concepts/llm-wiki.md")
            || written.ends_with("wiki\\concepts\\llm-wiki.md"));
        assert!(written.is_file());

        // File content has the canonical schema v1 shape: type: concept
        // (NOT "kind:"), owner: maintainer, schema: v1, source_raw_id
        // echoed, and the body appended after a blank line.
        let content = fs::read_to_string(&written).unwrap();
        assert!(content.starts_with("---\n"));
        assert!(content.contains("type: concept\n"));
        assert!(content.contains("status: draft\n"));
        assert!(content.contains("owner: maintainer\n"));
        assert!(content.contains("schema: v1\n"));
        assert!(content.contains("title: LLM Wiki\n"));
        assert!(content.contains(
            "summary: Karpathy three-layer cognitive asset architecture.\n"
        ));
        assert!(content.contains("source_raw_id: 42\n"));
        assert!(content.contains("# LLM Wiki"));
        assert!(content.contains("Three layers: raw/ wiki/ schema/."));
    }

    #[test]
    fn write_wiki_page_is_idempotent_by_slug() {
        // Canonical §4 blade 3 allows the maintainer to re-summarise
        // an existing page — the slug is the primary key, not the
        // numeric id. Test: write twice with the same slug, verify
        // the second write replaces (not duplicates) the first.
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let path1 = write_wiki_page(
            &paths,
            "topic-a",
            "First Title",
            "first summary",
            "First body.\n",
            Some(1),
        )
        .unwrap();

        let path2 = write_wiki_page(
            &paths,
            "topic-a",
            "Second Title",
            "second summary",
            "Second body revised.\n",
            Some(2),
        )
        .unwrap();

        // Same slug → same path
        assert_eq!(path1, path2);

        // Second write wins — file contains only the second content.
        let content = fs::read_to_string(&path2).unwrap();
        assert!(content.contains("title: Second Title"));
        assert!(content.contains("Second body revised"));
        assert!(!content.contains("First Title"));
        assert!(!content.contains("First body"));

        // And the list still shows exactly one page.
        let listed = list_wiki_pages(&paths).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].slug, "topic-a");
        assert_eq!(listed[0].title, "Second Title");
        assert_eq!(listed[0].source_raw_id, Some(2));
    }

    #[test]
    fn list_wiki_pages_returns_empty_for_fresh_wiki() {
        // Fresh wiki root, no writes yet. `list_wiki_pages` must
        // return an empty Vec (not error) even though the
        // `wiki/concepts/` subdirectory doesn't exist yet.
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let listed = list_wiki_pages(&paths).unwrap();
        assert!(listed.is_empty(), "fresh wiki must list zero pages");

        // Also: even after we write one page and then list, the sort
        // order is deterministic ascending-by-slug.
        write_wiki_page(&paths, "bravo", "B", "b sum", "body b", None).unwrap();
        write_wiki_page(&paths, "alpha", "A", "a sum", "body a", None).unwrap();
        write_wiki_page(&paths, "charlie", "C", "c sum", "body c", None).unwrap();

        let listed = list_wiki_pages(&paths).unwrap();
        assert_eq!(listed.len(), 3);
        assert_eq!(listed[0].slug, "alpha");
        assert_eq!(listed[1].slug, "bravo");
        assert_eq!(listed[2].slug, "charlie");

        // Decoy files are silently skipped.
        let concepts_dir = paths.wiki.join(WIKI_CONCEPTS_SUBDIR);
        fs::write(concepts_dir.join("not-a-page.txt"), "junk").unwrap();
        fs::write(concepts_dir.join("has space.md"), "still junk").unwrap();
        let listed = list_wiki_pages(&paths).unwrap();
        assert_eq!(listed.len(), 3, "decoys must not appear in listing");
    }

    #[test]
    fn read_wiki_page_roundtrip() {
        // Write → read → verify every field survives the round-trip.
        // This is the core contract between write_wiki_page and the
        // eventual `GET /api/wiki/pages/:slug` handler.
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let body_in =
            "# Engram\n\nOne-pass LLM maintainer.\n\nFires one chat_completion per raw entry.\n";
        write_wiki_page(
            &paths,
            "engram",
            "Engram",
            "Single-pass maintainer flavor.",
            body_in,
            Some(7),
        )
        .unwrap();

        let (summary, body) = read_wiki_page(&paths, "engram").unwrap();
        assert_eq!(summary.slug, "engram");
        assert_eq!(summary.title, "Engram");
        assert_eq!(summary.summary, "Single-pass maintainer flavor.");
        assert_eq!(summary.source_raw_id, Some(7));
        assert!(!summary.created_at.is_empty());
        assert!(summary.byte_size > 0);
        assert_eq!(body, body_in);
        assert!(!body.starts_with("---"));

        // Missing slug → Invalid (not NotFound — see docstring on
        // WikiStoreError::NotFound which is reserved for numeric ids).
        let err = read_wiki_page(&paths, "does-not-exist").unwrap_err();
        assert!(matches!(err, WikiStoreError::Invalid(_)));
    }

    #[test]
    fn write_wiki_page_rejects_invalid_slug() {
        // Slug validation is the last defense against a buggy LLM
        // (or a hostile HTTP caller) trying to path-escape the
        // wiki/concepts/ directory. Pin the contract explicitly.
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        // Empty
        let err = write_wiki_page(&paths, "", "T", "s", "b", None).unwrap_err();
        assert!(matches!(err, WikiStoreError::Invalid(_)));

        // Space
        let err = write_wiki_page(&paths, "has space", "T", "s", "b", None).unwrap_err();
        assert!(matches!(err, WikiStoreError::Invalid(_)));

        // Path traversal
        let err =
            write_wiki_page(&paths, "../escape", "T", "s", "b", None).unwrap_err();
        assert!(matches!(err, WikiStoreError::Invalid(_)));

        // > 64 chars
        let long = "a".repeat(65);
        let err = write_wiki_page(&paths, &long, "T", "s", "b", None).unwrap_err();
        assert!(matches!(err, WikiStoreError::Invalid(_)));

        // Non-ASCII
        let err =
            write_wiki_page(&paths, "中文", "T", "s", "b", None).unwrap_err();
        assert!(matches!(err, WikiStoreError::Invalid(_)));
    }

    #[test]
    fn wiki_frontmatter_emits_canonical_schema_v1() {
        // Pin the YAML shape so a future refactor can't silently
        // swap `type:` back to `kind:`. Canonical §"Frontmatter"
        // requires `type:`.
        let fm = WikiFrontmatter::for_concept(
            "LLM Wiki",
            "three layers",
            Some(3),
        );
        let yaml = fm.to_yaml_block();
        assert!(yaml.starts_with("---\n"));
        assert!(yaml.ends_with("---\n"));
        assert!(yaml.contains("type: concept\n"));
        assert!(!yaml.contains("kind:"), "must emit `type:` not `kind:`");
        assert!(yaml.contains("status: draft\n"));
        assert!(yaml.contains("owner: maintainer\n"));
        assert!(yaml.contains("schema: v1\n"));
        assert!(yaml.contains("title: LLM Wiki\n"));
        assert!(yaml.contains("summary: three layers\n"));
        assert!(yaml.contains("source_raw_id: 3\n"));
        assert!(yaml.contains("created_at: "));
    }

    #[test]
    fn wiki_frontmatter_omits_source_raw_id_when_none() {
        let fm = WikiFrontmatter::for_concept("Title", "Summary", None);
        let yaml = fm.to_yaml_block();
        assert!(!yaml.contains("source_raw_id:"));
    }

    // ── F1 wiki index + log tests ────────────────────────────────

    #[test]
    fn append_wiki_log_creates_file_with_header_on_first_call() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let log_path = wiki_log_path(&paths);
        assert!(!log_path.exists(), "log.md must not exist pre-append");

        let written = append_wiki_log(&paths, "write-concept", "LLM Wiki").unwrap();
        assert_eq!(written, log_path);
        assert!(log_path.is_file());

        let content = fs::read_to_string(&log_path).unwrap();
        assert!(content.starts_with("# ClawWiki log"));
        assert!(content.contains("Append-only timeline"));
        // The log entry must use the canonical §8 prefix.
        assert!(content.contains("] write-concept | LLM Wiki"));
        // Entry line starts with the ## marker for parseability.
        assert!(content.lines().any(|l| l.starts_with("## [")));
    }

    #[test]
    fn append_wiki_log_is_append_only_and_preserves_history() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        append_wiki_log(&paths, "write-concept", "Topic A").unwrap();
        append_wiki_log(&paths, "write-concept", "Topic B").unwrap();
        append_wiki_log(&paths, "resolve-inbox", "task #3 approved").unwrap();

        let content = fs::read_to_string(wiki_log_path(&paths)).unwrap();

        // Exactly one header (the idempotent seed on first write).
        let header_count = content.matches("# ClawWiki log").count();
        assert_eq!(header_count, 1, "header must only be seeded once");

        // All three entries present, in insertion order.
        let entries: Vec<&str> =
            content.lines().filter(|l| l.starts_with("## [")).collect();
        assert_eq!(entries.len(), 3);
        assert!(entries[0].contains("write-concept | Topic A"));
        assert!(entries[1].contains("write-concept | Topic B"));
        assert!(entries[2].contains("resolve-inbox | task #3 approved"));
    }

    #[test]
    fn append_wiki_log_is_thread_safe() {
        // Regression guard: same reasoning as inbox_append_is_thread_safe.
        // Without WIKI_WRITE_GUARD, two concurrent appenders can both
        // load the same content, each append their entry, and the
        // second rename overwrites the first — losing an entry.
        use std::sync::Arc;
        use std::thread;

        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = Arc::new(WikiPaths::resolve(tmp.path()));

        let mut handles = Vec::with_capacity(20);
        for i in 0..20u32 {
            let paths = Arc::clone(&paths);
            handles.push(thread::spawn(move || {
                append_wiki_log(
                    &paths,
                    "write-concept",
                    &format!("concurrent-{i}"),
                )
                .expect("append must succeed under contention")
            }));
        }

        for h in handles {
            h.join().expect("thread panic");
        }

        let content = fs::read_to_string(wiki_log_path(&paths)).unwrap();
        let entries: Vec<&str> =
            content.lines().filter(|l| l.starts_with("## [")).collect();
        assert_eq!(
            entries.len(),
            20,
            "log lost entries under concurrent appends"
        );
        // Every distinct title must survive.
        for i in 0..20u32 {
            let marker = format!("concurrent-{i}");
            assert!(
                entries.iter().any(|e| e.contains(&marker)),
                "entry '{marker}' is missing from log"
            );
        }
    }

    #[test]
    fn rebuild_wiki_index_empty_wiki_yields_header_and_zero_count() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let written = rebuild_wiki_index(&paths).unwrap();
        assert_eq!(written, wiki_index_path(&paths));
        assert!(written.is_file());

        let content = fs::read_to_string(&written).unwrap();
        assert!(content.starts_with("# ClawWiki index"));
        assert!(content.contains("Auto-generated catalog"));
        // Header says "(0)" when the wiki is empty.
        assert!(content.contains("## Concepts (0)"));
        // And an explicit placeholder message so the user sees
        // something informative when they open the raw file.
        assert!(content.contains("_No concept pages yet."));
    }

    #[test]
    fn rebuild_wiki_index_lists_all_concepts_alphabetically() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        // Write 3 pages out-of-order; list_wiki_pages sorts by slug.
        write_wiki_page(
            &paths,
            "bravo",
            "Bravo Topic",
            "Second in order.",
            "Body B.",
            Some(2),
        )
        .unwrap();
        write_wiki_page(
            &paths,
            "alpha",
            "Alpha Topic",
            "First in order.",
            "Body A.",
            Some(1),
        )
        .unwrap();
        write_wiki_page(
            &paths,
            "charlie",
            "Charlie Topic",
            "Third.",
            "Body C.",
            Some(3),
        )
        .unwrap();

        rebuild_wiki_index(&paths).unwrap();
        let content = fs::read_to_string(wiki_index_path(&paths)).unwrap();

        // Count header should reflect the 3 concepts.
        assert!(content.contains("## Concepts (3)"));

        // Each entry must appear as a markdown list item linking
        // into the concepts/ subdir.
        assert!(content.contains("- [Alpha Topic](concepts/alpha.md) — First in order."));
        assert!(content.contains("- [Bravo Topic](concepts/bravo.md) — Second in order."));
        assert!(content.contains("- [Charlie Topic](concepts/charlie.md) — Third."));

        // Alphabetical ordering: alpha appears before bravo appears
        // before charlie in the rendered content.
        let a = content.find("alpha.md").unwrap();
        let b = content.find("bravo.md").unwrap();
        let c = content.find("charlie.md").unwrap();
        assert!(a < b && b < c, "concepts must be listed alphabetically");
    }

    #[test]
    fn rebuild_wiki_index_is_deterministic_and_idempotent() {
        // Two back-to-back rebuilds with the same underlying wiki
        // must produce byte-identical index files. This is what
        // makes concurrent rebuilds race-safe: any interleaving
        // converges on the same final state.
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        write_wiki_page(
            &paths,
            "engram",
            "Engram",
            "One-pass maintainer.",
            "body",
            Some(1),
        )
        .unwrap();

        rebuild_wiki_index(&paths).unwrap();
        let first = fs::read_to_string(wiki_index_path(&paths)).unwrap();
        rebuild_wiki_index(&paths).unwrap();
        let second = fs::read_to_string(wiki_index_path(&paths)).unwrap();

        assert_eq!(first, second, "rebuild_wiki_index must be deterministic");
    }

    #[test]
    fn rebuild_wiki_index_handles_pages_with_missing_title_or_summary() {
        // Defensive: if a concept page somehow has empty title or
        // summary (e.g. a maintainer bug), the index should still
        // render with the slug as fallback title and no summary.
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        write_wiki_page(&paths, "bare-page", "", "", "minimal body", None).unwrap();

        rebuild_wiki_index(&paths).unwrap();
        let content = fs::read_to_string(wiki_index_path(&paths)).unwrap();
        // Falls back to slug as title; no " — summary" suffix.
        assert!(content.contains("- [bare-page](concepts/bare-page.md)\n"));
        assert!(!content.contains("bare-page.md — "));
    }

    // ── Q backlinks tests ────────────────────────────────────────

    #[test]
    fn extract_internal_links_finds_concept_links() {
        let body = "See [LLM Wiki](concepts/llm-wiki.md) and \
                    [RAG](concepts/rag-vs-llm-wiki.md) for details.";
        let links = extract_internal_links(body);
        assert_eq!(links.len(), 2);
        assert!(links.contains(&"llm-wiki".to_string()));
        assert!(links.contains(&"rag-vs-llm-wiki".to_string()));
    }

    #[test]
    fn extract_internal_links_ignores_external_links() {
        let body = "See [Example](https://example.com) and [Docs](docs/readme.md).";
        let links = extract_internal_links(body);
        assert!(links.is_empty());
    }

    #[test]
    fn extract_internal_links_deduplicates() {
        let body = "[A](concepts/alpha.md) and [A again](concepts/alpha.md).";
        let links = extract_internal_links(body);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0], "alpha");
    }

    #[test]
    fn extract_internal_links_case_insensitive() {
        let body = "[Cap](Concepts/Alpha.md) and [low](concepts/alpha.md).";
        let links = extract_internal_links(body);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0], "alpha");
    }

    #[test]
    fn list_backlinks_returns_referring_pages() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        write_wiki_page(
            &paths,
            "alpha",
            "Alpha",
            "Summary",
            "See [Bravo](concepts/bravo.md) for more.",
            Some(1),
        )
        .unwrap();
        write_wiki_page(
            &paths,
            "bravo",
            "Bravo",
            "Summary",
            "See [Alpha](concepts/alpha.md) for context.",
            Some(2),
        )
        .unwrap();
        write_wiki_page(
            &paths,
            "charlie",
            "Charlie",
            "Summary",
            "No internal links here.",
            Some(3),
        )
        .unwrap();

        let bl_for_bravo = list_backlinks(&paths, "bravo").unwrap();
        assert_eq!(bl_for_bravo.len(), 1);
        assert_eq!(bl_for_bravo[0].slug, "alpha");

        let bl_for_alpha = list_backlinks(&paths, "alpha").unwrap();
        assert_eq!(bl_for_alpha.len(), 1);
        assert_eq!(bl_for_alpha[0].slug, "bravo");

        let bl_for_charlie = list_backlinks(&paths, "charlie").unwrap();
        assert!(bl_for_charlie.is_empty());
    }

    #[test]
    fn list_backlinks_excludes_self_references() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        write_wiki_page(
            &paths,
            "self-ref",
            "Self Ref",
            "Summary",
            "I link to [myself](concepts/self-ref.md).",
            None,
        )
        .unwrap();

        let bl = list_backlinks(&paths, "self-ref").unwrap();
        assert!(bl.is_empty());
    }

    #[test]
    fn build_wiki_graph_includes_references_edges_from_backlinks() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        write_wiki_page(
            &paths,
            "page-a",
            "Page A",
            "summary",
            "Links to [Page B](concepts/page-b.md).",
            Some(1),
        )
        .unwrap();
        write_wiki_page(&paths, "page-b", "Page B", "summary", "No links.", Some(2))
            .unwrap();

        // Need at least 1 raw entry for the derived-from edges
        write_raw_entry(
            &paths,
            "paste",
            "First",
            "body",
            &RawFrontmatter::for_paste("paste", None),
        )
        .unwrap();
        write_raw_entry(
            &paths,
            "paste",
            "Second",
            "body",
            &RawFrontmatter::for_paste("paste", None),
        )
        .unwrap();

        let g = build_wiki_graph(&paths).unwrap();

        // Should have a references edge from page-a to page-b
        assert!(
            g.edges.iter().any(|e| e.from == "wiki-page-a"
                && e.to == "wiki-page-b"
                && e.kind == "references"),
            "missing references edge: {:?}",
            g.edges
        );

        // No reverse references edge (page-b doesn't link to page-a)
        assert!(!g
            .edges
            .iter()
            .any(|e| e.from == "wiki-page-b" && e.to == "wiki-page-a" && e.kind == "references"));
    }

    // ── R mark_conflict tests ────────────────────────────────────

    #[test]
    fn mark_conflict_creates_conflict_inbox_entry() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let affected = vec!["agentic-loop".to_string(), "rag-vs-llm-wiki".to_string()];
        let entry = mark_conflict(
            &paths,
            "Conflict: Agentic Loop v1 vs v2",
            &affected,
            Some(42),
            "v2 changes the loop termination semantics",
        )
        .unwrap();

        assert_eq!(entry.kind, InboxKind::Conflict);
        assert_eq!(entry.status, InboxStatus::Pending);
        assert_eq!(entry.title, "Conflict: Agentic Loop v1 vs v2");
        assert_eq!(entry.source_raw_id, Some(42));
        assert!(entry.description.contains("agentic-loop"));
        assert!(entry.description.contains("rag-vs-llm-wiki"));
        assert!(entry.description.contains("v2 changes the loop termination"));
    }

    #[test]
    fn mark_conflict_handles_empty_affected_slugs() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let entry = mark_conflict(
            &paths,
            "Generic conflict",
            &[],
            None,
            "ambient signal mismatch",
        )
        .unwrap();
        assert_eq!(entry.kind, InboxKind::Conflict);
        assert!(entry.description.contains("no specific page"));
    }

    #[test]
    fn mark_conflict_co_exists_with_new_raw_in_inbox() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        // Append one of each kind.
        let raw = write_raw_entry(
            &paths,
            "paste",
            "First",
            "body",
            &RawFrontmatter::for_paste("paste", None),
        )
        .unwrap();
        append_new_raw_task(&paths, &raw, "paste").unwrap();
        mark_conflict(&paths, "Conflict A", &["alpha".to_string()], Some(raw.id), "x")
            .unwrap();

        let all = list_inbox_entries(&paths).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].kind, InboxKind::NewRaw);
        assert_eq!(all[1].kind, InboxKind::Conflict);
        // IDs are sequential — both share the inbox monotonic counter.
        assert_eq!(all[0].id + 1, all[1].id);
    }

    // ── T wiki graph tests ───────────────────────────────────────

    #[test]
    fn build_wiki_graph_empty_wiki_returns_zero_counts() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let g = build_wiki_graph(&paths).unwrap();
        assert_eq!(g.raw_count, 0);
        assert_eq!(g.concept_count, 0);
        assert_eq!(g.edge_count, 0);
        assert!(g.nodes.is_empty());
        assert!(g.edges.is_empty());
    }

    #[test]
    fn build_wiki_graph_emits_raw_and_concept_nodes() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let raw = write_raw_entry(
            &paths,
            "paste",
            "Karpathy LLM Wiki",
            "Source body.",
            &RawFrontmatter::for_paste("paste", None),
        )
        .unwrap();
        write_wiki_page(
            &paths,
            "llm-wiki",
            "LLM Wiki",
            "Summary.",
            "Body.",
            Some(raw.id),
        )
        .unwrap();

        let g = build_wiki_graph(&paths).unwrap();
        assert_eq!(g.raw_count, 1);
        assert_eq!(g.concept_count, 1);
        assert_eq!(g.nodes.len(), 2);

        // Find raw node
        let raw_node = g
            .nodes
            .iter()
            .find(|n| n.kind == "raw")
            .expect("raw node");
        assert_eq!(raw_node.id, format!("raw-{}", raw.id));
        // Label is "{source}: {slug}". Slug is slugified,
        // lowercased — "karpathy-llm-wiki" or similar.
        assert!(raw_node.label.starts_with("paste:"));
        assert!(raw_node.label.to_lowercase().contains("karpathy"));

        // Find concept node
        let concept_node = g
            .nodes
            .iter()
            .find(|n| n.kind == "concept")
            .expect("concept node");
        assert_eq!(concept_node.id, "wiki-llm-wiki");
        assert_eq!(concept_node.label, "LLM Wiki");
    }

    #[test]
    fn build_wiki_graph_emits_derived_from_edges() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let raw1 = write_raw_entry(
            &paths,
            "paste",
            "Source A",
            "body",
            &RawFrontmatter::for_paste("paste", None),
        )
        .unwrap();
        let raw2 = write_raw_entry(
            &paths,
            "paste",
            "Source B",
            "body",
            &RawFrontmatter::for_paste("paste", None),
        )
        .unwrap();
        write_wiki_page(&paths, "alpha", "Alpha", "summary", "body", Some(raw1.id))
            .unwrap();
        write_wiki_page(&paths, "bravo", "Bravo", "summary", "body", Some(raw2.id))
            .unwrap();
        // One concept WITHOUT source_raw_id — should still get a node
        // but no derived-from edge.
        write_wiki_page(&paths, "orphan", "Orphan", "summary", "body", None).unwrap();

        let g = build_wiki_graph(&paths).unwrap();
        assert_eq!(g.raw_count, 2);
        assert_eq!(g.concept_count, 3);
        assert_eq!(g.edge_count, 2);

        // Edge for alpha → raw1
        assert!(g.edges.iter().any(|e| {
            e.from == "wiki-alpha"
                && e.to == format!("raw-{}", raw1.id)
                && e.kind == "derived-from"
        }));
        // Edge for bravo → raw2
        assert!(g.edges.iter().any(|e| {
            e.from == "wiki-bravo"
                && e.to == format!("raw-{}", raw2.id)
                && e.kind == "derived-from"
        }));
        // No edge for orphan
        assert!(!g.edges.iter().any(|e| e.from == "wiki-orphan"));
    }

    // ── M schema overwrite tests ─────────────────────────────────

    #[test]
    fn overwrite_schema_claude_md_replaces_seed() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        // After init_wiki, the seed template is on disk.
        let seed = fs::read_to_string(&paths.schema_claude_md).unwrap();
        assert!(!seed.is_empty());

        let new_content = "# CLAUDE.md\n\n## Role\n\nYou are a custom maintainer.\n";
        overwrite_schema_claude_md(&paths, new_content).unwrap();

        let actual = fs::read_to_string(&paths.schema_claude_md).unwrap();
        assert_eq!(actual, new_content);
    }

    #[test]
    fn overwrite_schema_claude_md_rejects_empty_content() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let err = overwrite_schema_claude_md(&paths, "").unwrap_err();
        assert!(matches!(err, WikiStoreError::Invalid(_)));

        let err = overwrite_schema_claude_md(&paths, "   \n  ").unwrap_err();
        assert!(matches!(err, WikiStoreError::Invalid(_)));
    }

    #[test]
    fn overwrite_schema_claude_md_is_idempotent() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let content = "# Custom\n\nbody\n";
        overwrite_schema_claude_md(&paths, content).unwrap();
        let first = fs::read_to_string(&paths.schema_claude_md).unwrap();
        overwrite_schema_claude_md(&paths, content).unwrap();
        let second = fs::read_to_string(&paths.schema_claude_md).unwrap();
        assert_eq!(first, second);
    }

    // ── S per-day changelog tests ────────────────────────────────

    #[test]
    fn append_changelog_entry_creates_dated_file_with_header() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let path = append_changelog_entry(&paths, "write-concept", "First entry").unwrap();
        assert!(path.is_file());
        assert!(path.starts_with(tmp.path().join(WIKI_DIR).join(WIKI_CHANGELOG_SUBDIR)));
        // Filename matches YYYY-MM-DD.md (10 + 3 chars stem)
        let stem = path.file_stem().unwrap().to_str().unwrap();
        assert_eq!(stem.len(), 10);
        assert_eq!(stem.chars().nth(4), Some('-'));
        assert_eq!(stem.chars().nth(7), Some('-'));

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("# Changelog "));
        assert!(content.contains("Per-day audit trail"));
        assert!(content.contains("] write-concept | First entry"));
    }

    #[test]
    fn append_changelog_entry_appends_to_existing_day_file() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        append_changelog_entry(&paths, "write-concept", "First").unwrap();
        append_changelog_entry(&paths, "write-concept", "Second").unwrap();
        append_changelog_entry(&paths, "resolve-inbox", "Third").unwrap();

        // All three entries should land in the same file (today's
        // date). Read whatever file exists in changelog/.
        let dir = tmp.path().join(WIKI_DIR).join(WIKI_CHANGELOG_SUBDIR);
        let files: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert_eq!(files.len(), 1, "expected single day file, got {}", files.len());

        let content = fs::read_to_string(files[0].path()).unwrap();
        // Header only seeded once.
        assert_eq!(content.matches("# Changelog ").count(), 1);
        // All three entries present.
        assert!(content.contains("write-concept | First"));
        assert!(content.contains("write-concept | Second"));
        assert!(content.contains("resolve-inbox | Third"));
    }

    #[test]
    fn append_changelog_entry_uses_short_hhmm_prefix() {
        // The day file uses HH:MM (no date) since the date is in
        // the filename. This is different from log.md which uses
        // YYYY-MM-DD HH:MM.
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let path = append_changelog_entry(&paths, "write-concept", "Test").unwrap();
        let content = fs::read_to_string(&path).unwrap();
        let entry_line = content.lines().find(|l| l.starts_with("## [")).unwrap();
        // Format: "## [HH:MM] verb | title"
        // Slice between [ and ] should be exactly 5 chars (HH:MM).
        let between = &entry_line[entry_line.find('[').unwrap() + 1
            ..entry_line.find(']').unwrap()];
        assert_eq!(between.len(), 5);
        assert_eq!(between.chars().nth(2), Some(':'));
    }

    #[test]
    fn append_changelog_entry_thread_safe() {
        use std::sync::Arc;
        use std::thread;

        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = Arc::new(WikiPaths::resolve(tmp.path()));

        let mut handles = Vec::with_capacity(15);
        for i in 0..15u32 {
            let paths = Arc::clone(&paths);
            handles.push(thread::spawn(move || {
                append_changelog_entry(&paths, "write-concept", &format!("entry-{i}"))
                    .expect("append must succeed under contention")
            }));
        }
        for h in handles {
            h.join().expect("thread panic");
        }

        // All 15 entries must be in the file (no lost writes).
        let dir = tmp.path().join(WIKI_DIR).join(WIKI_CHANGELOG_SUBDIR);
        let files: Vec<_> = fs::read_dir(&dir).unwrap().filter_map(|r| r.ok()).collect();
        assert_eq!(files.len(), 1);
        let content = fs::read_to_string(files[0].path()).unwrap();
        let entries: Vec<&str> = content.lines().filter(|l| l.starts_with("## [")).collect();
        assert_eq!(entries.len(), 15);
        for i in 0..15u32 {
            assert!(
                content.contains(&format!("entry-{i}")),
                "lost entry-{i}"
            );
        }
    }

    // ── G search_wiki_pages tests ────────────────────────────────

    fn seed_three_wiki_pages(paths: &WikiPaths) {
        write_wiki_page(
            paths,
            "llm-wiki",
            "LLM Wiki",
            "Karpathy three-layer cognitive asset architecture.",
            "# LLM Wiki\n\nA persistent wiki maintained by an LLM agent.\n\n\
             Karpathy describes three layers: raw sources, the wiki, and the schema.\n",
            Some(1),
        )
        .unwrap();
        write_wiki_page(
            paths,
            "rag-vs-llm-wiki",
            "RAG vs LLM Wiki",
            "Compare retrieval-augmented generation to persistent LLM wiki.",
            "# RAG vs LLM Wiki\n\nRAG retrieves raw chunks per query. \
             An LLM wiki compiles knowledge once and keeps it current.\n",
            Some(2),
        )
        .unwrap();
        write_wiki_page(
            paths,
            "engram",
            "Engram",
            "One-pass maintainer flavor.",
            "# Engram\n\nSingle LLM call per ingest. \
             Cheap and fast. No multi-pass compilation.\n",
            Some(3),
        )
        .unwrap();
    }

    #[test]
    fn search_wiki_pages_empty_query_returns_empty() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        seed_three_wiki_pages(&paths);

        assert!(search_wiki_pages(&paths, "").unwrap().is_empty());
        assert!(search_wiki_pages(&paths, "   ").unwrap().is_empty());
    }

    #[test]
    fn search_wiki_pages_slug_match_outranks_body_only_match() {
        // Design: two pages that both contain "engram" in their body
        // once (score +1 each), but only one has "engram" in its
        // slug (score +8). The slug-match page must sort first.
        // This isolates the slug weight from title/summary/body
        // weights so the test doesn't depend on lexicographic
        // content overlap.
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        // Page 1: slug contains "engram" + body mentions it once
        write_wiki_page(
            &paths,
            "engram-style",
            "Engram Style",
            "Single-pass maintainer pattern.",
            "A short body that mentions the concept once here.",
            Some(1),
        )
        .unwrap();

        // Page 2: slug doesn't contain "engram",only body does
        write_wiki_page(
            &paths,
            "maintainer-patterns",
            "Maintainer Patterns",
            "Overview of maintainer flavors.",
            "This page compares different flavors: engram, sage-wiki, and others.",
            Some(2),
        )
        .unwrap();

        let hits = search_wiki_pages(&paths, "engram").unwrap();
        assert_eq!(hits.len(), 2, "both pages should match");

        // Expected scores:
        //   engram-style       : slug +8 + title +5 + summary 0 + body 0 = 13
        //     (body has "concept once here", no "engram" — 0)
        //     actually wait: title "Engram Style" contains "engram" -> +5
        //     summary "Single-pass maintainer pattern" -> no +0
        //     body "...mentions the concept once here" -> no +0
        //     total: 8 + 5 = 13
        //   maintainer-patterns: slug 0 + title 0 + summary 0 + body 1 (one "engram" in body)
        //     total: 1
        assert_eq!(hits[0].page.slug, "engram-style");
        assert_eq!(hits[1].page.slug, "maintainer-patterns");
        assert!(
            hits[0].score > hits[1].score,
            "slug-match score {} must exceed body-only score {}",
            hits[0].score,
            hits[1].score
        );
        // Explicit values so a future refactor of the weights is
        // caught loudly.
        assert_eq!(hits[0].score, 13); // 8 slug + 5 title
        assert_eq!(hits[1].score, 1); // 1 body
    }

    #[test]
    fn search_wiki_pages_case_insensitive() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        seed_three_wiki_pages(&paths);

        let a = search_wiki_pages(&paths, "KARPATHY").unwrap();
        let b = search_wiki_pages(&paths, "karpathy").unwrap();
        let c = search_wiki_pages(&paths, "Karpathy").unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(a.len(), b.len());
        assert_eq!(a.len(), c.len());
        // All should point at the same page.
        assert_eq!(a[0].page.slug, b[0].page.slug);
        assert_eq!(a[0].page.slug, c[0].page.slug);
        assert_eq!(a[0].page.slug, "llm-wiki");
    }

    #[test]
    fn search_wiki_pages_snippet_contains_query_with_context() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        seed_three_wiki_pages(&paths);

        let hits = search_wiki_pages(&paths, "retrieves").unwrap();
        assert_eq!(hits.len(), 1);
        let snippet = &hits[0].snippet;
        assert!(
            snippet.to_lowercase().contains("retrieves"),
            "snippet missing query: {snippet}"
        );
        // Snippet should contain some context, not just the query alone.
        assert!(snippet.len() > "retrieves".len());
    }

    #[test]
    fn search_wiki_pages_zero_matches_returns_empty() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        seed_three_wiki_pages(&paths);

        let hits = search_wiki_pages(&paths, "nonexistent-term-xyz-123").unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn search_wiki_pages_stable_tiebreak_by_slug() {
        // When two pages have the same score, they should always
        // appear in the same order across calls (sorted by slug
        // ascending). This is what lets the UI avoid flicker on
        // repeated queries.
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        // Two pages with the query only in their body (+1 each).
        write_wiki_page(
            &paths,
            "zulu",
            "Zulu",
            "zulu desc",
            "body mentions flavor once.",
            None,
        )
        .unwrap();
        write_wiki_page(
            &paths,
            "alpha",
            "Alpha",
            "alpha desc",
            "body mentions flavor once.",
            None,
        )
        .unwrap();

        for _ in 0..3 {
            let hits = search_wiki_pages(&paths, "flavor").unwrap();
            assert_eq!(hits.len(), 2);
            // Both score 1; alpha sorts before zulu.
            assert_eq!(hits[0].page.slug, "alpha");
            assert_eq!(hits[1].page.slug, "zulu");
        }
    }

    #[test]
    fn search_wiki_pages_missing_wiki_dir_returns_empty() {
        let tmp = tempdir().unwrap();
        // Intentionally do NOT call init_wiki — wiki/concepts/ missing.
        let paths = WikiPaths::resolve(tmp.path());
        let hits = search_wiki_pages(&paths, "anything").unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn search_wiki_pages_handles_cjk_query() {
        // Defensive: CJK characters are multi-byte; the snippet
        // extractor's `is_char_boundary` walk must not panic.
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        write_wiki_page(
            &paths,
            "test",
            "测试页面",
            "一个测试页面",
            "这是正文内容,包含中文字符。",
            None,
        )
        .unwrap();

        let hits = search_wiki_pages(&paths, "中文").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].page.slug, "test");
        // Snippet should include "中文" without panicking.
        assert!(hits[0].snippet.contains("中文"));
    }

    // ── F3 git init + .gitignore tests ───────────────────────────

    #[test]
    fn init_wiki_seeds_gitignore_with_canonical_entries() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let gitignore = tmp.path().join(".gitignore");
        assert!(gitignore.is_file(), ".gitignore not seeded");

        let content = fs::read_to_string(&gitignore).unwrap();
        // Atomic-write sidecars
        assert!(content.contains("*.tmp"), "*.tmp missing from .gitignore");
        // Encrypted Codex pool (never commit)
        assert!(
            content.contains(".clawwiki/cloud-accounts.enc"),
            "cloud-accounts.enc not excluded"
        );
        // Ephemeral SQLite caches
        assert!(content.contains(".clawwiki/*.db"));
    }

    #[test]
    fn init_wiki_preserves_user_edited_gitignore() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let gitignore = tmp.path().join(".gitignore");

        let custom = "# my custom rules\nnode_modules/\n";
        fs::write(&gitignore, custom).unwrap();

        // Re-running init_wiki must not clobber the user's edits.
        init_wiki(tmp.path()).unwrap();
        let after = fs::read_to_string(&gitignore).unwrap();
        assert_eq!(after, custom, ".gitignore was overwritten");
    }

    #[test]
    fn init_wiki_git_init_soft_fails_gracefully() {
        // Whether git is on PATH or not, init_wiki must always
        // return Ok and leave the filesystem in a usable state.
        // This test asserts the "no panic, no error propagation"
        // contract regardless of the host environment.
        let tmp = tempdir().unwrap();
        let result = init_wiki(tmp.path());
        assert!(result.is_ok(), "init_wiki must soft-fail git errors");

        // The wiki dirs + schema + gitignore are always created
        // regardless of git availability.
        assert!(tmp.path().join(RAW_DIR).is_dir());
        assert!(tmp.path().join(WIKI_DIR).is_dir());
        assert!(tmp.path().join(SCHEMA_DIR).is_dir());
        assert!(tmp.path().join(".gitignore").is_file());

        // If git is available on this machine, `.git/` will be
        // created. If not, it won't. Both are acceptable — the
        // function must not crash either way.
        let git_dir = tmp.path().join(".git");
        let git_probe = std::process::Command::new("git")
            .arg("--version")
            .output();
        let git_available = matches!(git_probe, Ok(o) if o.status.success());
        if git_available {
            assert!(
                git_dir.is_dir(),
                "git is on PATH but .git/ was not created"
            );
        }
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
            append_inbox_pending(&paths, InboxKind::NewRaw, "first", "desc", Some(1)).unwrap();
        let e2 =
            append_inbox_pending(&paths, InboxKind::NewRaw, "second", "desc", Some(2)).unwrap();
        let e3 =
            append_inbox_pending(&paths, InboxKind::NewRaw, "third", "desc", None).unwrap();

        assert_eq!(e1.id, 1);
        assert_eq!(e2.id, 2);
        assert_eq!(e3.id, 3);
        assert_eq!(e1.status, InboxStatus::Pending);
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
            append_inbox_pending(&paths, InboxKind::NewRaw, "test", "desc", Some(1)).unwrap();
        let resolved = resolve_inbox_entry(&paths, entry.id, "approve").unwrap();

        assert_eq!(resolved.id, entry.id);
        assert_eq!(resolved.status, InboxStatus::Approved);
        assert!(resolved.resolved_at.is_some());

        // Re-reading from disk must reflect the change.
        let listed = list_inbox_entries(&paths).unwrap();
        assert_eq!(listed[0].status, InboxStatus::Approved);
        assert_eq!(count_pending_inbox(&paths).unwrap(), 0);
    }

    #[test]
    fn inbox_resolve_reject_marks_rejected() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        append_inbox_pending(&paths, InboxKind::NewRaw, "keep", "", Some(1)).unwrap();
        let bad = append_inbox_pending(&paths, InboxKind::NewRaw, "drop", "", Some(2)).unwrap();

        resolve_inbox_entry(&paths, bad.id, "reject").unwrap();

        let entries = list_inbox_entries(&paths).unwrap();
        assert_eq!(entries[0].status, InboxStatus::Pending);
        assert_eq!(entries[1].status, InboxStatus::Rejected);
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
        let e = append_inbox_pending(&paths, InboxKind::NewRaw, "t", "", None).unwrap();
        let err = resolve_inbox_entry(&paths, e.id, "banana").unwrap_err();
        assert!(matches!(err, WikiStoreError::Invalid(_)));
    }

    #[test]
    fn inbox_persists_across_calls() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        append_inbox_pending(&paths, InboxKind::NewRaw, "task-a", "", Some(1)).unwrap();
        append_inbox_pending(&paths, InboxKind::NewRaw, "task-b", "", Some(2)).unwrap();

        // Simulate process restart by re-resolving paths and reading.
        let paths_fresh = WikiPaths::resolve(tmp.path());
        let listed = list_inbox_entries(&paths_fresh).unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].title, "task-a");
        assert_eq!(listed[1].title, "task-b");
    }

    #[test]
    fn inbox_append_is_thread_safe() {
        // Regression guard for S4/S5/S6 review finding #2 (TOCTOU race).
        //
        // Before the `INBOX_WRITE_GUARD` fix, firing two concurrent
        // `append_inbox_pending` calls could produce duplicate ids:
        // both would `load_inbox_file` seeing max=N, both compute
        // next_id=N+1, both write — last one wins, id N+1 shows up
        // only once despite two distinct appends. This test fires
        // 20 parallel threads at a single inbox and asserts every
        // entry survives with a unique id.
        use std::sync::Arc;
        use std::thread;

        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = Arc::new(WikiPaths::resolve(tmp.path()));

        let mut handles = Vec::with_capacity(20);
        for i in 0..20u32 {
            let paths = Arc::clone(&paths);
            handles.push(thread::spawn(move || {
                append_inbox_pending(
                    &paths,
                    InboxKind::NewRaw,
                    &format!("concurrent-{i}"),
                    "body",
                    Some(i),
                )
                .expect("append must succeed under contention")
            }));
        }

        let results: Vec<InboxEntry> = handles
            .into_iter()
            .map(|h| h.join().expect("thread panic"))
            .collect();

        // Every returned id must be unique.
        let mut ids: Vec<u32> = results.iter().map(|e| e.id).collect();
        ids.sort_unstable();
        assert_eq!(
            ids,
            (1..=20).collect::<Vec<_>>(),
            "concurrent appends produced non-unique or missing ids"
        );

        // The file on disk must reflect all 20 entries, none lost.
        let listed = list_inbox_entries(&paths).unwrap();
        assert_eq!(listed.len(), 20, "inbox.json lost entries under contention");

        // And every source_raw_id must survive — this catches the bug
        // where two threads write distinct entries to the same id slot
        // and only one persists.
        let mut raw_ids: Vec<u32> = listed
            .iter()
            .filter_map(|e| e.source_raw_id)
            .collect();
        raw_ids.sort_unstable();
        assert_eq!(raw_ids, (0..20).collect::<Vec<_>>());
    }

    #[test]
    fn inbox_resolve_under_contention_keeps_all_entries() {
        // Companion to the append-concurrency test: verify that
        // parallel resolves don't clobber each other either.
        use std::sync::Arc;
        use std::thread;

        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = Arc::new(WikiPaths::resolve(tmp.path()));

        // Seed 10 pending entries sequentially (guarantees ids 1..=10).
        for i in 0..10u32 {
            append_inbox_pending(
                &paths,
                InboxKind::NewRaw,
                &format!("seed-{i}"),
                "body",
                Some(i),
            )
            .unwrap();
        }

        // Resolve all 10 in parallel, alternating approve/reject.
        let mut handles = Vec::new();
        for id in 1..=10u32 {
            let paths = Arc::clone(&paths);
            let action = if id % 2 == 0 { "approve" } else { "reject" };
            handles.push(thread::spawn(move || {
                resolve_inbox_entry(&paths, id, action)
                    .expect("resolve must succeed under contention")
            }));
        }

        for h in handles {
            h.join().expect("resolve thread panic");
        }

        // Every entry survives, every one reached a terminal state.
        let listed = list_inbox_entries(&paths).unwrap();
        assert_eq!(listed.len(), 10, "no entries may be lost to resolves");
        for entry in &listed {
            assert!(
                entry.status == InboxStatus::Approved || entry.status == InboxStatus::Rejected,
                "entry {} still pending after parallel resolve",
                entry.id
            );
        }
        assert_eq!(count_pending_inbox(&paths).unwrap(), 0);
    }
}

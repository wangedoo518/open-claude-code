//! `wiki_store` — on-disk layout and lifecycle for `~/.clawwiki/`.
//!
//! The current desktop-shell product defines a three-layer file system
//! rooted at `~/.clawwiki/`:
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

use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use serde::{Deserialize, Serialize};

// P1 End-to-End Provenance + Lineage Explorer.
//
// New child module that owns `.clawwiki/lineage.jsonl` — the append-only
// event log every P1 write point fires into. Exposed as `pub mod` rather
// than `pub use` of individual symbols so the caller site stays explicit
// (`wiki_store::provenance::fire_event`) and the struct / enum contract
// for LineageEvent / LineageRef lives in one discoverable namespace.
pub mod provenance;

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

/// Process-global guard that serializes `write_raw_entry` calls.
/// `next_raw_id()` reads the directory to compute `max(id) + 1`.
/// Without this guard, two concurrent writers can both observe the
/// same max id, producing duplicate filenames (TOCTOU race). The
/// guard covers the entire read-id → write-file critical section.
///
/// Poison recovery: same rationale as `INBOX_WRITE_GUARD` — the
/// on-disk state is always well-defined after a panic.
static RAW_WRITE_GUARD: OnceLock<Mutex<()>> = OnceLock::new();

fn lock_raw_writes() -> MutexGuard<'static, ()> {
    RAW_WRITE_GUARD
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

/// Process-global guard that serializes read-modify-write access to
/// `{meta}/_absorb_log.json`. Same pattern as [`INBOX_WRITE_GUARD`]:
/// prevents concurrent `append_absorb_log` calls from losing entries
/// due to TOCTOU races on the load → append → save cycle.
static ABSORB_LOG_GUARD: OnceLock<Mutex<()>> = OnceLock::new();

fn lock_absorb_log_writes() -> MutexGuard<'static, ()> {
    ABSORB_LOG_GUARD
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

/// Subdirectory under `wiki/` for concept pages specifically.
pub const WIKI_CONCEPTS_SUBDIR: &str = "concepts";

/// Subdirectory under `wiki/` for people pages (canonical §10).
/// Each page describes a person referenced in the user's raw sources
/// (authors, researchers, collaborators).
pub const WIKI_PEOPLE_SUBDIR: &str = "people";

/// Subdirectory under `wiki/` for topic pages (canonical §10).
/// Cross-concept theme aggregations (e.g. "AI Memory", "RAG
/// evolution").
pub const WIKI_TOPICS_SUBDIR: &str = "topics";

/// Subdirectory under `wiki/` for compare pages (canonical §10).
/// Structured A-vs-B comparisons with argument columns.
pub const WIKI_COMPARE_SUBDIR: &str = "compare";

/// All wiki page categories, in display order.
pub const WIKI_CATEGORIES: &[(&str, &str)] = &[
    ("concept", WIKI_CONCEPTS_SUBDIR),
    ("people", WIKI_PEOPLE_SUBDIR),
    ("topic", WIKI_TOPICS_SUBDIR),
    ("compare", WIKI_COMPARE_SUBDIR),
];

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

/// Embedded AGENTS.md template (canonical §10 schema layer).
const AGENTS_MD_TEMPLATE: &str = include_str!("../templates/AGENTS.md");

/// Page templates for each wiki category.
const TEMPLATE_CONCEPT: &str = include_str!("../templates/templates/concept.md");
const TEMPLATE_PEOPLE: &str = include_str!("../templates/templates/people.md");
const TEMPLATE_TOPIC: &str = include_str!("../templates/templates/topic.md");
const TEMPLATE_COMPARE: &str = include_str!("../templates/templates/compare.md");

/// Policy files governing maintainer behavior.
const POLICY_MAINTENANCE: &str = include_str!("../templates/policies/maintenance.md");
const POLICY_CONFLICT: &str = include_str!("../templates/policies/conflict.md");
const POLICY_DEPRECATION: &str = include_str!("../templates/policies/deprecation.md");
const POLICY_NAMING: &str = include_str!("../templates/policies/naming.md");

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
    /// `_absorb_log.json` is present but its contents are not valid JSON
    /// or do not match the expected `Vec<AbsorbLogEntry>` schema.
    #[error("absorb log corrupted: {0}")]
    AbsorbLogCorrupted(String),
    /// `_backlinks.json` is present but its contents are not valid JSON
    /// or do not match the expected `BacklinksIndex` schema.
    #[error("backlinks index corrupted: {0}")]
    BacklinksCorrupted(String),
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
/// Resolution order (per technical-design.md §7.2):
///   1. `$CLAWWIKI_HOME` if set — highest priority, no fallback triggered.
///   2. `$HOME/.clawwiki/` on Unix / `%USERPROFILE%/.clawwiki/` on Windows.
///      Attempts to create the directory. On Windows, if creation fails
///      with `PermissionDenied` (UAC / antivirus block), falls back to
///      `%LOCALAPPDATA%/clawwiki/`.
///   3. Relative `./.clawwiki/` if HOME/USERPROFILE unavailable (rare).
///
/// The Windows fallback addresses the common case where `C:\Users\{name}\`
/// is sandboxed by corporate AV or UAC policies. `%LOCALAPPDATA%` is
/// always user-writable.
#[must_use]
pub fn default_root() -> PathBuf {
    let override_var = std::env::var_os(ENV_OVERRIDE);
    let home = home_dir();
    let primary = default_root_from(override_var.clone(), home.as_deref());

    // If the env override was set, trust it verbatim — no fallback.
    if override_var.is_some() {
        return primary;
    }

    // Try to create the primary path. On success, use it.
    match try_create_dir(&primary) {
        Ok(()) => primary,
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            // Windows UAC / AV fallback to %LOCALAPPDATA%\clawwiki
            if let Some(fallback) = local_appdata_fallback() {
                if try_create_dir(&fallback).is_ok() {
                    eprintln!(
                        "[wiki_store] primary path {} denied (PermissionDenied), \
                         falling back to {}",
                        primary.display(),
                        fallback.display(),
                    );
                    return fallback;
                }
            }
            primary // give up, let init_wiki surface the real error
        }
        Err(_) => primary, // other errors (disk full, etc) — let init_wiki report
    }
}

/// Pure version of [`default_root`] that takes its inputs explicitly so
/// the resolution rules can be unit-tested without mutating the
/// process-wide environment (which would race with parallel tests).
///
/// **Note**: this function does not perform any I/O. The Windows UAC
/// fallback logic lives in [`default_root`].
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

/// Try to create a directory, returning `Ok(())` if it already exists.
fn try_create_dir(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(path)
}

/// Windows: return `%LOCALAPPDATA%\clawwiki` as the permission-safe fallback.
#[cfg(target_os = "windows")]
fn local_appdata_fallback() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|s| PathBuf::from(s).join("clawwiki"))
}

/// Non-Windows: no fallback needed (HOME is always user-writable).
#[cfg(not(target_os = "windows"))]
fn local_appdata_fallback() -> Option<PathBuf> {
    None
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
    // feat(W): create all 4 wiki category subdirectories so the
    // maintainer can write pages of any type from day one.
    for (_cat_name, subdir) in WIKI_CATEGORIES {
        let cat_dir = paths.wiki.join(subdir);
        fs::create_dir_all(&cat_dir).map_err(|e| WikiStoreError::io(cat_dir.clone(), e))?;
    }

    // Seed CLAUDE.md only if absent. We deliberately do NOT compare
    // contents — once the file exists we treat it as user-owned and
    // never overwrite. The user can `rm schema/CLAUDE.md` to re-seed.
    if !paths.schema_claude_md.exists() {
        fs::write(&paths.schema_claude_md, CLAUDE_MD_TEMPLATE)
            .map_err(|e| WikiStoreError::io(paths.schema_claude_md.clone(), e))?;
    }

    // Seed AGENTS.md only if absent (same contract as CLAUDE.md).
    let agents_md_path = paths.schema.join("AGENTS.md");
    if !agents_md_path.exists() {
        fs::write(&agents_md_path, AGENTS_MD_TEMPLATE)
            .map_err(|e| WikiStoreError::io(agents_md_path.clone(), e))?;
    }

    // Seed schema/templates/ directory with page templates.
    let templates_dir = paths.schema.join("templates");
    fs::create_dir_all(&templates_dir)
        .map_err(|e| WikiStoreError::io(templates_dir.clone(), e))?;
    for (name, content) in [
        ("concept.md", TEMPLATE_CONCEPT),
        ("people.md", TEMPLATE_PEOPLE),
        ("topic.md", TEMPLATE_TOPIC),
        ("compare.md", TEMPLATE_COMPARE),
    ] {
        let file = templates_dir.join(name);
        if !file.exists() {
            fs::write(&file, content).map_err(|e| WikiStoreError::io(file.clone(), e))?;
        }
    }

    // Seed schema/policies/ directory with governance rules.
    let policies_dir = paths.schema.join("policies");
    fs::create_dir_all(&policies_dir)
        .map_err(|e| WikiStoreError::io(policies_dir.clone(), e))?;
    for (name, content) in [
        ("maintenance.md", POLICY_MAINTENANCE),
        ("conflict.md", POLICY_CONFLICT),
        ("deprecation.md", POLICY_DEPRECATION),
        ("naming.md", POLICY_NAMING),
    ] {
        let file = policies_dir.join(name);
        if !file.exists() {
            fs::write(&file, content).map_err(|e| WikiStoreError::io(file.clone(), e))?;
        }
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
    /// In M4, this is always the **canonical** URL (normalized via
    /// `url_ingest::canonical::canonicalize`). The user-supplied raw
    /// URL (pre-canonicalization) is stored in `original_url` when it
    /// differs from the canonical form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    /// ISO-8601 datetime when the file was written.
    pub ingested_at: String,
    /// M4: SHA-256 hex (lowercase) of the cleaned body. Computed via
    /// `url_ingest::content_hash::compute_content_hash`. Used as the
    /// secondary dedupe signal ("content identity") so re-submissions
    /// with different canonical URLs but identical bodies collapse to
    /// the same raw via `find_raw_by_content_hash`. `None` on old
    /// entries persisted before M4 — back-compat only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    /// M4: the original user-supplied URL (pre-canonicalization) when
    /// it differs from `source_url`. Only populated when canonicalize
    /// mutated the input. Kept in frontmatter (not flattened to
    /// `RawEntry`) so Raw Library listing isn't bloated; the URL-ingest
    /// recent log is the primary surface that displays this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_url: Option<String>,
}

impl RawFrontmatter {
    /// Build a frontmatter for a `paste`-source text entry. The
    /// `ingested_at` field is filled with the current UTC datetime
    /// (ISO-8601, second precision).
    ///
    /// M4: `content_hash` / `original_url` default to `None`. Callers
    /// with identity signals (URL ingest orchestrator) should use
    /// [`Self::for_paste_with_identity`] to populate them.
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
            content_hash: None,
            original_url: None,
        }
    }

    /// M4: build a frontmatter carrying full identity metadata — the
    /// canonical URL (in `source_url`), the original user-supplied URL
    /// (only when it differs, to keep the file lean), and the SHA-256
    /// content hash. Used by the URL ingest orchestrator so that
    /// dedupe + observability can key on either the canonical URL or
    /// the content hash.
    #[must_use]
    pub fn for_paste_with_identity(
        source: &str,
        canonical_url: Option<String>,
        original_url: Option<String>,
        content_hash: Option<String>,
    ) -> Self {
        // Elide `original_url` when it matches the canonical form — no
        // signal in that field beyond what `source_url` already shows.
        let original_url = match (&canonical_url, original_url.as_deref()) {
            (Some(canonical), Some(original)) if canonical == original => None,
            _ => original_url,
        };
        Self {
            kind: "raw".to_string(),
            status: "ingested".to_string(),
            owner: "user".to_string(),
            schema: "v1".to_string(),
            source: source.to_string(),
            source_url: canonical_url,
            ingested_at: now_iso8601(),
            content_hash,
            original_url,
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
        // M4: identity signals — always appended last so old parsers
        // that stop at `ingested_at` still round-trip the core fields.
        if let Some(hash) = &self.content_hash {
            s.push_str(&format!("content_hash: {hash}\n"));
        }
        if let Some(url) = &self.original_url {
            s.push_str(&format!("original_url: {url}\n"));
        }
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
    /// Optional `source_url` from the frontmatter (canonical URL in M4).
    pub source_url: Option<String>,
    /// ISO-8601 datetime from the frontmatter.
    pub ingested_at: String,
    /// File size in bytes (for the listing UI).
    pub byte_size: u64,
    /// M4: SHA-256 hex of the cleaned body, when available. `None`
    /// on entries written pre-M4 or with empty bodies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    /// M4: the original user-supplied URL when it differs from the
    /// canonical `source_url`. `None` when the input was already
    /// canonical or when the entry was written pre-M4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_url: Option<String>,
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
/// * Replaces any run of non-ASCII-alphanumeric chars with a single `-`
/// * Adds a stable Unicode hash suffix when meaningful non-ASCII text
///   was present, so CJK titles do not all collapse to `"untitled"`
/// * Trims leading/trailing `-`
/// * Caps at 64 chars to keep filenames sane
/// * Returns `"untitled"` if the input has no ASCII or Unicode letters/digits
#[must_use]
pub fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut last_dash = true;
    let has_unicode_alnum = input
        .chars()
        .any(|c| !c.is_ascii() && c.is_alphanumeric());

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
        if has_unicode_alnum {
            format!("u-{}", stable_slug_hash(input))
        } else {
            "untitled".to_string()
        }
    } else if has_unicode_alnum {
        append_unicode_hash_suffix(trimmed, input)
    } else {
        trimmed.to_string()
    }
}

fn append_unicode_hash_suffix(ascii_base: &str, original: &str) -> String {
    let suffix = format!("-u{}", &stable_slug_hash(original)[..8]);
    if ascii_base.len() + suffix.len() <= 64 {
        return format!("{ascii_base}{suffix}");
    }

    let max_base_len = 64 - suffix.len();
    let shortened = ascii_base[..max_base_len].trim_matches('-');
    if shortened.is_empty() {
        format!("u-{}", stable_slug_hash(original))
    } else {
        format!("{shortened}{suffix}")
    }
}

fn stable_slug_hash(input: &str) -> String {
    // 64-bit FNV-1a: deterministic across platforms and avoids adding a
    // hashing dependency just for filesystem-safe Unicode fallback slugs.
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Sources whose `body` we trust enough to skip anti-bot / low-quality
/// validation inside `write_raw_entry`. These are user-authored inputs
/// (short notes, text messages) where legitimate content can be very
/// short and would trip the validator. Anything NOT in this allowlist
/// (fetched URLs, WeChat articles, PDFs, etc.) runs through
/// [`validate_raw_content`] before touching disk.
const RAW_CONTENT_VALIDATION_EXEMPT: &[&str] = &[
    "paste",        // user pasted text manually via UI
    "wechat-text",  // inbound plain WeChat message (may be a one-liner)
    "voice",        // transcript is whatever the user said
    "query",        // Q&A crystallization — short answers are fine
];

/// Reject pages that look like anti-bot / captcha placeholders before
/// they hit the raw layer. See
/// `wiki_ingest::validate_fetched_content` for the primary fetch-time
/// validator; this is the **last line of defense** that covers every
/// write path (including callers that bypass wiki_ingest — e.g. the
/// WeChat-iLink URL fetch that writes body as-is).
///
/// Intentionally does NOT have an upper length gate for marker hits.
/// A real anti-bot page can be >1000 bytes once it inlines HTML skeleton
/// / CSS / JS — the verification text might be buried but the marker
/// still fires. We trade a small false-positive surface (legitimate
/// articles that happen to contain "环境异常" verbatim) for not
/// silently pollinating the Inbox with placeholder pages.
fn validate_raw_content(body: &str) -> std::result::Result<(), String> {
    let trimmed = body.trim();

    if trimmed.len() < 50 {
        return Err(format!("内容过短 ({} 字符)", trimmed.len()));
    }

    const ANTI_BOT_MARKERS: &[&str] = &[
        "环境异常",
        "完成验证后即可继续访问",
        "请完成安全验证",
        "人机验证",
        "滑动验证",
        "拼图验证",
        "访问频繁",
        "Access Denied",
        "Please enable JavaScript",
        "403 Forbidden",
    ];
    for marker in ANTI_BOT_MARKERS {
        if trimmed.contains(marker) {
            return Err(format!("反爬验证页: 包含 '{marker}'"));
        }
    }

    let meaningful = trimmed
        .chars()
        .filter(|c| {
            c.is_ascii_alphanumeric() || ('\u{4E00}'..='\u{9FFF}').contains(c)
        })
        .count();
    if meaningful < 30 {
        return Err(format!("实际文字过少 ({meaningful} 字符)"));
    }

    Ok(())
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
/// # Content validation
///
/// For fetched sources (everything outside [`RAW_CONTENT_VALIDATION_EXEMPT`])
/// the body must pass [`validate_raw_content`]. This guards against
/// anti-bot / verification placeholder pages polluting the Inbox when
/// a caller forgot to pre-validate with
/// `wiki_ingest::validate_fetched_content`.
///
/// Errors:
///   * I/O errors (filesystem full, permission denied, etc.)
///   * `WikiStoreError::Invalid` if `slug` is empty after sanitization,
///     or the body fails anti-bot / quality validation for a fetched source
pub fn write_raw_entry(
    paths: &WikiPaths,
    source: &str,
    slug: &str,
    body: &str,
    frontmatter: &RawFrontmatter,
) -> Result<RawEntry> {
    // Last line of defense: for fetched sources, reject anti-bot pages
    // and empty placeholders. Caller-side `validate_fetched_content`
    // should normally catch these first, but the WeChat iLink path
    // (and any future direct caller) might miss it — this guarantees
    // the raw layer stays clean.
    if !RAW_CONTENT_VALIDATION_EXEMPT.contains(&source) {
        if let Err(reason) = validate_raw_content(body) {
            return Err(WikiStoreError::Invalid(format!(
                "内容验证失败 ({source}): {reason}"
            )));
        }
    }

    // Serialize the entire read-id → write-file section so two
    // concurrent callers (e.g. main thread + tokio::spawn) cannot
    // observe the same max id from next_raw_id().
    let _guard = lock_raw_writes();

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

    // P1 provenance: fire a `raw_written` event *inside* the
    // RAW_WRITE_GUARD critical section so the jsonl timeline
    // matches the physical write order. `fire_event` is soft-fail —
    // any provenance error is logged and swallowed so the caller
    // still gets the `RawEntry` back on the happy path.
    let mut upstream: Vec<provenance::LineageRef> = Vec::new();
    if let Some(url) = &frontmatter.source_url {
        upstream.push(provenance::LineageRef::UrlSource {
            canonical: url.clone(),
        });
    }
    let display_title = provenance::display_title_raw_written(&safe_slug);
    provenance::fire_event(
        paths,
        provenance::LineageEvent {
            event_id: provenance::new_event_id(),
            event_type: provenance::LineageEventType::RawWritten,
            timestamp_ms: provenance::now_unix_ms(),
            upstream,
            downstream: vec![provenance::LineageRef::Raw { id }],
            display_title,
            metadata: serde_json::json!({
                "source": source,
                "filename": filename.clone(),
            }),
        },
    );

    Ok(RawEntry {
        id,
        filename,
        source: source.to_string(),
        slug: safe_slug,
        date,
        source_url: frontmatter.source_url.clone(),
        ingested_at: frontmatter.ingested_at.clone(),
        byte_size: metadata.len(),
        content_hash: frontmatter.content_hash.clone(),
        original_url: frontmatter.original_url.clone(),
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

/// Delete a raw entry by id. Removes the .md file from disk and
/// cascades to remove any orphaned inbox entries that reference
/// this raw id via `source_raw_id`. Without this cascade, deleting
/// a raw entry leaves dangling inbox items that point at a
/// non-existent source, producing confusing 404s in the Inbox UI.
pub fn delete_raw_entry(paths: &WikiPaths, id: u32) -> Result<()> {
    let entries = list_raw_entries(paths)?;
    let entry = entries
        .into_iter()
        .find(|e| e.id == id)
        .ok_or_else(|| WikiStoreError::NotFound(id))?;
    let path = paths.raw.join(&entry.filename);
    fs::remove_file(&path).map_err(|e| WikiStoreError::io(path, e))?;

    // Cascade: remove orphaned inbox entries referencing this raw id.
    // Uses the INBOX_WRITE_GUARD to serialize against concurrent inbox
    // mutations (same pattern as append_inbox_pending / resolve_inbox_entry).
    let _guard = lock_inbox_writes();
    if let Ok(mut inbox) = load_inbox_file(paths) {
        let before = inbox.len();
        inbox.retain(|e| e.source_raw_id != Some(id));
        if inbox.len() != before {
            // Best-effort: if save fails, the raw file is already gone
            // and the orphaned inbox entries will be harmless stale items.
            let _ = save_inbox_file(paths, &inbox);
        }
    }
    Ok(())
}

/// Find the most recent raw entry whose frontmatter `source_url`
/// equals `canonical_url` exactly. Returns `Ok(None)` when no match
/// exists. The comparison is byte-identical — callers who want to
/// dedupe across tracking-param variants must canonicalize the URL
/// before calling (see `desktop-core::url_ingest::canonical`).
///
/// # Performance
///
/// O(n) in the raw count — we list every entry and scan. Fine for
/// M3 since typical raw counts stay well under 1000 during normal
/// use. A future `manifest.json` cache can pin this to O(1) without
/// changing the signature.
///
/// "Most recent" is defined by ascending id from [`list_raw_entries`]
/// — we return the highest matching id rather than the first match
/// so that re-ingests (which write a new raw with a larger id but
/// keep the same `source_url`) surface the newest landing.
///
/// Used by: `desktop-core::url_ingest::dedupe::decide` to derive the
/// dedupe key for M3 canonical-URL ingestion.
pub fn find_recent_raw_by_source_url(
    paths: &WikiPaths,
    canonical_url: &str,
) -> Result<Option<RawEntry>> {
    if canonical_url.is_empty() {
        return Ok(None);
    }
    let mut entries = list_raw_entries(paths)?;
    // list_raw_entries sorts ascending, so the final match is the
    // highest id with a matching url — the "most recent" landing.
    entries.reverse();
    for entry in entries {
        if entry.source_url.as_deref() == Some(canonical_url) {
            return Ok(Some(entry));
        }
    }
    Ok(None)
}

/// M4: find the most recent raw entry whose frontmatter `content_hash`
/// equals `target_hash` exactly. Skips raw entries persisted before
/// M4 (where `content_hash` is `None`) — those are invisible to this
/// lookup by design.
///
/// Comparison is byte-identical (SHA-256 hex is case-sensitive here,
/// but `compute_content_hash` always emits lowercase hex so in practice
/// this is stable).
///
/// # Performance
///
/// O(n) in the raw count — mirrors [`find_recent_raw_by_source_url`].
/// A future `manifest.json` cache can convert to O(1).
///
/// "Most recent" is defined by the highest matching id (same rule as
/// [`find_recent_raw_by_source_url`]).
///
/// Used by: `desktop-core::url_ingest::dedupe::decide` for the M4
/// "content identity" secondary dedupe path — when the canonical URL
/// doesn't match any existing raw, a content-hash match reuses the
/// prior landing anyway (same article reached via a different URL).
pub fn find_raw_by_content_hash(
    paths: &WikiPaths,
    target_hash: &str,
) -> Result<Option<RawEntry>> {
    if target_hash.is_empty() {
        return Ok(None);
    }
    let entries = list_raw_entries(paths)?;
    Ok(entries
        .into_iter()
        .filter(|e| e.content_hash.as_deref() == Some(target_hash))
        .max_by_key(|e| e.id))
}

/// Find the latest inbox entry (any status, any kind) whose
/// `source_raw_id` equals `raw_id`. Returns `Ok(None)` when no match
/// exists.
///
/// "Latest" means the highest `id` — the inbox append path is
/// monotonic, so a higher id is always strictly later in time. If
/// multiple entries reference the same raw (e.g. a Pending `NewRaw`
/// was resolved + a follow-up `Conflict` was later filed), this
/// returns whichever was filed most recently.
///
/// Used by: `desktop-core::url_ingest::dedupe::decide` to classify
/// an existing raw's inbox state (Pending / Approved / Rejected /
/// absent) when deciding whether to reuse or re-fetch.
pub fn find_inbox_by_source_raw_id(
    paths: &WikiPaths,
    raw_id: u32,
) -> Result<Option<InboxEntry>> {
    let entries = load_inbox_file(paths)?;
    Ok(entries
        .into_iter()
        .filter(|e| e.source_raw_id == Some(raw_id))
        .max_by_key(|e| e.id))
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
    let fields = parse_frontmatter_fields(&content);
    let metadata = fs::metadata(path).map_err(|e| WikiStoreError::io(path.to_path_buf(), e))?;

    Ok(RawEntry {
        id,
        filename,
        source,
        slug,
        date,
        source_url: fields.source_url,
        ingested_at: fields.ingested_at,
        byte_size: metadata.len(),
        content_hash: fields.content_hash,
        original_url: fields.original_url,
    })
}

/// M4: parsed view of the raw frontmatter fields consumed during
/// listing. Grouped into a struct so adding new optional fields (like
/// `content_hash` / `original_url`) doesn't churn every call site.
#[derive(Debug, Default)]
struct RawFrontmatterFields {
    source_url: Option<String>,
    ingested_at: String,
    content_hash: Option<String>,
    original_url: Option<String>,
}

/// Pull frontmatter fields out of the YAML block at the top of a file.
/// Tolerant: missing fields are returned as `None` / empty string
/// rather than erroring. Silently ignores unknown keys for forward
/// compat with future schema additions.
fn parse_frontmatter_fields(content: &str) -> RawFrontmatterFields {
    let mut fields = RawFrontmatterFields::default();
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
            fields.source_url = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("ingested_at: ") {
            fields.ingested_at = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("content_hash: ") {
            fields.content_hash = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("original_url: ") {
            fields.original_url = Some(rest.to_string());
        }
    }
    fields
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    /// v2: LLM confidence score [0.0, 1.0]. Computed by absorb_batch.
    #[serde(default, skip_serializing_if = "is_zero_f32")]
    pub confidence: f32,
}

fn is_zero_f32(v: &f32) -> bool { *v == 0.0 }

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
            confidence: 0.0,
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
        if self.confidence > 0.0 {
            s.push_str(&format!("confidence: {:.2}\n", self.confidence));
        }
        s.push_str("---\n");
        s
    }
}

/// On-disk metadata for a single wiki concept page, returned by
/// [`list_wiki_pages`] and [`read_wiki_page`]. Mirrors `RawEntry`'s
/// shape for the raw layer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    /// Wiki category: "concept", "people", "topic", or "compare".
    /// Set by `list_all_wiki_pages` from the directory the page
    /// lives in. Defaults to "concept" for backward compatibility.
    pub category: String,
    /// v2: Confidence score [0.0, 1.0] from frontmatter.
    #[serde(default)]
    pub confidence: f32,
    /// v2: ISO-8601 datetime when the page was last verified by maintenance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_verified: Option<String>,
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

/// Write a wiki page to any category subdir. Generalizes
/// `write_wiki_page` (which is hardcoded to concepts). The
/// `category` must be one of the keys in `WIKI_CATEGORIES`
/// ("concept", "people", "topic", "compare"). Returns the written
/// file path.
///
/// `page_type_tag` is the frontmatter `type:` value (same as
/// category name for MVP). The frontmatter gets `type: people`
/// or `type: topic` etc.
pub fn write_wiki_page_in_category(
    paths: &WikiPaths,
    category: &str,
    slug: &str,
    title: &str,
    summary: &str,
    body: &str,
    source_raw_id: Option<u32>,
) -> Result<PathBuf> {
    validate_wiki_slug(slug)?;
    let subdir = WIKI_CATEGORIES
        .iter()
        .find(|(name, _)| *name == category)
        .map(|(_, dir)| *dir)
        .ok_or_else(|| {
            WikiStoreError::Invalid(format!(
                "unknown wiki category: {category} (expected one of: concept, people, topic, compare)"
            ))
        })?;
    let cat_dir = paths.wiki.join(subdir);
    fs::create_dir_all(&cat_dir).map_err(|e| WikiStoreError::io(cat_dir.clone(), e))?;
    let path = cat_dir.join(format!("{slug}.md"));

    let mut fm = WikiFrontmatter::for_concept(title, summary, source_raw_id);
    fm.kind = category.to_string();
    let mut content = fm.to_yaml_block();
    content.push('\n');
    content.push_str(body);
    if !body.ends_with('\n') {
        content.push('\n');
    }

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
    list_pages_in_dir(&paths.wiki.join(WIKI_CONCEPTS_SUBDIR))
}

/// List wiki pages across ALL categories (concepts + people +
/// topics + compare), merged and sorted by slug. Used by the
/// Graph page and other surfaces that want the full picture.
pub fn list_all_wiki_pages(paths: &WikiPaths) -> Result<Vec<WikiPageSummary>> {
    let mut all: Vec<WikiPageSummary> = Vec::new();
    for (cat_name, subdir) in WIKI_CATEGORIES {
        let dir = paths.wiki.join(subdir);
        let mut pages = list_pages_in_dir(&dir)?;
        // Stamp each page with the category it was found under.
        for p in &mut pages {
            p.category = cat_name.to_string();
        }
        all.append(&mut pages);
    }
    all.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(all)
}

/// ── Q2 Target-Resolver read model ────────────────────────────────
///
/// Minimal projection of a wiki page exposed to
/// `wiki_maintainer::resolve_target_candidates`. Carries just the
/// fields the 8-signal scorer needs (slug + title + the optional
/// `source_raw_id` frontmatter field) — no bodies, no byte sizes,
/// no `confidence`. The resolver runs over every wiki page on each
/// call, and we'd rather hand it a small struct than leak the full
/// [`WikiPageSummary`] shape (which carries fields that are
/// irrelevant and would tempt scorer features to depend on them).
///
/// Pure read model — no `write_*` API consumes this type, and
/// [`list_page_summaries_for_resolver`] is the single constructor.
/// Data-loss risk is zero: this type never touches `write_raw_entry`
/// or `slugify`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PageSummaryForResolver {
    /// Kebab-case slug (primary key, filename stem).
    pub slug: String,
    /// Human-readable title from the frontmatter.
    pub title: String,
    /// Optional raw/ entry id that seeded the page. `None` when the
    /// page was hand-written or the frontmatter lacks the key. Drives
    /// the `shared_raw_source` scorer signal — see
    /// `wiki_maintainer::resolve_target_candidates`.
    pub source_raw_id: Option<u32>,
    /// Wiki category ("concept" | "people" | "topic" | "compare").
    /// Preserved so the resolver can (future) weight matches by
    /// category compatibility without re-opening the file.
    pub category: String,
}

/// Q2 Target-Resolver read API.
///
/// Return a minimal projection of every wiki page across all
/// categories, suitable for the O(N) pass in
/// `wiki_maintainer::resolve_target_candidates`. Delegates to
/// [`list_all_wiki_pages`] and trims each summary to the four
/// fields the scorer actually reads.
///
/// Pure read path — zero filesystem writes, zero side-effects.
/// Safe to call from any HTTP handler; same "no cache, re-parse
/// frontmatter on each call" philosophy as the underlying
/// `list_all_wiki_pages`.
pub fn list_page_summaries_for_resolver(
    paths: &WikiPaths,
) -> Result<Vec<PageSummaryForResolver>> {
    let pages = list_all_wiki_pages(paths)?;
    Ok(pages
        .into_iter()
        .map(|p| PageSummaryForResolver {
            slug: p.slug,
            title: p.title,
            source_raw_id: p.source_raw_id,
            category: p.category,
        })
        .collect())
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
    /// Node kind: "raw" or "concept". Coarse-grained node type.
    pub kind: String,
    /// Fine-grained category: "raw" for raw entries, or one of
    /// "concept"/"people"/"topic"/"compare" for wiki pages.
    /// Drives semantic coloring on the frontend graph visualization.
    pub category: String,
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
    let concepts = list_all_wiki_pages(paths).unwrap_or_default();

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
            category: "raw".to_string(),
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
            category: concept.category.clone(),
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
        let subdir = WIKI_CATEGORIES
            .iter()
            .find(|(name, _)| *name == concept.category)
            .map(|(_, dir)| *dir)
            .unwrap_or(WIKI_CONCEPTS_SUBDIR);
        let concept_file =
            paths.wiki.join(subdir).join(format!("{}.md", concept.slug));
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
/// mentions `[LLM Wiki](concepts/llm-wiki.md)` or `[[llm-wiki]]`
/// in its body, then `extract_internal_links(body_a)` returns
/// `["llm-wiki"]`, and `build_wiki_graph` emits a `references`
/// edge from A to B.
///
/// The regex is intentionally simple — we only look for
/// canonical `](concepts/slug.md)` suffixes plus `[[slug]]`
/// wikilinks used by the frontend article renderer.
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
            let slug = &lower[abs_start..abs_start + end_rel];
            push_internal_link_slug(&mut slugs, slug);
            search_from = abs_start + end_rel + suffix.len();
        } else {
            break;
        }
    }
    // Also support the wikilink syntax rendered by the frontend article
    // body component. Labels and anchors are display-only for backlinks.
    let mut wikilink_from = 0usize;
    while let Some(start_rel) = body[wikilink_from..].find("[[") {
        let inner_start = wikilink_from + start_rel + 2;
        if let Some(end_rel) = body[inner_start..].find("]]") {
            let raw_inner = &body[inner_start..inner_start + end_rel];
            let slug = raw_inner
                .split('|')
                .next()
                .unwrap_or("")
                .split('#')
                .next()
                .unwrap_or("");
            push_internal_link_slug(&mut slugs, slug);
            wikilink_from = inner_start + end_rel + 2;
        } else {
            break;
        }
    }
    slugs
}

fn push_internal_link_slug(slugs: &mut Vec<String>, candidate: &str) {
    let slug = candidate.trim().to_lowercase();
    if slug.is_empty()
        || !slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        || slugs.contains(&slug)
    {
        return;
    }
    slugs.push(slug);
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
            let (title, summary, source_raw_id, created_at, last_verified) =
                parse_wiki_frontmatter_fields(&content);
            // Parse confidence from frontmatter if present.
            let confidence = content.lines()
                .find(|l| l.trim_start().starts_with("confidence:"))
                .and_then(|l| l.split(':').nth(1)?.trim().parse::<f32>().ok())
                .unwrap_or(0.0);

            results.push(WikiPageSummary {
                slug,
                title,
                summary,
                source_raw_id,
                created_at,
                byte_size: metadata.len(),
                category: "concept".to_string(),
                confidence,
                last_verified,
            });
        }
    }
    results.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(results)
}

/// One "related page" hit returned by [`compute_related_pages`].
/// Mirrors [`WikiSearchHit`]'s shape but carries human-readable
/// `reasons` strings (shown verbatim under the "Related" section of
/// a wiki page) instead of a text snippet, plus a numeric `score`
/// used only for sort order.
///
/// The `score` field is deliberately exposed — the frontend hides
/// it from the user but may gate the display (e.g. "only show if
/// score ≥ 2"). `reasons` is the canonical channel for surfacing
/// *why* two pages are related to the reader.
///
/// Serialized as camel-case-free snake_case (same convention as
/// `WikiPageSummary`) so the frontend TS type mirror is mechanical.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RelatedPageHit {
    /// Slug of the related page.
    pub slug: String,
    /// Display title (from frontmatter).
    pub title: String,
    /// Wiki category: "concept", "people", "topic", or "compare".
    pub category: String,
    /// Short one-line summary from the frontmatter. `None` when the
    /// frontmatter has no summary field (rare but tolerated).
    pub summary: Option<String>,
    /// Human-readable reasons for the match, in the order they were
    /// discovered. Examples:
    ///   * `"共享来源: raw #00042"` — both derived from raw entry 42
    ///   * `"共同链接: llm-wiki"` — both pages link to `llm-wiki`
    ///
    /// Shown verbatim on the frontend (hence the deliberate Chinese
    /// phrasing — matches ClawWiki's copy language).
    pub reasons: Vec<String>,
    /// Raw numeric score: higher = more related. Used for sorting.
    /// Not typically rendered to the user.
    pub score: u32,
}

/// One "neighbor" entry in a [`PageGraph`]. Used for both outgoing
/// links and backlinks — a minimal slice of [`WikiPageSummary`] that
/// the page-graph renderer needs to draw a card or a node label.
///
/// Kept deliberately smaller than `WikiPageSummary` because the
/// page-graph endpoint returns O(N_neighbors) of these and we want
/// JSON payloads to stay small.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PageGraphNeighbor {
    /// Slug of the neighbor page.
    pub slug: String,
    /// Display title (from frontmatter).
    pub title: String,
    /// Wiki category: "concept", "people", "topic", or "compare".
    pub category: String,
}

/// Page-level graph payload for `GET /api/wiki/pages/{slug}/graph`.
/// Combines three neighborhood views of a single wiki page into one
/// response so the frontend can render the complete "what is this
/// page connected to" panel with a single request:
///
///   * `outgoing`  — pages this page explicitly links to (via the
///                   markdown `](concepts/X.md)` body parser).
///   * `backlinks` — pages that link INTO this page (reverse lookup).
///   * `related`   — pages computed by [`compute_related_pages`]
///                   using shared-outgoing-link + shared-source-raw
///                   signals, ordered by score descending.
///
/// The top-level fields (`slug`/`title`/`category`/`summary`) carry
/// the target page's own metadata so the frontend doesn't need a
/// separate `GET /api/wiki/pages/{slug}` round-trip to render the
/// header card.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PageGraph {
    /// Target page's slug.
    pub slug: String,
    /// Target page's display title.
    pub title: String,
    /// Target page's category.
    pub category: String,
    /// Target page's short summary (`None` if frontmatter has no
    /// summary, to keep the shape strict rather than conflating
    /// empty-string with missing).
    pub summary: Option<String>,
    /// Pages the target explicitly links to via `](concepts/X.md)`.
    /// Dangling links (pointing to non-existent slugs) are dropped.
    pub outgoing: Vec<PageGraphNeighbor>,
    /// Pages that link INTO the target page. Same shape as
    /// `outgoing`. Self-references excluded.
    pub backlinks: Vec<PageGraphNeighbor>,
    /// Pages algorithmically related to the target via shared
    /// signals. Ordered by score descending, capped at top 10.
    pub related: Vec<RelatedPageHit>,
}

/// Locate the on-disk path for a wiki page by searching across all
/// category subdirs. Returns `None` if the slug doesn't match any
/// file. Pure-ish (touches `path.is_file`).
///
/// We search in `WIKI_CATEGORIES` order (concept → people → topic →
/// compare), which matches the "concept pages are the common case"
/// ordering of the rest of the crate. A slug exists in at most one
/// category (enforced by the write path — `write_wiki_page_in_category`
/// rejects duplicates via the filesystem), so order only matters for
/// the "not found" early-exit cost.
fn locate_wiki_page_file(
    paths: &WikiPaths,
    slug: &str,
) -> Option<(PathBuf, &'static str)> {
    for (cat_name, subdir) in WIKI_CATEGORIES {
        let path = paths.wiki.join(subdir).join(format!("{slug}.md"));
        if path.is_file() {
            return Some((path, *cat_name));
        }
    }
    None
}

/// Compute related wiki pages for `target_slug` using two
/// deterministic signals:
///
///   1. **Shared outgoing link** — if both A and B link to the same
///      slug C, that's a weak "same topic cluster" signal.
///      Each match adds `+2` to score and appends a
///      `"共同链接: {C}"` reason.
///
///   2. **Shared `source_raw_id`** — if both A and B were derived
///      from the same raw entry, they're almost certainly covering
///      the same underlying material. Adds `+3` (strictly stronger
///      than a single shared link) and a
///      `"共享来源: raw #{id:05}"` reason.
///
/// Pages with `score = 0` (no shared signals) are dropped. Results
/// are sorted by score descending, then slug ascending for stable
/// output, and capped at **10 entries** to keep the response bounded
/// for large wikis.
///
/// The target slug itself is never included in the result. Dangling
/// outgoing links (pointing to non-existent slugs) are tolerated
/// silently — they just don't contribute any shared-link matches.
///
/// Errors:
///   * [`WikiStoreError::Invalid`] when `target_slug` fails slug
///     validation or no page with that slug exists in any category.
///   * I/O errors (rare — the function reads multiple body files,
///     so a mid-flight filesystem hiccup can surface as `Io`).
///
/// Cost is O(N × average_body_size) where N is the total concept
/// page count. That's the same asymptotic cost as `list_backlinks`
/// — see that function's rationale for why we haven't added a
/// persistent index yet (the concept count stays in the hundreds
/// for typical MVP users).
pub fn compute_related_pages(
    paths: &WikiPaths,
    target_slug: &str,
) -> Result<Vec<RelatedPageHit>> {
    validate_wiki_slug(target_slug)?;

    // Find the target page's own on-disk file so we can read its
    // body (for outgoing links) and frontmatter (for source_raw_id).
    // Searching across all categories mirrors how the page-graph UI
    // treats people/topic/compare pages as first-class citizens too.
    let (target_path, _target_category) = locate_wiki_page_file(paths, target_slug)
        .ok_or_else(|| {
            WikiStoreError::Invalid(format!("wiki page not found: {target_slug}"))
        })?;

    let target_content = fs::read_to_string(&target_path)
        .map_err(|e| WikiStoreError::io(target_path.clone(), e))?;
    let target_body = strip_frontmatter(&target_content);
    let (_t_title, _t_summary, target_source_raw, _t_created, _t_last_verified) =
        parse_wiki_frontmatter_fields(&target_content);

    // Build the target's outgoing link set, lowercased (the parser
    // already lowercases) and with self-references removed so we
    // don't spuriously match pages that happen to also link back
    // to the target.
    let target_slug_lower = target_slug.to_lowercase();
    let mut target_outgoing: Vec<String> = extract_internal_links(target_body);
    target_outgoing.retain(|s| *s != target_slug_lower);

    // Walk every wiki page (all four categories) and score each one
    // against the target. Skip the target itself.
    let all_pages = list_all_wiki_pages(paths)?;
    // slug → (score, reasons)
    let mut scored: HashMap<String, (u32, Vec<String>)> = HashMap::new();

    for page in &all_pages {
        if page.slug.to_lowercase() == target_slug_lower {
            continue;
        }

        // Load this page's body to extract its outgoing links.
        let (page_path, _) = match locate_wiki_page_file(paths, &page.slug) {
            Some(v) => v,
            None => continue, // disappeared between list and read — skip silently
        };
        let page_content = match fs::read_to_string(&page_path) {
            Ok(c) => c,
            Err(_) => continue, // permission / race — don't blow up
        };
        let page_body = strip_frontmatter(&page_content);
        let page_outgoing: Vec<String> = extract_internal_links(page_body);

        let mut score: u32 = 0;
        let mut reasons: Vec<String> = Vec::new();

        // Signal 1: shared outgoing links. `extract_internal_links`
        // already dedups within one page, so a linear scan is fine.
        for shared in &target_outgoing {
            // A page's own slug must not count as a "shared link"
            // even if the other page happens to link to it (that's
            // covered by the backlinks surface, not related).
            if *shared == page.slug.to_lowercase() {
                continue;
            }
            if page_outgoing.contains(shared) {
                score += 2;
                reasons.push(format!("共同链接: {shared}"));
            }
        }

        // Signal 2: shared source_raw_id. Strictly stronger than
        // a single link overlap (one well-chosen raw seed often
        // spawns a cluster of tightly-related concept pages).
        if let (Some(my_id), Some(their_id)) = (target_source_raw, page.source_raw_id) {
            if my_id == their_id {
                score += 3;
                reasons.push(format!("共享来源: raw #{my_id:05}"));
            }
        }

        if score > 0 {
            scored
                .entry(page.slug.clone())
                .and_modify(|(s, r)| {
                    *s += score;
                    r.extend(reasons.clone());
                })
                .or_insert((score, reasons));
        }
    }

    // Assemble `RelatedPageHit` records. `list_all_wiki_pages` already
    // gave us everything we need for the lookup; avoid re-reading each
    // page by joining on slug.
    let page_by_slug: HashMap<&str, &WikiPageSummary> =
        all_pages.iter().map(|p| (p.slug.as_str(), p)).collect();

    let mut hits: Vec<RelatedPageHit> = scored
        .into_iter()
        .filter_map(|(slug, (score, reasons))| {
            let page = page_by_slug.get(slug.as_str())?;
            let summary = if page.summary.is_empty() {
                None
            } else {
                Some(page.summary.clone())
            };
            Some(RelatedPageHit {
                slug: page.slug.clone(),
                title: page.title.clone(),
                category: page.category.clone(),
                summary,
                reasons,
                score,
            })
        })
        .collect();

    // Sort: score desc (primary), slug asc (tiebreaker for stability).
    hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.slug.cmp(&b.slug)));
    hits.truncate(10);
    Ok(hits)
}

/// Assemble the page-level graph payload for `target_slug`. Combines
/// the target page's metadata, outgoing links, backlinks, and
/// algorithmically-related pages into one [`PageGraph`] struct.
///
/// This is the engine behind `GET /api/wiki/pages/{slug}/graph` —
/// it's separated into a pure library function so the same data can
/// be consumed by the desktop-cli and by future offline exporters
/// without going through the HTTP layer.
///
/// Dangling outgoing links (pointing to non-existent slugs) are
/// dropped from the result rather than surfaced as "unknown" cards,
/// matching `list_backlinks`'s forgiving posture.
///
/// Errors:
///   * [`WikiStoreError::Invalid`] when `target_slug` fails slug
///     validation or no page with that slug exists.
///   * I/O errors (rare — `compute_related_pages` and `list_backlinks`
///     read many files, so a mid-flight filesystem hiccup is the
///     most likely cause).
pub fn get_page_graph(
    paths: &WikiPaths,
    target_slug: &str,
) -> Result<PageGraph> {
    validate_wiki_slug(target_slug)?;

    // Resolve the target's on-disk path across all categories.
    let (target_path, target_category) = locate_wiki_page_file(paths, target_slug)
        .ok_or_else(|| {
            WikiStoreError::Invalid(format!("wiki page not found: {target_slug}"))
        })?;

    let content = fs::read_to_string(&target_path)
        .map_err(|e| WikiStoreError::io(target_path.clone(), e))?;
    let body = strip_frontmatter(&content);
    let (title, summary, _source_raw, _created_at, _last_verified) =
        parse_wiki_frontmatter_fields(&content);
    let summary_opt = if summary.is_empty() {
        None
    } else {
        Some(summary)
    };

    // Precompute a slug → (title, category) map over ALL wiki pages
    // so we can enrich both outgoing and related-neighbor payloads
    // in a single pass without re-reading frontmatters.
    let all_pages = list_all_wiki_pages(paths)?;
    let page_meta: HashMap<String, (String, String)> = all_pages
        .iter()
        .map(|p| (p.slug.clone(), (p.title.clone(), p.category.clone())))
        .collect();

    // Outgoing: parse the markdown body for `](concepts/X.md)` links.
    // Dangling targets are silently dropped.
    let target_slug_lower = target_slug.to_lowercase();
    let outgoing: Vec<PageGraphNeighbor> = extract_internal_links(body)
        .into_iter()
        .filter(|s| *s != target_slug_lower) // strip self-references
        .filter_map(|s| {
            page_meta.get(&s).map(|(title, category)| PageGraphNeighbor {
                slug: s.clone(),
                title: title.clone(),
                category: category.clone(),
            })
        })
        .collect();

    // Backlinks: reuse the existing reverse-lookup (concept-only for
    // now, matching `list_backlinks`'s scope). Map each summary to a
    // lightweight neighbor record.
    let backlink_summaries = list_backlinks(paths, target_slug)?;
    let backlinks: Vec<PageGraphNeighbor> = backlink_summaries
        .into_iter()
        .map(|s| PageGraphNeighbor {
            slug: s.slug,
            title: s.title,
            category: s.category,
        })
        .collect();

    // Related: delegate to the scoring engine.
    let related = compute_related_pages(paths, target_slug)?;

    Ok(PageGraph {
        slug: target_slug.to_string(),
        title,
        category: target_category.to_string(),
        summary: summary_opt,
        outgoing,
        backlinks,
        related,
    })
}

/// One hit in a [`search_wiki_pages`] result. Carries the matching
/// page's summary, the computed relevance score, and a short text
/// snippet centered on the first body-level match (if any). The
/// frontend uses `snippet` to render the search result card's
/// "excerpt" line without re-fetching the full body.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
        let (title, summary, source_raw_id, created_at, last_verified) =
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
                category: "concept".to_string(),
                confidence: 0.0,
                last_verified,
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
    let (title, summary, source_raw_id, created_at, last_verified) =
        parse_wiki_frontmatter_fields(&content);
    let confidence = content
        .lines()
        .find(|l| l.trim_start().starts_with("confidence:"))
        .and_then(|l| l.split(':').nth(1)?.trim().parse::<f32>().ok())
        .unwrap_or(0.0);
    Ok(WikiPageSummary {
        slug: slug.to_string(),
        title,
        summary,
        source_raw_id,
        created_at,
        byte_size: metadata.len(),
        category: "concept".to_string(), // overridden by list_all_wiki_pages
        confidence,
        last_verified,
    })
}

/// Pull `title`, `summary`, `source_raw_id`, `created_at` out of the
/// YAML frontmatter of a wiki page. Tolerant of missing fields —
/// returns empty strings / `None` rather than erroring. Same
/// defensive posture as `parse_frontmatter_fields` for raw entries.
fn parse_wiki_frontmatter_fields(
    content: &str,
) -> (String, String, Option<u32>, String, Option<String>) {
    let mut title = String::new();
    let mut summary = String::new();
    let mut source_raw_id: Option<u32> = None;
    let mut created_at = String::new();
    let mut last_verified: Option<String> = None;
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
        } else if let Some(rest) = line.strip_prefix("last_verified: ") {
            last_verified = Some(rest.to_string());
        }
    }
    (title, summary, source_raw_id, created_at, last_verified)
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
/// ## Concept (N)
/// - [{title}](concepts/{slug}.md) - {summary}
/// - ...
/// ```
pub fn rebuild_wiki_index(paths: &WikiPaths) -> Result<PathBuf> {
    let _guard = lock_wiki_writes();
    fs::create_dir_all(&paths.wiki).map_err(|e| WikiStoreError::io(paths.wiki.clone(), e))?;
    let path = wiki_index_path(paths);

    // feat(W): loop over ALL 4 wiki categories so the index reflects
    // the full wiki breadth. Each category gets its own ## section.
    let mut content = String::new();
    content.push_str("# ClawWiki index\n\n");
    content.push_str(
        "Auto-generated catalog of every wiki page. Updated after every \
         maintainer write. Canonical §10, Karpathy llm-wiki §\"Indexing and logging\".\n\n",
    );
    for (cat_name, subdir) in WIKI_CATEGORIES {
        let cat_dir = paths.wiki.join(subdir);
        let pages = list_pages_in_dir(&cat_dir)?;
        let display_name = capitalize_first(cat_name);
        content.push_str(&format!("## {display_name} ({})\n\n", pages.len()));
        if pages.is_empty() {
            content.push_str(&format!("_No {cat_name} pages yet._\n\n"));
        } else {
            for page in &pages {
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
                    subdir = subdir,
                    slug = page.slug,
                    summary = summary,
                ));
            }
            content.push('\n');
        }
    }

    // Atomic write: tmp + rename.
    let tmp = path.with_extension("md.tmp");
    fs::write(&tmp, content.as_bytes()).map_err(|e| WikiStoreError::io(tmp.clone(), e))?;
    fs::rename(&tmp, &path).map_err(|e| WikiStoreError::io(path.clone(), e))?;
    Ok(path)
}

/// List pages in an arbitrary wiki subdir (concepts/ or people/ etc.).
/// Shared by both `list_wiki_pages` and `rebuild_wiki_index`.
fn list_pages_in_dir(dir: &Path) -> Result<Vec<WikiPageSummary>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let rd = fs::read_dir(dir).map_err(|e| WikiStoreError::io(dir.to_path_buf(), e))?;
    let mut pages: Vec<WikiPageSummary> = Vec::new();
    for entry in rd.flatten() {
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
        match parse_wiki_file(&path, &slug) {
            Ok(page) => pages.push(page),
            Err(_) => continue,
        }
    }
    pages.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(pages)
}

/// Capitalize the first char of a string for display.
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
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

    // ── W1 Maintainer Workbench additions (all optional) ─────────
    //
    // These fields were introduced in W1 when the maintainer flipped
    // from a black-box `approve` action to a structured three-choice
    // workflow (`create_new` / `update_existing` / `reject`). All
    // four are `Option<String>` with `#[serde(default)]` so that
    // older `inbox.json` files (pre-W1) deserialize cleanly with
    // `None` in each slot and `skip_serializing_if` keeps the
    // written-back JSON byte-identical for untouched entries.

    /// Kebab slug the server "proposed" from the raw (pre-commit).
    /// Populated when the propose pass runs, independently of whether
    /// the user later picks `create_new` or `update_existing`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_wiki_slug: Option<String>,

    /// Slug of the wiki page that was actually written on success.
    /// Set by `execute_maintain` for `CreateNew` (echoes the proposal
    /// slug) and for `UpdateExisting { target_page_slug }` (echoes the
    /// chosen target). `None` while the entry is still pending or
    /// rejected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_page_slug: Option<String>,

    /// Which maintainer action the user picked:
    /// `"create_new"` | `"update_existing"` | `"reject"`. Stored as
    /// a loose string rather than a typed enum so the wire format
    /// aligns with the TS `MaintainAction` union in
    /// `apps/desktop-shell/src/features/ingest/types.ts`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maintain_action: Option<String>,

    /// User-provided reason for a `Reject` action. Surfaced in the
    /// Inbox detail pane and written verbatim into `wiki/log.md`
    /// on the reject path so rejections are auditable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejection_reason: Option<String>,

    // ── W2 Proposal / Apply two-phase additions ───────────────────
    //
    // W2 turns `update_existing` from a deterministic append into a
    // proposal → review → apply workflow. The maintainer first asks
    // the LLM to merge the raw body into the existing page (producing
    // `proposed_after_markdown`), the user reviews the diff, and a
    // separate `apply` call commits the merge to disk. The four
    // fields below persist the proposal state between the two HTTP
    // calls and give the UI enough context to render a diff preview.
    //
    // All four are `Option<...>` with `#[serde(default)]` so pre-W2
    // inbox.json files deserialize cleanly with `None` everywhere,
    // and `skip_serializing_if = "Option::is_none"` keeps the
    // on-disk JSON byte-identical for untouched entries.

    /// Proposal lifecycle marker:
    ///   * `None` — no proposal has ever been generated for this entry.
    ///   * `Some("pending")` — `propose_update` has run, user has not
    ///     yet applied or cancelled.
    ///   * `Some("applied")` — `apply_update_proposal` succeeded; the
    ///     after-markdown has been written to disk.
    ///   * `Some("cancelled")` — user backed out of the proposal.
    ///     The entry returns to pending and can be re-proposed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal_status: Option<String>,

    /// The merged markdown the LLM produced for the target page.
    /// Populated by `propose_update`, consumed by `apply_update_proposal`,
    /// cleared by `cancel_update_proposal` or after a successful apply
    /// (we keep summary instead).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_after_markdown: Option<String>,

    /// Snapshot of the target page's markdown body at the moment
    /// `propose_update` was called. Stored so `apply_update_proposal`
    /// can detect concurrent edits: if the current page no longer
    /// matches this snapshot, the apply fails with a conflict error
    /// rather than silently overwriting another party's changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_markdown_snapshot: Option<String>,

    /// One-line human-readable description of what the LLM changed.
    /// Surfaced in the Workbench and the audit log. Retained after
    /// apply (unlike the full markdown) so history queries can show
    /// what an applied proposal did without storing the whole page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal_summary: Option<String>,
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
    append_inbox_pending_locked(paths, kind, title, description, source_raw_id)
}

/// Internal helper: same as `append_inbox_pending`, but assumes the
/// caller already holds `INBOX_WRITE_GUARD`. Used by
/// `append_new_raw_task` so its dedupe check (read) and the append
/// (write) happen in a single critical section — without this the
/// two ops would race across the guard boundary (TOCTOU), letting a
/// concurrent caller slip a duplicate NewRaw entry between them.
fn append_inbox_pending_locked(
    paths: &WikiPaths,
    kind: InboxKind,
    title: &str,
    description: &str,
    source_raw_id: Option<u32>,
) -> Result<InboxEntry> {
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
        // W1 Maintainer Workbench additions — start unset and let
        // `execute_maintain` populate them when the user picks an
        // action. skip_serializing_if keeps these invisible in the
        // on-disk JSON while still None.
        proposed_wiki_slug: None,
        target_page_slug: None,
        maintain_action: None,
        rejection_reason: None,
        // W2 proposal/apply additions — unset until `propose_update`
        // fires. All four fields travel together through the two-phase
        // lifecycle (pending → applied | cancelled).
        proposal_status: None,
        proposed_after_markdown: None,
        before_markdown_snapshot: None,
        proposal_summary: None,
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

/// W1 Maintainer Workbench: atomically update the maintain-action
/// bookkeeping fields on an inbox entry in a single critical section.
///
/// Writes (all optional, `None` leaves the existing value untouched):
///   * `status` — always applied; callers stamp `Approved` for
///     create/update and `Rejected` for reject.
///   * `maintain_action` — `"create_new"` | `"update_existing"` |
///     `"reject"`.
///   * `proposed_wiki_slug` — the slug the LLM suggested (create path).
///   * `target_page_slug` — the slug that was actually written to.
///   * `rejection_reason` — human-entered reason for the reject path.
///
/// Also stamps `resolved_at` (mirrors `resolve_inbox_entry`). Thread-
/// safe via [`INBOX_WRITE_GUARD`]. Returns `NotFound` if `id` is stale.
///
/// Why a dedicated helper instead of extending `resolve_inbox_entry`:
/// the W0 resolve path takes a bare `"approve"` / `"reject"` string,
/// and its signature is public API used by `POST /api/wiki/inbox/{id}
/// /resolve`. Keeping the W1 field writes on a separate function lets
/// the existing resolve endpoint stay byte-for-byte compatible with
/// clients that haven't shipped the Workbench yet.
pub fn update_inbox_maintain(
    paths: &WikiPaths,
    id: u32,
    status: InboxStatus,
    maintain_action: Option<String>,
    proposed_wiki_slug: Option<String>,
    target_page_slug: Option<String>,
    rejection_reason: Option<String>,
) -> Result<InboxEntry> {
    let _guard = lock_inbox_writes();
    let mut entries = load_inbox_file(paths)?;
    let found = entries
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or(WikiStoreError::NotFound(id))?;
    found.status = status;
    found.resolved_at = Some(now_iso8601());
    if let Some(action) = maintain_action {
        found.maintain_action = Some(action);
    }
    if let Some(slug) = proposed_wiki_slug {
        found.proposed_wiki_slug = Some(slug);
    }
    if let Some(slug) = target_page_slug {
        found.target_page_slug = Some(slug);
    }
    if let Some(reason) = rejection_reason {
        found.rejection_reason = Some(reason);
    }
    let updated = found.clone();
    save_inbox_file(paths, &entries)?;
    Ok(updated)
}

/// W2 Proposal/Apply: atomically patch the W2 proposal fields on an
/// inbox entry. Exists because `update_inbox_maintain` is reserved
/// for the "resolve" moment (stamps `resolved_at`, flips status) and
/// the propose step happens BEFORE resolution — the entry is still
/// pending. Separating the two helpers keeps each path auditable.
///
/// Semantics of each `Option`:
///   * `status` — if `Some`, overwrite the current status. `None`
///     leaves the status untouched (e.g. propose keeps it `Pending`).
///     Pass `Some(Approved)` on apply.
///   * `proposal_status` — overwrite / clear depending on
///     `ClearableOption` below.
///   * Every other field uses the same take-or-clear mechanism.
///
/// `resolved_at` is only stamped when `status == Some(Approved)`
/// (the apply path) so propose/cancel don't mark the entry resolved.
///
/// Thread-safe via [`INBOX_WRITE_GUARD`]. Returns `NotFound` if `id`
/// is stale.
pub fn update_inbox_proposal(
    paths: &WikiPaths,
    id: u32,
    patch: InboxProposalPatch,
) -> Result<InboxEntry> {
    let _guard = lock_inbox_writes();
    let mut entries = load_inbox_file(paths)?;
    let found = entries
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or(WikiStoreError::NotFound(id))?;

    if let Some(status) = patch.status {
        if matches!(status, InboxStatus::Approved | InboxStatus::Rejected) {
            found.resolved_at = Some(now_iso8601());
        }
        found.status = status;
    }
    patch.proposal_status.apply(&mut found.proposal_status);
    patch
        .proposed_after_markdown
        .apply(&mut found.proposed_after_markdown);
    patch
        .before_markdown_snapshot
        .apply(&mut found.before_markdown_snapshot);
    patch.proposal_summary.apply(&mut found.proposal_summary);
    patch.maintain_action.apply(&mut found.maintain_action);
    patch.target_page_slug.apply(&mut found.target_page_slug);

    let updated = found.clone();
    save_inbox_file(paths, &entries)?;
    Ok(updated)
}

/// Patch payload for [`update_inbox_proposal`]. Each field models
/// a "no-op / set-to-value / clear" choice via [`ClearableOption`],
/// so callers can distinguish "don't touch" from "overwrite with
/// None" — important because the proposal lifecycle has both.
#[derive(Debug, Default, Clone)]
pub struct InboxProposalPatch {
    pub status: Option<InboxStatus>,
    pub proposal_status: ClearableOption<String>,
    pub proposed_after_markdown: ClearableOption<String>,
    pub before_markdown_snapshot: ClearableOption<String>,
    pub proposal_summary: ClearableOption<String>,
    /// Optional W1 bookkeeping stamps. The propose step writes
    /// `maintain_action="update_existing"` + `target_page_slug` so
    /// a page refresh mid-proposal doesn't lose the user's choice.
    /// `Keep` leaves them untouched (the apply path uses this).
    pub maintain_action: ClearableOption<String>,
    pub target_page_slug: ClearableOption<String>,
}

/// Three-valued patch primitive: leave the target unchanged, set it
/// to a new value, or clear it back to `None`. Used in
/// [`InboxProposalPatch`] so callers can e.g. clear the stored
/// `proposed_after_markdown` on apply while leaving
/// `proposal_summary` intact — a plain `Option` can't express that.
#[derive(Debug, Default, Clone)]
pub enum ClearableOption<T> {
    /// Don't touch the field (default).
    #[default]
    Keep,
    /// Overwrite the field with `Some(value)`.
    Set(T),
    /// Overwrite the field with `None`.
    Clear,
}

impl<T: Clone> ClearableOption<T> {
    /// Apply this patch primitive to the target `Option<T>`.
    pub fn apply(&self, target: &mut Option<T>) {
        match self {
            Self::Keep => {}
            Self::Set(v) => *target = Some(v.clone()),
            Self::Clear => *target = None,
        }
    }
}

/// Strip markdown noise so the leftover text is human-readable for
/// titles / Inbox previews. Drops:
///   - leading `#` heading markers
///   - `_italic underscores_` wrapping single-line metadata
///   - inline image markdown `![alt](url)` (whole construct gone)
///   - inline link markdown `[label](url)` → keeps `label`
///   - leading bullet markers (`-`, `*`, `+`)
///   - tab/multi-space runs collapsed to one space
///
/// Intentionally regex-free (we don't pull `regex` into wiki_store
/// for one helper) — a single linear pass over the bytes is plenty
/// for the < 200 char strings these previews care about.
fn strip_markdown_noise(line: &str) -> String {
    let trimmed = line.trim();
    // Strip leading heading hashes + bullet markers.
    let after_marker = trimmed
        .trim_start_matches(|c: char| c == '#' || c == '-' || c == '*' || c == '+' || c == '>' )
        .trim_start();
    // Strip `_..._` italics wrapping metadata lines (e.g. `_Author: x_`).
    let body = if after_marker.starts_with('_') && after_marker.ends_with('_') && after_marker.len() > 1 {
        &after_marker[1..after_marker.len() - 1]
    } else {
        after_marker
    };

    // UTF-8-safe scanner: copy non-marker bytes through verbatim and
    // only special-case ASCII brackets / parens (all 1 byte). The
    // original input is valid UTF-8 and we only ever skip whole
    // ASCII-bounded constructs, so multi-byte CJK survives intact.
    let bytes = body.as_bytes();
    let mut buf: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        // Image: `![alt](url)` — drop the whole thing.
        if bytes[i] == b'!' && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            if let Some(close_bracket) = find_byte_from(bytes, b']', i + 2) {
                if close_bracket + 1 < bytes.len() && bytes[close_bracket + 1] == b'(' {
                    if let Some(close_paren) = find_byte_from(bytes, b')', close_bracket + 2) {
                        i = close_paren + 1;
                        continue;
                    }
                }
            }
        }
        // Link: `[label](url)` — keep the label only.
        if bytes[i] == b'[' {
            if let Some(close_bracket) = find_byte_from(bytes, b']', i + 1) {
                if close_bracket + 1 < bytes.len() && bytes[close_bracket + 1] == b'(' {
                    if let Some(close_paren) = find_byte_from(bytes, b')', close_bracket + 2) {
                        buf.extend_from_slice(&bytes[i + 1..close_bracket]);
                        i = close_paren + 1;
                        continue;
                    }
                }
            }
        }
        buf.push(bytes[i]);
        i += 1;
    }
    let out = String::from_utf8(buf)
        .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());

    // Squash runs of whitespace.
    let mut squashed = String::with_capacity(out.len());
    let mut prev_space = false;
    for ch in out.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                squashed.push(' ');
            }
            prev_space = true;
        } else {
            squashed.push(ch);
            prev_space = false;
        }
    }
    squashed.trim().to_string()
}

/// First line of `body` that yields ≥ `min_chars` of human-readable
/// text after `strip_markdown_noise`. Falls back to the first
/// non-empty line if nothing else qualifies, then to a hard-cut of
/// the whole body. Returns at most `max_chars` characters.
///
/// Italics-wrapped metadata lines like `_Author: 新智元_` or
/// `_Published: 2026-04-15_` are skipped during the primary pass so
/// they don't crowd out actual content as the title — they're still
/// usable via the fallback if nothing better exists.
fn extract_readable_title(body: &str, max_chars: usize) -> String {
    const MIN_USEFUL_CHARS: usize = 5;
    let mut fallback: Option<String> = None;
    for line in body.lines() {
        let raw_trim = line.trim();
        // Italics-wrapped key: value metadata → skip in primary pass.
        let is_metadata_line = raw_trim.starts_with('_')
            && raw_trim.ends_with('_')
            && raw_trim.len() > 2
            && raw_trim[1..raw_trim.len() - 1].contains(": ");

        let cleaned = strip_markdown_noise(line);
        if cleaned.is_empty() {
            continue;
        }
        if is_metadata_line {
            // Save as fallback in case nothing else qualifies.
            if fallback.is_none() {
                fallback = Some(cleaned);
            }
            continue;
        }
        if cleaned.chars().count() >= MIN_USEFUL_CHARS {
            return cleaned.chars().take(max_chars).collect();
        }
        if fallback.is_none() {
            fallback = Some(cleaned);
        }
    }
    if let Some(f) = fallback {
        return f.chars().take(max_chars).collect();
    }
    body.chars().take(max_chars).collect::<String>().trim().to_string()
}

#[inline]
fn find_byte_from(haystack: &[u8], needle: u8, from: usize) -> Option<usize> {
    if from >= haystack.len() {
        return None;
    }
    haystack[from..].iter().position(|&b| b == needle).map(|p| p + from)
}

/// Convenience wrapper for the two callers that currently append a
/// `NewRaw` inbox task after a raw entry lands (desktop-server's
/// HTTP ingest handler and wechat_ilink's on_message hook). Review
/// nit #15: consolidates the title/description formatting so schema
/// changes (e.g. adding an origin tag) don't need to be made in two
/// call sites in lockstep.
///
/// Title / description are extracted with `extract_readable_title`,
/// which strips markdown noise (image stubs, heading hashes, link
/// wrappers) so the Inbox doesn't surface garbage like
/// `### ![](https://mmbiz.qpic.cn/mmbiz_png/...)`. Regression from
/// the 2026-04 mp.weixin pipeline.
///
/// `origin` is a short, human-readable tag such as `"paste"` or
/// `"WeChat user abcd1234"` that lands inside the description.
///
/// Idempotent on `source_raw_id`: if a **Pending** `NewRaw` inbox
/// entry already references `entry.id`, this returns the existing
/// entry untouched instead of creating a duplicate. Scope of the
/// check is deliberately narrow — same raw id, same kind, pending
/// status. It does NOT dedupe across `source_url` (Raw Library
/// manual re-ingest produces a new `entry.id`, so those always get
/// a fresh Inbox task) and does NOT suppress when the prior task
/// was already resolved (a re-ingest should surface again). See
/// Explorer B's 2026-04 scope note: "只在已经有相同 source_raw_id
/// 的 pending inbox 时才跳过".
pub fn append_new_raw_task(
    paths: &WikiPaths,
    entry: &RawEntry,
    origin: &str,
) -> Result<InboxEntry> {
    // Hold the inbox write guard across the dedupe read and the
    // subsequent append so a concurrent caller can't slip a duplicate
    // between the check and the write.
    let _guard = lock_inbox_writes();

    // Dedupe: if a pending NewRaw for this raw id already exists,
    // return it without appending. Intentionally no-op rather than an
    // error — callers treat Ok as "inbox state is consistent".
    let existing = load_inbox_file(paths)?;
    if let Some(prior) = existing.iter().find(|e| {
        e.kind == InboxKind::NewRaw
            && e.status == InboxStatus::Pending
            && e.source_raw_id == Some(entry.id)
    }) {
        return Ok(prior.clone());
    }

    let (display_title, description) = match read_raw_entry(paths, entry.id) {
        Ok((_entry, body)) => {
            // Pull a clean title from the body (skips images / metadata).
            let title = {
                let extracted = extract_readable_title(&body, 60);
                if extracted.is_empty() {
                    format!("素材 #{:05}", entry.id)
                } else {
                    extracted
                }
            };
            // Description: first 150 chars of clean text. Use the full
            // body so we can pick up text past metadata blocks.
            let preview = extract_readable_title(&body, 150);
            let desc = if preview.is_empty() {
                format!("来源：{origin}。建议操作：总结为概念知识页面。")
            } else {
                format!("{preview}...")
            };
            (title, desc)
        }
        Err(_) => (
            format!("素材 #{:05}", entry.id),
            format!("来源：{origin}。建议操作：总结为概念知识页面。"),
        ),
    };
    let inbox_entry = append_inbox_pending_locked(
        paths,
        InboxKind::NewRaw,
        &display_title,
        &description,
        Some(entry.id),
    )?;

    // P1 provenance: fire `inbox_appended` after a successful append
    // under INBOX_WRITE_GUARD. `fire_event` is soft-fail so a
    // lineage.jsonl failure never rolls back the inbox write.
    // Upstream = the raw entry that caused this task; downstream =
    // the new inbox id. `display_title` mirrors the inbox title so
    // the UI timeline reads naturally.
    provenance::fire_event(
        paths,
        provenance::LineageEvent {
            event_id: provenance::new_event_id(),
            event_type: provenance::LineageEventType::InboxAppended,
            timestamp_ms: provenance::now_unix_ms(),
            upstream: vec![provenance::LineageRef::Raw { id: entry.id }],
            downstream: vec![provenance::LineageRef::Inbox {
                id: inbox_entry.id,
            }],
            display_title: provenance::display_title_inbox_appended(&inbox_entry.title),
            metadata: serde_json::json!({
                "origin": origin,
                "kind": "new_raw",
            }),
        },
    );

    Ok(inbox_entry)
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

/// Scan existing concept pages for mentions of a newly-written page
/// and append `Stale` inbox tasks for each affected page. This is
/// the MVP half of canonical §8 Triggers row 2: "update affected
/// concept/people/topic/compare pages". The actual LLM re-write of
/// the affected page is a future feat — this function only creates
/// the Inbox notification so the user can see "these pages might
/// need updating".
///
/// `new_slug` and `new_title` identify the page that was just written.
/// The function scans every OTHER concept page's body (case-insensitive)
/// for occurrences of `new_slug` or `new_title`. If found, that page
/// gets a `Stale` inbox entry.
///
/// Returns the number of affected pages found (0 is normal for the
/// first page in a fresh wiki).
pub fn notify_affected_pages(
    paths: &WikiPaths,
    new_slug: &str,
    new_title: &str,
) -> Result<usize> {
    let new_slug_lc = new_slug.to_lowercase();
    let new_title_lc = new_title.to_lowercase();
    if new_slug_lc.is_empty() && new_title_lc.is_empty() {
        return Ok(0);
    }

    let concepts_dir = paths.wiki.join(WIKI_CONCEPTS_SUBDIR);
    if !concepts_dir.is_dir() {
        return Ok(0);
    }

    let mut count = 0usize;
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
        // Skip the newly-written page itself.
        if slug.to_lowercase() == new_slug_lc {
            continue;
        }
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let body = strip_frontmatter(&content);
        let body_lc = body.to_lowercase();

        let mentioned = (!new_slug_lc.is_empty() && body_lc.contains(&new_slug_lc))
            || (!new_title_lc.is_empty() && body_lc.contains(&new_title_lc));

        if mentioned {
            let title = format!("Affected: `{slug}` mentions new page `{new_slug}`");
            let description = format!(
                "Page `{slug}` mentions `{new_slug}` (or its title `{new_title}`). \
                 The new page may introduce information that this page should \
                 incorporate. Consider re-running the maintainer on this page."
            );
            // Soft-fail: if the inbox append fails, we log but don't
            // block the caller. Losing a notification is recoverable.
            if let Err(e) =
                append_inbox_pending(paths, InboxKind::Stale, &title, &description, None)
            {
                eprintln!(
                    "[warn] notify_affected_pages: inbox append for `{slug}` failed: {e}"
                );
            } else {
                count += 1;
            }
        }
    }
    Ok(count)
}

/// Hand-rolled UTC ISO-8601 timestamp formatter, second precision.
/// We use `std::time` rather than pulling `chrono` for one function.
/// Made `pub` in v2 so downstream crates (`wiki_maintainer`, etc.)
/// can stamp `AbsorbLogEntry.timestamp` with the same format.
pub fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_iso8601(secs)
}

/// Return today's date as `YYYY-MM-DD` in UTC. Used by `wiki_stats`
/// to filter "today's" ingested entries.
pub fn today_date_string() -> String {
    now_iso8601()[..10].to_string()
}

/// Return the date 7 days ago as `YYYY-MM-DD` in UTC. Used by
/// `wiki_stats` to compute weekly metrics.
fn week_ago_date_string() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let week_ago = secs.saturating_sub(7 * 86_400);
    format_iso8601(week_ago)[..10].to_string()
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

// ─────────────────────────────────────────────────────────────────────
// v2: Absorb log persistence  (technical-design.md §3.5, §4.1.1)
// ─────────────────────────────────────────────────────────────────────

/// Filename for the absorb log inside `{meta}/`.
const ABSORB_LOG_FILENAME: &str = "_absorb_log.json";

/// Record of a single absorb operation result.
/// Persisted to `{meta}/_absorb_log.json` as a JSON array.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AbsorbLogEntry {
    pub entry_id: u32,
    pub timestamp: String,
    pub action: String,
    pub page_slug: Option<String>,
    pub page_title: Option<String>,
    pub page_category: Option<String>,
}

fn absorb_log_path(paths: &WikiPaths) -> PathBuf {
    paths.meta.join(ABSORB_LOG_FILENAME)
}

fn load_absorb_log_file(paths: &WikiPaths) -> Result<Vec<AbsorbLogEntry>> {
    let path = absorb_log_path(paths);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let bytes = fs::read(&path).map_err(|e| WikiStoreError::io(path.clone(), e))?;
    let parsed: Vec<AbsorbLogEntry> = serde_json::from_slice(&bytes)
        .map_err(|e| WikiStoreError::AbsorbLogCorrupted(format!("{e}")))?;
    Ok(parsed)
}

fn save_absorb_log_file(paths: &WikiPaths, entries: &[AbsorbLogEntry]) -> Result<()> {
    fs::create_dir_all(&paths.meta).map_err(|e| WikiStoreError::io(paths.meta.clone(), e))?;
    let bytes = serde_json::to_vec_pretty(entries)
        .map_err(|e| WikiStoreError::AbsorbLogCorrupted(format!("serialize: {e}")))?;
    let path = absorb_log_path(paths);
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &bytes).map_err(|e| WikiStoreError::io(tmp.clone(), e))?;
    fs::rename(&tmp, &path).map_err(|e| WikiStoreError::io(path.clone(), e))?;
    Ok(())
}

/// Append one absorb log entry to `{meta}/_absorb_log.json`.
/// Atomic write: load → append → tmp + rename.
/// Thread-safe: serialized through [`ABSORB_LOG_GUARD`].
pub fn append_absorb_log(
    paths: &WikiPaths,
    entry: AbsorbLogEntry,
) -> Result<()> {
    let _guard = lock_absorb_log_writes();
    let mut entries = load_absorb_log_file(paths)?;
    entries.push(entry);
    save_absorb_log_file(paths, &entries)
}

/// Read the complete absorb log, ordered by timestamp descending.
/// Returns an empty `Vec` if the file does not exist (not an error).
pub fn list_absorb_log(paths: &WikiPaths) -> Result<Vec<AbsorbLogEntry>> {
    let mut entries = load_absorb_log_file(paths)?;
    entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(entries)
}

/// Check whether a raw entry has already been absorbed (action != "skip").
/// Linear scan of `_absorb_log.json`.
pub fn is_entry_absorbed(paths: &WikiPaths, entry_id: u32) -> bool {
    let entries = load_absorb_log_file(paths).unwrap_or_default();
    entries
        .iter()
        .any(|e| e.entry_id == entry_id && e.action != "skip")
}

// ─────────────────────────────────────────────────────────────────────
// v2: Backlinks index  (technical-design.md §3.6, §4.1.1)
// ─────────────────────────────────────────────────────────────────────

/// Filename for the backlinks index inside `{meta}/`.
const BACKLINKS_INDEX_FILENAME: &str = "_backlinks.json";

/// Reverse-link index: key = target slug, value = list of slugs
/// that reference the target. Values are deduplicated and sorted
/// alphabetically.
pub type BacklinksIndex = HashMap<String, Vec<String>>;

fn backlinks_index_path(paths: &WikiPaths) -> PathBuf {
    paths.meta.join(BACKLINKS_INDEX_FILENAME)
}

/// Rebuild the complete backlinks index from disk.
/// Walks all wiki pages, calls [`extract_internal_links`] on each,
/// and collects `target_slug → [referring_slugs]` mappings.
/// The return value is **not** persisted — the caller decides
/// whether to call [`save_backlinks_index`].
pub fn build_backlinks_index(paths: &WikiPaths) -> Result<BacklinksIndex> {
    let all_pages = list_all_wiki_pages(paths)?;
    let mut index: BacklinksIndex = HashMap::new();

    for page in &all_pages {
        let body = match read_wiki_page(paths, &page.slug) {
            Ok((_summary, body)) => body,
            Err(_) => continue,
        };
        let outgoing = extract_internal_links(&body);
        for target_slug in outgoing {
            // Skip self-links.
            if target_slug == page.slug {
                continue;
            }
            index
                .entry(target_slug)
                .or_default()
                .push(page.slug.clone());
        }
    }

    // Deduplicate and sort each value list alphabetically.
    for refs in index.values_mut() {
        refs.sort();
        refs.dedup();
    }

    // Per §3.6: "无入链的页面不出现在 index 中" — we don't insert
    // empty vecs, so this is satisfied by construction.

    Ok(index)
}

/// Persist the backlinks index to `{meta}/_backlinks.json`.
/// Atomic write: tmp + rename.
pub fn save_backlinks_index(
    paths: &WikiPaths,
    index: &BacklinksIndex,
) -> Result<()> {
    fs::create_dir_all(&paths.meta).map_err(|e| WikiStoreError::io(paths.meta.clone(), e))?;
    let bytes = serde_json::to_vec_pretty(index)
        .map_err(|e| WikiStoreError::BacklinksCorrupted(format!("serialize: {e}")))?;
    let path = backlinks_index_path(paths);
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &bytes).map_err(|e| WikiStoreError::io(tmp.clone(), e))?;
    fs::rename(&tmp, &path).map_err(|e| WikiStoreError::io(path.clone(), e))?;
    Ok(())
}

/// Load a previously persisted backlinks index.
/// Returns an empty `HashMap` if the file does not exist.
pub fn load_backlinks_index(paths: &WikiPaths) -> Result<BacklinksIndex> {
    let path = backlinks_index_path(paths);
    if !path.is_file() {
        return Ok(HashMap::new());
    }
    let bytes = fs::read(&path).map_err(|e| WikiStoreError::io(path.clone(), e))?;
    let parsed: BacklinksIndex = serde_json::from_slice(&bytes)
        .map_err(|e| WikiStoreError::BacklinksCorrupted(format!("{e}")))?;
    Ok(parsed)
}

// ─────────────────────────────────────────────────────────────────────
// v2: Schema validation  (technical-design.md §3.7, §4.1.1–4.1.2)
// ─────────────────────────────────────────────────────────────────────

/// Schema template for wiki page frontmatter validation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchemaTemplate {
    pub name: String,
    pub fields: Vec<TemplateField>,
    pub required_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TemplateField {
    pub name: String,
    pub field_type: FieldType,
    pub description: String,
    pub default_value: Option<serde_json::Value>,
    pub validation: Option<FieldValidation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FieldType {
    String,
    Number,
    Boolean,
    StringList,
    Enum(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FieldValidation {
    pub max_length: Option<usize>,
    pub min_length: Option<usize>,
    pub pattern: Option<String>,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
}

/// A single frontmatter validation error produced by [`validate_frontmatter`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub field: String,
    pub expected: String,
    pub actual: String,
    pub message: String,
}

/// Validate wiki page frontmatter against a [`SchemaTemplate`].
/// Returns all violations; an empty list means fully compliant.
pub fn validate_frontmatter(
    content: &str,
    template: &SchemaTemplate,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    // Extract YAML frontmatter block between first pair of `---` lines.
    let fm_text = match extract_frontmatter_text(content) {
        Some(text) => text,
        None => {
            errors.push(ValidationError {
                field: "(frontmatter)".into(),
                expected: "YAML frontmatter block".into(),
                actual: "missing".into(),
                message: "页面缺少 YAML frontmatter 块".into(),
            });
            return errors;
        }
    };

    // Parse YAML as a loose key-value map.
    let fm_map: HashMap<String, serde_json::Value> = match serde_json::from_str(&yaml_to_json_loose(&fm_text)) {
        Ok(map) => map,
        Err(_) => {
            // Fallback: try to parse line-by-line as "key: value" pairs.
            parse_frontmatter_loose(&fm_text)
        }
    };

    // Check required fields.
    for required in &template.required_fields {
        if !fm_map.contains_key(required) {
            errors.push(ValidationError {
                field: required.clone(),
                expected: "present".into(),
                actual: "missing".into(),
                message: format!("必填字段 `{required}` 缺失"),
            });
        }
    }

    // Validate each field that is both defined in the template and present in the page.
    for tf in &template.fields {
        if let Some(value) = fm_map.get(&tf.name) {
            // Type check.
            let type_ok = match &tf.field_type {
                FieldType::String => value.is_string(),
                FieldType::Number => value.is_number(),
                FieldType::Boolean => value.is_boolean(),
                FieldType::StringList => value.is_array(),
                FieldType::Enum(allowed) => {
                    value.as_str().map(|s| allowed.contains(&s.to_string())).unwrap_or(false)
                }
            };
            if !type_ok {
                errors.push(ValidationError {
                    field: tf.name.clone(),
                    expected: format!("{:?}", tf.field_type),
                    actual: format!("{value}"),
                    message: format!("字段 `{}` 类型不匹配", tf.name),
                });
            }

            // Length validation for strings.
            if let (Some(validation), Some(s)) = (&tf.validation, value.as_str()) {
                if let Some(max) = validation.max_length {
                    if s.len() > max {
                        errors.push(ValidationError {
                            field: tf.name.clone(),
                            expected: format!("max_length={max}"),
                            actual: format!("length={}", s.len()),
                            message: format!("字段 `{}` 超过最大长度 {max}", tf.name),
                        });
                    }
                }
                if let Some(min) = validation.min_length {
                    if s.len() < min {
                        errors.push(ValidationError {
                            field: tf.name.clone(),
                            expected: format!("min_length={min}"),
                            actual: format!("length={}", s.len()),
                            message: format!("字段 `{}` 低于最小长度 {min}", tf.name),
                        });
                    }
                }
            }
        }
    }

    errors
}

/// Extract the YAML text between the first pair of `---` delimiters.
fn extract_frontmatter_text(content: &str) -> Option<String> {
    let mut lines = content.lines();
    if lines.next()? != "---" {
        return None;
    }
    let mut fm_lines = Vec::new();
    for line in lines {
        if line == "---" {
            return Some(fm_lines.join("\n"));
        }
        fm_lines.push(line);
    }
    None
}

/// Minimal YAML-to-JSON conversion for simple `key: value` frontmatter.
/// Handles strings (quoted or unquoted), numbers, booleans, and lists.
fn yaml_to_json_loose(yaml: &str) -> String {
    let mut pairs = Vec::new();
    for line in yaml.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, val)) = line.split_once(':') {
            let key = key.trim().trim_matches('"');
            let val = val.trim();
            let json_val = if val.starts_with('"') && val.ends_with('"') {
                val.to_string()
            } else if val.starts_with('[') {
                val.to_string()
            } else if val == "true" || val == "false" {
                val.to_string()
            } else if val.parse::<f64>().is_ok() {
                val.to_string()
            } else {
                format!("\"{}\"", val.replace('"', "\\\""))
            };
            pairs.push(format!("\"{key}\": {json_val}"));
        }
    }
    format!("{{{}}}", pairs.join(", "))
}

/// Fallback line-by-line parser for YAML frontmatter.
fn parse_frontmatter_loose(yaml: &str) -> HashMap<String, serde_json::Value> {
    let mut map = HashMap::new();
    for line in yaml.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, val)) = line.split_once(':') {
            let key = key.trim().to_string();
            let val = val.trim();
            let json_val = if val.starts_with('"') && val.ends_with('"') {
                serde_json::Value::String(val[1..val.len()-1].to_string())
            } else if val == "true" {
                serde_json::Value::Bool(true)
            } else if val == "false" {
                serde_json::Value::Bool(false)
            } else if let Ok(n) = val.parse::<i64>() {
                serde_json::Value::Number(n.into())
            } else if let Ok(n) = val.parse::<f64>() {
                serde_json::json!(n)
            } else {
                serde_json::Value::String(val.to_string())
            };
            map.insert(key, json_val);
        }
    }
    map
}

// ─────────────────────────────────────────────────────────────────────
// v2: Schema template API info (technical-design.md §2.9)
//
// `SchemaTemplate` above is the *validation* domain object consumed by
// `validate_frontmatter`. The API surface (`GET /api/wiki/schema/templates`)
// needs a richer, human-facing shape — category, Chinese display name,
// body-writing hint, on-disk path. Keeping these concerns in separate
// types lets the validator stay lean while the API stays descriptive.
// ─────────────────────────────────────────────────────────────────────

/// API-facing schema template metadata for `GET /api/wiki/schema/templates`.
/// See technical-design.md §2.9.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchemaTemplateInfo {
    pub category: String,
    pub display_name: String,
    pub fields: Vec<TemplateFieldInfo>,
    pub body_hint: String,
    pub file_path: String,
}

/// Lightweight field descriptor in the API response. Every frontmatter
/// key declared in the template is treated as required (= `true`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TemplateFieldInfo {
    pub name: String,
    pub required: bool,
    pub field_type: String,
    pub description: String,
}

/// Chinese display label for a built-in template category.
/// Unknown categories fall back to the raw category string.
fn template_display_name(category: &str) -> String {
    match category {
        "concept" => "概念".to_string(),
        "people" => "人物".to_string(),
        "topic" => "主题".to_string(),
        "compare" => "对比".to_string(),
        other => other.to_string(),
    }
}

/// Scan `schema/templates/*.md` and return rich `SchemaTemplateInfo` values.
/// Returns an empty vec if the directory does not exist.
///
/// Parsing strategy:
///   - Frontmatter keys → `fields` (all marked `required: true`, `field_type: "String"`).
///   - Body after the closing `---` → `body_hint` (trimmed).
///   - File stem → `category`; mapped via `template_display_name`.
///
/// Results are sorted alphabetically by category for stable API output.
pub fn load_schema_template_infos(paths: &WikiPaths) -> Result<Vec<SchemaTemplateInfo>> {
    let templates_dir = paths.schema.join("templates");
    if !templates_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let dir = fs::read_dir(&templates_dir)
        .map_err(|e| WikiStoreError::io(templates_dir.clone(), e))?;
    for entry in dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let category = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let content = fs::read_to_string(&path)
            .map_err(|e| WikiStoreError::io(path.clone(), e))?;
        let fields = parse_template_field_infos(&content);
        let body_hint = extract_template_body_hint(&content);
        out.push(SchemaTemplateInfo {
            category: category.clone(),
            display_name: template_display_name(&category),
            fields,
            body_hint,
            file_path: path.to_string_lossy().to_string(),
        });
    }
    out.sort_by(|a, b| a.category.cmp(&b.category));
    Ok(out)
}

/// Convert YAML frontmatter keys into `TemplateFieldInfo` entries.
fn parse_template_field_infos(content: &str) -> Vec<TemplateFieldInfo> {
    let fm_text = match extract_frontmatter_text(content) {
        Some(s) => s,
        None => return Vec::new(),
    };
    let mut fields = Vec::new();
    for line in fm_text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, _)) = line.split_once(':') {
            let key = key.trim().trim_matches('"').to_string();
            if !key.is_empty() {
                fields.push(TemplateFieldInfo {
                    name: key,
                    required: true,
                    field_type: "String".to_string(),
                    description: String::new(),
                });
            }
        }
    }
    fields
}

/// Return the body content that follows the closing `---` of the frontmatter.
/// Trimmed; empty string if there is no body or frontmatter is malformed.
fn extract_template_body_hint(content: &str) -> String {
    // Reuse the same scanning rule as extract_frontmatter_text: first two
    // `---` lines mark the frontmatter. Body is everything after the
    // closing marker.
    let mut lines = content.lines();
    if lines.next() != Some("---") {
        return content.trim().to_string();
    }
    let mut saw_close = false;
    let mut body_lines = Vec::new();
    for line in lines {
        if !saw_close {
            if line == "---" {
                saw_close = true;
            }
            continue;
        }
        body_lines.push(line);
    }
    if !saw_close {
        return String::new();
    }
    body_lines.join("\n").trim().to_string()
}

// ─────────────────────────────────────────────────────────────────────
// v2: Wiki stats  (technical-design.md §3.9, §4.1.1)
// ─────────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────────
// v2: Patrol types  (technical-design.md §3.8)
// ─────────────────────────────────────────────────────────────────────

/// A single issue found by the wiki patrol system.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PatrolIssue {
    pub kind: PatrolIssueKind,
    pub page_slug: String,
    pub description: String,
    pub suggested_action: String,
}

/// Category of patrol issue.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PatrolIssueKind {
    Orphan,
    Stale,
    SchemaViolation,
    Oversized,
    Stub,
    /// v2: Confidence score has decayed (high confidence + old sources).
    ConfidenceDecay,
    /// v2: Crystallization mechanism check (query results not being captured).
    Uncrystallized,
}

/// Full patrol report aggregating all issues.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatrolReport {
    pub issues: Vec<PatrolIssue>,
    pub summary: PatrolSummary,
    pub checked_at: String,
}

/// Count of issues by category.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatrolSummary {
    pub orphans: usize,
    pub stale: usize,
    pub schema_violations: usize,
    pub oversized: usize,
    pub stubs: usize,
    #[serde(default)]
    pub confidence_decay: usize,
    #[serde(default)]
    pub uncrystallized: usize,
}

/// Record of a superseded claim (technical-design.md §3.3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupersessionRecord {
    pub claim: String,
    pub replaced_by: String,
    pub date: String,
    pub source: String,
}

/// Update the confidence score in a wiki page's frontmatter.
/// Reads the file, modifies the `confidence:` line, writes back atomically.
pub fn update_page_confidence(paths: &WikiPaths, slug: &str, new_confidence: f32) -> Result<()> {
    let (summary, _body) = read_wiki_page(paths, slug)?;
    let page_path = find_page_file(paths, slug, &summary.category);
    let content = std::fs::read_to_string(&page_path)
        .map_err(|e| WikiStoreError::io(page_path.clone(), e))?;
    let verified_at = now_iso8601();

    // Replace or insert confidence in frontmatter.
    let updated = if content.contains("confidence:") {
        // Replace existing confidence line.
        let mut result = String::new();
        for line in content.lines() {
            if line.trim_start().starts_with("confidence:") {
                result.push_str(&format!("confidence: {:.2}", new_confidence));
            } else {
                result.push_str(line);
            }
            result.push('\n');
        }
        result
    } else {
        // Insert confidence before the closing --- of frontmatter.
        content.replacen(
            "\n---\n",
            &format!("\nconfidence: {:.2}\n---\n", new_confidence),
            1,
        )
    };
    let updated = if updated.contains("last_verified:") {
        let mut result = String::new();
        for line in updated.lines() {
            if line.trim_start().starts_with("last_verified:") {
                result.push_str(&format!("last_verified: {verified_at}"));
            } else {
                result.push_str(line);
            }
            result.push('\n');
        }
        result
    } else {
        updated.replacen(
            "\n---\n",
            &format!("\nlast_verified: {verified_at}\n---\n"),
            1,
        )
    };

    let tmp = page_path.with_extension("md.tmp");
    std::fs::write(&tmp, &updated).map_err(|e| WikiStoreError::io(tmp.clone(), e))?;
    std::fs::rename(&tmp, &page_path).map_err(|e| WikiStoreError::io(page_path.clone(), e))?;
    Ok(())
}

/// Find the filesystem path for a wiki page given slug and category.
fn find_page_file(paths: &WikiPaths, slug: &str, category: &str) -> std::path::PathBuf {
    let subdir = match category {
        "concept" => WIKI_CONCEPTS_SUBDIR,
        "people" => WIKI_PEOPLE_SUBDIR,
        "topic" => WIKI_TOPICS_SUBDIR,
        "compare" => WIKI_COMPARE_SUBDIR,
        _ => WIKI_CONCEPTS_SUBDIR,
    };
    paths.wiki.join(subdir).join(format!("{slug}.md"))
}

/// Filename for the patrol report inside `{meta}/`.
const PATROL_REPORT_FILENAME: &str = "_patrol_report.json";

/// Save a patrol report to `{meta}/_patrol_report.json`.
pub fn save_patrol_report(paths: &WikiPaths, report: &PatrolReport) -> Result<()> {
    fs::create_dir_all(&paths.meta).map_err(|e| WikiStoreError::io(paths.meta.clone(), e))?;
    let bytes = serde_json::to_vec_pretty(report)
        .map_err(|e| WikiStoreError::Invalid(format!("patrol report serialize: {e}")))?;
    let path = paths.meta.join(PATROL_REPORT_FILENAME);
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &bytes).map_err(|e| WikiStoreError::io(tmp.clone(), e))?;
    fs::rename(&tmp, &path).map_err(|e| WikiStoreError::io(path.clone(), e))?;
    Ok(())
}

/// Load the last saved patrol report. Returns `None` if no report exists.
pub fn load_patrol_report(paths: &WikiPaths) -> Result<Option<PatrolReport>> {
    let path = paths.meta.join(PATROL_REPORT_FILENAME);
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(&path).map_err(|e| WikiStoreError::io(path.clone(), e))?;
    let report: PatrolReport = serde_json::from_slice(&bytes)
        .map_err(|e| WikiStoreError::Invalid(format!("patrol report parse: {e}")))?;
    Ok(Some(report))
}

// ─────────────────────────────────────────────────────────────────────
// v2: Unified orphan detection  (shared by wiki_stats + wiki_patrol)
// ─────────────────────────────────────────────────────────────────────

/// Determine whether a page is an orphan: no inbound links in the
/// backlinks index AND not referenced by `wiki/index.md`.
///
/// Both `wiki_stats().orphan_count` and `wiki_patrol::detect_orphans`
/// must use this predicate so the two APIs never disagree.
pub fn is_page_orphan(
    page: &WikiPageSummary,
    backlinks: &BacklinksIndex,
    index_content: &str,
) -> bool {
    let has_inbound = backlinks
        .get(&page.slug)
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    let in_index = index_content.contains(&format!("{}.md", page.slug));
    !has_inbound && !in_index
}

/// Count how many pages are orphans. Loads backlinks + index once.
pub fn compute_orphan_count(paths: &WikiPaths) -> usize {
    let pages = list_all_wiki_pages(paths).unwrap_or_default();
    let backlinks = load_backlinks_index(paths).unwrap_or_default();
    let index_content =
        std::fs::read_to_string(paths.wiki.join(WIKI_INDEX_FILENAME)).unwrap_or_default();
    pages
        .iter()
        .filter(|p| is_page_orphan(p, &backlinks, &index_content))
        .count()
}

// ─────────────────────────────────────────────────────────────────────
// v2: Wiki stats  (technical-design.md §3.9, §4.1.1)
// ─────────────────────────────────────────────────────────────────────

/// Aggregated wiki statistics, computed on every call (no caching).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiStats {
    pub raw_count: usize,
    pub wiki_count: usize,
    pub concept_count: usize,
    pub people_count: usize,
    pub topic_count: usize,
    pub compare_count: usize,
    pub edge_count: usize,
    pub orphan_count: usize,
    pub inbox_pending: usize,
    pub inbox_resolved: usize,
    pub today_ingest_count: usize,
    pub week_new_pages: usize,
    pub avg_page_words: usize,
    pub absorb_success_rate: f64,
    pub knowledge_velocity: f64,
    pub last_absorb_at: Option<String>,
}

/// Compute and return aggregated wiki statistics.
/// Real-time computation, no caching. Suitable for Dashboard and
/// `/api/wiki/stats`.
pub fn wiki_stats(paths: &WikiPaths) -> Result<WikiStats> {
    let raws = list_raw_entries(paths).unwrap_or_default();
    let pages = list_all_wiki_pages(paths).unwrap_or_default();
    let inbox = load_inbox_file(paths).unwrap_or_default();
    let graph = build_wiki_graph(paths).unwrap_or_else(|_| WikiGraph {
        nodes: Vec::new(),
        edges: Vec::new(),
        raw_count: 0,
        concept_count: 0,
        edge_count: 0,
    });

    let concept_count = pages.iter().filter(|p| p.category == "concept").count();
    let people_count = pages.iter().filter(|p| p.category == "people").count();
    let topic_count = pages.iter().filter(|p| p.category == "topic").count();
    let compare_count = pages.iter().filter(|p| p.category == "compare").count();

    // Orphan count: reuse the unified definition (no inbound links AND
    // not referenced by index.md). Same rule as wiki_patrol::detect_orphans
    // so /api/wiki/stats.orphan_count == /api/wiki/patrol.summary.orphans.
    let orphan_count = compute_orphan_count(paths);

    let inbox_pending = inbox.iter().filter(|e| e.status == InboxStatus::Pending).count();
    let inbox_resolved = inbox
        .iter()
        .filter(|e| e.status == InboxStatus::Approved || e.status == InboxStatus::Rejected)
        .count();

    // Today's ingested raw entries.
    let today = today_date_string();
    let today_ingest_count = raws.iter().filter(|r| r.date == today).count();

    // Wiki pages created in the last 7 days.
    let week_ago = week_ago_date_string();
    let week_new_pages = pages
        .iter()
        .filter(|p| p.created_at.as_str() >= week_ago.as_str())
        .count();

    // Average word count across all wiki pages.
    let avg_page_words = if pages.is_empty() {
        0
    } else {
        let total_words: usize = pages
            .iter()
            .filter_map(|p| {
                read_wiki_page(paths, &p.slug)
                    .ok()
                    .map(|(_, body)| count_words(&body))
            })
            .sum();
        total_words / pages.len()
    };

    // absorb_success_rate = resolved / (resolved + pending)
    let total_inbox = inbox_resolved + inbox_pending;
    let absorb_success_rate = if total_inbox == 0 {
        0.0
    } else {
        inbox_resolved as f64 / total_inbox as f64
    };

    // knowledge_velocity = week_new_pages / 7.0
    let knowledge_velocity = week_new_pages as f64 / 7.0;

    // last_absorb_at: last entry's timestamp from absorb log.
    let absorb_log = load_absorb_log_file(paths).unwrap_or_default();
    let last_absorb_at = absorb_log
        .iter()
        .max_by(|a, b| a.timestamp.cmp(&b.timestamp))
        .map(|e| e.timestamp.clone());

    Ok(WikiStats {
        raw_count: raws.len(),
        wiki_count: pages.len(),
        concept_count,
        people_count,
        topic_count,
        compare_count,
        edge_count: graph.edge_count,
        orphan_count,
        inbox_pending,
        inbox_resolved,
        today_ingest_count,
        week_new_pages,
        avg_page_words,
        absorb_success_rate,
        knowledge_velocity,
        last_absorb_at,
    })
}

/// Count words in a markdown body. For CJK text each character counts
/// as one word; for ASCII, whitespace-split tokens are counted.
/// Count words in a body string. CJK characters count as one word
/// each; ASCII tokens are counted by whitespace boundaries.
/// Mixed tokens (e.g. "abc中") count as: ASCII word part + CJK chars.
fn count_words(body: &str) -> usize {
    let body = strip_frontmatter(body);
    let mut count = 0usize;
    let mut in_ascii_word = false;

    for ch in body.chars() {
        if ch.is_whitespace() {
            in_ascii_word = false;
        } else if ch.is_ascii() {
            // ASCII non-whitespace: count on transition into a word.
            if !in_ascii_word {
                count += 1;
                in_ascii_word = true;
            }
        } else {
            // CJK / non-ASCII: each character = 1 word.
            count += 1;
            in_ascii_word = false;
        }
    }
    count
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

    // ── Windows UAC fallback tests (v2 Phase 4 bugfix) ──────────

    /// When `$CLAWWIKI_HOME` is set to a writable temp path, `default_root`
    /// uses it verbatim and does NOT probe for permission failures.
    #[test]
    fn env_override_wins_over_permission_check() {
        let tmp = tempdir().unwrap();
        let custom = tmp.path().join("my-custom-wiki");

        // Set the env var for this test (restored in Drop).
        struct EnvGuard;
        impl Drop for EnvGuard {
            fn drop(&mut self) {
                std::env::remove_var(ENV_OVERRIDE);
            }
        }
        let _guard = EnvGuard;
        std::env::set_var(ENV_OVERRIDE, &custom);

        let resolved = default_root();
        assert_eq!(resolved, custom);
        // Should NOT have created the directory (env override trusts the caller).
        // Note: caller is expected to init_wiki(&resolved) explicitly.
    }

    /// When the primary path is writable, `default_root` returns it and
    /// does not fall back.
    #[test]
    fn primary_path_succeeds_when_writable() {
        // Use default_root_from (pure) to derive a path, then verify
        // default_root would return the same (assuming no env override
        // and writable home). We simulate by clearing the env var.
        let _ = std::env::remove_var(ENV_OVERRIDE);

        let pure = default_root_from(None, Some(Path::new("/tmp/x")));
        assert_eq!(pure, Path::new("/tmp/x").join(DEFAULT_DIRNAME));
    }

    /// `default_root_from` stays pure — no I/O, no fallback logic.
    #[test]
    fn default_root_from_is_pure_no_fallback() {
        // Even with a "denied" looking path, pure function just joins.
        let result = default_root_from(None, Some(Path::new("/nonexistent/nowhere")));
        assert_eq!(result, Path::new("/nonexistent/nowhere").join(DEFAULT_DIRNAME));
    }

    /// On non-Windows, `local_appdata_fallback` returns None.
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn local_appdata_fallback_is_none_on_unix() {
        assert!(local_appdata_fallback().is_none());
    }

    /// On Windows, `local_appdata_fallback` returns `%LOCALAPPDATA%\clawwiki`.
    #[cfg(target_os = "windows")]
    #[test]
    fn local_appdata_fallback_uses_localappdata_on_windows() {
        if let Some(fallback) = local_appdata_fallback() {
            assert!(fallback.ends_with("clawwiki"));
            // Should not be empty and should point under LOCALAPPDATA.
            let expected_prefix = std::env::var_os("LOCALAPPDATA");
            assert!(expected_prefix.is_some(), "LOCALAPPDATA should be set on Windows");
        }
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
        // Hard-pin a few canonical landmarks. If the seeded template
        // drifts away from the expected maintainer-rule structure,
        // these break loudly.
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
        // Pure non-ascii gets a stable ASCII fallback instead of every
        // CJK title collapsing to "untitled".
        let chinese_slug = slugify("中文标题");
        assert!(chinese_slug.starts_with("u-"));
        assert_ne!(chinese_slug, "untitled");
        assert_eq!(chinese_slug, slugify("中文标题"));
        assert_ne!(chinese_slug, slugify("另一个标题"));
        assert!(
            chinese_slug
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
        );
        // Mixed ASCII + CJK keeps the readable prefix but adds a hash
        // suffix so different Chinese tails do not collide.
        let mixed_slug = slugify("Hello 中文");
        assert!(mixed_slug.starts_with("hello-u"));
        assert!(mixed_slug.len() <= 64);
        // Empty
        assert_eq!(slugify(""), "untitled");
        // Pure symbols still have no meaningful title.
        assert_eq!(slugify("!!!"), "untitled");
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

    // ── Regression: anti-bot content validation (2026-04) ───────────
    //
    // Pre-fix, WeChat-iLink and a few other paths wrote fetched bodies
    // directly to raw/ without calling `wiki_ingest::validate_fetched_content`.
    // Result: anti-bot placeholder pages ("环境异常 ... 完成验证后即可
    // 继续访问 ... 去验证") ended up in the Inbox. The fix puts a
    // second validation gate inside `write_raw_entry` that every
    // caller traverses.

    #[test]
    fn write_raw_entry_rejects_short_anti_bot_page_for_wechat_url() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let body = "**\n\n## 环境异常\n\n当前环境异常，完成验证后即可继续访问。\n\n去验证";
        let fm = RawFrontmatter::for_paste(
            "wechat-url",
            Some("https://mp.weixin.qq.com/s/abc".to_string()),
        );
        let err = write_raw_entry(&paths, "wechat-url", "**", body, &fm).unwrap_err();
        assert!(matches!(err, WikiStoreError::Invalid(_)));
        let msg = format!("{err}");
        assert!(msg.contains("反爬") || msg.contains("内容验证失败"));
    }

    #[test]
    fn write_raw_entry_rejects_long_anti_bot_page_with_html_skeleton() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        // 18KB-style page: lots of HTML skeleton inflating the length
        // but the actual text is the anti-bot block. This is what the
        // mp.weixin.qq.com production incident looked like.
        let skeleton = "<div class='wrap'></div>\n".repeat(800);
        let body = format!(
            "{skeleton}\n\n## 环境异常\n\n完成验证后即可继续访问。"
        );
        assert!(body.len() > 10_000, "body must be long enough to bypass old length gate");
        let fm = RawFrontmatter::for_paste("url", Some("https://example.com/".to_string()));
        let err = write_raw_entry(&paths, "url", "example", &body, &fm).unwrap_err();
        assert!(matches!(err, WikiStoreError::Invalid(_)));
    }

    #[test]
    fn write_raw_entry_allows_short_paste_from_user() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        // User-authored sources are exempt: "paste" and "wechat-text"
        // may legitimately be very short (e.g. "明天开会讨论产品方向").
        let body = "明天开会讨论产品方向";
        let fm = RawFrontmatter::for_paste("paste", None);
        write_raw_entry(&paths, "paste", "idea", body, &fm).expect("paste short note should pass");

        let fm2 = RawFrontmatter::for_paste("wechat-text", None);
        write_raw_entry(&paths, "wechat-text", "msg", "你好", &fm2)
            .expect("wechat-text short message should pass");
    }

    // ── extract_readable_title regression (2026-04 mp.weixin) ───
    //
    // Pre-fix, append_new_raw_task ran a naïve `find(starts_with("# "))`
    // which missed `### ![](url)` and ended up surfacing the raw image
    // markdown line as the Inbox title. The new helper strips heading
    // markers, image stubs, and link wrappers before deciding what to
    // show.

    #[test]
    fn extract_readable_title_skips_heading_with_image() {
        let body = "_Author: 新智元_\n\n\
            _Published: 2026年4月15日_\n\n\
            ### ![](https://mmbiz.qpic.cn/mmbiz_png/abc123/640)\n\n\
            新智元报道 编辑：好困";
        let title = extract_readable_title(body, 30);
        assert_eq!(title, "新智元报道 编辑：好困");
    }

    #[test]
    fn extract_readable_title_strips_link_wrappers_keeps_label() {
        let body = "[Claude Code](https://example.com/cc) 重构了桌面端";
        let title = extract_readable_title(body, 60);
        assert!(title.starts_with("Claude Code 重构"), "got: {title}");
        assert!(!title.contains("http"), "url leaked: {title}");
    }

    #[test]
    fn extract_readable_title_handles_pure_image_lines() {
        let body = "![](https://example.com/a.png)\n\
            ![](https://example.com/b.png)\n\
            真正的标题文字";
        let title = extract_readable_title(body, 30);
        assert_eq!(title, "真正的标题文字");
    }

    #[test]
    fn extract_readable_title_falls_back_when_only_short_lines() {
        // Every line < 5 chars after stripping → first non-empty stripped
        // line should still be returned, not "素材 #" placeholder (that
        // happens at the caller level in append_new_raw_task).
        let body = "a\n\nb\n\nc";
        let title = extract_readable_title(body, 10);
        assert_eq!(title, "a");
    }

    #[test]
    fn extract_readable_title_truncates_to_max_chars() {
        let body = "这是一段非常非常非常非常非常长的中文标题用来测试截断逻辑";
        let title = extract_readable_title(body, 10);
        assert_eq!(title.chars().count(), 10);
    }

    #[test]
    fn extract_readable_title_prefers_content_over_metadata() {
        // `_Author: ..._` italics-wrapped metadata is skipped during
        // the primary pass so a real heading / paragraph wins.
        let body = "_Author: 新智元_\n\n# 真正的标题";
        let title = extract_readable_title(body, 30);
        assert_eq!(title, "真正的标题", "real heading should win over metadata");
    }

    #[test]
    fn extract_readable_title_falls_back_to_metadata_when_nothing_else() {
        // If the body has nothing but metadata, we accept it as the
        // fallback — better than the "素材 #N" placeholder.
        let body = "_Author: 新智元_\n\n_Published: 2026-04-15_";
        let title = extract_readable_title(body, 30);
        assert!(
            title.starts_with("Author") || title.starts_with("Published"),
            "expected metadata fallback, got: {title}",
        );
    }

    #[test]
    fn write_raw_entry_allows_substantive_wechat_article() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        // A real fetched article should pass — enough meaningful CJK
        // characters, no anti-bot phrases.
        let body = "# 今天的入库\n\n\
            本文讨论了 Rust 异步编程的主要范式。async/await 语法让\
            异步代码看起来像同步代码，但底层依赖 Future trait 和 \
            executor 运行时。Tokio 是最广泛使用的运行时，提供任务\
            调度器、定时器和 I/O 多路复用。理解 Pin 和 Unpin 对于\
            处理自引用结构体至关重要。实际应用中，常见的陷阱包括\
            借用检查器在 await 点的行为、以及 Send/Sync 约束。";
        let fm = RawFrontmatter::for_paste(
            "wechat-article",
            Some("https://mp.weixin.qq.com/s/real".to_string()),
        );
        write_raw_entry(&paths, "wechat-article", "rust-async", body, &fm)
            .expect("substantive article should pass");
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
            content_hash: None,
            original_url: None,
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
    fn frontmatter_round_trips_content_hash_and_original_url() {
        // M4: RawFrontmatter::for_paste_with_identity populates all
        // four identity fields and emits them into the YAML block.
        let fm = RawFrontmatter::for_paste_with_identity(
            "url",
            Some("https://example.com/canonical".to_string()),
            Some("https://example.com/canonical?utm_source=x".to_string()),
            Some(
                "a".repeat(64), // fake hex hash
            ),
        );
        let yaml = fm.to_yaml_block();
        assert!(yaml.contains("source_url: https://example.com/canonical\n"));
        assert!(yaml.contains(&format!("content_hash: {}\n", "a".repeat(64))));
        assert!(yaml.contains(
            "original_url: https://example.com/canonical?utm_source=x\n"
        ));
    }

    #[test]
    fn frontmatter_identity_elides_original_url_when_matches_canonical() {
        // M4 optimization: don't waste bytes on `original_url` when it
        // already equals the canonical form — the field is a signal
        // only when canonicalize actually mutated the input.
        let url = "https://example.com/identical".to_string();
        let fm = RawFrontmatter::for_paste_with_identity(
            "url",
            Some(url.clone()),
            Some(url),
            Some("b".repeat(64)),
        );
        assert!(fm.original_url.is_none());
        let yaml = fm.to_yaml_block();
        assert!(!yaml.contains("original_url:"));
    }

    #[test]
    fn frontmatter_omits_identity_fields_when_none() {
        let fm = RawFrontmatter::for_paste("paste", None);
        let yaml = fm.to_yaml_block();
        assert!(!yaml.contains("content_hash:"));
        assert!(!yaml.contains("original_url:"));
    }

    #[test]
    fn parse_frontmatter_fields_extracts_content_hash() {
        let yaml = "---\nkind: raw\nstatus: ingested\nowner: user\nschema: v1\n\
                   source: url\nsource_url: https://example.com/\n\
                   ingested_at: 2026-04-17T00:00:00Z\n\
                   content_hash: deadbeefcafe\n\
                   original_url: https://example.com/?utm_source=x\n---\n\nbody text";
        let fields = parse_frontmatter_fields(yaml);
        assert_eq!(fields.source_url.as_deref(), Some("https://example.com/"));
        assert_eq!(fields.content_hash.as_deref(), Some("deadbeefcafe"));
        assert_eq!(
            fields.original_url.as_deref(),
            Some("https://example.com/?utm_source=x")
        );
    }

    #[test]
    fn find_raw_by_content_hash_hits_when_present() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        let hash = "c".repeat(64);

        let fm = RawFrontmatter::for_paste_with_identity(
            "url",
            Some("https://a.example/".to_string()),
            None,
            Some(hash.clone()),
        );
        let written = write_raw_entry(
            &paths,
            "url",
            "content-hash-test",
            "this is body content with enough text to pass validation exemption",
            &fm,
        )
        .unwrap();

        let found = find_raw_by_content_hash(&paths, &hash).unwrap();
        assert!(found.is_some(), "hash lookup should hit");
        assert_eq!(found.unwrap().id, written.id);
    }

    #[test]
    fn find_raw_by_content_hash_misses_when_absent() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        // Pre-M4 entry without content_hash still shouldn't match
        // anything (the field stays None after a round-trip through
        // list_raw_entries).
        let fm = RawFrontmatter::for_paste("paste", None);
        write_raw_entry(&paths, "paste", "no-hash", "some body content here with enough text", &fm)
            .unwrap();

        let found = find_raw_by_content_hash(&paths, &"d".repeat(64)).unwrap();
        assert!(found.is_none(), "hash lookup against absent hash should miss");
    }

    #[test]
    fn find_raw_by_content_hash_picks_latest_on_collision() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        let hash = "e".repeat(64);

        let fm_a = RawFrontmatter::for_paste_with_identity(
            "url",
            Some("https://first.example/".to_string()),
            None,
            Some(hash.clone()),
        );
        let first = write_raw_entry(
            &paths,
            "url",
            "first",
            "body text one, longer than fifty characters to satisfy the raw validation gate",
            &fm_a,
        )
        .unwrap();

        let fm_b = RawFrontmatter::for_paste_with_identity(
            "url",
            Some("https://second.example/".to_string()),
            None,
            Some(hash.clone()),
        );
        let second = write_raw_entry(
            &paths,
            "url",
            "second",
            "body text two, longer than fifty characters to satisfy the raw validation gate",
            &fm_b,
        )
        .unwrap();

        let found = find_raw_by_content_hash(&paths, &hash).unwrap().unwrap();
        assert!(
            found.id > first.id,
            "should return the newer of the two matching entries"
        );
        assert_eq!(found.id, second.id);
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

        // After feat(W) init_wiki creates all 4 category dirs up front.
        let concepts_dir = paths.wiki.join(WIKI_CONCEPTS_SUBDIR);
        assert!(concepts_dir.is_dir(), "init_wiki should create concepts/");

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
        assert!(content.contains("## Concept (0)"));
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
        assert!(content.contains("## Concept (3)"));

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

    // ── Schema seed tests ──────────────────────────────────────

    #[test]
    fn init_wiki_seeds_agents_md() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let agents = tmp.path().join(SCHEMA_DIR).join("AGENTS.md");
        assert!(agents.is_file());
        let content = fs::read_to_string(&agents).unwrap();
        assert!(content.contains("wiki-maintainer"));
        assert!(content.contains("ask-runtime"));
    }

    #[test]
    fn init_wiki_seeds_page_templates() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let tpl_dir = tmp.path().join(SCHEMA_DIR).join("templates");
        assert!(tpl_dir.join("concept.md").is_file());
        assert!(tpl_dir.join("people.md").is_file());
        assert!(tpl_dir.join("topic.md").is_file());
        assert!(tpl_dir.join("compare.md").is_file());
    }

    #[test]
    fn init_wiki_seeds_policy_files() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let pol_dir = tmp.path().join(SCHEMA_DIR).join("policies");
        assert!(pol_dir.join("maintenance.md").is_file());
        assert!(pol_dir.join("conflict.md").is_file());
        assert!(pol_dir.join("deprecation.md").is_file());
        assert!(pol_dir.join("naming.md").is_file());
        // Verify content is meaningful
        let naming = fs::read_to_string(pol_dir.join("naming.md")).unwrap();
        assert!(naming.contains("Lowercase ASCII"));
    }

    #[test]
    fn init_wiki_preserves_user_edited_agents_md() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let agents = tmp.path().join(SCHEMA_DIR).join("AGENTS.md");
        let custom = "# Custom agents\n\nMy rules.\n";
        fs::write(&agents, custom).unwrap();
        // Re-running init_wiki must not clobber the user's edits.
        init_wiki(tmp.path()).unwrap();
        let after = fs::read_to_string(&agents).unwrap();
        assert_eq!(after, custom, "AGENTS.md was overwritten");
    }

    // ── W multi-category wiki tests ──────────────────────────────

    #[test]
    fn init_wiki_creates_all_four_category_dirs() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        assert!(tmp.path().join(WIKI_DIR).join(WIKI_CONCEPTS_SUBDIR).is_dir());
        assert!(tmp.path().join(WIKI_DIR).join(WIKI_PEOPLE_SUBDIR).is_dir());
        assert!(tmp.path().join(WIKI_DIR).join(WIKI_TOPICS_SUBDIR).is_dir());
        assert!(tmp.path().join(WIKI_DIR).join(WIKI_COMPARE_SUBDIR).is_dir());
    }

    #[test]
    fn write_wiki_page_in_category_writes_to_correct_subdir() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        write_wiki_page_in_category(
            &paths,
            "people",
            "karpathy",
            "Andrej Karpathy",
            "AI researcher.",
            "Bio body.",
            Some(1),
        )
        .unwrap();

        // File lands in wiki/people/karpathy.md, NOT concepts/
        let people_file = tmp
            .path()
            .join(WIKI_DIR)
            .join(WIKI_PEOPLE_SUBDIR)
            .join("karpathy.md");
        assert!(people_file.is_file());
        let content = fs::read_to_string(&people_file).unwrap();
        assert!(content.contains("type: people"));
        assert!(content.contains("Andrej Karpathy"));
    }

    #[test]
    fn write_wiki_page_in_category_rejects_unknown_category() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let err = write_wiki_page_in_category(
            &paths,
            "unknown-cat",
            "test",
            "Test",
            "summary",
            "body",
            None,
        )
        .unwrap_err();
        assert!(matches!(err, WikiStoreError::Invalid(_)));
    }

    #[test]
    fn list_all_wiki_pages_returns_pages_from_all_categories() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        write_wiki_page(&paths, "llm-wiki", "LLM Wiki", "s", "b", None).unwrap();
        write_wiki_page_in_category(&paths, "people", "karpathy", "K", "s", "b", None)
            .unwrap();
        write_wiki_page_in_category(&paths, "topic", "ai-memory", "AI Memory", "s", "b", None)
            .unwrap();

        let concepts = list_wiki_pages(&paths).unwrap();
        assert_eq!(concepts.len(), 1); // only concepts/

        let all = list_all_wiki_pages(&paths).unwrap();
        assert_eq!(all.len(), 3); // all categories
    }

    #[test]
    fn rebuild_wiki_index_includes_all_categories() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        write_wiki_page(&paths, "alpha", "Alpha", "s", "b", None).unwrap();
        write_wiki_page_in_category(&paths, "people", "bob", "Bob", "s", "b", None)
            .unwrap();
        write_wiki_page_in_category(&paths, "compare", "a-vs-b", "A vs B", "s", "b", None)
            .unwrap();

        rebuild_wiki_index(&paths).unwrap();
        let content = fs::read_to_string(wiki_index_path(&paths)).unwrap();

        assert!(content.contains("## Concept (1)"));
        assert!(content.contains("## People (1)"));
        assert!(content.contains("## Topic (0)"));
        assert!(content.contains("## Compare (1)"));
        assert!(content.contains("[Alpha](concepts/alpha.md)"));
        assert!(content.contains("[Bob](people/bob.md)"));
        assert!(content.contains("[A vs B](compare/a-vs-b.md)"));
    }

    // ── P notify_affected_pages tests ────────────────────────────

    #[test]
    fn notify_affected_pages_finds_mentions_by_slug() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        // Existing page mentions "karpathy" in its body.
        write_wiki_page(
            &paths,
            "rag",
            "RAG",
            "Retrieval-augmented generation.",
            "Karpathy proposed a different approach to knowledge management.",
            Some(1),
        )
        .unwrap();

        // Write new page whose slug is "karpathy".
        write_wiki_page(
            &paths,
            "karpathy",
            "Karpathy",
            "Andrej Karpathy.",
            "Pioneer of LLM wiki.",
            Some(2),
        )
        .unwrap();

        let count = notify_affected_pages(&paths, "karpathy", "Karpathy").unwrap();
        assert_eq!(count, 1);

        // The inbox should now have a Stale entry for "rag".
        let entries = list_inbox_entries(&paths).unwrap();
        let stale = entries
            .iter()
            .find(|e| e.kind == InboxKind::Stale)
            .expect("should have a Stale entry");
        assert!(stale.title.contains("rag"));
        assert!(stale.description.contains("karpathy"));
    }

    #[test]
    fn notify_affected_pages_skips_the_new_page_itself() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        write_wiki_page(
            &paths,
            "self-mention",
            "Self Mention",
            "A page.",
            "This page is about self-mention.",
            None,
        )
        .unwrap();

        let count = notify_affected_pages(&paths, "self-mention", "Self Mention").unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn notify_affected_pages_returns_zero_for_empty_wiki() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let count = notify_affected_pages(&paths, "anything", "Anything").unwrap();
        assert_eq!(count, 0);
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
    fn extract_internal_links_finds_wikilinks() {
        let body = "See [[Alpha]] plus [[rag-vs-llm-wiki|RAG vs LLM Wiki]] \
                    and [[beta.model#section|Beta]].";
        let links = extract_internal_links(body);
        assert_eq!(links.len(), 3);
        assert!(links.contains(&"alpha".to_string()));
        assert!(links.contains(&"rag-vs-llm-wiki".to_string()));
        assert!(links.contains(&"beta.model".to_string()));
    }

    #[test]
    fn extract_internal_links_deduplicates_markdown_and_wikilinks() {
        let body = "[Alpha](concepts/alpha.md), [[Alpha]], and [[alpha|A]].";
        let links = extract_internal_links(body);
        assert_eq!(links, vec!["alpha".to_string()]);
    }

    #[test]
    fn list_backlinks_returns_wikilink_referring_pages() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        write_wiki_page(
            &paths,
            "alpha",
            "Alpha",
            "Summary",
            "See [[bravo|Bravo]] for more.",
            Some(1),
        )
        .unwrap();
        write_wiki_page(
            &paths,
            "bravo",
            "Bravo",
            "Summary",
            "Standalone.",
            Some(2),
        )
        .unwrap();

        let backlinks = list_backlinks(&paths, "bravo").unwrap();
        assert_eq!(backlinks.len(), 1);
        assert_eq!(backlinks[0].slug, "alpha");
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

    // ── G1 related-pages + page-graph tests ──────────────────────

    /// Pages A and B both link to X (a third page). Page C links
    /// to something else. `compute_related_pages(A)` should return
    /// [B] with a "共同链接" reason, not C.
    #[test]
    fn compute_related_shared_outgoing_link() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        // All four pages exist so `extract_internal_links` targets
        // resolve and dangling-filter doesn't drop them.
        write_wiki_page(
            &paths,
            "xray",
            "X-Ray",
            "A shared target.",
            "Body.",
            None,
        )
        .unwrap();
        write_wiki_page(
            &paths,
            "yankee",
            "Yankee",
            "An unrelated target.",
            "Body.",
            None,
        )
        .unwrap();
        write_wiki_page(
            &paths,
            "alpha",
            "Alpha",
            "Alpha summary.",
            "Alpha refers to [X](concepts/xray.md).",
            None,
        )
        .unwrap();
        write_wiki_page(
            &paths,
            "bravo",
            "Bravo",
            "Bravo summary.",
            "Bravo also sees [X](concepts/xray.md).",
            None,
        )
        .unwrap();
        write_wiki_page(
            &paths,
            "charlie",
            "Charlie",
            "Charlie summary.",
            "Charlie cites [Y](concepts/yankee.md).",
            None,
        )
        .unwrap();

        let related = compute_related_pages(&paths, "alpha").unwrap();
        assert_eq!(
            related.len(),
            1,
            "expected only [bravo] as related, got {:?}",
            related.iter().map(|r| &r.slug).collect::<Vec<_>>()
        );
        assert_eq!(related[0].slug, "bravo");
        assert!(related[0].score >= 2, "score should include +2 for shared link");
        assert!(
            related[0]
                .reasons
                .iter()
                .any(|r| r.contains("共同链接") && r.contains("xray")),
            "expected a 共同链接: xray reason, got {:?}",
            related[0].reasons
        );
        // Charlie must not appear (links to a different slug).
        assert!(!related.iter().any(|r| r.slug == "charlie"));
    }

    /// Pages A and B both have `source_raw_id: 42` in their
    /// frontmatter (no shared outgoing links). `compute_related_pages(A)`
    /// should surface [B] with a "共享来源: raw #00042" reason.
    #[test]
    fn compute_related_shared_source_raw_id() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        // Seed a raw so the ids we pick land in the typical range.
        // (compute_related doesn't validate raw existence — it only
        //  checks the slug frontmatter — but being realistic keeps
        //  the test future-proof against stricter validation.)
        write_raw_entry(
            &paths,
            "paste",
            "source doc",
            "body",
            &RawFrontmatter::for_paste("paste", None),
        )
        .unwrap();

        write_wiki_page(&paths, "delta", "Delta", "summary d", "Body d.", Some(42))
            .unwrap();
        write_wiki_page(&paths, "echo", "Echo", "summary e", "Body e.", Some(42))
            .unwrap();
        // Sanity: a third page with a different source should NOT show up.
        write_wiki_page(&paths, "foxtrot", "Foxtrot", "summary f", "Body f.", Some(99))
            .unwrap();

        let related = compute_related_pages(&paths, "delta").unwrap();
        assert!(
            related.iter().any(|r| r.slug == "echo"),
            "expected echo as related via shared source_raw_id"
        );
        let echo_hit = related.iter().find(|r| r.slug == "echo").unwrap();
        assert!(echo_hit.score >= 3, "score should include +3 for shared raw");
        assert!(
            echo_hit
                .reasons
                .iter()
                .any(|r| r.contains("共享来源") && r.contains("00042")),
            "expected a 共享来源 raw #00042 reason, got {:?}",
            echo_hit.reasons
        );
        assert!(!related.iter().any(|r| r.slug == "foxtrot"));
    }

    /// The target slug itself must never appear in its own related
    /// list, even when the page self-references or when the scoring
    /// would otherwise match.
    #[test]
    fn compute_related_excludes_self() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        // A self-referencing page + one other page that also links to
        // it. Any shared-link scoring that forgot to strip self-refs
        // would falsely match this setup.
        write_wiki_page(
            &paths,
            "selfie",
            "Selfie",
            "Self-reference summary.",
            "See [me](concepts/selfie.md) and [peer](concepts/peer.md).",
            Some(7),
        )
        .unwrap();
        write_wiki_page(
            &paths,
            "peer",
            "Peer",
            "Peer summary.",
            "Peer body.",
            Some(7),
        )
        .unwrap();

        let related = compute_related_pages(&paths, "selfie").unwrap();
        assert!(
            !related.iter().any(|r| r.slug == "selfie"),
            "self must not appear in own related list"
        );
        // Peer still matches (shared source_raw_id = 7).
        assert!(related.iter().any(|r| r.slug == "peer"));
    }

    /// A page that links to a non-existent slug must not crash, and
    /// must still report correct related pages via other signals.
    #[test]
    fn compute_related_handles_dangling() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        // Ghost target is never written. Real peer shares a source_raw_id.
        write_wiki_page(
            &paths,
            "golf",
            "Golf",
            "Golf summary.",
            "Refs [ghost](concepts/ghost.md) for context.",
            Some(11),
        )
        .unwrap();
        write_wiki_page(
            &paths,
            "hotel",
            "Hotel",
            "Hotel summary.",
            "Hotel body.",
            Some(11),
        )
        .unwrap();

        let related = compute_related_pages(&paths, "golf").unwrap();
        // Must not contain the dangling slug.
        assert!(!related.iter().any(|r| r.slug == "ghost"));
        // Must still surface hotel via source_raw_id overlap.
        assert!(related.iter().any(|r| r.slug == "hotel"));
    }

    /// `get_page_graph` must return all four sections (header fields,
    /// outgoing, backlinks, related) populated in one payload. This
    /// is the integration-level guarantee the frontend relies on.
    #[test]
    fn get_page_graph_returns_all_sections() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        // hub is the target: links out to spoke-a, spoke-b; linked into
        // by back-ref; back-ref also links to spoke-a (shared outgoing
        // → back-ref is related); side shares the same source_raw_id
        // (related via raw signal).
        write_wiki_page(
            &paths,
            "spoke-a",
            "Spoke A",
            "spoke-a summary",
            "Spoke A body.",
            None,
        )
        .unwrap();
        write_wiki_page(
            &paths,
            "spoke-b",
            "Spoke B",
            "spoke-b summary",
            "Spoke B body.",
            None,
        )
        .unwrap();
        write_wiki_page(
            &paths,
            "hub",
            "Hub",
            "hub summary",
            "Hub links [A](concepts/spoke-a.md) and [B](concepts/spoke-b.md).",
            Some(55),
        )
        .unwrap();
        write_wiki_page(
            &paths,
            "back-ref",
            "Back Ref",
            "back-ref summary",
            "Back ref points at [Hub](concepts/hub.md) and [A](concepts/spoke-a.md).",
            None,
        )
        .unwrap();
        write_wiki_page(
            &paths,
            "side",
            "Side",
            "side summary",
            "Side body — no internal links.",
            Some(55),
        )
        .unwrap();

        let graph = get_page_graph(&paths, "hub").unwrap();

        // Header fields.
        assert_eq!(graph.slug, "hub");
        assert_eq!(graph.title, "Hub");
        assert_eq!(graph.category, "concept");
        assert_eq!(graph.summary.as_deref(), Some("hub summary"));

        // Outgoing — both existing spokes, no dangling.
        let outgoing_slugs: Vec<&str> =
            graph.outgoing.iter().map(|n| n.slug.as_str()).collect();
        assert!(outgoing_slugs.contains(&"spoke-a"));
        assert!(outgoing_slugs.contains(&"spoke-b"));
        assert_eq!(graph.outgoing.len(), 2);

        // Backlinks — only back-ref links into hub.
        assert_eq!(graph.backlinks.len(), 1);
        assert_eq!(graph.backlinks[0].slug, "back-ref");

        // Related — should contain back-ref (shared outgoing spoke-a)
        // and side (shared source_raw_id 55). Order: side has +3,
        // back-ref has +2, so side comes first.
        let related_slugs: Vec<&str> =
            graph.related.iter().map(|r| r.slug.as_str()).collect();
        assert!(related_slugs.contains(&"back-ref"));
        assert!(related_slugs.contains(&"side"));
        // Self never appears.
        assert!(!related_slugs.contains(&"hub"));
        // Each related hit has reasons.
        for hit in &graph.related {
            assert!(!hit.reasons.is_empty(), "hit {} has empty reasons", hit.slug);
        }
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

    // ── v2 absorb_log tests ─────────────────────────────────────

    #[test]
    fn absorb_log_append_and_list_roundtrip() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let entry = AbsorbLogEntry {
            entry_id: 1,
            timestamp: "2026-04-14T10:00:00Z".to_string(),
            action: "create".to_string(),
            page_slug: Some("test-page".to_string()),
            page_title: Some("Test Page".to_string()),
            page_category: Some("concept".to_string()),
        };
        append_absorb_log(&paths, entry.clone()).unwrap();

        let log = list_absorb_log(&paths).unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].entry_id, 1);
        assert_eq!(log[0].action, "create");
        assert_eq!(log[0].page_slug.as_deref(), Some("test-page"));
    }

    #[test]
    fn absorb_log_empty_file_returns_empty_vec() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        // Don't write any log entries.
        let log = list_absorb_log(&paths).unwrap();
        assert!(log.is_empty());
    }

    #[test]
    fn absorb_log_list_is_reverse_chronological() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        for i in 1..=3 {
            append_absorb_log(&paths, AbsorbLogEntry {
                entry_id: i,
                timestamp: format!("2026-04-14T10:0{i}:00Z"),
                action: "create".to_string(),
                page_slug: Some(format!("page-{i}")),
                page_title: None,
                page_category: None,
            }).unwrap();
        }

        let log = list_absorb_log(&paths).unwrap();
        assert_eq!(log.len(), 3);
        // Should be descending by timestamp.
        assert_eq!(log[0].entry_id, 3);
        assert_eq!(log[2].entry_id, 1);
    }

    #[test]
    fn is_entry_absorbed_true_for_create() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        append_absorb_log(&paths, AbsorbLogEntry {
            entry_id: 42,
            timestamp: now_iso8601(),
            action: "create".to_string(),
            page_slug: Some("x".to_string()),
            page_title: None,
            page_category: None,
        }).unwrap();

        assert!(is_entry_absorbed(&paths, 42));
    }

    #[test]
    fn is_entry_absorbed_false_for_skip() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        append_absorb_log(&paths, AbsorbLogEntry {
            entry_id: 7,
            timestamp: now_iso8601(),
            action: "skip".to_string(),
            page_slug: None,
            page_title: None,
            page_category: None,
        }).unwrap();

        assert!(!is_entry_absorbed(&paths, 7), "skip should not count as absorbed");
    }

    #[test]
    fn is_entry_absorbed_false_for_unknown() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        assert!(!is_entry_absorbed(&paths, 999));
    }

    // ── v2 backlinks_index tests ────────────────────────────────

    #[test]
    fn backlinks_save_load_roundtrip() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let mut index = BacklinksIndex::new();
        index.insert("target".to_string(), vec!["source-a".to_string(), "source-b".to_string()]);

        save_backlinks_index(&paths, &index).unwrap();
        let loaded = load_backlinks_index(&paths).unwrap();
        assert_eq!(loaded.get("target").unwrap().len(), 2);
        assert!(loaded.get("target").unwrap().contains(&"source-a".to_string()));
    }

    #[test]
    fn load_backlinks_index_missing_file_returns_empty() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        let index = load_backlinks_index(&paths).unwrap();
        assert!(index.is_empty());
    }

    #[test]
    fn build_backlinks_index_with_links() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        // Create two pages where page-a links to page-b.
        write_wiki_page_in_category(
            &paths, "concept", "page-a", "Page A", "Summary A",
            "# Page A\n\nSee [Page B](concepts/page-b.md) for details.", Some(1),
        ).unwrap();
        write_wiki_page_in_category(
            &paths, "concept", "page-b", "Page B", "Summary B",
            "# Page B\n\nStandalone page.", Some(2),
        ).unwrap();

        let index = build_backlinks_index(&paths).unwrap();
        // page-b should have page-a as a backlink.
        let refs = index.get("page-b").unwrap();
        assert!(refs.contains(&"page-a".to_string()));
        // page-a has no inbound links.
        assert!(index.get("page-a").is_none());
    }

    #[test]
    fn build_backlinks_index_with_wikilinks() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        write_wiki_page_in_category(
            &paths,
            "concept",
            "page-a",
            "Page A",
            "Summary A",
            "# Page A\n\nSee [[page-b|Page B]] for details.",
            Some(1),
        )
        .unwrap();
        write_wiki_page_in_category(
            &paths,
            "concept",
            "page-b",
            "Page B",
            "Summary B",
            "# Page B\n\nStandalone page.",
            Some(2),
        )
        .unwrap();

        let index = build_backlinks_index(&paths).unwrap();
        assert_eq!(index.get("page-b"), Some(&vec!["page-a".to_string()]));
        assert!(index.get("page-a").is_none());
    }

    #[test]
    fn build_backlinks_index_empty_wiki() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        let index = build_backlinks_index(&paths).unwrap();
        assert!(index.is_empty());
    }

    // ── Sprint 1-A gap-closure tests (worksheet §"单元测试" matrix) ──
    //
    // These cover the explicit four scenarios from the sprint worksheet
    // that weren't yet in the preceding absorb_log / backlinks suite:
    //   1. concurrent append_absorb_log (ABSORB_LOG_GUARD correctness)
    //   2. circular wikilinks (A→B + B→A)
    //   3. orphan pages (no inbound → must not appear as key)
    //   4. unknown / invalid target slugs (current behaviour: preserved)

    #[test]
    fn append_absorb_log_thread_safe() {
        // Spin 10 threads each appending one entry; ABSORB_LOG_GUARD
        // must serialize the load→push→save cycle so no entry is lost.
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());
        let paths_arc = std::sync::Arc::new(paths);

        let mut handles = Vec::new();
        for i in 1u32..=10 {
            let paths = std::sync::Arc::clone(&paths_arc);
            handles.push(std::thread::spawn(move || {
                let entry = AbsorbLogEntry {
                    entry_id: i,
                    timestamp: format!("2026-04-14T10:00:{i:02}Z"),
                    action: "create".to_string(),
                    page_slug: Some(format!("page-{i}")),
                    page_title: None,
                    page_category: None,
                };
                append_absorb_log(paths.as_ref(), entry).unwrap();
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        let log = list_absorb_log(paths_arc.as_ref()).unwrap();
        assert_eq!(log.len(), 10, "expected 10 entries, got {}", log.len());
        let mut ids: Vec<u32> = log.iter().map(|e| e.entry_id).collect();
        ids.sort_unstable();
        assert_eq!(ids, (1u32..=10).collect::<Vec<_>>());
    }

    #[test]
    fn build_backlinks_index_circular_links() {
        // page-a → page-b AND page-b → page-a. Both slugs should
        // appear as keys, each with the other as a backlink.
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        write_wiki_page_in_category(
            &paths,
            "concept",
            "page-a",
            "Page A",
            "Summary A",
            "# Page A\n\nRefers to [Page B](concepts/page-b.md).",
            Some(1),
        )
        .unwrap();
        write_wiki_page_in_category(
            &paths,
            "concept",
            "page-b",
            "Page B",
            "Summary B",
            "# Page B\n\nRefers to [Page A](concepts/page-a.md).",
            Some(2),
        )
        .unwrap();

        let index = build_backlinks_index(&paths).unwrap();
        assert_eq!(index.len(), 2, "both slugs must be keys; got {index:?}");
        assert_eq!(index.get("page-a").unwrap(), &vec!["page-b".to_string()]);
        assert_eq!(index.get("page-b").unwrap(), &vec!["page-a".to_string()]);
    }

    #[test]
    fn build_backlinks_index_excludes_orphan_pages() {
        // Three pages: page-orphan (no inbound, no outbound),
        // page-source (→ page-target), page-target (no outbound).
        // Index must contain page-target but NOT page-orphan as a key
        // (per §3.6 spec: "无入链的页面不出现在 index 中").
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        write_wiki_page_in_category(
            &paths,
            "concept",
            "page-orphan",
            "Orphan",
            "Standalone",
            "# Orphan\n\nNo links in or out.",
            Some(1),
        )
        .unwrap();
        write_wiki_page_in_category(
            &paths,
            "concept",
            "page-source",
            "Source",
            "Links outward",
            "# Source\n\nSee [Target](concepts/page-target.md).",
            Some(2),
        )
        .unwrap();
        write_wiki_page_in_category(
            &paths,
            "concept",
            "page-target",
            "Target",
            "Receives one backlink",
            "# Target\n\nEnd of the chain.",
            Some(3),
        )
        .unwrap();

        let index = build_backlinks_index(&paths).unwrap();
        assert!(
            !index.contains_key("page-orphan"),
            "orphan page must not appear as an index key"
        );
        assert!(
            !index.contains_key("page-source"),
            "outbound-only page must not appear as an index key"
        );
        let target_refs = index
            .get("page-target")
            .expect("page-target must have a backlink entry");
        assert_eq!(target_refs, &vec!["page-source".to_string()]);
    }

    #[test]
    fn build_backlinks_index_preserves_unknown_targets() {
        // `extract_internal_links` parses `[text](concepts/SLUG.md)`
        // lexically — it does NOT check that SLUG exists on disk.
        // `build_backlinks_index` therefore records the target slug
        // verbatim even when no page with that slug exists. This
        // matches the §4.1 contract (pure parse, no existence check)
        // and lets patrol-type callers surface dangling links later.
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        write_wiki_page_in_category(
            &paths,
            "concept",
            "page-a",
            "Has Dangling Link",
            "Summary",
            "# A\n\nSee [Ghost](concepts/nonexistent-slug.md).",
            Some(1),
        )
        .unwrap();

        let index = build_backlinks_index(&paths).unwrap();
        let refs = index
            .get("nonexistent-slug")
            .expect("unknown target should still be keyed in the index");
        assert_eq!(refs, &vec!["page-a".to_string()]);
    }

    // ── v2 validate_frontmatter tests ───────────────────────────

    #[test]
    fn validate_frontmatter_compliant_page() {
        let content = "---\ntype: concept\ntitle: Test\nsummary: A test page\n---\n# Body";
        let template = SchemaTemplate {
            name: "concept".to_string(),
            fields: vec![
                TemplateField {
                    name: "type".to_string(),
                    field_type: FieldType::String,
                    description: "".to_string(),
                    default_value: None,
                    validation: None,
                },
                TemplateField {
                    name: "title".to_string(),
                    field_type: FieldType::String,
                    description: "".to_string(),
                    default_value: None,
                    validation: None,
                },
            ],
            required_fields: vec!["type".to_string(), "title".to_string()],
        };
        let errors = validate_frontmatter(content, &template);
        assert!(errors.is_empty(), "compliant page should have no errors: {errors:?}");
    }

    #[test]
    fn validate_frontmatter_missing_required_field() {
        let content = "---\ntype: concept\n---\n# Body";
        let template = SchemaTemplate {
            name: "concept".to_string(),
            fields: vec![],
            required_fields: vec!["type".to_string(), "title".to_string()],
        };
        let errors = validate_frontmatter(content, &template);
        assert_eq!(errors.len(), 1, "should report 1 missing field");
        assert_eq!(errors[0].field, "title");
    }

    #[test]
    fn validate_frontmatter_no_frontmatter_block() {
        let content = "# Just a body, no frontmatter";
        let template = SchemaTemplate {
            name: "any".to_string(),
            fields: vec![],
            required_fields: vec![],
        };
        let errors = validate_frontmatter(content, &template);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].field, "(frontmatter)");
    }

    // ── v2 SchemaTemplateInfo / load_schema_template_infos tests ─

    #[test]
    fn load_schema_template_infos_returns_all_builtin_templates() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let infos = load_schema_template_infos(&paths).unwrap();
        let categories: Vec<&str> = infos.iter().map(|t| t.category.as_str()).collect();
        assert!(categories.contains(&"concept"), "expected concept template");
        assert!(categories.contains(&"people"), "expected people template");
        assert!(categories.contains(&"topic"), "expected topic template");
        assert!(categories.contains(&"compare"), "expected compare template");

        // Display-name mapping
        let concept = infos.iter().find(|t| t.category == "concept").unwrap();
        assert_eq!(concept.display_name, "概念");
        let people = infos.iter().find(|t| t.category == "people").unwrap();
        assert_eq!(people.display_name, "人物");

        // Frontmatter parsed into field list
        let concept_field_names: Vec<&str> =
            concept.fields.iter().map(|f| f.name.as_str()).collect();
        assert!(concept_field_names.contains(&"type"));
        assert!(concept_field_names.contains(&"title"));
        assert!(concept_field_names.iter().all(|n| !n.is_empty()));

        // body_hint contains body content (non-empty for seeded templates)
        assert!(!concept.body_hint.is_empty(), "concept body_hint should be non-empty");

        // file_path points to an existing .md file
        assert!(concept.file_path.ends_with("concept.md"));
    }

    #[test]
    fn load_schema_template_infos_missing_dir_returns_empty() {
        let tmp = tempdir().unwrap();
        // Do NOT init_wiki — templates dir absent
        let paths = WikiPaths::resolve(tmp.path());
        let infos = load_schema_template_infos(&paths).unwrap();
        assert!(infos.is_empty());
    }

    // ── v2 wiki_stats tests ─────────────────────────────────────

    #[test]
    fn wiki_stats_empty_wiki() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        let stats = wiki_stats(&paths).unwrap();
        assert_eq!(stats.raw_count, 0);
        assert_eq!(stats.wiki_count, 0);
        assert_eq!(stats.concept_count, 0);
        assert_eq!(stats.edge_count, 0);
        assert_eq!(stats.orphan_count, 0);
        assert_eq!(stats.inbox_pending, 0);
        assert_eq!(stats.avg_page_words, 0);
        assert!(stats.absorb_success_rate == 0.0);
        assert!(stats.knowledge_velocity == 0.0);
        assert!(stats.last_absorb_at.is_none());
    }

    #[test]
    fn wiki_stats_with_data() {
        let tmp = tempdir().unwrap();
        init_wiki(tmp.path()).unwrap();
        let paths = WikiPaths::resolve(tmp.path());

        // Seed a raw entry and a wiki page.
        let fm = RawFrontmatter::for_paste("paste", None);
        write_raw_entry(&paths, "paste", "test", "body content", &fm).unwrap();

        write_wiki_page_in_category(
            &paths, "concept", "test-concept", "Test Concept", "Summary",
            "This is a test concept page with some words.", Some(1),
        ).unwrap();

        let stats = wiki_stats(&paths).unwrap();
        assert_eq!(stats.raw_count, 1);
        assert_eq!(stats.wiki_count, 1);
        assert_eq!(stats.concept_count, 1);
        assert!(stats.avg_page_words > 0);
    }

    // ── W2 Proposal/Apply backward-compatibility tests ───────────

    /// Legacy inbox.json blobs (pre-W2) have neither the W1 maintain
    /// fields nor the new W2 proposal fields. They must deserialize
    /// cleanly with `None` for all eight optional slots — if this
    /// ever breaks, every in-the-wild inbox file becomes unreadable
    /// on upgrade. Hard guard.
    #[test]
    fn inbox_entry_legacy_json_without_proposal_fields_deserializes() {
        let legacy = r#"[
            {
                "id": 42,
                "kind": "new-raw",
                "status": "pending",
                "title": "legacy entry",
                "description": "written before W2 shipped",
                "created_at": "2026-04-15T12:00:00Z"
            }
        ]"#;
        let parsed: Vec<InboxEntry> = serde_json::from_str(legacy).unwrap();
        assert_eq!(parsed.len(), 1);
        let entry = &parsed[0];
        assert_eq!(entry.id, 42);
        // W1 fields — None
        assert!(entry.maintain_action.is_none());
        assert!(entry.target_page_slug.is_none());
        assert!(entry.proposed_wiki_slug.is_none());
        assert!(entry.rejection_reason.is_none());
        // W2 fields — None (the point of this test)
        assert!(entry.proposal_status.is_none(),
            "proposal_status must default to None for legacy JSON");
        assert!(entry.proposed_after_markdown.is_none(),
            "proposed_after_markdown must default to None for legacy JSON");
        assert!(entry.before_markdown_snapshot.is_none(),
            "before_markdown_snapshot must default to None for legacy JSON");
        assert!(entry.proposal_summary.is_none(),
            "proposal_summary must default to None for legacy JSON");
    }

    /// Round-trip of a W2 entry with all four proposal fields set
    /// ensures the serde annotations preserve the values verbatim
    /// (no rename/skip-when-present accidents).
    #[test]
    fn inbox_entry_with_proposal_fields_round_trips() {
        // Build the JSON via serde_json::json! so the embedded `#`
        // heading marker doesn't confuse Rust's raw-string parser.
        let with_proposal = serde_json::json!([
            {
                "id": 7,
                "kind": "new-raw",
                "status": "pending",
                "title": "W2 entry",
                "description": "with proposal",
                "created_at": "2026-04-18T00:00:00Z",
                "proposal_status": "pending",
                "proposed_after_markdown": "# Merged\n\nbody",
                "before_markdown_snapshot": "# Old\n\nbody",
                "proposal_summary": "Appended new section under attention"
            }
        ])
        .to_string();
        let parsed: Vec<InboxEntry> = serde_json::from_str(&with_proposal).unwrap();
        let entry = &parsed[0];
        assert_eq!(entry.proposal_status.as_deref(), Some("pending"));
        assert_eq!(
            entry.proposed_after_markdown.as_deref(),
            Some("# Merged\n\nbody")
        );
        assert_eq!(
            entry.before_markdown_snapshot.as_deref(),
            Some("# Old\n\nbody")
        );
        assert_eq!(
            entry.proposal_summary.as_deref(),
            Some("Appended new section under attention")
        );

        // Round-trip: serialize → parse → equality.
        let back = serde_json::to_string(&parsed).unwrap();
        let reparse: Vec<InboxEntry> = serde_json::from_str(&back).unwrap();
        assert_eq!(reparse, parsed);
    }
}

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
}

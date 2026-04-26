//! Runtime configuration for the WeChat auto-ingest bridge (M5).
//!
//! Persisted to `~/.clawwiki/wechat_ingest_config.json`. Two fields:
//!
//!   * `enabled_mode` (`"all"` or `"whitelist"`) — when `"whitelist"`,
//!     only messages whose `group_id` appears in `enabled_group_ids`
//!     survive the middleware guard in `desktop_handler::on_message()`.
//!     Every other inbound is marked `DedupeResult::Skipped` and
//!     short-circuits the handler without touching the ingest path.
//!   * `enabled_group_ids` (list of strings) — WeChat-side group IDs
//!     the user has whitelisted for auto-ingest. Empty by default.
//!
//! The HTTP routes `GET/POST /api/wechat/bridge/config` read and write
//! this file via the global singleton [`global`]. The handler-side
//! accessor is [`read_snapshot`] which takes a short Mutex clone —
//! cheap even on the fast path.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

/// Whitelist mode marker used when comparing `enabled_mode`.
pub const MODE_WHITELIST: &str = "whitelist";
/// Wide-open mode marker (default). Any inbound event flows through the
/// ingest pipeline as long as it clears dedupe.
pub const MODE_ALL: &str = "all";

/// Persisted shape. Field names match the HTTP wire contract one-to-one
/// so the frontend and the Rust side share a single source of truth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeChatIngestConfig {
    pub enabled_mode: String,
    pub enabled_group_ids: Vec<String>,
}

impl Default for WeChatIngestConfig {
    fn default() -> Self {
        Self {
            enabled_mode: MODE_ALL.into(),
            enabled_group_ids: Vec::new(),
        }
    }
}

impl WeChatIngestConfig {
    /// Returns `true` when the event from `group_id` should proceed
    /// through the handler. `"all"` mode always passes; `"whitelist"`
    /// mode requires a non-empty `group_id` that appears in
    /// `enabled_group_ids`.
    #[must_use]
    pub fn allows(&self, group_id: Option<&str>) -> bool {
        if self.enabled_mode != MODE_WHITELIST {
            return true;
        }
        match group_id {
            Some(id) if !id.is_empty() => self.enabled_group_ids.iter().any(|g| g == id),
            _ => false,
        }
    }
}

/// Path of the persisted config under `~/.clawwiki/`.
pub fn default_config_path() -> PathBuf {
    wiki_store::default_root().join("wechat_ingest_config.json")
}

/// Best-effort load: returns the parsed config on success, the default
/// value when the file is missing, and `Err` only on corrupt JSON.
/// Exposed as a plain helper so HTTP handlers and tests can share a
/// single path resolver.
pub fn load_from(path: &Path) -> std::io::Result<WeChatIngestConfig> {
    match fs::read_to_string(path) {
        Ok(body) => serde_json::from_str(&body)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(WeChatIngestConfig::default()),
        Err(err) => Err(err),
    }
}

/// Atomic write via tmp file + rename. Creates the parent dir on
/// demand so callers never have to think about `~/.clawwiki/` bootstrap.
pub fn save_to(path: &Path, config: &WeChatIngestConfig) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    let body = serde_json::to_vec_pretty(config)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(&tmp, body)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Process-global config cache. Hydrated on first read; mutated by
/// POST handlers via [`update`].
static GLOBAL: OnceLock<Mutex<WeChatIngestConfig>> = OnceLock::new();

fn cell() -> &'static Mutex<WeChatIngestConfig> {
    GLOBAL.get_or_init(|| {
        let loaded = load_from(&default_config_path()).unwrap_or_else(|err| {
            eprintln!("[wechat ingest_config] load failed: {err}; using defaults");
            WeChatIngestConfig::default()
        });
        Mutex::new(loaded)
    })
}

/// Cheap read — clones the cached config. Used on every inbound
/// message so the middleware check is self-contained. Returning an
/// owned value avoids surfacing the `MutexGuard` lifetime to callers.
#[must_use]
pub fn read_snapshot() -> WeChatIngestConfig {
    match cell().lock() {
        Ok(g) => g.clone(),
        Err(e) => e.into_inner().clone(),
    }
}

/// Replace the cached config and flush to disk. Returns the value
/// that was ultimately stored (equal to `new_config` on success).
pub fn update(new_config: WeChatIngestConfig) -> std::io::Result<WeChatIngestConfig> {
    save_to(&default_config_path(), &new_config)?;
    let stored = new_config.clone();
    let mut guard = match cell().lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    *guard = new_config;
    Ok(stored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn default_mode_is_all_with_no_groups() {
        let cfg = WeChatIngestConfig::default();
        assert_eq!(cfg.enabled_mode, MODE_ALL);
        assert!(cfg.enabled_group_ids.is_empty());
    }

    #[test]
    fn all_mode_allows_any_group() {
        let cfg = WeChatIngestConfig::default();
        assert!(cfg.allows(None));
        assert!(cfg.allows(Some("grp1")));
        assert!(cfg.allows(Some("")));
    }

    #[test]
    fn whitelist_mode_rejects_missing_or_unknown() {
        let cfg = WeChatIngestConfig {
            enabled_mode: MODE_WHITELIST.into(),
            enabled_group_ids: vec!["grp1".into()],
        };
        assert!(!cfg.allows(None));
        assert!(!cfg.allows(Some("")));
        assert!(!cfg.allows(Some("other")));
        assert!(cfg.allows(Some("grp1")));
    }

    #[test]
    fn load_missing_file_returns_default() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nope.json");
        let cfg = load_from(&path).unwrap();
        assert_eq!(cfg, WeChatIngestConfig::default());
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cfg.json");
        let cfg = WeChatIngestConfig {
            enabled_mode: MODE_WHITELIST.into(),
            enabled_group_ids: vec!["grp1".into(), "grp2".into()],
        };
        save_to(&path, &cfg).unwrap();
        assert_eq!(load_from(&path).unwrap(), cfg);
    }
}

//! Persistent storage for WeChat iLink bot accounts.
//!
//! Mirrors the layout used by `~/.openclaw/openclaw-weixin/` so the format
//! looks familiar to anyone debugging both products, but lives under
//! `~/.warwolf/wechat/` to keep our state isolated from OpenClaw's.
//!
//! Layout:
//! ```text
//! ~/.warwolf/wechat/
//! ├── accounts.json                       # JSON array of account ids
//! └── accounts/
//!     ├── <bot-id>.json                   # WeixinAccountData
//!     ├── <bot-id>.sync.json              # { "get_updates_buf": "..." }
//!     └── <bot-id>.context-tokens.json    # { "<user_id>": "<context_token>" }
//! ```
//!
//! Bot ids written to disk use the **normalized** form (e.g.
//! `09cf1cc91c42-im-bot` instead of the wire format `09cf1cc91c42@im.bot`)
//! because Windows filesystems reject `@` in some contexts. We translate
//! between forms in `normalize_account_id` / `denormalize_account_id`.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::types::{WeixinAccountData, DEFAULT_BASE_URL};

/// Errors specific to account persistence.
#[derive(Debug, thiserror::Error)]
pub enum AccountError {
    #[error("filesystem error: {0}")]
    Io(#[from] io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("HOME directory could not be resolved")]
    NoHome,
    #[error("invalid account id: {0}")]
    InvalidId(String),
}

/// Resolve the root state directory for WeChat iLink credentials.
///
/// Resolution order:
///   1. `WARWOLF_WECHAT_DIR` env var override (used by tests + power users)
///   2. On Windows: `%LOCALAPPDATA%\warwolf\wechat\`
///      (the standard per-user, non-roaming, always-writable location)
///   3. On Unix: `$HOME/.warwolf/wechat/`
///
/// Why `LOCALAPPDATA` on Windows?
///   - `~/.warwolf` (i.e. `%USERPROFILE%\.warwolf`) inherits the user-profile
///     ACL which on some installations is RX-only for `BUILTIN\Users` if the
///     directory was originally created by an elevated process. That makes
///     a *non*-elevated `desktop-server` unable to write.
///   - `%LOCALAPPDATA%` is the canonical Windows location for non-roaming
///     per-user app state and is always full-control for the user.
pub fn state_dir() -> Result<PathBuf, AccountError> {
    if let Ok(override_dir) = std::env::var("WARWOLF_WECHAT_DIR") {
        return Ok(PathBuf::from(override_dir));
    }

    #[cfg(windows)]
    {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            return Ok(PathBuf::from(local).join("warwolf").join("wechat"));
        }
    }

    let home = home_dir().ok_or(AccountError::NoHome)?;
    Ok(home.join(".warwolf").join("wechat"))
}

/// Cross-platform best-effort home directory lookup. Used as the final
/// fallback when neither the env var override nor `%LOCALAPPDATA%` is set.
fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

fn accounts_dir() -> Result<PathBuf, AccountError> {
    Ok(state_dir()?.join("accounts"))
}

fn account_index_path() -> Result<PathBuf, AccountError> {
    Ok(state_dir()?.join("accounts.json"))
}

/// Convert `09cf1cc91c42@im.bot` ⇒ `09cf1cc91c42-im-bot` for use as a
/// filename. Idempotent — already-normalized ids pass through.
pub fn normalize_account_id(raw: &str) -> String {
    raw.replace('@', "-").replace('.', "-")
}

/// Inverse of `normalize_account_id` for `@im.bot` / `@im.wechat` suffixes.
/// Returns `None` if the id doesn't match a known suffix pattern.
pub fn denormalize_account_id(normalized: &str) -> Option<String> {
    if let Some(prefix) = normalized.strip_suffix("-im-bot") {
        return Some(format!("{prefix}@im.bot"));
    }
    if let Some(prefix) = normalized.strip_suffix("-im-wechat") {
        return Some(format!("{prefix}@im.wechat"));
    }
    None
}

fn validate_id(id: &str) -> Result<(), AccountError> {
    if id.is_empty()
        || id.contains("..")
        || id.contains('/')
        || id.contains('\\')
        || id.contains('\0')
    {
        return Err(AccountError::InvalidId(id.to_string()));
    }
    Ok(())
}

fn account_file_path(id: &str) -> Result<PathBuf, AccountError> {
    validate_id(id)?;
    Ok(accounts_dir()?.join(format!("{id}.json")))
}

fn sync_file_path(id: &str) -> Result<PathBuf, AccountError> {
    validate_id(id)?;
    Ok(accounts_dir()?.join(format!("{id}.sync.json")))
}

fn context_tokens_file_path(id: &str) -> Result<PathBuf, AccountError> {
    validate_id(id)?;
    Ok(accounts_dir()?.join(format!("{id}.context-tokens.json")))
}

fn openid_sessions_file_path(id: &str) -> Result<PathBuf, AccountError> {
    validate_id(id)?;
    Ok(accounts_dir()?.join(format!("{id}.openid-sessions.json")))
}

// ── Account index ────────────────────────────────────────────────────

/// List all bot account ids registered in the index file. Returns an
/// empty `Vec` if the file doesn't exist or is malformed.
pub fn list_account_ids() -> Result<Vec<String>, AccountError> {
    let path = account_index_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(&path)?;
    let parsed: Vec<String> =
        serde_json::from_str(&raw).unwrap_or_default();
    Ok(parsed
        .into_iter()
        .filter(|s| !s.trim().is_empty())
        .collect())
}

/// Add `account_id` to the index file. No-op if already present.
pub fn register_account_id(account_id: &str) -> Result<(), AccountError> {
    validate_id(account_id)?;
    let dir = state_dir()?;
    fs::create_dir_all(&dir)?;
    let mut current = list_account_ids()?;
    if current.iter().any(|id| id == account_id) {
        return Ok(());
    }
    current.push(account_id.to_string());
    let path = account_index_path()?;
    fs::write(path, serde_json::to_vec_pretty(&current)?)?;
    Ok(())
}

/// Remove `account_id` from the index file (does NOT delete credential files).
pub fn unregister_account_id(account_id: &str) -> Result<(), AccountError> {
    let mut current = list_account_ids()?;
    let before = current.len();
    current.retain(|id| id != account_id);
    if current.len() != before {
        let path = account_index_path()?;
        fs::write(path, serde_json::to_vec_pretty(&current)?)?;
    }
    Ok(())
}

// ── Per-account credential files ─────────────────────────────────────

/// Read account data from disk. Returns `None` if no file exists.
pub fn load_account(account_id: &str) -> Result<Option<WeixinAccountData>, AccountError> {
    let path = account_file_path(account_id)?;
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path)?;
    let data: WeixinAccountData = serde_json::from_str(&raw)?;
    Ok(Some(data))
}

/// Persist account data to disk, also registering the id in the index file.
/// Existing fields are merged: passing `None` preserves the prior value.
pub fn save_account(
    account_id: &str,
    update: WeixinAccountData,
) -> Result<WeixinAccountData, AccountError> {
    validate_id(account_id)?;
    let dir = accounts_dir()?;
    fs::create_dir_all(&dir)?;

    let existing = load_account(account_id)?.unwrap_or_default();
    let merged = WeixinAccountData {
        token: update.token.or(existing.token),
        saved_at: Some(now_iso8601()),
        base_url: update
            .base_url
            .or(existing.base_url)
            .or_else(|| Some(DEFAULT_BASE_URL.to_string())),
        user_id: update.user_id.or(existing.user_id),
    };

    let path = account_file_path(account_id)?;
    fs::write(&path, serde_json::to_vec_pretty(&merged)?)?;

    // Best-effort permission tighten on Unix; no-op on Windows.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }

    register_account_id(account_id)?;
    Ok(merged)
}

/// Delete an account's credential file, sync cursor, and context-token cache.
/// Also unregisters from the index. Best-effort: missing files are ignored.
pub fn clear_account(account_id: &str) -> Result<(), AccountError> {
    let _ = fs::remove_file(account_file_path(account_id)?);
    let _ = fs::remove_file(sync_file_path(account_id)?);
    let _ = fs::remove_file(context_tokens_file_path(account_id)?);
    unregister_account_id(account_id)?;
    Ok(())
}

// ── getUpdates cursor ────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SyncBuf {
    get_updates_buf: String,
}

/// Load the persisted `get_updates_buf` cursor for an account. Returns an
/// empty string when no cursor file exists (the protocol expects `""` on
/// the very first request).
pub fn load_sync_buf(account_id: &str) -> Result<String, AccountError> {
    let path = sync_file_path(account_id)?;
    if !path.exists() {
        return Ok(String::new());
    }
    let raw = fs::read_to_string(&path)?;
    let parsed: SyncBuf = serde_json::from_str(&raw).unwrap_or_default();
    Ok(parsed.get_updates_buf)
}

/// Persist the `get_updates_buf` cursor returned by the server.
pub fn save_sync_buf(account_id: &str, buf: &str) -> Result<(), AccountError> {
    let path = sync_file_path(account_id)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = SyncBuf {
        get_updates_buf: buf.to_string(),
    };
    fs::write(path, serde_json::to_vec(&payload)?)?;
    Ok(())
}

// ── Per-user context_token cache ─────────────────────────────────────

/// In-disk map: WeChat user_id → most recent context_token observed.
/// Required when sending replies, since the protocol forbids reusing a
/// stale context_token.
pub fn load_context_tokens(account_id: &str) -> Result<HashMap<String, String>, AccountError> {
    let path = context_tokens_file_path(account_id)?;
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let raw = fs::read_to_string(&path)?;
    let parsed: HashMap<String, String> =
        serde_json::from_str(&raw).unwrap_or_default();
    Ok(parsed)
}

/// Overwrite the context-token cache for an account.
pub fn save_context_tokens(
    account_id: &str,
    tokens: &HashMap<String, String>,
) -> Result<(), AccountError> {
    let path = context_tokens_file_path(account_id)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(tokens)?)?;
    Ok(())
}

/// Update a single user's context token, persisting the new value to disk.
pub fn upsert_context_token(
    account_id: &str,
    user_id: &str,
    context_token: &str,
) -> Result<(), AccountError> {
    if user_id.is_empty() || context_token.is_empty() {
        return Ok(());
    }
    let mut current = load_context_tokens(account_id)?;
    current.insert(user_id.to_string(), context_token.to_string());
    save_context_tokens(account_id, &current)
}

// ── Per-WeChat-user → desktop session mapping ────────────────────────

/// Load the persistent mapping `WeChat user_id → desktop_session_id` for
/// an account. Used by the DesktopAgentHandler to keep each WeChat user's
/// conversation in its own dedicated desktop session across restarts.
///
/// Returns an empty map if the file doesn't exist (e.g. first launch).
pub fn load_openid_sessions(account_id: &str) -> Result<HashMap<String, String>, AccountError> {
    let path = openid_sessions_file_path(account_id)?;
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let raw = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

/// Overwrite the openid → session mapping for an account.
pub fn save_openid_sessions(
    account_id: &str,
    mapping: &HashMap<String, String>,
) -> Result<(), AccountError> {
    let path = openid_sessions_file_path(account_id)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(mapping)?)?;
    Ok(())
}

/// Atomic upsert: insert/replace a single openid → session pair and
/// persist to disk in one call.
pub fn upsert_openid_session(
    account_id: &str,
    openid: &str,
    session_id: &str,
) -> Result<(), AccountError> {
    if openid.is_empty() || session_id.is_empty() {
        return Ok(());
    }
    let mut current = load_openid_sessions(account_id)?;
    current.insert(openid.to_string(), session_id.to_string());
    save_openid_sessions(account_id, &current)
}

// ── Helpers ──────────────────────────────────────────────────────────

fn now_iso8601() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Minimal ISO 8601 — good enough for a "savedAt" stamp; OpenClaw
    // also writes ISO 8601 here.
    format_iso8601_utc(now)
}

fn format_iso8601_utc(secs: u64) -> String {
    // Days/months/years computation without an external dep. Good for
    // arbitrary epochs in the year range we care about (2026..2099).
    let days = (secs / 86_400) as i64;
    let secs_of_day = secs % 86_400;
    let hour = secs_of_day / 3_600;
    let minute = (secs_of_day % 3_600) / 60;
    let second = secs_of_day % 60;

    // Civil-from-days algorithm (Howard Hinnant).
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y, m, d, hour, minute, second
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serialize the test cases that touch process-wide env state so they
    /// don't race against each other.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Set the `WARWOLF_WECHAT_DIR` env var to a fresh tempdir for one test.
    /// The returned guard restores the previous value on drop.
    struct TempDirGuard {
        _guard: std::sync::MutexGuard<'static, ()>,
        _tmp: tempfile::TempDir,
        prev: Option<String>,
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            match &self.prev {
                Some(v) => std::env::set_var("WARWOLF_WECHAT_DIR", v),
                None => std::env::remove_var("WARWOLF_WECHAT_DIR"),
            }
        }
    }

    fn temp_state_dir() -> TempDirGuard {
        let guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = tempfile::tempdir().expect("tempdir");
        let prev = std::env::var("WARWOLF_WECHAT_DIR").ok();
        std::env::set_var("WARWOLF_WECHAT_DIR", tmp.path());
        TempDirGuard {
            _guard: guard,
            _tmp: tmp,
            prev,
        }
    }

    #[test]
    fn normalize_round_trip() {
        let raw = "09cf1cc91c42@im.bot";
        let normalized = normalize_account_id(raw);
        assert_eq!(normalized, "09cf1cc91c42-im-bot");
        assert_eq!(
            denormalize_account_id(&normalized),
            Some(raw.to_string())
        );
    }

    #[test]
    fn normalize_idempotent() {
        let already = "09cf1cc91c42-im-bot";
        assert_eq!(normalize_account_id(already), already);
    }

    #[test]
    fn validate_rejects_traversal() {
        assert!(validate_id("../etc").is_err());
        assert!(validate_id("foo/bar").is_err());
        assert!(validate_id("foo\\bar").is_err());
        assert!(validate_id("").is_err());
        assert!(validate_id("good-id").is_ok());
    }

    #[test]
    fn save_load_round_trip() {
        let _g = temp_state_dir();
        let id = "test-bot-1";
        let saved = save_account(
            id,
            WeixinAccountData {
                token: Some("tok-abc".to_string()),
                base_url: Some("https://ilinkai.weixin.qq.com".to_string()),
                user_id: Some("alice@im.wechat".to_string()),
                ..Default::default()
            },
        )
        .expect("save");
        assert_eq!(saved.token.as_deref(), Some("tok-abc"));
        assert!(saved.saved_at.is_some());

        let loaded = load_account(id).expect("load").expect("present");
        assert_eq!(loaded.token, saved.token);
        assert_eq!(loaded.user_id.as_deref(), Some("alice@im.wechat"));
    }

    #[test]
    fn save_merges_existing_fields() {
        let _g = temp_state_dir();
        let id = "test-bot-2";
        save_account(
            id,
            WeixinAccountData {
                token: Some("first-token".to_string()),
                base_url: Some("https://ilinkai.weixin.qq.com".to_string()),
                ..Default::default()
            },
        )
        .expect("save 1");

        // Update only the user_id; token and base_url should be preserved.
        save_account(
            id,
            WeixinAccountData {
                user_id: Some("bob@im.wechat".to_string()),
                ..Default::default()
            },
        )
        .expect("save 2");

        let loaded = load_account(id).expect("load").unwrap();
        assert_eq!(loaded.token.as_deref(), Some("first-token"));
        assert_eq!(loaded.user_id.as_deref(), Some("bob@im.wechat"));
    }

    #[test]
    fn account_index_dedup() {
        let _g = temp_state_dir();
        register_account_id("a").unwrap();
        register_account_id("b").unwrap();
        register_account_id("a").unwrap(); // dup
        let ids = list_account_ids().unwrap();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"a".to_string()));
        assert!(ids.contains(&"b".to_string()));

        unregister_account_id("a").unwrap();
        let ids = list_account_ids().unwrap();
        assert_eq!(ids, vec!["b".to_string()]);
    }

    #[test]
    fn sync_buf_round_trip() {
        let _g = temp_state_dir();
        let id = "test-bot-3";
        // Need to create the accounts dir first.
        fs::create_dir_all(accounts_dir().unwrap()).unwrap();

        assert_eq!(load_sync_buf(id).unwrap(), "");
        save_sync_buf(id, "cursor-abc-123").unwrap();
        assert_eq!(load_sync_buf(id).unwrap(), "cursor-abc-123");
    }

    #[test]
    fn context_tokens_round_trip() {
        let _g = temp_state_dir();
        let id = "test-bot-4";
        fs::create_dir_all(accounts_dir().unwrap()).unwrap();

        assert!(load_context_tokens(id).unwrap().is_empty());
        upsert_context_token(id, "user-1@im.wechat", "ctx-1").unwrap();
        upsert_context_token(id, "user-2@im.wechat", "ctx-2").unwrap();
        upsert_context_token(id, "user-1@im.wechat", "ctx-1-updated").unwrap();

        let loaded = load_context_tokens(id).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.get("user-1@im.wechat").unwrap(), "ctx-1-updated");
        assert_eq!(loaded.get("user-2@im.wechat").unwrap(), "ctx-2");
    }

    #[test]
    fn clear_account_removes_all_files() {
        let _g = temp_state_dir();
        let id = "test-bot-5";
        save_account(
            id,
            WeixinAccountData {
                token: Some("t".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        save_sync_buf(id, "cursor").unwrap();
        upsert_context_token(id, "u", "c").unwrap();

        clear_account(id).unwrap();
        assert!(load_account(id).unwrap().is_none());
        assert_eq!(load_sync_buf(id).unwrap(), "");
        assert!(load_context_tokens(id).unwrap().is_empty());
        assert!(!list_account_ids().unwrap().contains(&id.to_string()));
    }

    #[test]
    fn iso8601_format_basic() {
        // 2026-04-08 06:00:00 UTC corresponds to a known epoch.
        let s = format_iso8601_utc(1_775_628_000);
        assert!(s.starts_with("2026-04-"), "got: {s}");
        assert!(s.ends_with(":00Z"));
    }

    #[test]
    fn openid_sessions_round_trip() {
        let _g = temp_state_dir();
        let id = "test-bot-6";
        fs::create_dir_all(accounts_dir().unwrap()).unwrap();

        assert!(load_openid_sessions(id).unwrap().is_empty());
        upsert_openid_session(id, "alice@im.wechat", "desktop-session-1").unwrap();
        upsert_openid_session(id, "bob@im.wechat", "desktop-session-2").unwrap();
        upsert_openid_session(id, "alice@im.wechat", "desktop-session-99").unwrap();

        let loaded = load_openid_sessions(id).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(
            loaded.get("alice@im.wechat").unwrap(),
            "desktop-session-99"
        );
        assert_eq!(loaded.get("bob@im.wechat").unwrap(), "desktop-session-2");
    }

    #[test]
    fn openid_sessions_ignores_empty_inputs() {
        let _g = temp_state_dir();
        let id = "test-bot-7";
        fs::create_dir_all(accounts_dir().unwrap()).unwrap();

        upsert_openid_session(id, "", "session-x").unwrap();
        upsert_openid_session(id, "openid-x", "").unwrap();
        assert!(load_openid_sessions(id).unwrap().is_empty());
    }
}

//! Persistent storage for WeChat Customer Service configuration.
//!
//! Layout:
//!   ~/.warwolf/wechat-kefu/
//!     config.json              # KefuConfig
//!     cursor.json              # { "cursor": "..." }
//!     session-map.json         # { "external_userid": "desktop_session_id" }

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use super::types::KefuConfig;
pub use crate::wechat_ilink::AccountError;

const DIR_NAME: &str = "wechat-kefu";

fn state_dir() -> Result<PathBuf, AccountError> {
    if let Ok(dir) = std::env::var("WARWOLF_WECHAT_KEFU_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let home = std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .ok_or(AccountError::NoHome)?;
    Ok(home.join(".warwolf").join(DIR_NAME))
}

fn ensure_dir() -> Result<PathBuf, AccountError> {
    let dir = state_dir()?;
    fs::create_dir_all(&dir).map_err(AccountError::Io)?;
    Ok(dir)
}

// --- Config ---

pub fn load_config() -> Result<Option<KefuConfig>, AccountError> {
    let path = state_dir()?.join("config.json");
    if !path.exists() {
        return Ok(None);
    }
    let data = fs::read_to_string(&path).map_err(AccountError::Io)?;
    let config: KefuConfig = serde_json::from_str(&data).map_err(AccountError::Json)?;
    Ok(Some(config))
}

pub fn save_config(config: &KefuConfig) -> Result<(), AccountError> {
    let dir = ensure_dir()?;
    let mut config = config.clone();
    config.saved_at = Some(now_iso8601());
    let json = serde_json::to_string_pretty(&config).map_err(AccountError::Json)?;
    let path = dir.join("config.json");
    fs::write(&path, json).map_err(AccountError::Io)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn clear_config() -> Result<(), AccountError> {
    let dir = state_dir()?;
    let _ = fs::remove_file(dir.join("config.json"));
    let _ = fs::remove_file(dir.join("cursor.json"));
    let _ = fs::remove_file(dir.join("session-map.json"));
    Ok(())
}

// --- Cursor ---

pub fn load_cursor() -> Result<String, AccountError> {
    let path = state_dir()?.join("cursor.json");
    if !path.exists() {
        return Ok(String::new());
    }
    let data = fs::read_to_string(&path).map_err(AccountError::Io)?;
    let parsed: serde_json::Value = serde_json::from_str(&data).map_err(AccountError::Json)?;
    Ok(parsed
        .get("cursor")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string())
}

pub fn save_cursor(cursor: &str) -> Result<(), AccountError> {
    let dir = ensure_dir()?;
    let json = serde_json::json!({ "cursor": cursor });
    fs::write(
        dir.join("cursor.json"),
        serde_json::to_string_pretty(&json).map_err(AccountError::Json)?,
    )
    .map_err(AccountError::Io)
}

// --- Session map ---

pub fn load_session_map() -> Result<HashMap<String, String>, AccountError> {
    let path = state_dir()?.join("session-map.json");
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let data = fs::read_to_string(&path).map_err(AccountError::Io)?;
    let map: HashMap<String, String> = serde_json::from_str(&data).map_err(AccountError::Json)?;
    Ok(map)
}

pub fn upsert_session(external_userid: &str, session_id: &str) -> Result<(), AccountError> {
    let dir = ensure_dir()?;
    let mut map = load_session_map().unwrap_or_default();
    map.insert(external_userid.to_string(), session_id.to_string());
    let json = serde_json::to_string_pretty(&map).map_err(AccountError::Json)?;
    fs::write(dir.join("session-map.json"), json).map_err(AccountError::Io)
}

fn now_iso8601() -> String {
    use time::format_description::well_known::Rfc3339;
    time::OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string())
}

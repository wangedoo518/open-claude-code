//! OpenClaw CLI binary detection and version checking.
//!
//! Matches cherry-studio's dual-path detection:
//! 1. Managed binary: ~/.warwolf/bin/openclaw (installed by warwolf)
//! 2. System PATH: old npm-installed versions (needsMigration)

use std::path::PathBuf;
use std::process::Command;

/// Get the managed binary directory: ~/.warwolf/bin/
pub fn managed_bin_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".warwolf").join("bin"))
}

/// Get the managed binary path: ~/.warwolf/bin/openclaw
pub fn managed_binary_path() -> Option<PathBuf> {
    let bin_dir = managed_bin_dir()?;
    #[cfg(windows)]
    {
        Some(bin_dir.join("openclaw.exe"))
    }
    #[cfg(not(windows))]
    {
        Some(bin_dir.join("openclaw"))
    }
}

fn read_install_state() -> Option<serde_json::Value> {
    let home = dirs::home_dir()?;
    let state_file = home.join(".warwolf").join("openclaw-install-state.json");
    let content = std::fs::read_to_string(state_file).ok()?;
    serde_json::from_str(&content).ok()
}

fn installed_path_from_state() -> Option<String> {
    let state = read_install_state()?;
    let binary_path = state.get("binary_path")?.as_str()?.trim();
    if binary_path.is_empty() {
        return None;
    }
    if !std::path::Path::new(binary_path).exists() {
        return None;
    }
    Some(binary_path.to_string())
}

/// Check installation status matching cherry-studio's checkInstalled():
/// - If managed binary exists → installed: true, needsMigration: false
/// - If not managed but found in PATH → installed: false, needsMigration: true
/// - If not found anywhere → installed: false, needsMigration: false
pub fn check_installed() -> (bool, Option<String>, bool) {
    // 1. Check managed binary location (~/.warwolf/bin/openclaw)
    if let Some(managed_path) = managed_binary_path() {
        if managed_path.exists() {
            let path_str = managed_path.to_string_lossy().to_string();
            return (true, Some(path_str), false);
        }
    }

    // 2. Check Warwolf install state (npm-installed or reused system binary)
    if let Some(state_path) = installed_path_from_state() {
        if health_check(&state_path) {
            return (true, Some(state_path), false);
        }
    }

    // 3. Check system PATH for old npm-installed version
    if let Some(env_path) = find_in_system_path() {
        // Found in PATH but not in managed location → needs migration
        return (false, Some(env_path), true);
    }

    // 4. Not found anywhere
    (false, None, false)
}

/// Find `openclaw` binary in system PATH (not managed location).
fn find_in_system_path() -> Option<String> {
    #[cfg(unix)]
    {
        let output = Command::new("which").arg("openclaw").output().ok()?;
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                // Skip if it's our managed path
                if let Some(managed) = managed_binary_path() {
                    if path == managed.to_string_lossy() {
                        return None;
                    }
                }
                return Some(path);
            }
        }
    }

    #[cfg(windows)]
    {
        let output = Command::new("where").arg("openclaw").output().ok()?;
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !path.is_empty() {
                if let Some(managed) = managed_binary_path() {
                    if path == managed.to_string_lossy() {
                        return None;
                    }
                }
                return Some(path);
            }
        }
    }

    None
}

/// Find the openclaw binary. Only uses managed binary location.
/// Never falls back to PATH (matching cherry-studio's findOpenClawBinary).
pub fn find_openclaw_binary() -> Option<String> {
    if let Some(managed) = managed_binary_path() {
        if managed.exists() {
            return Some(managed.to_string_lossy().to_string());
        }
    }
    if let Some(state_path) = installed_path_from_state() {
        return Some(state_path);
    }

    // Also check system PATH as fallback for existing installs
    find_in_system_path()
}

/// Run `openclaw --version` and return the version string.
pub fn get_openclaw_version(binary_path: &str) -> Option<String> {
    let output = Command::new(binary_path)
        .arg("--version")
        .output()
        .ok()?;
    if output.status.success() {
        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let version = version
            .strip_prefix("openclaw ")
            .or_else(|| version.strip_prefix("OpenClaw "))
            .unwrap_or(&version)
            .to_string();
        if !version.is_empty() {
            return Some(version);
        }
    }
    None
}

/// Run a health check on the openclaw binary.
pub fn health_check(binary_path: &str) -> bool {
    get_openclaw_version(binary_path).is_some()
}

/// Get Node.js version from the system.
pub fn get_node_version() -> Option<String> {
    let output = Command::new("node").arg("--version").output().ok()?;
    if output.status.success() {
        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Some(version);
    }
    None
}

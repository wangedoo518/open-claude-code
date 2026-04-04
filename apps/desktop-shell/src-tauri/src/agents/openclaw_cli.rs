//! OpenClaw CLI binary detection and version checking.

use std::process::Command;

/// Check if `openclaw` binary exists on the system and return its path.
pub fn find_openclaw_binary() -> Option<String> {
    // Try `which openclaw` on Unix
    #[cfg(unix)]
    {
        let output = Command::new("which").arg("openclaw").output().ok()?;
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(path);
            }
        }
    }

    // Try `where openclaw` on Windows
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
                return Some(path);
            }
        }
    }

    None
}

/// Run `openclaw --version` and return the version string.
pub fn get_openclaw_version(binary_path: &str) -> Option<String> {
    let output = Command::new(binary_path)
        .arg("--version")
        .output()
        .ok()?;
    if output.status.success() {
        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        // Strip "openclaw " prefix if present
        let version = version
            .strip_prefix("openclaw ")
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

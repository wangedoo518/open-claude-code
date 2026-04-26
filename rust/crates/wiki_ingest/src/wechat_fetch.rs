//! WeChat article fetcher — Playwright-based sidecar for mp.weixin.qq.com.
//!
//! Spawns `python wechat_fetcher.py` to render the page in a real browser
//! and extract the article content, bypassing WeChat's JS-based anti-scraping.

use crate::IngestResult;
use std::path::Path;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

const TIMEOUT_SECS: u64 = 90;
const MAX_OUTPUT_BYTES: usize = 5 * 1024 * 1024;

fn worker_script_path() -> std::path::PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));

    let candidates = [
        exe_dir.as_ref().map(|d| d.join("wechat_fetcher.py")),
        Some(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src")
                .join("wechat_fetcher.py"),
        ),
    ];

    for candidate in candidates.iter().flatten() {
        if candidate.exists() {
            return candidate.clone();
        }
    }

    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("wechat_fetcher.py")
}

/// Check if Playwright is available.
pub async fn check_environment() -> Result<String, String> {
    let output = Command::new("python")
        .args([
            "-c",
            "from playwright.sync_api import sync_playwright; print('ok')",
        ])
        .output()
        .await
        .map_err(|e| format!("Python not found: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("No module named") {
            return Err(
                "Playwright not installed. Run: pip install playwright && python -m playwright install chromium"
                    .to_string(),
            );
        }
        return Err(format!("Python error: {stderr}"));
    }

    Ok("playwright available".to_string())
}

/// Fetch a WeChat article via Playwright and return as IngestResult.
/// Fetch any URL via Playwright + defuddle. Works for WeChat and all other sites.
pub async fn fetch_wechat_article(url: &str) -> Result<IngestResult, crate::IngestError> {
    let worker = worker_script_path();
    let request = serde_json::json!({ "url": url });

    let mut child = Command::new("python")
        .arg(worker.to_string_lossy().as_ref())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| {
            crate::IngestError::Parse(format!(
                "Failed to spawn Python: {e}. Is Python installed and on PATH?"
            ))
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(request.to_string().as_bytes())
            .await
            .map_err(|e| crate::IngestError::Parse(format!("stdin write failed: {e}")))?;
        drop(stdin);
    }

    let output = timeout(Duration::from_secs(TIMEOUT_SECS), child.wait_with_output())
        .await
        .map_err(|_| {
            crate::IngestError::Parse(format!("WeChat fetch timed out after {TIMEOUT_SECS}s"))
        })?
        .map_err(|e| crate::IngestError::Parse(format!("Process error: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.len() > MAX_OUTPUT_BYTES {
        return Err(crate::IngestError::TooLarge {
            bytes: stdout.len(),
            max: MAX_OUTPUT_BYTES,
        });
    }

    let response: serde_json::Value = serde_json::from_str(&stdout).map_err(|e| {
        crate::IngestError::Parse(format!(
            "Invalid JSON from wechat_fetcher: {e}. Output: {}",
            &stdout[..stdout.len().min(200)]
        ))
    })?;

    if response.get("ok").and_then(|v| v.as_bool()) != Some(true) {
        let error = response
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        return Err(crate::IngestError::Parse(format!(
            "WeChat fetch failed: {error}"
        )));
    }

    let title = response
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let author = response
        .get("author")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let publish_time = response
        .get("publish_time")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let markdown = response
        .get("markdown")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Build frontmatter-style body. Author / published lines stay raw
    // (single-line metadata, no HTML entities expected) but the article
    // markdown itself goes through `sanitize_markdown` to decode HTML
    // entities and drop truncated data:image/svg+xml stubs that defuddle
    // sometimes leaks through (regression from 2026-04 mp.weixin grab).
    let cleaned_markdown = crate::sanitize_markdown(&markdown);
    let mut body = String::new();
    if !author.is_empty() {
        body.push_str(&format!("_Author: {author}_\n\n"));
    }
    if !publish_time.is_empty() {
        body.push_str(&format!("_Published: {publish_time}_\n\n"));
    }
    body.push_str(&cleaned_markdown);

    Ok(IngestResult {
        title: if title.is_empty() {
            "WeChat Article".to_string()
        } else {
            title
        },
        body,
        source_url: Some(url.to_string()),
        source: "wechat-article".to_string(),
    })
}

//! MarkItDown Python sidecar adapter.
//!
//! Spawns `python markitdown_worker.py` as a subprocess, feeds it a
//! file path via stdin JSON, and parses the stdout JSON response into
//! an `IngestResult`.
//!
//! Falls back gracefully when Python or markitdown is not installed.

use crate::IngestResult;
use std::path::Path;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

/// Maximum time to wait for the Python process (seconds).
const TIMEOUT_SECS: u64 = 120;

/// Maximum output size from the Python worker (bytes).
const MAX_OUTPUT_BYTES: usize = 10 * 1024 * 1024; // 10 MiB

/// Path to the worker script, relative to this source file.
/// At runtime this resolves via the executable's directory.
fn worker_script_path() -> std::path::PathBuf {
    // Try several locations in order:
    // 1. Next to the running executable
    // 2. In the wiki_ingest crate source (development)
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));

    let candidates = [
        exe_dir
            .as_ref()
            .map(|d| d.join("markitdown_worker.py")),
        Some(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src")
                .join("markitdown_worker.py"),
        ),
    ];

    for candidate in candidates.iter().flatten() {
        if candidate.exists() {
            return candidate.clone();
        }
    }

    // Fallback: assume it's in the crate source dir
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("markitdown_worker.py")
}

/// Check if Python 3 and markitdown are available.
pub async fn check_environment() -> Result<String, String> {
    let output = Command::new("python")
        .args(["-c", "import markitdown; print(markitdown.__version__)"])
        .output()
        .await
        .map_err(|e| format!("Python not found: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("No module named") {
            return Err("markitdown not installed. Run: pip install 'markitdown[all]'".to_string());
        }
        return Err(format!("Python error: {stderr}"));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Convert a file to Markdown using the MarkItDown Python sidecar.
pub async fn extract_via_markitdown(path: &Path) -> Result<IngestResult, crate::IngestError> {
    if !path.exists() {
        return Err(crate::IngestError::NotFound(
            path.display().to_string(),
        ));
    }

    let worker = worker_script_path();
    let request = serde_json::json!({ "path": path.to_string_lossy() });

    let mut child = Command::new("python")
        .arg(worker.to_string_lossy().as_ref())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| crate::IngestError::Parse(format!(
            "Failed to spawn Python: {e}. Is Python installed?"
        )))?;

    // Write request to stdin
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(request.to_string().as_bytes())
            .await
            .map_err(|e| crate::IngestError::Parse(format!("stdin write failed: {e}")))?;
        drop(stdin); // Close stdin so the child reads EOF
    }

    // Wait with timeout
    let output = timeout(Duration::from_secs(TIMEOUT_SECS), child.wait_with_output())
        .await
        .map_err(|_| crate::IngestError::Parse(format!(
            "MarkItDown timed out after {TIMEOUT_SECS}s"
        )))?
        .map_err(|e| crate::IngestError::Parse(format!("Process error: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.len() > MAX_OUTPUT_BYTES {
        return Err(crate::IngestError::TooLarge {
            bytes: stdout.len(),
            max: MAX_OUTPUT_BYTES,
        });
    }

    // Parse JSON response
    let response: serde_json::Value = serde_json::from_str(&stdout)
        .map_err(|e| crate::IngestError::Parse(format!(
            "Invalid JSON from MarkItDown worker: {e}. Output: {}",
            &stdout[..stdout.len().min(200)]
        )))?;

    if response.get("ok").and_then(|v| v.as_bool()) != Some(true) {
        let error = response
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        return Err(crate::IngestError::Parse(format!(
            "MarkItDown conversion failed: {error}"
        )));
    }

    let title = response
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("untitled")
        .to_string();
    let markdown = response
        .get("markdown")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let source = response
        .get("source")
        .and_then(|v| v.as_str())
        .unwrap_or("file")
        .to_string();

    Ok(IngestResult {
        title,
        body: markdown,
        source_url: Some(path.display().to_string()),
        source,
    })
}

/// List of file extensions that MarkItDown can handle.
pub fn supported_extensions() -> &'static [&'static str] {
    &[
        "pdf", "docx", "doc", "pptx", "ppt", "xlsx", "xls",
        "jpg", "jpeg", "png", "gif", "webp", "svg",
        "mp3", "wav", "m4a", "ogg", "flac",
        "mp4", "mkv", "avi", "mov",
        "html", "htm", "csv", "json", "xml",
        "epub", "ipynb", "zip",
    ]
}

/// Check if a file extension is supported by MarkItDown.
pub fn is_supported(ext: &str) -> bool {
    supported_extensions().contains(&ext.to_lowercase().as_str())
}

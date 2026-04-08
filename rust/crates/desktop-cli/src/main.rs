//! `ocl` — open-claude-code CLI client.
//!
//! Thin wrapper over the desktop-server HTTP API. Lets users script
//! agent workflows from the shell without going through the Tauri
//! desktop app.
//!
//! ── Usage ───────────────────────────────────────────────────────
//!
//! ocl health                           # GET /healthz
//! ocl sessions list                    # GET /api/desktop/workbench
//! ocl sessions show <id>               # GET /api/desktop/sessions/<id>
//! ocl sessions new                     # POST /api/desktop/sessions
//! ocl sessions send <id> <message>     # POST .../messages
//! ocl sessions cancel <id>             # POST .../cancel
//! ocl sessions compact <id>            # POST .../compact
//! ocl sessions status <id> <state>     # POST .../lifecycle
//! ocl sessions flag <id> <true|false>  # POST .../flag
//! ocl mcp probe <project_path>         # POST /debug/mcp/probe
//! ocl mcp call <name> <args_json>      # POST /debug/mcp/call
//! ocl permission-mode [get|set <mode>] # GET/POST settings/permission-mode
//!
//! ── Global flags ────────────────────────────────────────────────
//!
//! --server <url>    Override base URL (default: http://127.0.0.1:4357)
//! --json            Emit raw JSON (useful for piping to jq)
//! --help, -h        Print help

use std::process::ExitCode;
use std::time::Duration;

use serde_json::Value;

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:4357";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("failed to start tokio runtime: {e}");
            return ExitCode::FAILURE;
        }
    };

    match rt.block_on(run(args)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::UsageError(msg)) => {
            eprintln!("Error: {msg}\n");
            print_usage();
            ExitCode::from(2)
        }
        Err(CliError::HttpError(msg)) => {
            eprintln!("HTTP error: {msg}");
            ExitCode::FAILURE
        }
        Err(CliError::ApiError { status, body }) => {
            eprintln!("API error ({status}): {body}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Debug)]
enum CliError {
    UsageError(String),
    HttpError(String),
    ApiError { status: u16, body: String },
}

impl From<reqwest::Error> for CliError {
    fn from(e: reqwest::Error) -> Self {
        Self::HttpError(e.to_string())
    }
}

struct Config {
    base_url: String,
    json: bool,
    client: reqwest::Client,
}

async fn run(mut args: Vec<String>) -> Result<(), CliError> {
    // Pull out global flags first.
    let mut base_url = std::env::var("OCL_SERVER")
        .unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
    let mut json = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--server" => {
                if i + 1 >= args.len() {
                    return Err(CliError::UsageError("--server requires a value".into()));
                }
                base_url = args[i + 1].clone();
                args.drain(i..=i + 1);
            }
            "--json" => {
                json = true;
                args.remove(i);
            }
            "--help" | "-h" => {
                print_usage();
                return Ok(());
            }
            _ => i += 1,
        }
    }

    if args.is_empty() {
        print_usage();
        return Ok(());
    }

    let config = Config {
        base_url,
        json,
        client: reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new()),
    };

    match args[0].as_str() {
        "health" => cmd_health(&config).await,
        "sessions" => cmd_sessions(&config, &args[1..]).await,
        "mcp" => cmd_mcp(&config, &args[1..]).await,
        "permission-mode" => cmd_permission_mode(&config, &args[1..]).await,
        other => Err(CliError::UsageError(format!("unknown command: {other}"))),
    }
}

fn print_usage() {
    println!(
        "{}",
        r#"ocl — open-claude-code CLI client

USAGE:
  ocl [FLAGS] <COMMAND> [ARGS...]

FLAGS:
  --server <url>    Backend URL (default: http://127.0.0.1:4357)
  --json            Emit raw JSON response
  -h, --help        Show this help

COMMANDS:
  health                                       Server health check
  sessions list                                List all sessions
  sessions show <id>                           Show session detail
  sessions new [--title <t>] [--path <p>]      Create a new session
  sessions send <id> <message>                 Send a message
  sessions cancel <id>                         Cancel a running turn
  sessions compact <id>                        Compact session messages
  sessions status <id> <todo|in_progress|needs_review|done|archived>
  sessions flag <id> <true|false>              Flag or unflag session
  mcp probe <project_path>                     Discover MCP tools
  mcp call <qualified_name> <args_json>        Call an MCP tool
  permission-mode [get] [--project-path <p>]   Show mode (default: cwd)
  permission-mode set <mode> [--project-path <p>]
                                                Set mode (default: cwd)

ENV:
  OCL_SERVER        Same as --server, used when flag omitted
"#
    );
}

// ── Output helpers ──────────────────────────────────────────────────

/// JSON object keys whose values should be masked in `--json` output.
/// Matching is case-insensitive and applied recursively.
///
/// SG-04: Without redaction, any future backend route that returns a
/// secret (access tokens, API keys, passwords) would leak it to the
/// shell history when a user ran e.g. `ocl --json settings | tee out.json`.
const SENSITIVE_KEYS: &[&str] = &[
    "token",
    "access_token",
    "refresh_token",
    "id_token",
    "password",
    "passwd",
    "secret",
    "api_key",
    "apikey",
    "authorization",
    "auth",
    "credential",
    "credentials",
    "client_secret",
    "private_key",
    "session_token",
];

/// Walk a JSON value tree and replace the value of any key whose
/// case-insensitive name matches one in `SENSITIVE_KEYS` with the
/// string `"***redacted***"`. Operates in place on a cloned tree so
/// the original backend response is preserved for debugging.
pub(crate) fn redact_sensitive_fields(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                let lower = key.to_ascii_lowercase();
                if SENSITIVE_KEYS
                    .iter()
                    .any(|sensitive| lower == *sensitive)
                {
                    *child = Value::String("***redacted***".to_string());
                } else {
                    redact_sensitive_fields(child);
                }
            }
        }
        Value::Array(arr) => {
            for child in arr.iter_mut() {
                redact_sensitive_fields(child);
            }
        }
        _ => {}
    }
}

fn print_output(config: &Config, value: &Value) {
    if config.json {
        // Clone + redact before printing so callers of print_output don't
        // have to worry about mutation of the upstream value.
        let mut sanitized = value.clone();
        redact_sensitive_fields(&mut sanitized);
        println!(
            "{}",
            serde_json::to_string_pretty(&sanitized).unwrap_or_default()
        );
    } else {
        // Pretty, human-readable format. Redact here too so an operator
        // screen-recording a terminal session doesn't accidentally reveal
        // a token in the UI.
        let mut sanitized = value.clone();
        redact_sensitive_fields(&mut sanitized);
        print_pretty(&sanitized, 0);
    }
}

fn print_pretty(value: &Value, indent: usize) {
    let pad = "  ".repeat(indent);
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                match v {
                    Value::Object(_) | Value::Array(_) => {
                        println!("{pad}{k}:");
                        print_pretty(v, indent + 1);
                    }
                    _ => {
                        println!("{pad}{k}: {}", format_scalar(v));
                    }
                }
            }
        }
        Value::Array(arr) => {
            for (idx, item) in arr.iter().enumerate() {
                match item {
                    Value::Object(_) | Value::Array(_) => {
                        println!("{pad}[{idx}]");
                        print_pretty(item, indent + 1);
                    }
                    _ => {
                        println!("{pad}- {}", format_scalar(item));
                    }
                }
            }
        }
        _ => println!("{pad}{}", format_scalar(value)),
    }
}

fn format_scalar(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

// ── HTTP helpers ────────────────────────────────────────────────────

async fn http_get(config: &Config, path: &str) -> Result<Value, CliError> {
    let url = format!("{}{path}", config.base_url);
    let response = config.client.get(&url).send().await?;
    parse_response(response).await
}

async fn http_post(config: &Config, path: &str, body: Value) -> Result<Value, CliError> {
    let url = format!("{}{path}", config.base_url);
    let response = config.client.post(&url).json(&body).send().await?;
    parse_response(response).await
}

async fn http_delete(config: &Config, path: &str) -> Result<Value, CliError> {
    let url = format!("{}{path}", config.base_url);
    let response = config.client.delete(&url).send().await?;
    parse_response(response).await
}

async fn parse_response(response: reqwest::Response) -> Result<Value, CliError> {
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(CliError::ApiError {
            status: status.as_u16(),
            body,
        });
    }
    // Try JSON; fall back to treating body as a string.
    let text = response.text().await?;
    Ok(serde_json::from_str(&text).unwrap_or(Value::String(text)))
}

// ── Command handlers ────────────────────────────────────────────────

async fn cmd_health(config: &Config) -> Result<(), CliError> {
    let value = http_get(config, "/healthz").await?;
    print_output(config, &value);
    Ok(())
}

async fn cmd_sessions(config: &Config, args: &[String]) -> Result<(), CliError> {
    if args.is_empty() {
        return Err(CliError::UsageError("sessions: missing subcommand".into()));
    }
    match args[0].as_str() {
        "list" => {
            let value = http_get(config, "/api/desktop/workbench").await?;
            print_output(config, &value);
        }
        "show" => {
            let id = args.get(1).ok_or_else(|| {
                CliError::UsageError("sessions show: missing session id".into())
            })?;
            let value = http_get(config, &format!("/api/desktop/sessions/{id}")).await?;
            print_output(config, &value);
        }
        "new" => {
            let mut title = None;
            let mut project_path = None;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--title" => {
                        title = args.get(i + 1).cloned();
                        i += 2;
                    }
                    "--path" => {
                        project_path = args.get(i + 1).cloned();
                        i += 2;
                    }
                    _ => i += 1,
                }
            }
            let mut body = serde_json::Map::new();
            if let Some(t) = title {
                body.insert("title".into(), Value::String(t));
            }
            if let Some(p) = project_path {
                body.insert("project_path".into(), Value::String(p));
            }
            let value = http_post(config, "/api/desktop/sessions", Value::Object(body)).await?;
            print_output(config, &value);
        }
        "send" => {
            let id = args.get(1).ok_or_else(|| {
                CliError::UsageError("sessions send: missing session id".into())
            })?;
            let message = args.get(2).ok_or_else(|| {
                CliError::UsageError("sessions send: missing message".into())
            })?;
            let value = http_post(
                config,
                &format!("/api/desktop/sessions/{id}/messages"),
                serde_json::json!({ "message": message }),
            )
            .await?;
            print_output(config, &value);
        }
        "cancel" => {
            let id = args.get(1).ok_or_else(|| {
                CliError::UsageError("sessions cancel: missing session id".into())
            })?;
            let value = http_post(
                config,
                &format!("/api/desktop/sessions/{id}/cancel"),
                Value::Object(serde_json::Map::new()),
            )
            .await?;
            print_output(config, &value);
        }
        "compact" => {
            let id = args.get(1).ok_or_else(|| {
                CliError::UsageError("sessions compact: missing session id".into())
            })?;
            let value = http_post(
                config,
                &format!("/api/desktop/sessions/{id}/compact"),
                Value::Object(serde_json::Map::new()),
            )
            .await?;
            print_output(config, &value);
        }
        "status" => {
            let id = args.get(1).ok_or_else(|| {
                CliError::UsageError("sessions status: missing session id".into())
            })?;
            let status = args.get(2).ok_or_else(|| {
                CliError::UsageError(
                    "sessions status: missing status (todo|in_progress|needs_review|done|archived)"
                        .into(),
                )
            })?;
            let value = http_post(
                config,
                &format!("/api/desktop/sessions/{id}/lifecycle"),
                serde_json::json!({ "status": status }),
            )
            .await?;
            print_output(config, &value);
        }
        "flag" => {
            let id = args.get(1).ok_or_else(|| {
                CliError::UsageError("sessions flag: missing session id".into())
            })?;
            let flagged = args
                .get(2)
                .and_then(|s| s.parse::<bool>().ok())
                .ok_or_else(|| {
                    CliError::UsageError("sessions flag: expected true or false".into())
                })?;
            let value = http_post(
                config,
                &format!("/api/desktop/sessions/{id}/flag"),
                serde_json::json!({ "flagged": flagged }),
            )
            .await?;
            print_output(config, &value);
        }
        "delete" => {
            let id = args.get(1).ok_or_else(|| {
                CliError::UsageError("sessions delete: missing session id".into())
            })?;
            let value = http_delete(config, &format!("/api/desktop/sessions/{id}")).await?;
            print_output(config, &value);
        }
        other => {
            return Err(CliError::UsageError(format!(
                "unknown sessions subcommand: {other}"
            )))
        }
    }
    Ok(())
}

async fn cmd_mcp(config: &Config, args: &[String]) -> Result<(), CliError> {
    if args.is_empty() {
        return Err(CliError::UsageError("mcp: missing subcommand".into()));
    }
    match args[0].as_str() {
        "probe" => {
            let project_path = args.get(1).ok_or_else(|| {
                CliError::UsageError("mcp probe: missing project_path".into())
            })?;
            let value = http_post(
                config,
                "/api/desktop/debug/mcp/probe",
                serde_json::json!({ "project_path": project_path }),
            )
            .await?;
            print_output(config, &value);
        }
        "call" => {
            let qualified_name = args.get(1).ok_or_else(|| {
                CliError::UsageError("mcp call: missing qualified_name".into())
            })?;
            let args_json = args.get(2).map(String::as_str).unwrap_or("{}");
            let arguments: Value = serde_json::from_str(args_json).map_err(|e| {
                CliError::UsageError(format!("mcp call: invalid JSON arguments: {e}"))
            })?;
            let value = http_post(
                config,
                "/api/desktop/debug/mcp/call",
                serde_json::json!({
                    "qualified_name": qualified_name,
                    "arguments": arguments,
                }),
            )
            .await?;
            print_output(config, &value);
        }
        other => {
            return Err(CliError::UsageError(format!(
                "unknown mcp subcommand: {other}"
            )))
        }
    }
    Ok(())
}

/// Resolve a `--project-path <p>` flag from arg slice, or fall back to the
/// current working directory. Returns (path_string, remaining_args_without_flag).
///
/// Backend requires `project_path` in the request body since the S-02 hardening
/// (validate_project_path checks for `..` traversal + canonicalize + is_dir).
/// Defaulting to cwd keeps the CLI ergonomic — users typically run `ocl` from
/// within the project they want to operate on.
fn extract_project_path(args: &[String]) -> Result<(String, Vec<String>), CliError> {
    let mut remaining = Vec::with_capacity(args.len());
    let mut path: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--project-path" || args[i] == "-p" {
            let value = args.get(i + 1).ok_or_else(|| {
                CliError::UsageError(format!("{}: missing value", args[i]))
            })?;
            path = Some(value.clone());
            i += 2;
        } else {
            remaining.push(args[i].clone());
            i += 1;
        }
    }
    let resolved = match path {
        Some(p) => p,
        None => std::env::current_dir()
            .map_err(|e| CliError::UsageError(format!("cannot resolve cwd: {e}")))?
            .display()
            .to_string(),
    };
    Ok((resolved, remaining))
}

async fn cmd_permission_mode(config: &Config, args: &[String]) -> Result<(), CliError> {
    let (project_path, rest) = extract_project_path(args)?;

    if rest.is_empty() || rest[0] == "get" {
        let path = format!(
            "/api/desktop/settings/permission-mode?project_path={}",
            urlencode(&project_path)
        );
        let value = http_get(config, &path).await?;
        print_output(config, &value);
        return Ok(());
    }
    if rest[0] == "set" {
        let mode = rest
            .get(1)
            .ok_or_else(|| CliError::UsageError("permission-mode set: missing mode".into()))?;
        let value = http_post(
            config,
            "/api/desktop/settings/permission-mode",
            serde_json::json!({
                "mode": mode,
                "project_path": project_path,
            }),
        )
        .await?;
        print_output(config, &value);
        return Ok(());
    }
    Err(CliError::UsageError(format!(
        "unknown permission-mode subcommand: {}",
        rest[0]
    )))
}

/// Minimal application/x-www-form-urlencoded encoder for URL query strings.
/// Only escapes characters known to need encoding in path/query contexts.
fn urlencode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}

#[cfg(test)]
mod cli_helper_tests {
    use super::{extract_project_path, urlencode};

    #[test]
    fn extract_project_path_uses_explicit_flag() {
        let args = vec![
            "set".to_string(),
            "default".to_string(),
            "--project-path".to_string(),
            "C:/foo/bar".to_string(),
        ];
        let (path, rest) = extract_project_path(&args).expect("ok");
        assert_eq!(path, "C:/foo/bar");
        assert_eq!(rest, vec!["set".to_string(), "default".to_string()]);
    }

    #[test]
    fn extract_project_path_short_flag() {
        let args = vec!["-p".to_string(), "C:/x".to_string(), "get".to_string()];
        let (path, rest) = extract_project_path(&args).expect("ok");
        assert_eq!(path, "C:/x");
        assert_eq!(rest, vec!["get".to_string()]);
    }

    #[test]
    fn extract_project_path_falls_back_to_cwd() {
        let args = vec!["get".to_string()];
        let (path, rest) = extract_project_path(&args).expect("ok");
        // cwd cannot be empty under normal conditions
        assert!(!path.is_empty());
        assert_eq!(rest, vec!["get".to_string()]);
    }

    #[test]
    fn extract_project_path_missing_value_errors() {
        let args = vec!["--project-path".to_string()];
        assert!(extract_project_path(&args).is_err());
    }

    #[test]
    fn urlencode_preserves_safe_chars() {
        assert_eq!(urlencode("abc-123_x.y~z"), "abc-123_x.y~z");
    }

    #[test]
    fn urlencode_escapes_special_chars() {
        assert_eq!(urlencode("a/b c"), "a%2Fb%20c");
        assert_eq!(urlencode("D:/foo"), "D%3A%2Ffoo");
    }
}

#[cfg(test)]
mod redaction_tests {
    use super::redact_sensitive_fields;
    use serde_json::json;

    #[test]
    fn redacts_top_level_token() {
        let mut value = json!({ "token": "secret-token-value" });
        redact_sensitive_fields(&mut value);
        assert_eq!(value["token"], "***redacted***");
    }

    #[test]
    fn redacts_case_insensitive_key() {
        let mut value = json!({ "API_KEY": "abc123", "ApiKey": "xyz" });
        redact_sensitive_fields(&mut value);
        assert_eq!(value["API_KEY"], "***redacted***");
        assert_eq!(value["ApiKey"], "***redacted***");
    }

    #[test]
    fn redacts_whole_sensitive_object() {
        // "auth" is itself in SENSITIVE_KEYS, so the entire sub-object is
        // replaced rather than recursively walked. This is intentional —
        // a stricter posture that protects against new secret-bearing
        // fields being added under `auth` without updating this list.
        let mut value = json!({
            "user": "alice",
            "auth": {
                "access_token": "eyJ...",
                "refresh_token": "r-token"
            }
        });
        redact_sensitive_fields(&mut value);
        assert_eq!(value["user"], "alice");
        assert_eq!(value["auth"], "***redacted***");
    }

    #[test]
    fn redacts_nested_sensitive_leaf_inside_non_sensitive_parent() {
        // When the parent key is NOT in SENSITIVE_KEYS, walk recursively
        // and redact matching children.
        let mut value = json!({
            "session": {
                "id": "sess-1",
                "metadata": {
                    "access_token": "eyJ...",
                    "display_name": "Test"
                }
            }
        });
        redact_sensitive_fields(&mut value);
        assert_eq!(value["session"]["id"], "sess-1");
        assert_eq!(value["session"]["metadata"]["access_token"], "***redacted***");
        assert_eq!(value["session"]["metadata"]["display_name"], "Test");
    }

    #[test]
    fn redacts_inside_array() {
        let mut value = json!([
            { "name": "a", "password": "pw1" },
            { "name": "b", "password": "pw2" }
        ]);
        redact_sensitive_fields(&mut value);
        assert_eq!(value[0]["password"], "***redacted***");
        assert_eq!(value[1]["password"], "***redacted***");
        assert_eq!(value[0]["name"], "a");
    }

    #[test]
    fn preserves_non_sensitive_fields() {
        let mut value = json!({
            "id": "foo",
            "title": "Hello",
            "count": 42
        });
        let expected = value.clone();
        redact_sensitive_fields(&mut value);
        assert_eq!(value, expected);
    }

    #[test]
    fn does_not_match_partial_key() {
        // "tokenizer" should NOT be redacted — only exact matches are.
        let mut value = json!({ "tokenizer": "bpe", "mytokenfield": "keep" });
        let before = value.clone();
        redact_sensitive_fields(&mut value);
        assert_eq!(value, before);
    }
}

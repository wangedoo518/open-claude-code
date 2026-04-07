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
  permission-mode [get]                        Show current mode
  permission-mode set <mode>                   Set permission mode

ENV:
  OCL_SERVER        Same as --server, used when flag omitted
"#
    );
}

// ── Output helpers ──────────────────────────────────────────────────

fn print_output(config: &Config, value: &Value) {
    if config.json {
        println!("{}", serde_json::to_string_pretty(value).unwrap_or_default());
    } else {
        // Pretty, human-readable format.
        print_pretty(value, 0);
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

async fn cmd_permission_mode(config: &Config, args: &[String]) -> Result<(), CliError> {
    if args.is_empty() || args[0] == "get" {
        let value = http_get(config, "/api/desktop/settings/permission-mode").await?;
        print_output(config, &value);
        return Ok(());
    }
    if args[0] == "set" {
        let mode = args
            .get(1)
            .ok_or_else(|| CliError::UsageError("permission-mode set: missing mode".into()))?;
        let value = http_post(
            config,
            "/api/desktop/settings/permission-mode",
            serde_json::json!({ "mode": mode }),
        )
        .await?;
        print_output(config, &value);
        return Ok(());
    }
    Err(CliError::UsageError(format!(
        "unknown permission-mode subcommand: {}",
        args[0]
    )))
}

use std::env;
use std::net::SocketAddr;
use std::time::Duration;

use desktop_core::wechat_ilink::{
    account::{normalize_account_id, save_account},
    login::{LoginStatus, QrLoginSession},
    types::{WeixinAccountData, DEFAULT_BASE_URL},
};
use desktop_core::DesktopState;
use desktop_server::{serve_with_shutdown, AppState};
use tokio_util::sync::CancellationToken;

const DEFAULT_ADDRESS: &str = "127.0.0.1:4357";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Subcommand routing. We keep the binary single-purpose by default
    // (run the HTTP server) but ship a couple of one-shot ops as subcommands
    // so users don't need a second binary just to scan a QR code.
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("wechat-login") => run_wechat_login().await,
        Some("--help") | Some("-h") => {
            print_help();
            Ok(())
        }
        Some(cmd) if !cmd.starts_with('-') => Err(format!(
            "unknown subcommand: {cmd}\n\nrun `desktop-server --help` for usage"
        )
        .into()),
        _ => run_server().await,
    }
}

fn print_help() {
    println!(
        "desktop-server — open-claude-code HTTP server\n\
         \n\
         USAGE:\n\
           desktop-server                Start the HTTP server (default)\n\
           desktop-server wechat-login   Bind a new WeChat ClawBot via QR code\n\
           desktop-server --help         Show this message\n\
         \n\
         ENVIRONMENT:\n\
           OPEN_CLAUDE_CODE_DESKTOP_ADDR   Listen address (default 127.0.0.1:4357)\n\
           WARWOLF_WECHAT_DIR              Override ~/.warwolf/wechat state dir"
    );
}

async fn run_server() -> Result<(), Box<dyn std::error::Error>> {
    // Auto-install Python dependencies (markitdown + playwright) silently.
    // This runs in the background so server startup is not blocked.
    tokio::spawn(async {
        auto_install_python_deps().await;
    });

    // Workaround for a Windows-specific path bug in the upstream `runtime`
    // crate's `default_config_home()`: it reads `HOME` directly via
    // `std::env::var_os`, which under git-bash resolves to a Unix-style
    // string like `/c/Users/111`. Joining `.claw` to that produces an
    // invalid Windows path (`\c\Users\111\.claw`), which Windows then
    // interprets relative to the current drive letter, ending up at
    // `D:\c\Users\111\.claw` and failing every persist with "拒绝访问".
    //
    // Fix: ensure `CLAW_CONFIG_HOME` is set to a real Windows path BEFORE
    // any code in `runtime` reads it. We default to `%USERPROFILE%\.claw`
    // when neither override is present. Honors a pre-set value so power
    // users can still relocate state via env var.
    ensure_claw_config_home_is_valid();

    // v2 Phase 4 bugfix: resolve wiki root once at startup and pin it into
    // CLAWWIKI_HOME. `wiki_store::default_root()` already performs the
    // Windows UAC fallback (primary `%USERPROFILE%\.clawwiki` → denied →
    // fallback `%LOCALAPPDATA%\clawwiki`). By writing the resolved path
    // back into the env var, every subsequent call across all subsystems
    // (including spawned tokio tasks) sees the same root without re-running
    // the probe logic.
    if std::env::var_os(wiki_store::ENV_OVERRIDE).is_none() {
        let resolved = wiki_store::default_root();
        std::env::set_var(wiki_store::ENV_OVERRIDE, &resolved);
        println!("[startup] CLAWWIKI_HOME = {}", resolved.display());
    } else {
        println!(
            "[startup] CLAWWIKI_HOME = {} (from env)",
            std::env::var_os(wiki_store::ENV_OVERRIDE)
                .map(|v| v.to_string_lossy().into_owned())
                .unwrap_or_default()
        );
    }

    let address = env::var("OPEN_CLAUDE_CODE_DESKTOP_ADDR")
        .unwrap_or_else(|_| DEFAULT_ADDRESS.to_string())
        .parse::<SocketAddr>()?;

    // Build a single DesktopState that's shared between the HTTP server and
    // the WeChat monitor. Both surfaces operate on the same session store
    // so messages received via WeChat appear in the desktop UI in real time.
    let state = DesktopState::live();

    // ── Graceful shutdown wire-up (Sprint: Tauri graceful shutdown) ──
    //
    // Three sources can trip `cancel`:
    //   1. Ctrl-C  — always on.
    //   2. SIGTERM — Unix only (Windows has no SIGTERM proper; the
    //      Tauri parent uses the HTTP path below instead).
    //   3. POST /internal/shutdown — the Tauri shell hits this on
    //      window-close / Cmd-Q / app-exit-requested. Auth-gated by
    //      a per-process secret (the `OCL_SHUTDOWN_TOKEN` env var).
    //
    // Without this, `axum::serve()` would block forever and a
    // window-close would fall back to `Child::kill`, which force-aborts
    // every tokio task and so `SessionCleanupGuard::drop` (landed in
    // commit 3085a1e) never runs. Then sessions leak in the `Running`
    // state until the next launch's startup reconcile cleans them up.
    let cancel = CancellationToken::new();

    // Batch-C §3: inject the shutdown cancel into DesktopState BEFORE
    // spawning WeChat monitors, so every monitor's internal cancel token
    // is a child of this one. A graceful shutdown cancel then cascades
    // automatically — each monitor's existing `tokio::select!` on its
    // own `cancel` picks up the cascade without any extra branch.
    state.set_shutdown_cancel(cancel.clone()).await;

    // Spawn the WeChat iLink long-poll monitor(s) for every persisted account
    // before we start the HTTP server. Handles are now stored inside
    // DesktopState so HTTP routes can cancel them dynamically when the user
    // deletes a WeChat account from the frontend (Phase 6C).
    state.spawn_wechat_monitors_for_all_accounts().await;

    // Channel B: auto-start kefu monitor if configured
    state.auto_start_kefu_monitor().await;

    // R1.2 reliability gate · spawn the durable WeChat outbox replay
    // worker. Reverts any `Sending` rows from a previous crash to
    // `Pending`, then ticks every 30s to retry transient failures
    // and surface terminal ones. Cancellation cascades from
    // `set_shutdown_cancel` above.
    state.spawn_wechat_outbox_worker().await;

    // Read the auth token from the spawn env if the Tauri shell
    // provided one; otherwise fabricate one so the handler still has a
    // valid credential to compare against (standalone `cargo run -p
    // desktop-server` runs with no Tauri parent and no client that
    // needs the route).
    let shutdown_token =
        env::var("OCL_SHUTDOWN_TOKEN").unwrap_or_else(|_| uuid::Uuid::new_v4().to_string());

    // Ctrl-C handler. `ctrl_c()` completes on the first Ctrl-C and
    // the spawned task exits; a second Ctrl-C would be handled by the
    // default behavior (immediate termination).
    {
        let cancel = cancel.clone();
        tokio::spawn(async move {
            if let Err(err) = tokio::signal::ctrl_c().await {
                eprintln!("[shutdown] ctrl_c listener error: {err}");
                return;
            }
            eprintln!("[shutdown] Ctrl-C received → signalling graceful shutdown");
            cancel.cancel();
        });
    }

    // SIGTERM handler. Only compile this on Unix — on Windows the
    // `tokio::signal::unix` module does not exist.
    #[cfg(unix)]
    {
        let cancel = cancel.clone();
        tokio::spawn(async move {
            use tokio::signal::unix::{signal, SignalKind};
            let mut term = match signal(SignalKind::terminate()) {
                Ok(s) => s,
                Err(err) => {
                    eprintln!("[shutdown] SIGTERM listener setup failed: {err}");
                    return;
                }
            };
            if term.recv().await.is_some() {
                eprintln!("[shutdown] SIGTERM received → signalling graceful shutdown");
                cancel.cancel();
            }
        });
    }

    let state = AppState::new_with_shutdown(state, shutdown_token, cancel.clone());
    serve_with_shutdown(state, address, cancel).await?;
    eprintln!("[shutdown] axum::serve returned — process exiting cleanly");
    Ok(())
}

/// Ensure `CLAW_CONFIG_HOME` is set to a path that is BOTH valid on the
/// host platform AND writable by the current process.
///
/// Why this is more involved than it looks:
///   1. The upstream `runtime` crate's `default_config_home()` reads
///      `$HOME` directly. Under git-bash on Windows, `HOME` is a Unix-style
///      string (`/c/Users/111`) which the Rust stdlib does not normalize,
///      producing an invalid Windows path (`\c\Users\111\.claw`) that
///      Windows then resolves relative to the current drive letter
///      (`D:\c\Users\111\.claw`) and fails every `fs::create_dir_all`.
///   2. Even with a valid Windows-form `HOME`, on this developer's machine
///      a system filter denies creation of any directory matching `claw*`
///      under `%USERPROFILE%` (likely a sandbox/AV rule). Verified
///      empirically: `mkdir C:\Users\111\claw_anything` fails for the
///      regular user account.
///
/// To make `runtime`'s persistence work regardless, we forcibly set
/// `CLAW_CONFIG_HOME` to `%LOCALAPPDATA%\warwolf\claw\` on Windows. This
/// directory:
///   * is per-user, non-roaming, always full-control for the owner
///   * lives under `LOCALAPPDATA` so AV/sandbox `claw*` filters on the
///     home directory don't apply
///   * sits next to `%LOCALAPPDATA%\warwolf\wechat\` which we already use
///     for the iLink token store, so all warwolf state is colocated
///
/// We honor a user-supplied `CLAW_CONFIG_HOME` if already set, and never
/// touch the env on macOS/Linux.
fn ensure_claw_config_home_is_valid() {
    if env::var_os("CLAW_CONFIG_HOME").is_none() {
        #[cfg(windows)]
        {
            let target = env::var_os("LOCALAPPDATA")
                .map(std::path::PathBuf::from)
                .map(|p| p.join("warwolf").join("claw"))
                .or_else(|| {
                    env::var_os("USERPROFILE")
                        .map(std::path::PathBuf::from)
                        .map(|p| p.join("AppData").join("Local").join("warwolf").join("claw"))
                });

            if let Some(path) = target {
                eprintln!(
                    "[startup] forcing CLAW_CONFIG_HOME = {} (Windows AV/sandbox workaround)",
                    path.display()
                );
                if let Err(e) = std::fs::create_dir_all(&path) {
                    eprintln!(
                        "[startup] warning: failed to pre-create {}: {e}",
                        path.display()
                    );
                }
                env::set_var("CLAW_CONFIG_HOME", &path);
            }
        }
    }

    // S2: apply the same LOCALAPPDATA workaround to the
    // secure_storage key file. The module defaults to
    // `$USERPROFILE/.warwolf/.secret-key` which is blocked by AV on
    // some Windows machines (the whole `.warwolf` dir is read-only
    // even though `%LOCALAPPDATA%\warwolf\` works fine). Without this
    // redirect `codex_broker::sync_cloud_accounts` fails with
    // `os error 5 Access denied` when it tries to write the key.
    if env::var_os("WARWOLF_SECRET_KEY_FILE").is_none()
        && env::var_os("WARWOLF_SECRET_KEY_DIR").is_none()
    {
        #[cfg(windows)]
        {
            let dir = env::var_os("LOCALAPPDATA")
                .map(std::path::PathBuf::from)
                .map(|p| p.join("warwolf"));
            if let Some(dir) = dir {
                if let Err(e) = std::fs::create_dir_all(&dir) {
                    eprintln!(
                        "[startup] warning: failed to pre-create secret-key dir {}: {e}",
                        dir.display()
                    );
                }
                eprintln!(
                    "[startup] forcing WARWOLF_SECRET_KEY_DIR = {} (Windows AV/sandbox workaround)",
                    dir.display()
                );
                env::set_var("WARWOLF_SECRET_KEY_DIR", &dir);
            }
        }
    }
}

/// One-shot QR-code login flow for the WeChat ClawBot iLink integration.
///
/// 1. Calls `ilink/bot/get_bot_qrcode` (anonymous endpoint)
/// 2. Prints the resulting `qrcode_img_content` URL — user opens it on
///    their phone via the WeChat ClawBot plugin
/// 3. Long-polls `ilink/bot/get_qrcode_status` until the user confirms
/// 4. Persists the resulting `bot_token` to `~/.warwolf/wechat/accounts/`
async fn run_wechat_login() -> Result<(), Box<dyn std::error::Error>> {
    println!("[wechat-login] starting QR-code login flow...");
    let mut session = QrLoginSession::new(Some(DEFAULT_BASE_URL.to_string()))?;

    let qr = session.fetch_qr_code().await?;
    println!();
    println!("──────────────────────────────────────────────────────────────");
    println!("  请用微信 ClawBot 插件扫描下面的二维码进行绑定:");
    println!();
    println!("  {}", qr.qrcode_img_content);
    println!();
    println!("  操作步骤:");
    println!("    1. 在手机上用浏览器打开上面的链接 (或在电脑上点击)");
    println!("    2. 用手机微信扫码 (我 → 设置 → 插件 → 微信 ClawBot)");
    println!("    3. 在微信里点击「确认绑定」");
    println!("──────────────────────────────────────────────────────────────");
    println!();
    println!("[wechat-login] waiting for scan...");

    // 8-minute total deadline; refreshes QR up to 3 times.
    let timeout = Duration::from_secs(8 * 60);
    let confirmation = session
        .wait_for_login(timeout, |status| match status {
            LoginStatus::Wait => print!("."),
            LoginStatus::Scanned => println!("\n[wechat-login] 已扫码，等待用户确认..."),
            LoginStatus::Expired => {
                println!("\n[wechat-login] 二维码过期，正在刷新...")
            }
            LoginStatus::Confirmed => println!("\n[wechat-login] ✓ 已确认！"),
        })
        .await?;

    use std::io::Write as _;
    let _ = std::io::stdout().flush();

    // Persist to disk under our private state dir.
    let normalized_id = normalize_account_id(&confirmation.ilink_bot_id);
    let saved = save_account(
        &normalized_id,
        WeixinAccountData {
            token: Some(confirmation.bot_token.clone()),
            base_url: Some(confirmation.base_url.clone()),
            user_id: confirmation.user_id.clone(),
            ..Default::default()
        },
    )?;

    println!();
    println!("──────────────────────────────────────────────────────────────");
    println!("  ✓ WeChat ClawBot 绑定成功");
    println!();
    println!("  account id : {}", confirmation.ilink_bot_id);
    println!("  base url   : {}", confirmation.base_url);
    if let Some(uid) = &confirmation.user_id {
        println!("  user id    : {uid}");
    }
    if let Some(saved_at) = &saved.saved_at {
        println!("  saved at   : {saved_at}");
    }
    println!();
    if let Ok(dir) = desktop_core::wechat_ilink::account::state_dir() {
        println!(
            "  凭证已存储于 {}\\accounts\\{normalized_id}.json",
            dir.display()
        );
    }
    println!("──────────────────────────────────────────────────────────────");

    Ok(())
}

/// Silently auto-install Python deps on startup (background task).
async fn auto_install_python_deps() {
    // Check Python
    let py = tokio::process::Command::new("python")
        .args(["--version"])
        .output()
        .await;
    match py {
        Ok(o) if o.status.success() => {
            eprintln!(
                "[auto-install] {}",
                String::from_utf8_lossy(&o.stdout).trim()
            );
        }
        _ => {
            eprintln!("[auto-install] Python not found, skipping");
            return;
        }
    }

    // markitdown
    let ok = tokio::process::Command::new("python")
        .args(["-c", "import markitdown"])
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !ok {
        eprintln!("[auto-install] installing markitdown[all]...");
        let _ = tokio::process::Command::new("python")
            .args([
                "-m",
                "pip",
                "install",
                "--upgrade",
                "--quiet",
                "markitdown[all]",
            ])
            .output()
            .await;
        eprintln!("[auto-install] markitdown done");
    }

    // playwright (pip package only — NO chromium download!)
    // wechat_fetcher.py uses find_local_chrome() to use the system's
    // Chrome/Edge instead of downloading a separate Chromium binary.
    let ok = tokio::process::Command::new("python")
        .args(["-c", "from playwright.sync_api import sync_playwright"])
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !ok {
        eprintln!("[auto-install] installing playwright pip package...");
        let _ = tokio::process::Command::new("python")
            .args(["-m", "pip", "install", "--upgrade", "--quiet", "playwright"])
            .output()
            .await;
        eprintln!(
            "[auto-install] playwright pip done (using system Chrome/Edge, no Chromium download)"
        );
    }

    // defuddle (Node.js content extraction)
    let defuddle_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../wiki_ingest/src");
    if !defuddle_dir.join("node_modules/defuddle").exists() {
        eprintln!("[auto-install] installing defuddle (npm)...");
        let _ = tokio::process::Command::new("npm")
            .args(["install", "--prefix", &defuddle_dir.to_string_lossy()])
            .output()
            .await;
        eprintln!("[auto-install] defuddle done");
    }

    eprintln!("[auto-install] all deps ready");
}

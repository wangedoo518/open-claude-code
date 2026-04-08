use std::env;
use std::net::SocketAddr;
use std::time::Duration;

use desktop_core::wechat_ilink::{
    account::{normalize_account_id, save_account},
    login::{LoginStatus, QrLoginSession},
    types::{WeixinAccountData, DEFAULT_BASE_URL},
};
use desktop_core::DesktopState;
use desktop_server::{serve, AppState};

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
    let address = env::var("OPEN_CLAUDE_CODE_DESKTOP_ADDR")
        .unwrap_or_else(|_| DEFAULT_ADDRESS.to_string())
        .parse::<SocketAddr>()?;
    serve(AppState::new(DesktopState::live()), address).await?;
    Ok(())
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
    println!("  凭证已存储于 ~/.warwolf/wechat/accounts/{normalized_id}.json");
    println!("──────────────────────────────────────────────────────────────");

    Ok(())
}

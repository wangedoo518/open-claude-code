//! One-scan pipeline orchestrator for kefu setup.
//! Coordinates 5 phases: CF register → Worker deploy → WeCom auth →
//! callback config → kefu account creation.

use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as TokioCommand;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::client::KefuClient;
use super::deployer::WranglerDeployer;
use super::email_client::EmailClient;
use super::pipeline_types::*;

pub struct KefuPipeline {
    state_tx: watch::Sender<PipelineState>,
    cancel: CancellationToken,
    // Skip flags
    skip_cf_register: bool,
    skip_callback_config: bool,
    // Pre-existing credentials
    existing_cf_token: Option<String>,
    existing_corpid: Option<String>,
    existing_secret: Option<String>,
}

enum CloudflareSignupOutcome {
    LoggedIn,
    NeedsEmailVerification,
}

impl KefuPipeline {
    pub fn new(cancel: CancellationToken) -> (Self, watch::Receiver<PipelineState>) {
        let (state_tx, state_rx) = watch::channel(PipelineState::new());
        (
            Self {
                state_tx,
                cancel,
                skip_cf_register: false,
                skip_callback_config: false,
                existing_cf_token: None,
                existing_corpid: None,
                existing_secret: None,
            },
            state_rx,
        )
    }

    pub fn skip_cf_register(&mut self, cf_token: String) {
        self.skip_cf_register = true;
        self.existing_cf_token = Some(cf_token);
    }

    pub fn skip_callback_config(&mut self, corpid: String, secret: String) {
        self.skip_callback_config = true;
        self.existing_corpid = Some(corpid);
        self.existing_secret = Some(secret);
    }

    /// Run the complete 5-phase pipeline.
    pub async fn run(&self) -> Result<PipelineResult, PipelineError> {
        self.mark_started();
        self.log_line("开始新的微信客服接入流程");
        if let Err(error) = self.check_prerequisites().await {
            let phase = if self.skip_cf_register {
                PipelinePhase::WecomAuth
            } else {
                PipelinePhase::CfRegister
            };
            self.fail(phase, &error.to_string());
            return Err(error);
        }

        // Phase 1: CF registration
        let cf = if self.skip_cf_register {
            self.log_phase(
                PipelinePhase::CfRegister,
                "跳过 Cloudflare 账号注册，使用已有账号",
            );
            self.update(PipelinePhase::CfRegister, PhaseStatus::Skipped, None);
            CfCredentials {
                email: String::new(),
                password: String::new(),
                api_token: self.existing_cf_token.clone().unwrap_or_default(),
            }
        } else {
            self.log_phase(PipelinePhase::CfRegister, "开始 Cloudflare 账号注册");
            self.update(PipelinePhase::CfRegister, PhaseStatus::Running, None);
            match self.phase1_cf_register().await {
                Ok(result) => {
                    self.log_phase(
                        PipelinePhase::CfRegister,
                        if result.api_token.is_empty() {
                            format!("Cloudflare 与 Wrangler OAuth 已就绪，邮箱 {}", result.email)
                        } else {
                            format!("Cloudflare API Token 获取成功，邮箱 {}", result.email)
                        },
                    );
                    self.update(PipelinePhase::CfRegister, PhaseStatus::Done, None);
                    result
                }
                Err(e) => {
                    self.fail(PipelinePhase::CfRegister, &e.to_string());
                    return Err(e);
                }
            }
        };

        // Phase 2: Worker deploy
        self.log_phase(
            PipelinePhase::WorkerDeploy,
            "开始部署 Cloudflare Worker 中继",
        );
        self.update(PipelinePhase::WorkerDeploy, PhaseStatus::Running, None);
        let deploy = match self.phase2_deploy_worker(&cf.api_token).await {
            Ok(r) => {
                self.log_phase(
                    PipelinePhase::WorkerDeploy,
                    format!("中继部署成功：{}", r.worker_url),
                );
                self.update(
                    PipelinePhase::WorkerDeploy,
                    PhaseStatus::Done,
                    Some(r.worker_url.clone()),
                );
                r
            }
            Err(e) => {
                self.fail(PipelinePhase::WorkerDeploy, &e.to_string());
                return Err(e);
            }
        };

        // Phase 3: WeCom QR scan (user action)
        self.log_phase(PipelinePhase::WecomAuth, "等待企业微信扫码授权");
        self.update(PipelinePhase::WecomAuth, PhaseStatus::WaitingScan, None);
        let wecom = match self.phase3_wecom_auth().await {
            Ok(r) => {
                self.log_phase(
                    PipelinePhase::WecomAuth,
                    format!("企业微信授权完成，企业 ID {}", r.corpid),
                );
                self.update(PipelinePhase::WecomAuth, PhaseStatus::Done, None);
                r
            }
            Err(e) => {
                self.fail(PipelinePhase::WecomAuth, &e.to_string());
                return Err(e);
            }
        };

        // Phase 4: Callback config
        let secret = if self.skip_callback_config {
            self.log_phase(
                PipelinePhase::CallbackConfig,
                "跳过回调配置，使用现有 Secret",
            );
            self.update(PipelinePhase::CallbackConfig, PhaseStatus::Skipped, None);
            self.existing_secret.clone().unwrap_or_default()
        } else {
            self.log_phase(PipelinePhase::CallbackConfig, "开始配置微信客服回调地址");
            self.update(PipelinePhase::CallbackConfig, PhaseStatus::Running, None);
            match self
                .phase4_configure_callback(
                    &deploy.callback_url,
                    &deploy.callback_token,
                    &deploy.encoding_aes_key,
                )
                .await
            {
                Ok(s) => {
                    self.log_phase(PipelinePhase::CallbackConfig, "回调配置完成");
                    self.update(PipelinePhase::CallbackConfig, PhaseStatus::Done, None);
                    s
                }
                Err(e) => {
                    self.fail(PipelinePhase::CallbackConfig, &e.to_string());
                    return Err(e);
                }
            }
        };

        // Phase 5: Kefu account creation
        self.log_phase(
            PipelinePhase::KefuCreate,
            "开始创建 ClaudeWiki 助手客服账号",
        );
        self.update(PipelinePhase::KefuCreate, PhaseStatus::Running, None);
        let result = match self.phase5_create_kefu(&wecom.corpid, &secret).await {
            Ok(r) => {
                self.log_phase(
                    PipelinePhase::KefuCreate,
                    format!("客服账号创建完成，open_kfid={}", r.open_kfid),
                );
                let mut state = self.state_tx.borrow().clone();
                state.contact_url = Some(r.contact_url.clone());
                state.update_phase(PipelinePhase::KefuCreate, PhaseStatus::Done, None);
                state.finished_at = Some(now_iso8601());
                let _ = self.state_tx.send(state);
                r
            }
            Err(e) => {
                self.fail(PipelinePhase::KefuCreate, &e.to_string());
                return Err(e);
            }
        };

        // Persist full config
        self.save_full_config(&cf, &deploy, &wecom, &secret, &result)
            .await?;
        self.log_line("接入流程完成，已保存配置并准备启动监控");

        Ok(result)
    }

    async fn check_prerequisites(&self) -> Result<(), PipelineError> {
        let prereqs = WranglerDeployer::check_prerequisites();
        if !prereqs.node_ok {
            return Err(PipelineError::Prerequisite(
                "Node.js not found. Install: brew install node".into(),
            ));
        }
        if !prereqs.npx_ok {
            return Err(PipelineError::Prerequisite(
                "npx not found. Ensure Node.js is installed correctly.".into(),
            ));
        }
        if self.resolve_opencli_command().is_none() {
            return Err(PipelineError::Prerequisite(
                "OpenCLI unavailable. Install it globally or allow ClaudeWiki to use `npx --yes @jackwener/opencli`.".into(),
            ));
        }
        if self.requires_browser_bridge() {
            self.log_line("检查 OpenCLI Browser Bridge 连接状态");
            self.ensure_browser_bridge_ready().await?;
            self.log_line("OpenCLI Browser Bridge 已连接");
        }
        Ok(())
    }

    async fn ensure_browser_bridge_ready(&self) -> Result<(), PipelineError> {
        let report = self.opencli(&["doctor"]).await?;
        for line in report.lines().filter(|line| !line.trim().is_empty()) {
            let trimmed = line.trim();
            if trimmed.starts_with("[OK]")
                || trimmed.starts_with("[MISSING]")
                || trimmed.starts_with("[FAIL]")
                || trimmed.starts_with("Issues:")
            {
                self.log_line(format!("doctor: {trimmed}"));
            }
        }

        let extension_missing = report.contains("[MISSING] Extension: not connected")
            || report.contains("Browser Bridge extension not connected");
        let connectivity_failed = report.contains("[FAIL] Connectivity:");
        let extension_connected = report.contains("[OK] Extension: connected");
        let about_blank_false_negative =
            report.contains("Cannot access contents of url \"about:blank\"");

        if extension_missing || connectivity_failed {
            if extension_connected && about_blank_false_negative {
                self.log_line(
                    "doctor 连通性测试命中了 about:blank 权限限制，但扩展已连接；继续执行真实页面操作"
                );
                return Ok(());
            }
            return Err(PipelineError::Prerequisite(
                "OpenCLI Browser Bridge 未连接。请打开 Chrome，进入 chrome://extensions，确认已安装并启用 opencli Browser Bridge 扩展；若刚安装完，请刷新扩展或重启 Chrome。然后运行 `npx --yes @jackwener/opencli doctor`，直到看到 `[OK] Extension: connected` 与 `[OK] Connectivity: connected` 后再点“重新开始”。".into(),
            ));
        }

        Ok(())
    }

    fn requires_browser_bridge(&self) -> bool {
        !self.skip_cf_register || !self.skip_callback_config
    }

    async fn phase1_cf_register(&self) -> Result<CfCredentials, PipelineError> {
        let email_client = EmailClient::new();
        if let Some(existing_email) = self.detect_logged_in_cloudflare_session().await? {
            if is_disposable_email(&existing_email) {
                self.log_phase(
                    PipelinePhase::CfRegister,
                    format!(
                        "检测到 Cloudflare 已登录临时会话（{}），先退出旧会话，避免复用未验证邮箱或脏状态",
                        existing_email
                    ),
                );
                self.ensure_cloudflare_logged_out().await?;
            } else {
                self.log_phase(
                    PipelinePhase::CfRegister,
                    format!(
                        "检测到可复用的 Cloudflare 已登录会话（{}），直接复用并切换 Wrangler OAuth 登录态",
                        existing_email
                    ),
                );
                self.ensure_wrangler_oauth_login().await?;
                return Ok(CfCredentials {
                    email: existing_email,
                    password: String::new(),
                    api_token: String::new(),
                });
            }
        }

        self.log_phase(PipelinePhase::CfRegister, "申请临时邮箱地址");
        let account = email_client.create_address().await?;
        let password = generate_cloudflare_password(16);
        self.log_phase(
            PipelinePhase::CfRegister,
            format!("已创建临时邮箱 {}", account.address),
        );

        self.log_phase(PipelinePhase::CfRegister, "打开 Cloudflare 注册页");
        self.opencli(&["browser", "open", "https://dash.cloudflare.com/sign-up"])
            .await?;
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        self.prefill_cloudflare_signup_form(&account.address, &password)
            .await?;
        self.wait_for_cloudflare_human_check_and_submit(&account.address, &password)
            .await?;

        match self.await_cloudflare_signup_outcome().await? {
            CloudflareSignupOutcome::LoggedIn => {
                self.log_phase(
                    PipelinePhase::CfRegister,
                    "Cloudflare 注册已完成并进入控制台，继续准备 Wrangler 与邮箱验证",
                );
            }
            CloudflareSignupOutcome::NeedsEmailVerification => {
                self.verify_cloudflare_email(&email_client, &account.jwt)
                    .await?;
            }
        }

        self.ensure_wrangler_oauth_login().await?;
        self.ensure_cloudflare_workers_email_verified(&email_client, &account.jwt)
            .await?;

        Ok(CfCredentials {
            email: account.address,
            password,
            api_token: String::new(),
        })
    }

    async fn detect_logged_in_cloudflare_session(&self) -> Result<Option<String>, PipelineError> {
        self.log_phase(
            PipelinePhase::CfRegister,
            "探测当前浏览器中的 Cloudflare 登录态",
        );
        self.opencli(&[
            "browser",
            "open",
            "https://dash.cloudflare.com/profile/settings",
        ])
        .await?;
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let state = self
            .opencli_json_eval(
                r#"(() => JSON.stringify({
                url: window.location.href,
                title: document.title,
                hasEmailInput: !!document.querySelector('[name=email]'),
                hasPasswordInput: !!document.querySelector('[name=password]'),
                bodyText: (document.body?.innerText ?? '').slice(0, 3000)
            }))()"#,
            )
            .await?;

        let url = state["url"].as_str().unwrap_or("");
        let _title = state["title"].as_str().unwrap_or("");
        let has_email_input = state["hasEmailInput"].as_bool().unwrap_or(false);
        let has_password_input = state["hasPasswordInput"].as_bool().unwrap_or(false);
        let body_text = state["bodyText"].as_str().unwrap_or("");

        let on_dashboard = url.starts_with("https://dash.cloudflare.com/profile/settings")
            && !has_email_input
            && !has_password_input
            && (body_text.contains("Profile")
                || body_text.contains("Update email")
                || body_text.contains("Member since"));

        if !on_dashboard {
            return Ok(None);
        }

        let email = regex_lite::Regex::new(r"([A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,})")
            .ok()
            .and_then(|re| {
                re.captures(body_text)
                    .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
            })
            .unwrap_or_else(|| "current-account".to_string());

        Ok(Some(email))
    }

    async fn ensure_cloudflare_logged_out(&self) -> Result<(), PipelineError> {
        self.log_phase(PipelinePhase::CfRegister, "退出当前 Cloudflare 会话");
        self.opencli(&["browser", "open", "https://dash.cloudflare.com/logout"])
            .await?;
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        Ok(())
    }

    async fn verify_cloudflare_email(
        &self,
        email_client: &EmailClient,
        jwt: &str,
    ) -> Result<(), PipelineError> {
        self.log_phase(PipelinePhase::CfRegister, "等待 Cloudflare 验证邮件");
        let mail = self
            .wait_for_cloudflare_verification_mail(
                email_client,
                jwt,
                std::time::Duration::from_secs(300),
            )
            .await?;

        let html = mail["raw"]
            .as_str()
            .or_else(|| mail["html"].as_str())
            .or_else(|| mail["text"].as_str())
            .unwrap_or("");
        if let Some(link) = EmailClient::extract_verification_link(html) {
            self.log_phase(PipelinePhase::CfRegister, "收到验证邮件，打开验证链接");
            self.opencli(&["browser", "open", &link]).await?;
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            return Ok(());
        }
        Err(PipelineError::Email(
            "收到 Cloudflare 邮件，但未提取到可用的验证链接".into(),
        ))
    }

    async fn wait_for_cloudflare_verification_mail(
        &self,
        email_client: &EmailClient,
        jwt: &str,
        timeout: std::time::Duration,
    ) -> Result<serde_json::Value, PipelineError> {
        let deadline = tokio::time::Instant::now() + timeout;
        let mut seen_ids = std::collections::HashSet::new();
        let mut last_progress_log = tokio::time::Instant::now();

        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(PipelineError::Email(format!(
                    "timeout waiting for email ({}s)",
                    timeout.as_secs()
                )));
            }

            let mails = email_client.fetch_mails(jwt, 10).await?;
            for mail in &mails {
                let id = mail["id"].as_str().unwrap_or("").to_string();
                if !id.is_empty() && !seen_ids.insert(id) {
                    continue;
                }
                if let Some(summary) = summarize_mail(mail) {
                    self.log_phase(PipelinePhase::CfRegister, format!("收到新邮件：{summary}"));
                }
                if is_cloudflare_verification_mail(mail) {
                    return Ok(mail.clone());
                }
            }

            if last_progress_log.elapsed() >= std::time::Duration::from_secs(15) {
                self.log_phase(
                    PipelinePhase::CfRegister,
                    "邮箱暂未收到 Cloudflare 验证邮件，继续等待",
                );
                last_progress_log = tokio::time::Instant::now();
            }

            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
    }

    async fn ensure_cloudflare_workers_email_verified(
        &self,
        email_client: &EmailClient,
        jwt: &str,
    ) -> Result<(), PipelineError> {
        self.log_phase(
            PipelinePhase::CfRegister,
            "检查 Cloudflare Workers 是否要求邮箱验证",
        );
        self.open_cloudflare_workers_dashboard().await?;
        self.opencli_wait_text("Workers", 20_000).await?;

        let state = self.read_cloudflare_workers_state().await?;
        let needs_verification = state["needsVerification"].as_bool().unwrap_or(false);
        let email = state["email"].as_str().unwrap_or("");
        if !needs_verification {
            self.log_phase(PipelinePhase::CfRegister, "Workers 页面未要求额外邮箱验证");
            return Ok(());
        }

        let display_email = if email.is_empty() {
            "当前账号邮箱"
        } else {
            email
        };
        self.log_phase(
            PipelinePhase::CfRegister,
            format!("Workers 仍要求验证邮箱（{display_email}），触发重发验证邮件"),
        );
        self.click_exact_button_by_text("Resend email").await?;
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        self.verify_cloudflare_email(email_client, jwt).await?;

        self.open_cloudflare_workers_dashboard().await?;
        self.opencli_wait_text("Workers", 20_000).await?;
        let after = self.read_cloudflare_workers_state().await?;
        if after["needsVerification"].as_bool().unwrap_or(false) {
            return Err(PipelineError::Email(
                "验证链接已打开，但 Workers 页面仍提示邮箱未验证".into(),
            ));
        }

        self.log_phase(
            PipelinePhase::CfRegister,
            "Cloudflare 邮箱验证完成，Workers 权限已解锁",
        );
        Ok(())
    }

    async fn open_cloudflare_workers_dashboard(&self) -> Result<(), PipelineError> {
        self.opencli(&[
            "browser",
            "open",
            "https://dash.cloudflare.com/?to=/:account/workers-and-pages",
        ])
        .await?;
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        Ok(())
    }

    async fn read_cloudflare_workers_state(&self) -> Result<serde_json::Value, PipelineError> {
        self.opencli_json_eval(
            r#"(() => JSON.stringify({
                url: window.location.href,
                email: (document.body?.innerText ?? '').match(/[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/)?.[0] ?? '',
                needsVerification: (document.body?.innerText ?? '').includes('Verify your account')
                    || (document.body?.innerText ?? '').includes('To verify your account, click the link in the email.')
                    || !!Array.from(document.querySelectorAll('button')).find((button) => {
                        const text = (button.innerText || button.textContent || '').trim();
                        return text === 'Resend email';
                    }),
                bodyText: (document.body?.innerText ?? '').slice(0, 6000)
            }))()"#,
        )
        .await
    }

    async fn ensure_wrangler_oauth_login(&self) -> Result<(), PipelineError> {
        self.log_phase(PipelinePhase::CfRegister, "重置 Wrangler 登录态");
        let _ = self.run_wrangler_command(&["logout"]).await;

        self.log_phase(PipelinePhase::CfRegister, "启动 wrangler 官方 OAuth 登录");
        let mut child = TokioCommand::new("npx");
        child
            .args(["--yes", "wrangler", "login"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = child
            .spawn()
            .map_err(|e| PipelineError::OpenCli(format!("spawn wrangler login failed: {e}")))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| PipelineError::OpenCli("wrangler login stdout not captured".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| PipelineError::OpenCli("wrangler login stderr not captured".into()))?;
        let mut stdout_lines = BufReader::new(stdout).lines();
        let mut stderr_lines = BufReader::new(stderr).lines();
        let mut combined_output = String::new();
        let mut allow_clicked = false;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(120);

        loop {
            if tokio::time::Instant::now() >= deadline {
                let _ = child.kill().await;
                return Err(PipelineError::OpenCli(
                    "wrangler login timed out waiting for OAuth authorization".into(),
                ));
            }

            tokio::select! {
                line = stdout_lines.next_line() => {
                    let line = line
                        .map_err(|e| PipelineError::OpenCli(format!("read wrangler stdout failed: {e}")))?;
                    if let Some(line) = line {
                        let trimmed = strip_ansi_for_logs(&line);
                        if !trimmed.trim().is_empty() {
                            self.log_phase(PipelinePhase::CfRegister, format!("wrangler: {trimmed}"));
                            combined_output.push_str(&trimmed);
                            combined_output.push('\n');
                        }
                    }
                }
                line = stderr_lines.next_line() => {
                    let line = line
                        .map_err(|e| PipelineError::OpenCli(format!("read wrangler stderr failed: {e}")))?;
                    if let Some(line) = line {
                        let trimmed = strip_ansi_for_logs(&line);
                        if !trimmed.trim().is_empty() {
                            self.log_phase(PipelinePhase::CfRegister, format!("wrangler: {trimmed}"));
                            combined_output.push_str(&trimmed);
                            combined_output.push('\n');
                        }
                    }
                }
                status = child.wait() => {
                    let status = status
                        .map_err(|e| PipelineError::OpenCli(format!("wait wrangler login failed: {e}")))?;
                    if !status.success() {
                        return Err(PipelineError::OpenCli(format!(
                            "wrangler login failed: {}",
                            combined_output.trim()
                        )));
                    }
                    break;
                }
                _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                    if !allow_clicked && self.cloudflare_oauth_waiting_for_consent().await.unwrap_or(false) {
                        self.log_phase(PipelinePhase::CfRegister, "检测到 Wrangler OAuth 授权页，自动点击 Allow");
                        self.click_exact_button_by_text("Allow").await?;
                        allow_clicked = true;
                    }
                }
            }
        }

        let whoami = self.run_wrangler_command(&["whoami"]).await?;
        let email = extract_email_from_text(&whoami).unwrap_or_else(|| "unknown".to_string());
        self.log_phase(
            PipelinePhase::CfRegister,
            format!("Wrangler OAuth 登录成功，当前账号 {}", email),
        );
        Ok(())
    }

    async fn cloudflare_oauth_waiting_for_consent(&self) -> Result<bool, PipelineError> {
        let state = self
            .opencli_json_eval(
                r#"(() => JSON.stringify({
                url: window.location.href,
                bodyText: (document.body?.innerText ?? '').slice(0, 4000)
            }))()"#,
            )
            .await?;
        let url = state["url"].as_str().unwrap_or("");
        let body = state["bodyText"].as_str().unwrap_or("");
        Ok(url.contains("/oauth/consent-form")
            && body.contains("Allow Wrangler access to your Cloudflare account?"))
    }

    async fn run_wrangler_command(&self, args: &[&str]) -> Result<String, PipelineError> {
        let args_owned: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
        let output = tokio::task::spawn_blocking(move || {
            std::process::Command::new("npx")
                .args(["--yes", "wrangler"])
                .args(&args_owned)
                .output()
        })
        .await
        .map_err(|e| PipelineError::OpenCli(format!("wrangler task join failed: {e}")))?
        .map_err(|e| PipelineError::OpenCli(format!("spawn wrangler command failed: {e}")))?;

        let stdout = strip_ansi_for_logs(&String::from_utf8_lossy(&output.stdout));
        let stderr = strip_ansi_for_logs(&String::from_utf8_lossy(&output.stderr));
        if !output.status.success() {
            let detail = if stderr.trim().is_empty() {
                stdout.trim().to_string()
            } else {
                stderr.trim().to_string()
            };
            return Err(PipelineError::OpenCli(format!(
                "wrangler {} failed: {}",
                args.first().copied().unwrap_or("command"),
                detail
            )));
        }

        Ok(if stdout.trim().is_empty() {
            stderr
        } else {
            stdout
        })
    }

    async fn await_cloudflare_signup_outcome(
        &self,
    ) -> Result<CloudflareSignupOutcome, PipelineError> {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(45);

        loop {
            if tokio::time::Instant::now() >= deadline {
                self.log_phase(
                    PipelinePhase::CfRegister,
                    "提交注册后未检测到控制台跳转，转入邮件验证兜底分支",
                );
                return Ok(CloudflareSignupOutcome::NeedsEmailVerification);
            }

            let state = self.read_cloudflare_page_state().await?;
            let url = state["url"].as_str().unwrap_or("");
            let has_email_input = state["hasEmailInput"].as_bool().unwrap_or(false);
            let has_password_input = state["hasPasswordInput"].as_bool().unwrap_or(false);
            let body_text = state["bodyText"].as_str().unwrap_or("");

            let on_authenticated_dashboard = url.starts_with("https://dash.cloudflare.com/")
                && !url.contains("/sign-up")
                && !url.contains("/login")
                && !has_email_input
                && !has_password_input
                && (body_text.contains("Welcome! How do you plan to use Cloudflare?")
                    || body_text.contains("User API Tokens")
                    || body_text.contains("API Keys")
                    || body_text.contains("Quick search")
                    || body_text.contains("Accounts")
                    || body_text.contains("Websites"));

            if on_authenticated_dashboard {
                return Ok(CloudflareSignupOutcome::LoggedIn);
            }

            if body_text.contains("Check your email")
                || body_text.contains("verify your email")
                || body_text.contains("Verify your email")
                || body_text.contains("verification email")
            {
                self.log_phase(
                    PipelinePhase::CfRegister,
                    "Cloudflare 明确要求邮箱验证，开始等待验证邮件",
                );
                return Ok(CloudflareSignupOutcome::NeedsEmailVerification);
            }

            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    }

    async fn prefill_cloudflare_signup_form(
        &self,
        email: &str,
        password: &str,
    ) -> Result<(), PipelineError> {
        self.log_phase(PipelinePhase::CfRegister, "预填 Cloudflare 邮箱和密码");
        self.wait_for_cloudflare_signup_inputs().await?;
        let email_script = format!(
            r#"(() => {{
                const email = document.querySelector('[name=email]');
                if (!email) {{
                    throw new Error('email input not found');
                }}
                const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
                if (setter) {{
                    setter.call(email, {email_json});
                }} else {{
                    email.value = {email_json};
                }}
                email.dispatchEvent(new Event('input', {{ bubbles: true }}));
                email.dispatchEvent(new Event('change', {{ bubbles: true }}));
                email.blur();
                return JSON.stringify({{ email: email.value }});
            }})()"#,
            email_json = serde_json::to_string(email).unwrap_or_else(|_| "\"\"".to_string()),
        );
        self.opencli_json_eval(&email_script).await?;
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;

        let password_script = format!(
            r#"(() => {{
                const password = document.querySelector('[name=password]');
                if (!password) {{
                    throw new Error('password input not found');
                }}
                password.focus();
                const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
                if (setter) {{
                    setter.call(password, {password_json});
                }} else {{
                    password.value = {password_json};
                }}
                password.dispatchEvent(new Event('input', {{ bubbles: true }}));
                password.dispatchEvent(new Event('change', {{ bubbles: true }}));
                password.blur();
                return JSON.stringify({{ passwordLen: password.value.length }});
            }})()"#,
            password_json = serde_json::to_string(password).unwrap_or_else(|_| "\"\"".to_string()),
        );
        self.opencli_json_eval(&password_script).await?;
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;

        let result = self.read_cloudflare_signup_state().await?;
        let actual_email = result["email"].as_str().unwrap_or("");
        let password_len = result["passwordLen"].as_u64().unwrap_or(0) as usize;

        if actual_email == email && password_len == password.chars().count() {
            return Ok(());
        }

        self.log_phase(
            PipelinePhase::CfRegister,
            format!(
                "React 方式预填未完全生效，改用浏览器真实输入兜底（email_len={}, password_len={password_len}）",
                actual_email.len()
            ),
        );
        self.opencli(&["browser", "type", "[name=email]", email])
            .await?;
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        self.opencli(&["browser", "type", "[name=password]", password])
            .await?;
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;

        let retry_result = self.read_cloudflare_signup_state().await?;
        let retry_email = retry_result["email"].as_str().unwrap_or("");
        let retry_password_len = retry_result["passwordLen"].as_u64().unwrap_or(0) as usize;
        if retry_email != email || retry_password_len != password.chars().count() {
            return Err(PipelineError::OpenCli(format!(
                "Cloudflare signup form did not retain values (email_len={}, password_len={retry_password_len})",
                retry_email.len(),
            )));
        }

        Ok(())
    }

    async fn wait_for_cloudflare_signup_inputs(&self) -> Result<(), PipelineError> {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            let state = self.read_cloudflare_page_state().await?;
            let has_email_input = state["hasEmailInput"].as_bool().unwrap_or(false);
            let has_password_input = state["hasPasswordInput"].as_bool().unwrap_or(false);
            if has_email_input && has_password_input {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(PipelineError::OpenCli(
                    "Cloudflare signup inputs did not appear in time".into(),
                ));
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }

    async fn wait_for_cloudflare_human_check_and_submit(
        &self,
        email: &str,
        password: &str,
    ) -> Result<(), PipelineError> {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(600);
        let mut reminded = false;

        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(PipelineError::OpenCli(
                    "timeout waiting for Cloudflare human verification (600s)".into(),
                ));
            }

            let status = self.read_cloudflare_signup_state().await?;
            let actual_email = status["email"].as_str().unwrap_or("");
            let password_len = status["passwordLen"].as_u64().unwrap_or(0) as usize;
            let challenge_present = status["challengePresent"].as_bool().unwrap_or(false);
            let challenge_value = status["challengeValue"].as_str().unwrap_or("");
            let body_text = status["bodyText"].as_str().unwrap_or("");

            if actual_email != email || password_len != password.chars().count() {
                self.log_phase(
                    PipelinePhase::CfRegister,
                    "检测到 Cloudflare 表单被清空，重新填入邮箱和密码",
                );
                self.prefill_cloudflare_signup_form(email, password).await?;
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }

            if challenge_present && challenge_value.is_empty() {
                if !reminded {
                    self.update(
                        PipelinePhase::CfRegister,
                        PhaseStatus::WaitingScan,
                        Some("等待 Cloudflare 真人验证".to_string()),
                    );
                    self.log_phase(
                        PipelinePhase::CfRegister,
                        "请在已打开的 Cloudflare 页面完成「请验证您是真人」；完成后 ClaudeWiki 会自动继续提交",
                    );
                    reminded = true;
                }
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                continue;
            }

            if reminded {
                self.update(
                    PipelinePhase::CfRegister,
                    PhaseStatus::Running,
                    Some("真人验证已完成，继续提交注册".to_string()),
                );
                self.log_phase(PipelinePhase::CfRegister, "检测到真人验证已完成，继续执行");
            }

            self.log_phase(
                PipelinePhase::CfRegister,
                "同步 Turnstile token 到表单并提交",
            );
            self.opencli(&[
                "browser",
                "eval",
                r#"(() => {
                    /* ── 1. Read the token from the Turnstile widget ── */
                    let token = '';
                    if (window.turnstile) {
                        const widgets = document.querySelectorAll('[data-testid=challenge-widget-container] iframe');
                        /* try getResponse with any known widgetId first */
                        if (window.__cwTurnstileWidgetId) {
                            token = window.turnstile.getResponse(window.__cwTurnstileWidgetId) || '';
                        }
                        /* fallback: grab from hidden inputs the widget already created */
                        if (!token) {
                            token = document.querySelector('[name=cf-turnstile-response]')?.value
                                 || document.querySelector('[name=cf_challenge_response]')?.value
                                 || '';
                        }
                    }

                    /* ── 2. Ensure hidden fields exist and sync the token via React-compatible setter ── */
                    const form = document.querySelector('form[data-testid=signup-form]') || document.querySelector('form');
                    const ensureField = (name) => {
                        let el = document.querySelector(`[name="${name}"]`);
                        if (!el && form) {
                            el = document.createElement('input');
                            el.type = 'hidden';
                            el.name = name;
                            form.appendChild(el);
                        }
                        return el;
                    };
                    const setValue = (input, val) => {
                        if (!input) return;
                        const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
                        if (setter) setter.call(input, val);
                        else input.value = val;
                        input.dispatchEvent(new Event('input', { bubbles: true }));
                        input.dispatchEvent(new Event('change', { bubbles: true }));
                    };
                    if (token) {
                        setValue(ensureField('cf-turnstile-response'), token);
                        setValue(ensureField('cf_challenge_response'), token);
                    }

                    /* ── 3. Click submit ── */
                    const btn = document.querySelector('[data-testid=signup-submit-button]');
                    if (!btn) throw new Error('signup submit button missing');
                    btn.click();
                    return JSON.stringify({ clicked: true, tokenLen: token.length });
                })()"#,
            ])
            .await?;
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;

            let after_submit = self.read_cloudflare_signup_state().await?;
            let after_text = after_submit["bodyText"].as_str().unwrap_or("");
            if after_text.contains("An email address is required") {
                return Err(PipelineError::OpenCli(
                    "Cloudflare signup still reports missing email after submit".into(),
                ));
            }
            if after_text.contains("1 special character")
                || after_text.contains("Try another password")
            {
                return Err(PipelineError::OpenCli(
                    "Cloudflare signup rejected the generated password requirements".into(),
                ));
            }

            if body_text.contains("Let us know you are human") && challenge_present {
                self.log_phase(
                    PipelinePhase::CfRegister,
                    "真人验证已完成，已提交注册；等待 Cloudflare 返回注册结果",
                );
            }
            return Ok(());
        }
    }

    async fn read_cloudflare_page_state(&self) -> Result<serde_json::Value, PipelineError> {
        self.opencli_json_eval(
            r#"(() => JSON.stringify({
                url: window.location.href,
                title: document.title,
                hasEmailInput: !!document.querySelector('[name=email]'),
                hasPasswordInput: !!document.querySelector('[name=password]'),
                bodyText: (document.body?.innerText ?? '').slice(0, 4000)
            }))()"#,
        )
        .await
    }

    async fn read_cloudflare_signup_state(&self) -> Result<serde_json::Value, PipelineError> {
        self.opencli_json_eval(
            r#"(() => JSON.stringify({
                url: window.location.href,
                email: document.querySelector('[name=email]')?.value ?? '',
                passwordLen: (document.querySelector('[name=password]')?.value ?? '').length,
                challengePresent: !!document.querySelector('[name=cf_challenge_response], [name=cf-turnstile-response]'),
                challengeValue: document.querySelector('[name=cf_challenge_response]')?.value
                    ?? document.querySelector('[name=cf-turnstile-response]')?.value
                    ?? '',
                submitDisabled: !!document.querySelector('[data-testid=signup-submit-button]')?.disabled,
                bodyText: (document.body?.innerText ?? '').slice(0, 2000)
            }))()"#,
        )
        .await
    }

    #[allow(dead_code)]
    async fn programmatically_resolve_cloudflare_turnstile(&self) -> Result<bool, PipelineError> {
        self.log_phase(
            PipelinePhase::CfRegister,
            "尝试通过页面脚本补全 Turnstile token",
        );
        let value = self.opencli_json_eval(
            r#"(async () => {
                const host = document.querySelector('[data-testid=challenge-widget-container]');
                if (!host || !window.turnstile) {
                    return JSON.stringify({ ok: false, reason: 'host-or-turnstile-missing' });
                }
                const reactKey = Object.keys(host).find((key) => key.startsWith('__reactProps'));
                const props = reactKey ? host[reactKey]?.children?.[1]?.props : null;
                if (!props?.sitekey) {
                    return JSON.stringify({ ok: false, reason: 'sitekey-missing' });
                }

                const mount = host.querySelector('div > div') || host;
                let widgetId = window.__cwTurnstileWidgetId;
                if (!widgetId) {
                    mount.innerHTML = '';
                    widgetId = window.turnstile.render(mount, {
                        sitekey: props.sitekey,
                        action: props.action,
                        responseField: true,
                        responseFieldName: 'cf-turnstile-response',
                    });
                    window.__cwTurnstileWidgetId = widgetId;
                }

                try {
                    window.turnstile.execute(widgetId);
                } catch (error) {
                    return JSON.stringify({ ok: false, reason: String(error) });
                }

                await new Promise((resolve) => setTimeout(resolve, 5000));
                const token = window.turnstile.getResponse(widgetId) || '';
                if (!token) {
                    return JSON.stringify({ ok: false, reason: 'empty-response', widgetId });
                }

                const form = document.querySelector('form[data-testid=signup-form]');
                let legacy = document.querySelector('[name=cf_challenge_response]');
                if (!legacy && form) {
                    legacy = document.createElement('input');
                    legacy.type = 'hidden';
                    legacy.name = 'cf_challenge_response';
                    form.appendChild(legacy);
                }
                const turnstileField = document.querySelector('[name=cf-turnstile-response]');
                const setValue = (input) => {
                    if (!input) return;
                    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
                    if (setter) setter.call(input, token);
                    else input.value = token;
                    input.dispatchEvent(new Event('input', { bubbles: true }));
                    input.dispatchEvent(new Event('change', { bubbles: true }));
                };
                setValue(legacy);
                setValue(turnstileField);

                return JSON.stringify({
                    ok: true,
                    widgetId,
                    tokenLen: token.length,
                    legacyLen: legacy?.value?.length || 0,
                    turnstileLen: turnstileField?.value?.length || 0,
                });
            })()"#,
        )
        .await?;

        if value["ok"].as_bool().unwrap_or(false) {
            return Ok(true);
        }

        self.log_phase(
            PipelinePhase::CfRegister,
            format!(
                "页面脚本未拿到 Turnstile token：{}",
                value["reason"].as_str().unwrap_or("unknown")
            ),
        );
        Ok(false)
    }

    async fn opencli_wait_text(&self, text: &str, timeout_ms: u64) -> Result<(), PipelineError> {
        self.opencli(&[
            "browser",
            "wait",
            "text",
            text,
            "--timeout",
            &timeout_ms.to_string(),
        ])
        .await
        .map(|_| ())
    }

    async fn click_exact_button_by_text(&self, label: &str) -> Result<(), PipelineError> {
        let script = format!(
            r#"(() => {{
                const needle = {label_json}.trim().toLowerCase();
                const nodes = Array.from(document.querySelectorAll('button, [role="button"], input[type="submit"], a[role="button"]'));
                const labels = nodes.map((node) => {{
                    const raw = node.innerText || node.textContent || node.value || '';
                    return raw.replace(/\s+/g, ' ').trim();
                }});
                const index = labels.findIndex((text) => text.toLowerCase() === needle);
                if (index === -1) {{
                    return JSON.stringify({{
                        ok: false,
                        labels: labels.filter(Boolean).slice(0, 80),
                    }});
                }}
                nodes[index].click();
                return JSON.stringify({{
                    ok: true,
                    clicked: labels[index],
                }});
            }})()"#,
            label_json = serde_json::to_string(label).unwrap_or_else(|_| "\"\"".to_string()),
        );
        let value = self.opencli_json_eval(&script).await?;
        if value["ok"].as_bool().unwrap_or(false) {
            return Ok(());
        }

        let labels = value["labels"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str())
                    .collect::<Vec<_>>()
                    .join(" | ")
            })
            .unwrap_or_else(|| "<none>".to_string());
        Err(PipelineError::OpenCli(format!(
            "未在 Cloudflare 页面中找到精确按钮：{label}；候选按钮：{labels}"
        )))
    }

    async fn phase2_deploy_worker(&self, cf_token: &str) -> Result<DeployResult, PipelineError> {
        if cf_token.trim().is_empty() {
            self.log_phase(
                PipelinePhase::WorkerDeploy,
                "调用 wrangler deploy（使用 OAuth 登录态）",
            );
        } else {
            self.log_phase(
                PipelinePhase::WorkerDeploy,
                "调用 wrangler deploy（使用 API Token）",
            );
        }
        let deployer = WranglerDeployer::new((!cf_token.trim().is_empty()).then_some(cf_token));
        let result = tokio::task::spawn_blocking(move || deployer.deploy("pending"))
            .await
            .map_err(|e| PipelineError::Deploy(e.to_string()))??;

        self.log_phase(PipelinePhase::WorkerDeploy, "执行健康检查");
        super::deployer::health_check(&result.worker_url).await?;
        Ok(result)
    }

    async fn phase3_wecom_auth(&self) -> Result<WecomCredentials, PipelineError> {
        // Use opencli wecom-cli for QR scan auth
        self.log_phase(PipelinePhase::WecomAuth, "调用 OpenCLI 发起企业微信扫码");
        let output = self
            .opencli(&["wecom-cli", "auth", "login", "--scan", "--format", "json"])
            .await?;

        let parsed: serde_json::Value = serde_json::from_str(&output)
            .map_err(|e| PipelineError::WecomAuth(format!("parse wecom output: {e}")))?;

        let corpid = parsed["corpid"]
            .as_str()
            .or_else(|| parsed["corp_id"].as_str())
            .ok_or_else(|| PipelineError::WecomAuth("missing corpid in wecom output".into()))?
            .to_string();

        // Try to get kefu secret
        self.log_phase(PipelinePhase::WecomAuth, "尝试读取微信客服 Secret");
        let secret_output = self
            .opencli(&["wecom-cli", "kefu", "secret", "get", "--format", "json"])
            .await
            .unwrap_or_default();

        let secret = serde_json::from_str::<serde_json::Value>(&secret_output)
            .ok()
            .and_then(|v| v["secret"].as_str().map(|s| s.to_string()))
            .unwrap_or_default();

        Ok(WecomCredentials { corpid, secret })
    }

    async fn phase4_configure_callback(
        &self,
        callback_url: &str,
        callback_token: &str,
        encoding_aes_key: &str,
    ) -> Result<String, PipelineError> {
        // Open kf.weixin.qq.com callback config page
        self.log_phase(
            PipelinePhase::CallbackConfig,
            format!("打开回调配置页，目标 URL {}", callback_url),
        );
        self.opencli(&[
            "browser",
            "open",
            "https://kf.weixin.qq.com/kf/frame#/config/api_setting?isfirst=1",
        ])
        .await?;

        // Wait for form to load (may need WeChat Work scan login)
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;

        // Fill callback URL
        self.log_phase(
            PipelinePhase::CallbackConfig,
            "填入回调 URL / Token / EncodingAESKey",
        );
        self.opencli(&["browser", "type", "input:nth(0)", callback_url])
            .await?;
        // Fill Token
        self.opencli(&["browser", "type", "input:nth(1)", callback_token])
            .await?;
        // Fill EncodingAESKey
        self.opencli(&["browser", "type", "input:nth(2)", encoding_aes_key])
            .await?;

        // Click submit
        self.log_phase(PipelinePhase::CallbackConfig, "提交微信客服回调配置");
        self.opencli(&["browser", "click", "button:text('完成')"])
            .await?;

        // Wait for verification
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;

        // Try to extract Secret from the config page
        self.opencli(&[
            "browser",
            "open",
            "https://kf.weixin.qq.com/kf/frame#/config",
        ])
        .await?;
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        let page_text = self
            .opencli(&["browser", "get", "text", "body"])
            .await
            .unwrap_or_default();

        // Extract Secret (pattern: "Secret" followed by the value)
        let secret = extract_secret_from_page(&page_text).unwrap_or_default();
        Ok(secret)
    }

    async fn phase5_create_kefu(
        &self,
        corpid: &str,
        secret: &str,
    ) -> Result<PipelineResult, PipelineError> {
        self.log_phase(PipelinePhase::KefuCreate, "调用微信客服 API 创建账号");
        let client = KefuClient::new(corpid, secret);

        // Create account
        let open_kfid = client
            .create_account("ClaudeWiki助手")
            .await
            .map_err(|e| PipelineError::KefuApi(e.to_string()))?;

        // Generate contact URL
        let contact_url = client
            .get_contact_url(&open_kfid)
            .await
            .map_err(|e| PipelineError::KefuApi(e.to_string()))?;

        Ok(PipelineResult {
            open_kfid,
            contact_url,
        })
    }

    /// Unified opencli command execution.
    ///
    /// U2 fix: Use `run_node_tool` so the sibling-path derivation from
    /// D1 (node.exe's directory → npm.cmd / npx.cmd on Windows) also
    /// covers the pipeline path. Without this, Environment Doctor can
    /// report OpenCLI as available while "一键接入" still errors with
    /// "OpenCLI unavailable" because the desktop launch doesn't have
    /// npx in PATH.
    async fn opencli(&self, args: &[&str]) -> Result<String, PipelineError> {
        if self.cancel.is_cancelled() {
            return Err(PipelineError::Cancelled);
        }

        let (program, prefix_args) = self.resolve_opencli_command().ok_or_else(|| {
            PipelineError::Prerequisite(
                "OpenCLI unavailable. Install it globally or allow `npx --yes @jackwener/opencli`."
                    .into(),
            )
        })?;

        let args_owned: Vec<String> = prefix_args
            .iter()
            .map(|s| s.to_string())
            .chain(args.iter().map(|s| s.to_string()))
            .collect();
        let program_owned = program.to_string();
        let output = tokio::task::spawn_blocking(move || {
            let args_ref: Vec<&str> = args_owned.iter().map(String::as_str).collect();
            super::deployer::run_node_tool(&program_owned, &args_ref)
        })
        .await
        .map_err(|e| PipelineError::OpenCli(e.to_string()))?
        .map_err(|e| PipelineError::OpenCli(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PipelineError::OpenCli(format!(
                "opencli {} failed: {}",
                args.first().unwrap_or(&""),
                stderr
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    async fn opencli_json_eval(&self, script: &str) -> Result<serde_json::Value, PipelineError> {
        let output = self.opencli(&["browser", "eval", script]).await?;
        serde_json::from_str(output.trim()).map_err(|e| {
            PipelineError::OpenCli(format!(
                "failed to parse browser eval JSON: {e}; raw={}",
                output.trim()
            ))
        })
    }

    /// U2 fix: Probe via `run_node_tool` so sibling-path derivation
    /// covers desktop-launched processes that inherit only a partial
    /// Node PATH. See `deployer::run_node_tool` and the 4/19 observation
    /// ("desktop-launched backend inherits only part of the Node
    /// toolchain PATH") for the root-cause rationale.
    fn resolve_opencli_command(&self) -> Option<(&'static str, Vec<&'static str>)> {
        let opencli_ok = super::deployer::run_node_tool("opencli", &["--version"])
            .map(|o| o.status.success())
            .unwrap_or(false);
        if opencli_ok {
            return Some(("opencli", vec![]));
        }

        let npx_ok =
            super::deployer::run_node_tool("npx", &["--yes", "@jackwener/opencli", "--version"])
                .map(|o| o.status.success())
                .unwrap_or(false);
        if npx_ok {
            return Some(("npx", vec!["--yes", "@jackwener/opencli"]));
        }

        None
    }

    fn update(&self, phase: PipelinePhase, status: PhaseStatus, msg: Option<String>) {
        let mut state = self.state_tx.borrow().clone();
        state.update_phase(phase, status, msg);
        let _ = self.state_tx.send(state);
    }

    fn mark_started(&self) {
        let mut state = self.state_tx.borrow().clone();
        if state.started_at.is_none() {
            state.started_at = Some(now_iso8601());
            let _ = self.state_tx.send(state);
        }
    }

    fn fail(&self, phase: PipelinePhase, error: &str) {
        self.log_phase(phase, format!("失败：{error}"));
        let mut state = self.state_tx.borrow().clone();
        state.mark_failed(phase, error.to_string());
        if state.finished_at.is_none() {
            state.finished_at = Some(now_iso8601());
        }
        let _ = self.state_tx.send(state);
    }

    fn log_line(&self, line: impl Into<String>) {
        let mut state = self.state_tx.borrow().clone();
        state.logs.push(format!("[{}] {}", now_hms(), line.into()));
        if state.logs.len() > 200 {
            let keep_from = state.logs.len().saturating_sub(200);
            state.logs = state.logs.split_off(keep_from);
        }
        let _ = self.state_tx.send(state);
    }

    fn log_phase(&self, phase: PipelinePhase, line: impl Into<String>) {
        self.log_line(format!("[{}] {}", phase_label(phase), line.into()));
    }

    async fn save_full_config(
        &self,
        cf: &CfCredentials,
        deploy: &DeployResult,
        wecom: &WecomCredentials,
        secret: &str,
        result: &PipelineResult,
    ) -> Result<(), PipelineError> {
        let config = super::types::KefuConfig {
            corpid: wecom.corpid.clone(),
            secret: secret.to_string(),
            token: deploy.callback_token.clone(),
            encoding_aes_key: deploy.encoding_aes_key.clone(),
            open_kfid: Some(result.open_kfid.clone()),
            contact_url: Some(result.contact_url.clone()),
            account_name: Some("ClaudeWiki助手".to_string()),
            saved_at: None,
            cf_api_token: (!cf.api_token.trim().is_empty()).then_some(cf.api_token.clone()),
            worker_url: Some(deploy.worker_url.clone()),
            relay_ws_url: Some(deploy.ws_url.clone()),
            relay_auth_token: Some(deploy.auth_token.clone()),
            callback_url: Some(deploy.callback_url.clone()),
            callback_token_generated: Some(deploy.callback_token.clone()),
        };
        super::account::save_config(&config).map_err(|e| {
            PipelineError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })?;
        Ok(())
    }
}

fn extract_secret_from_page(text: &str) -> Option<String> {
    // Look for a pattern like "Secret" followed by an alphanumeric string
    if let Ok(re) = regex_lite::Regex::new(r"Secret\s*[:\s]*([a-zA-Z0-9_-]{20,})") {
        if let Some(caps) = re.captures(text) {
            return caps.get(1).map(|m| m.as_str().to_string());
        }
    }
    None
}

fn now_iso8601() -> String {
    use time::format_description::well_known::Rfc3339;
    time::OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "unknown".to_string())
}

fn now_hms() -> String {
    let now = time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    let format =
        time::format_description::parse("[hour]:[minute]:[second]").expect("valid hms format");
    now.format(&format)
        .unwrap_or_else(|_| "??:??:??".to_string())
}

fn generate_cloudflare_password(len: usize) -> String {
    use rand::seq::SliceRandom;
    use rand::Rng;

    const UPPER: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    const LOWER: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
    const DIGITS: &[u8] = b"0123456789";
    const SPECIAL: &[u8] = b"!$%&*#@";
    const ALL: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!$%&*#@";

    let target_len = len.max(12);
    let mut rng = rand::thread_rng();
    let mut chars = Vec::with_capacity(target_len);
    chars.push(UPPER[rng.gen_range(0..UPPER.len())] as char);
    chars.push(LOWER[rng.gen_range(0..LOWER.len())] as char);
    chars.push(DIGITS[rng.gen_range(0..DIGITS.len())] as char);
    chars.push(SPECIAL[rng.gen_range(0..SPECIAL.len())] as char);
    while chars.len() < target_len {
        chars.push(ALL[rng.gen_range(0..ALL.len())] as char);
    }
    chars.shuffle(&mut rng);
    chars.into_iter().collect()
}

fn is_disposable_email(email: &str) -> bool {
    let domain = super::email_client::configured_mail_domain();
    email
        .trim()
        .to_ascii_lowercase()
        .ends_with(&format!("@{}", domain.to_ascii_lowercase()))
}

fn is_cloudflare_verification_mail(mail: &serde_json::Value) -> bool {
    let haystack = [
        mail["subject"].as_str(),
        mail["from"].as_str(),
        mail["raw"].as_str(),
        mail["html"].as_str(),
        mail["text"].as_str(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("\n")
    .to_ascii_lowercase();

    haystack.contains("cloudflare")
        && (haystack.contains("verify")
            || haystack.contains("verification")
            || haystack.contains("confirm")
            || haystack.contains("activate"))
}

fn summarize_mail(mail: &serde_json::Value) -> Option<String> {
    let subject = mail["subject"].as_str().unwrap_or("").trim();
    let from = mail["from"].as_str().unwrap_or("").trim();
    let raw = mail["raw"].as_str().unwrap_or("");
    let fallback = raw
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim();

    if !subject.is_empty() || !from.is_empty() {
        Some(format!(
            "{}{}{}",
            if from.is_empty() { "" } else { from },
            if from.is_empty() || subject.is_empty() {
                ""
            } else {
                " · "
            },
            if subject.is_empty() {
                fallback
            } else {
                subject
            },
        ))
    } else if !fallback.is_empty() {
        Some(fallback.chars().take(120).collect())
    } else {
        None
    }
}

fn extract_email_from_text(text: &str) -> Option<String> {
    regex_lite::Regex::new(r"([A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,})")
        .ok()
        .and_then(|re| {
            re.captures(text)
                .and_then(|caps| caps.get(1).map(|m| m.as_str().to_string()))
        })
}

fn strip_ansi_for_logs(input: &str) -> String {
    regex_lite::Regex::new(r"\x1b\[[0-9;]*[A-Za-z]")
        .map(|re| re.replace_all(input, "").to_string())
        .unwrap_or_else(|_| input.to_string())
}

fn phase_label(phase: PipelinePhase) -> &'static str {
    match phase {
        PipelinePhase::CfRegister => "Cloudflare 注册",
        PipelinePhase::WorkerDeploy => "Worker 部署",
        PipelinePhase::WecomAuth => "企业微信授权",
        PipelinePhase::CallbackConfig => "回调配置",
        PipelinePhase::KefuCreate => "客服创建",
    }
}

// Re-export for deployer
pub use super::deployer::generate_random_alphanum;

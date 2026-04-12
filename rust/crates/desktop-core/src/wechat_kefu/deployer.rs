//! wrangler CLI deployer — scaffolds and deploys Worker to user's CF account.

use std::path::PathBuf;
use std::process::Command;

use super::pipeline_types::{DeployResult, PipelineError};

const WORKER_NAME: &str = "claudewiki-kefu-relay";

const WRANGLER_TOML: &str = include_str!("relay_template/wrangler.toml");
const WORKER_JS: &str = include_str!("relay_template/src/index.js");
const PACKAGE_JSON: &str = include_str!("relay_template/package.json");

pub struct PrereqStatus {
    pub node_ok: bool,
    pub npx_ok: bool,
    pub node_version: Option<String>,
}

pub struct WranglerDeployer {
    cf_api_token: Option<String>,
    project_dir: PathBuf,
}

impl WranglerDeployer {
    pub fn new(cf_api_token: Option<&str>) -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        Self {
            cf_api_token: cf_api_token
                .map(str::trim)
                .filter(|token| !token.is_empty())
                .map(ToOwned::to_owned),
            project_dir: PathBuf::from(home)
                .join(".warwolf")
                .join("wechat-kefu")
                .join("relay"),
        }
    }

    pub fn check_prerequisites() -> PrereqStatus {
        let node = Command::new("node")
            .arg("--version")
            .output()
            .ok();
        let npx = Command::new("npx")
            .arg("--version")
            .output()
            .ok();

        PrereqStatus {
            node_ok: node
                .as_ref()
                .map(|o| o.status.success())
                .unwrap_or(false),
            npx_ok: npx
                .as_ref()
                .map(|o| o.status.success())
                .unwrap_or(false),
            node_version: node
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string()),
        }
    }

    /// Write project template to disk.
    fn scaffold_project(&self) -> Result<(), PipelineError> {
        let dir = &self.project_dir;
        std::fs::create_dir_all(dir.join("src"))
            .map_err(|e| PipelineError::Deploy(format!("mkdir: {e}")))?;
        std::fs::write(dir.join("wrangler.toml"), WRANGLER_TOML)
            .map_err(|e| PipelineError::Deploy(format!("write wrangler.toml: {e}")))?;
        std::fs::write(dir.join("package.json"), PACKAGE_JSON)
            .map_err(|e| PipelineError::Deploy(format!("write package.json: {e}")))?;
        std::fs::write(dir.join("src/index.js"), WORKER_JS)
            .map_err(|e| PipelineError::Deploy(format!("write index.js: {e}")))?;
        eprintln!("[deployer] scaffolded project at {:?}", dir);
        Ok(())
    }

    /// Full deploy: scaffold → wrangler deploy → parse URL → secret bulk.
    pub fn deploy(&self, corpid: &str) -> Result<DeployResult, PipelineError> {
        self.scaffold_project()?;

        // wrangler deploy
        eprintln!("[deployer] running wrangler deploy...");
        let deploy_out = self
            .wrangler_command(["deploy"])
            .current_dir(&self.project_dir)
            .output()
            .map_err(|e| PipelineError::Deploy(format!("spawn wrangler: {e}")))?;

        if !deploy_out.status.success() {
            let stderr = strip_ansi(&String::from_utf8_lossy(&deploy_out.stderr));
            return Err(PipelineError::Deploy(format!(
                "wrangler deploy failed: {stderr}"
            )));
        }

        let stdout = String::from_utf8_lossy(&deploy_out.stdout);
        let worker_url = parse_worker_url(&stdout)?;
        eprintln!("[deployer] deployed: {worker_url}");

        // Generate secrets
        let auth_token = generate_random_hex(32);
        let callback_token = generate_random_alphanum(13);
        let encoding_aes_key = generate_random_alphanum(43);

        // wrangler secret bulk
        let secrets = serde_json::json!({
            "AUTH_TOKEN": auth_token,
            "CALLBACK_TOKEN": callback_token,
            "ENCODING_AES_KEY": encoding_aes_key,
            "CORPID": corpid,
        });

        eprintln!("[deployer] setting secrets...");
        let mut child = self
            .wrangler_command(["secret", "bulk"])
            .current_dir(&self.project_dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| PipelineError::Deploy(format!("spawn secret bulk: {e}")))?;

        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(serde_json::to_string(&secrets).unwrap().as_bytes());
        }

        let secret_out = child
            .wait_with_output()
            .map_err(|e| PipelineError::Deploy(format!("secret bulk: {e}")))?;

        if !secret_out.status.success() {
            let stderr = strip_ansi(&String::from_utf8_lossy(&secret_out.stderr));
            return Err(PipelineError::Deploy(format!(
                "secret bulk failed: {stderr}"
            )));
        }
        eprintln!("[deployer] secrets set successfully");

        let ws_url = worker_url.replace("https://", "wss://") + "/ws";

        Ok(DeployResult {
            worker_url: worker_url.clone(),
            callback_url: format!("{worker_url}/callback"),
            ws_url,
            auth_token,
            callback_token,
            encoding_aes_key,
        })
    }

    /// Re-deploy after Worker template update.
    pub fn upgrade(&self) -> Result<(), PipelineError> {
        std::fs::write(
            self.project_dir.join("src/index.js"),
            WORKER_JS,
        )
        .map_err(|e| PipelineError::Deploy(format!("write index.js: {e}")))?;

        let out = self
            .wrangler_command(["deploy"])
            .current_dir(&self.project_dir)
            .output()
            .map_err(|e| PipelineError::Deploy(format!("upgrade: {e}")))?;

        if !out.status.success() {
            return Err(PipelineError::Deploy(
                strip_ansi(&String::from_utf8_lossy(&out.stderr)),
            ));
        }
        Ok(())
    }

    /// Delete the Worker.
    pub fn undeploy(&self) -> Result<(), PipelineError> {
        let _ = self
            .wrangler_command(["delete", "--name", WORKER_NAME, "--force"])
            .current_dir(&self.project_dir)
            .output();
        Ok(())
    }

    fn wrangler_command<const N: usize>(&self, args: [&str; N]) -> Command {
        let mut command = Command::new("npx");
        command.args(["--yes", "wrangler"]);
        command.args(args);
        if let Some(token) = &self.cf_api_token {
            command.env("CLOUDFLARE_API_TOKEN", token);
        }
        command
    }
}

/// Async health check: GET {worker_url}/health.
pub async fn health_check(worker_url: &str) -> Result<(), PipelineError> {
    let http = reqwest::Client::new();
    let resp = http
        .get(format!("{worker_url}/health"))
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| PipelineError::Deploy(format!("health check: {e}")))?;

    if resp.status().is_success() {
        Ok(())
    } else {
        Err(PipelineError::Deploy(format!(
            "health check returned {}",
            resp.status()
        )))
    }
}

fn parse_worker_url(stdout: &str) -> Result<String, PipelineError> {
    // wrangler outputs: "Published claudewiki-kefu-relay (x.xxs)"
    // followed by: "https://claudewiki-kefu-relay.xxx.workers.dev"
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("https://") && trimmed.contains(".workers.dev") {
            return Ok(trimmed.to_string());
        }
    }
    // Fallback: regex
    if let Ok(re) =
        regex_lite::Regex::new(r"(https://[a-z0-9-]+\.[a-z0-9-]+\.workers\.dev)")
    {
        if let Some(caps) = re.captures(stdout) {
            return Ok(caps.get(1).unwrap().as_str().to_string());
        }
    }
    Err(PipelineError::Deploy(
        "could not parse worker URL from wrangler output".into(),
    ))
}

fn generate_random_hex(len: usize) -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..len).map(|_| format!("{:x}", rng.gen::<u8>() & 0xf)).collect()
}

pub fn generate_random_alphanum(len: usize) -> String {
    use rand::Rng;
    let charset = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| charset[rng.gen_range(0..charset.len())] as char)
        .collect()
}

fn strip_ansi(input: &str) -> String {
    regex_lite::Regex::new(r"\x1b\[[0-9;]*[A-Za-z]")
        .map(|re| re.replace_all(input, "").to_string())
        .unwrap_or_else(|_| input.to_string())
}

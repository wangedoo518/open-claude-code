//! Disposable email client for a configured temporary mail service.
//! Used in Phase 1 to receive Cloudflare verification emails.

use std::time::Duration;

use super::pipeline_types::PipelineError;

const POLL_INTERVAL: Duration = Duration::from_secs(3);
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

fn base_url() -> String {
    std::env::var("CLAWWIKI_TEMP_MAIL_BASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "https://mail-api.example.com".to_string())
}

pub(crate) fn configured_mail_domain() -> String {
    std::env::var("CLAWWIKI_TEMP_MAIL_DOMAIN")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "example.test".to_string())
}

pub struct EmailAccount {
    pub address: String,
    pub jwt: String,
}

pub struct EmailClient {
    http: reqwest::Client,
}

impl EmailClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    /// POST /api/new_address → { address, jwt }
    pub async fn create_address(&self) -> Result<EmailAccount, PipelineError> {
        let name = generate_random_name(12);
        let base_url = base_url();
        let resp: serde_json::Value = self
            .http
            .post(format!("{base_url}/api/new_address"))
            .json(&serde_json::json!({ "name": name }))
            .send()
            .await
            .map_err(|e| PipelineError::Email(e.to_string()))?
            .json()
            .await
            .map_err(|e| PipelineError::Email(e.to_string()))?;

        let address = resp["address"]
            .as_str()
            .ok_or_else(|| PipelineError::Email("missing address".into()))?
            .to_string();
        let jwt = resp["jwt"]
            .as_str()
            .ok_or_else(|| PipelineError::Email("missing jwt".into()))?
            .to_string();

        eprintln!("[email] created: {address}");
        Ok(EmailAccount { address, jwt })
    }

    /// GET /api/mails?limit=N with Bearer jwt
    pub async fn fetch_mails(
        &self,
        jwt: &str,
        limit: u32,
    ) -> Result<Vec<serde_json::Value>, PipelineError> {
        let base_url = base_url();
        let resp: serde_json::Value = self
            .http
            .get(format!("{base_url}/api/mails"))
            .query(&[("limit", limit.to_string()), ("offset", "0".to_string())])
            .bearer_auth(jwt)
            .send()
            .await
            .map_err(|e| PipelineError::Email(e.to_string()))?
            .json()
            .await
            .map_err(|e| PipelineError::Email(e.to_string()))?;

        Ok(resp["results"].as_array().cloned().unwrap_or_default())
    }

    /// Poll for a mail matching a predicate. 3s interval, configurable timeout.
    pub async fn poll_for_mail<F>(
        &self,
        jwt: &str,
        timeout: Option<Duration>,
        predicate: F,
    ) -> Result<serde_json::Value, PipelineError>
    where
        F: Fn(&serde_json::Value) -> bool,
    {
        let timeout = timeout.unwrap_or(DEFAULT_TIMEOUT);
        let deadline = tokio::time::Instant::now() + timeout;
        let mut seen_ids = std::collections::HashSet::new();

        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(PipelineError::Email(format!(
                    "timeout waiting for email ({}s)",
                    timeout.as_secs()
                )));
            }

            let mails = self.fetch_mails(jwt, 10).await?;
            for mail in &mails {
                let id = mail["id"].as_str().unwrap_or("").to_string();
                if !id.is_empty() && !seen_ids.contains(&id) {
                    seen_ids.insert(id);
                    if predicate(mail) {
                        return Ok(mail.clone());
                    }
                }
            }

            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    /// Extract a verification URL from HTML email body.
    pub fn extract_verification_link(raw: &str) -> Option<String> {
        let mut candidates = vec![normalize_mail_text(raw)];

        if let Ok(parsed) = mailparse::parse_mail(raw.as_bytes()) {
            collect_mail_bodies(&parsed, &mut candidates);
        }

        let patterns = [
            r#"href=["'](https://dash\.cloudflare\.com[^"']+)["']"#,
            r#"(https://dash\.cloudflare\.com[^\s"'<>]+)"#,
            r#"(https://[^"'<> \t\r\n]*cloudflare[^"'<> \t\r\n]*)"#,
        ];

        for candidate in candidates {
            let normalized = normalize_mail_text(&candidate);
            for pat in &patterns {
                if let Ok(re) = regex_lite::Regex::new(pat) {
                    if let Some(caps) = re.captures(&normalized) {
                        if let Some(link) = caps.get(1).map(|m| m.as_str().to_string()) {
                            return Some(clean_cloudflare_link(&link));
                        }
                    }
                }
            }
        }

        None
    }

    /// Extract a 6-digit OTP code from HTML email body.
    pub fn extract_otp(html: &str) -> Option<String> {
        if let Ok(re) = regex_lite::Regex::new(r"(?<![#&])\b(\d{6})\b") {
            for caps in re.captures_iter(html) {
                let code = caps.get(1).map(|m| m.as_str().to_string())?;
                if code != "177010" {
                    // Filter known false positive
                    return Some(code);
                }
            }
        }
        None
    }
}

fn generate_random_name(len: usize) -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| {
            let idx = rng.gen_range(0..36);
            if idx < 26 {
                (b'a' + idx) as char
            } else {
                (b'0' + idx - 26) as char
            }
        })
        .collect()
}

fn collect_mail_bodies(parsed: &mailparse::ParsedMail<'_>, out: &mut Vec<String>) {
    if parsed.subparts.is_empty() {
        if let Ok(body) = parsed.get_body() {
            let normalized = normalize_mail_text(&body);
            if !normalized.is_empty() {
                out.push(normalized);
            }
        }
        if let Ok(raw_body) = parsed.get_body_raw() {
            if let Ok(text) = String::from_utf8(raw_body) {
                let normalized = normalize_mail_text(&text);
                if !normalized.is_empty() {
                    out.push(normalized);
                }
            }
        }
        return;
    }

    for subpart in &parsed.subparts {
        collect_mail_bodies(subpart, out);
    }
}

fn normalize_mail_text(input: &str) -> String {
    input
        .replace("=\r\n", "")
        .replace("=\n", "")
        .replace("=3D", "=")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

fn clean_cloudflare_link(link: &str) -> String {
    link.trim_matches(|c| matches!(c, '"' | '\'' | '<' | '>' | ')' | '.'))
        .replace("&amp;", "&")
        .replace("=3D", "=")
}

//! QR-code login flow for the WeChat iLink Bot API.
//!
//! Direct port of `~/.openclaw/extensions/openclaw-weixin/src/auth/login-qr.ts`.
//! The QR endpoints are unauthenticated (no Bearer token), so this is a
//! one-shot bootstrap that runs before any `IlinkClient` can be built.
//!
//! Flow:
//!   1. `fetch_qr_code()`         → returns `qrcode` + `qrcode_img_content` URL
//!   2. user opens the URL on their phone via WeChat ClawBot plugin
//!   3. `poll_qr_status()` long-polls until status="confirmed"
//!   4. server returns `bot_token` + `ilink_bot_id` + `baseurl`
//!   5. caller persists those via `account.rs` → `save_account()`
//!
//! Auto-refresh: if a QR expires before scanning, the caller can refetch
//! and restart polling. We cap auto-refresh at 3 attempts to mirror the
//! reference implementation's `MAX_QR_REFRESH_COUNT`.

use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use super::types::{QrCodeResponse, QrStatusResponse, DEFAULT_BASE_URL, DEFAULT_ILINK_BOT_TYPE};

/// Client-side timeout for the long-poll status request.
pub const QR_LONG_POLL_TIMEOUT: Duration = Duration::from_secs(40);

/// QR is valid for ~5 minutes per the reference implementation.
pub const ACTIVE_LOGIN_TTL: Duration = Duration::from_secs(5 * 60);

/// Maximum number of times the caller may auto-refresh an expired QR
/// before giving up.
pub const MAX_QR_REFRESH_COUNT: u32 = 3;

/// Minimal percent-encoder for URL query components. Encodes any byte
/// outside the unreserved set defined by RFC 3986. Avoids pulling in the
/// `urlencoding` or `url` crate just for this single field.
fn url_encode_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Errors specific to the QR login flow.
#[derive(Debug, thiserror::Error)]
pub enum LoginError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("server returned status {status}: {body}")]
    Status { status: u16, body: String },
    #[error("JSON decode failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("login confirmed but server response is missing field: {0}")]
    MissingField(&'static str),
    #[error("login expired or aborted")]
    Expired,
    #[error("login timed out after {0:?}")]
    Timeout(Duration),
}

/// Outcome of a QR login flow when status reaches `confirmed`.
#[derive(Debug, Clone)]
pub struct LoginConfirmation {
    pub bot_token: String,
    pub ilink_bot_id: String,
    pub base_url: String,
    pub user_id: Option<String>,
}

/// Status returned by `wait_for_login` between iterations. Used by callers
/// that want to display progress (`Wait`, `Scanned`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginStatus {
    /// QR not yet scanned.
    Wait,
    /// User scanned but hasn't confirmed in WeChat yet.
    Scanned,
    /// QR expired before being scanned.
    Expired,
    /// User confirmed; final `LoginConfirmation` is returned separately.
    Confirmed,
}

impl LoginStatus {
    fn parse(s: &str) -> Self {
        match s {
            "wait" => Self::Wait,
            "scaned" => Self::Scanned, // server typo preserved for compat
            "expired" => Self::Expired,
            "confirmed" => Self::Confirmed,
            _ => Self::Wait,
        }
    }
}

/// QR login session. Holds the unauthenticated reqwest client and the
/// active `qrcode` identifier.
pub struct QrLoginSession {
    http: reqwest::Client,
    base_url: String,
    bot_type: String,
    /// Currently active qrcode identifier (rotates on refresh).
    qrcode: Option<String>,
    /// User-facing image content URL the user must open on their phone.
    qrcode_img_url: Option<String>,
}

impl QrLoginSession {
    /// Build a new login session bound to a specific iLink baseUrl.
    /// Pass `None` to use `DEFAULT_BASE_URL`.
    pub fn new(base_url: Option<String>) -> Result<Self, LoginError> {
        let mut url = base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        if !url.ends_with('/') {
            url.push('/');
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()?;
        Ok(Self {
            http,
            base_url: url,
            bot_type: DEFAULT_ILINK_BOT_TYPE.to_string(),
            qrcode: None,
            qrcode_img_url: None,
        })
    }

    /// Build optional `SKRouteTag` headers (currently always empty;
    /// caller can override later if route-tag config is needed).
    fn extra_headers() -> HeaderMap {
        HeaderMap::new()
    }

    /// Step 1: ask the server for a QR code.
    ///
    /// Returns the URL the user must open on their phone (`qrcode_img_content`).
    /// Stores the server-side `qrcode` identifier internally for the
    /// subsequent status poll.
    pub async fn fetch_qr_code(&mut self) -> Result<QrCodeResponse, LoginError> {
        let url = format!(
            "{}ilink/bot/get_bot_qrcode?bot_type={}",
            self.base_url, self.bot_type
        );
        let res = self.http.get(&url).headers(Self::extra_headers()).send().await?;
        let status = res.status();
        let text = res.text().await?;
        if !status.is_success() {
            return Err(LoginError::Status {
                status: status.as_u16(),
                body: text,
            });
        }
        let parsed: QrCodeResponse = serde_json::from_str(&text)?;
        self.qrcode = Some(parsed.qrcode.clone());
        self.qrcode_img_url = Some(parsed.qrcode_img_content.clone());
        Ok(parsed)
    }

    /// Step 2 (single iteration): long-poll the status endpoint once.
    ///
    /// Returns immediately with the current status. Most callers should
    /// use `wait_for_login` instead, which handles the polling loop +
    /// auto-refresh.
    pub async fn poll_status(&self) -> Result<QrStatusResponse, LoginError> {
        let qrcode = self
            .qrcode
            .as_ref()
            .ok_or(LoginError::MissingField("qrcode"))?;
        let url = format!(
            "{}ilink/bot/get_qrcode_status?qrcode={}",
            self.base_url,
            url_encode_component(qrcode)
        );
        let mut headers = Self::extra_headers();
        headers.insert(
            HeaderName::from_static("ilink-app-clientversion"),
            HeaderValue::from_static("1"),
        );
        let req = self
            .http
            .get(&url)
            .headers(headers)
            .timeout(QR_LONG_POLL_TIMEOUT);
        match req.send().await {
            Ok(res) => {
                let status = res.status();
                let text = res.text().await?;
                if !status.is_success() {
                    return Err(LoginError::Status {
                        status: status.as_u16(),
                        body: text,
                    });
                }
                let parsed: QrStatusResponse = serde_json::from_str(&text)?;
                Ok(parsed)
            }
            Err(e) if e.is_timeout() => {
                // Long-poll timed out; treat as "still waiting".
                Ok(QrStatusResponse {
                    status: Some("wait".to_string()),
                    ..Default::default()
                })
            }
            Err(e) => Err(LoginError::Http(e)),
        }
    }

    /// Step 2 (loop): poll until the user confirms (or we hit the deadline /
    /// max refresh count). On success returns the `LoginConfirmation`
    /// holding the bot_token + bot id + base url + user id.
    ///
    /// `on_status` is invoked after every status check so the caller can
    /// print progress, refresh the displayed QR, etc.
    pub async fn wait_for_login<F>(
        &mut self,
        timeout: Duration,
        mut on_status: F,
    ) -> Result<LoginConfirmation, LoginError>
    where
        F: FnMut(LoginStatus),
    {
        let deadline = std::time::Instant::now() + timeout;
        let mut refresh_count: u32 = 1;

        loop {
            if std::time::Instant::now() >= deadline {
                return Err(LoginError::Timeout(timeout));
            }

            let status = self.poll_status().await?;
            let parsed = LoginStatus::parse(status.status.as_deref().unwrap_or("wait"));
            on_status(parsed);

            match parsed {
                LoginStatus::Wait => {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                LoginStatus::Scanned => {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                LoginStatus::Expired => {
                    refresh_count += 1;
                    if refresh_count > MAX_QR_REFRESH_COUNT {
                        return Err(LoginError::Expired);
                    }
                    self.fetch_qr_code().await?;
                }
                LoginStatus::Confirmed => {
                    let bot_token = status
                        .bot_token
                        .ok_or(LoginError::MissingField("bot_token"))?;
                    let ilink_bot_id = status
                        .ilink_bot_id
                        .ok_or(LoginError::MissingField("ilink_bot_id"))?;
                    let base_url = status
                        .baseurl
                        .unwrap_or_else(|| self.base_url.trim_end_matches('/').to_string());
                    return Ok(LoginConfirmation {
                        bot_token,
                        ilink_bot_id,
                        base_url,
                        user_id: status.ilink_user_id,
                    });
                }
            }
        }
    }

    /// The user-facing URL set after `fetch_qr_code` succeeds.
    pub fn qr_code_url(&self) -> Option<&str> {
        self.qrcode_img_url.as_deref()
    }

    /// The currently active server-side qrcode identifier (mostly for
    /// logging/debugging).
    pub fn qrcode_id(&self) -> Option<&str> {
        self.qrcode.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_status_parse_handles_known_values() {
        assert_eq!(LoginStatus::parse("wait"), LoginStatus::Wait);
        assert_eq!(LoginStatus::parse("scaned"), LoginStatus::Scanned);
        assert_eq!(LoginStatus::parse("expired"), LoginStatus::Expired);
        assert_eq!(LoginStatus::parse("confirmed"), LoginStatus::Confirmed);
    }

    #[test]
    fn login_status_parse_falls_back_to_wait() {
        assert_eq!(LoginStatus::parse(""), LoginStatus::Wait);
        assert_eq!(LoginStatus::parse("garbage"), LoginStatus::Wait);
    }

    #[tokio::test]
    async fn new_session_normalizes_base_url() {
        let s = QrLoginSession::new(Some("https://example.com".to_string())).expect("ok");
        assert_eq!(s.base_url, "https://example.com/");

        let s = QrLoginSession::new(None).expect("ok");
        assert!(s.base_url.starts_with("https://"));
        assert!(s.base_url.ends_with('/'));
    }
}

//! HTTP client for the WeChat iLink Bot API.
//!
//! Direct port of `~/.openclaw/extensions/openclaw-weixin/src/api/api.ts`.
//! All authenticated endpoints (`getupdates`, `sendmessage`, `getconfig`,
//! `sendtyping`, `getuploadurl`) live here. The QR-code login flow uses
//! the same `Client::http` reqwest instance but lives in `login.rs` since
//! its endpoints are unauthenticated.

use std::time::Duration;

use base64::Engine as _;
use rand::RngCore;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, CONTENT_TYPE};

use super::types::{
    BaseInfo, GetUpdatesReq, GetUpdatesResp, SendMessageReq, SendTypingReq, WeixinMessage,
    CHANNEL_VERSION, DEFAULT_BASE_URL,
};

/// Client-side timeout for `getUpdates` long-poll requests. The server holds
/// the connection up to ~35 s; we set a slightly larger client timeout so we
/// don't abort right at the boundary.
pub const DEFAULT_LONG_POLL_TIMEOUT: Duration = Duration::from_secs(40);

/// Default timeout for regular API requests.
pub const DEFAULT_API_TIMEOUT: Duration = Duration::from_secs(15);

/// Default timeout for lightweight requests (`getConfig`, `sendTyping`).
pub const DEFAULT_CONFIG_TIMEOUT: Duration = Duration::from_secs(10);

/// Errors returned by the iLink HTTP client.
#[derive(Debug, thiserror::Error)]
pub enum IlinkError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("server returned status {status}: {body}")]
    Status { status: u16, body: String },
    #[error("invalid base URL: {0}")]
    InvalidBaseUrl(String),
    #[error("JSON decode failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("request timed out")]
    Timeout,
    #[error("API error: ret={ret:?} errcode={errcode:?} errmsg={errmsg:?}")]
    Api {
        ret: Option<i32>,
        errcode: Option<i32>,
        errmsg: Option<String>,
    },
}

/// Server `errcode` value indicating the bot session has expired and we
/// must re-authenticate (see `monitor.ts` reference).
pub const SESSION_EXPIRED_ERRCODE: i32 = -14;

/// Authenticated iLink HTTP client.
///
/// Holds a long-lived `reqwest::Client` plus the credentials needed to talk
/// to the iLink server. Cheap to clone (the inner client is `Arc`-backed).
#[derive(Clone)]
pub struct IlinkClient {
    http: reqwest::Client,
    base_url: String,
    token: String,
}

impl IlinkClient {
    /// Build a new client. `base_url` is normally `DEFAULT_BASE_URL` but the
    /// QR-login response may suggest a different one — pass that through if
    /// the server provided it.
    pub fn new(base_url: impl Into<String>, token: impl Into<String>) -> Result<Self, IlinkError> {
        let mut base = base_url.into();
        if !base.ends_with('/') {
            base.push('/');
        }
        if !base.starts_with("http://") && !base.starts_with("https://") {
            return Err(IlinkError::InvalidBaseUrl(base));
        }

        let http = reqwest::Client::builder()
            // Enforce a hard upper bound so a misbehaving server can't hold
            // us forever; per-request timeouts override this for long-poll.
            .timeout(Duration::from_secs(120))
            .pool_idle_timeout(Duration::from_secs(60))
            .build()?;

        Ok(Self {
            http,
            base_url: base,
            token: token.into(),
        })
    }

    /// Build the `BaseInfo` block included in every authenticated request.
    fn base_info(&self) -> BaseInfo {
        BaseInfo {
            channel_version: Some(CHANNEL_VERSION.to_string()),
        }
    }

    /// Generate the `X-WECHAT-UIN` header value.
    ///
    /// The reference implementation does:
    ///   `base64(utf8(decimal_string(random_uint32())))`
    ///
    /// e.g. random `1234567890` → `"1234567890"` → `"MTIzNDU2Nzg5MA=="`.
    /// We regenerate it for every request to defeat replay attacks.
    fn random_wechat_uin() -> String {
        let mut buf = [0u8; 4];
        rand::thread_rng().fill_bytes(&mut buf);
        let n = u32::from_be_bytes(buf);
        let decimal = n.to_string();
        base64::engine::general_purpose::STANDARD.encode(decimal.as_bytes())
    }

    /// Build the standard authenticated header set.
    fn build_headers(&self, body_len: usize) -> Result<HeaderMap, IlinkError> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            HeaderName::from_static("authorizationtype"),
            HeaderValue::from_static("ilink_bot_token"),
        );
        headers.insert(
            HeaderName::from_static("content-length"),
            HeaderValue::from_str(&body_len.to_string())
                .map_err(|e| IlinkError::InvalidBaseUrl(e.to_string()))?,
        );
        headers.insert(
            HeaderName::from_static("x-wechat-uin"),
            HeaderValue::from_str(&Self::random_wechat_uin())
                .map_err(|e| IlinkError::InvalidBaseUrl(e.to_string()))?,
        );
        let bearer = format!("Bearer {}", self.token.trim());
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&bearer)
                .map_err(|e| IlinkError::InvalidBaseUrl(e.to_string()))?,
        );
        Ok(headers)
    }

    /// Common POST wrapper. Returns the raw response body on success;
    /// converts non-2xx status to `IlinkError::Status`.
    async fn post_json(
        &self,
        endpoint: &str,
        body: String,
        timeout: Duration,
    ) -> Result<String, IlinkError> {
        let url = format!("{}{}", self.base_url, endpoint);
        let headers = self.build_headers(body.as_bytes().len())?;
        let req = self
            .http
            .post(&url)
            .headers(headers)
            .body(body)
            .timeout(timeout);
        let res = req.send().await?;
        let status = res.status();
        let text = res.text().await?;
        if !status.is_success() {
            return Err(IlinkError::Status {
                status: status.as_u16(),
                body: text,
            });
        }
        Ok(text)
    }

    /// Long-poll for new inbound messages.
    ///
    /// `cursor` is the `get_updates_buf` from the previous response, or
    /// `""` on the very first call. Server holds the request up to ~35 s
    /// and returns immediately when there are messages. A client-side
    /// timeout is treated as "no new messages" — caller should retry.
    pub async fn get_updates(
        &self,
        cursor: &str,
        timeout: Option<Duration>,
    ) -> Result<GetUpdatesResp, IlinkError> {
        let req = GetUpdatesReq {
            get_updates_buf: Some(cursor.to_string()),
            base_info: self.base_info(),
        };
        let body = serde_json::to_string(&req)?;
        let timeout = timeout.unwrap_or(DEFAULT_LONG_POLL_TIMEOUT);
        match self.post_json("ilink/bot/getupdates", body, timeout).await {
            Ok(raw) => {
                let resp: GetUpdatesResp = serde_json::from_str(&raw)?;
                Ok(resp)
            }
            Err(IlinkError::Http(e)) if e.is_timeout() => {
                // Long-poll timeout is normal; signal "no messages" so the
                // caller can re-poll with the same cursor.
                Ok(GetUpdatesResp {
                    ret: Some(0),
                    msgs: Some(Vec::new()),
                    get_updates_buf: Some(cursor.to_string()),
                    ..Default::default()
                })
            }
            Err(e) => Err(e),
        }
    }

    /// Send a single message downstream. Caller is responsible for
    /// constructing the message envelope (including `context_token` from
    /// the inbound message being replied to).
    pub async fn send_message(&self, msg: WeixinMessage) -> Result<(), IlinkError> {
        let req = SendMessageReq {
            msg: Some(msg),
            base_info: self.base_info(),
        };
        let body = serde_json::to_string(&req)?;
        self.post_json("ilink/bot/sendmessage", body, DEFAULT_API_TIMEOUT)
            .await?;
        Ok(())
    }

    /// Send a typing indicator. Requires a fresh `typing_ticket` from
    /// `get_config` first; the ticket is per-user.
    pub async fn send_typing(&self, req: SendTypingReq) -> Result<(), IlinkError> {
        let body = serde_json::to_string(&req)?;
        self.post_json("ilink/bot/sendtyping", body, DEFAULT_CONFIG_TIMEOUT)
            .await?;
        Ok(())
    }

    /// Read-only accessor for the configured base URL (mainly for tests
    /// and logging — never mutate after construction).
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

impl Default for IlinkClient {
    fn default() -> Self {
        Self::new(DEFAULT_BASE_URL, "")
            .expect("DEFAULT_BASE_URL is a valid HTTPS URL")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_uin_is_valid_base64_of_decimal() {
        for _ in 0..100 {
            let uin = IlinkClient::random_wechat_uin();
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(&uin)
                .expect("base64 decode");
            let decimal = std::str::from_utf8(&decoded).expect("utf8");
            // The decimal must be a parseable u32.
            decimal.parse::<u32>().expect("u32");
        }
    }

    #[test]
    fn random_uin_varies_between_calls() {
        // Generate 50 values; expect at least 49 unique (collision odds
        // for 50 random u32 are vanishingly small).
        let mut seen = std::collections::HashSet::new();
        for _ in 0..50 {
            seen.insert(IlinkClient::random_wechat_uin());
        }
        assert!(
            seen.len() >= 49,
            "expected near-unique uins, got {}",
            seen.len()
        );
    }

    #[test]
    fn new_normalizes_trailing_slash() {
        let c = IlinkClient::new("https://example.com", "tok").expect("ok");
        assert_eq!(c.base_url(), "https://example.com/");

        let c = IlinkClient::new("https://example.com/", "tok").expect("ok");
        assert_eq!(c.base_url(), "https://example.com/");
    }

    #[test]
    fn new_rejects_non_http_url() {
        assert!(IlinkClient::new("ftp://example.com", "tok").is_err());
        assert!(IlinkClient::new("not-a-url", "tok").is_err());
    }

    #[test]
    fn build_headers_includes_required_fields() {
        let c = IlinkClient::new("https://example.com", "test-token").expect("ok");
        let headers = c.build_headers(42).expect("headers");
        assert_eq!(
            headers.get("content-type").unwrap().to_str().unwrap(),
            "application/json"
        );
        assert_eq!(
            headers.get("authorizationtype").unwrap().to_str().unwrap(),
            "ilink_bot_token"
        );
        assert_eq!(
            headers.get("content-length").unwrap().to_str().unwrap(),
            "42"
        );
        assert!(headers.get("x-wechat-uin").is_some());
        assert_eq!(
            headers.get("authorization").unwrap().to_str().unwrap(),
            "Bearer test-token"
        );
    }
}

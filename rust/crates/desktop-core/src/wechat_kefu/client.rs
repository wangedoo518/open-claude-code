//! Thin wrapper over wxkefu-rs for WeChat Customer Service API calls.
//!
//! Uses wxkefu-rs for access_token and account management, but calls
//! sync_msg and send_msg directly with our own reqwest client (which
//! has rustls-tls enabled) to avoid TLS issues in wxkefu-rs's internal client.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;
use wxkefu_rs::token::{Auth, KfClient};

const TOKEN_REFRESH_BUFFER: Duration = Duration::from_secs(300);

#[derive(Debug, thiserror::Error)]
pub enum KefuClientError {
    #[error("wxkefu api: {0}")]
    Api(String),
}

/// sync_msg result using raw JSON values to handle all message types
/// (wxkefu-rs's typed enum doesn't support `link`, `business_card`, etc.)
#[derive(Debug, Clone)]
pub struct SyncMsgResult {
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub msg_list: Vec<serde_json::Value>,
}

/// HTTP client for the official WeChat Customer Service API.
#[derive(Clone)]
pub struct KefuClient {
    inner: KfClient,
    http: reqwest::Client,
    auth: Auth,
    token_cache: Arc<RwLock<Option<(String, Instant, Duration)>>>,
}

impl KefuClient {
    pub fn new(corpid: &str, secret: &str) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .pool_idle_timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            inner: KfClient::with_http(http.clone()),
            http,
            auth: Auth::WeCom {
                corp_id: corpid.to_string(),
                corp_secret: secret.to_string(),
            },
            token_cache: Arc::new(RwLock::new(None)),
        }
    }

    /// Get a valid access_token, auto-refreshing if expired.
    pub async fn access_token(&self) -> Result<String, KefuClientError> {
        {
            let cache = self.token_cache.read().await;
            if let Some((ref token, created_at, lifetime)) = *cache {
                if created_at.elapsed() < lifetime - TOKEN_REFRESH_BUFFER {
                    return Ok(token.clone());
                }
            }
        }

        let result = self
            .inner
            .get_access_token(&self.auth)
            .await
            .map_err(|e| KefuClientError::Api(e.to_string()))?;

        let token = result.access_token.clone();
        let lifetime = Duration::from_secs(result.expires_in as u64);
        {
            let mut cache = self.token_cache.write().await;
            *cache = Some((token.clone(), Instant::now(), lifetime));
        }
        eprintln!(
            "[kefu client] refreshed access_token (expires in {}s)",
            result.expires_in
        );
        Ok(token)
    }

    // --- Account management (via wxkefu-rs) ---

    pub async fn create_account(&self, name: &str) -> Result<String, KefuClientError> {
        let token = self.access_token().await?;
        let req = wxkefu_rs::account::AccountAddRequest {
            name: name.to_string(),
            media_id: String::new(),
        };
        let resp = self
            .inner
            .account_add(&token, &req)
            .await
            .map_err(|e| KefuClientError::Api(e.to_string()))?;
        Ok(resp.open_kfid)
    }

    pub async fn list_accounts(
        &self,
    ) -> Result<Vec<wxkefu_rs::account::AccountListItem>, KefuClientError> {
        let token = self.access_token().await?;
        let req = wxkefu_rs::account::AccountListRequest {
            offset: None,
            limit: None,
        };
        let resp = self
            .inner
            .account_list(&token, &req)
            .await
            .map_err(|e| KefuClientError::Api(e.to_string()))?;
        Ok(resp.account_list)
    }

    pub async fn get_contact_url(&self, open_kfid: &str) -> Result<String, KefuClientError> {
        let token = self.access_token().await?;
        let req = wxkefu_rs::account::AddContactWayRequest {
            open_kfid: open_kfid.to_string(),
            scene: None,
        };
        let resp = self
            .inner
            .add_contact_way(&token, &req)
            .await
            .map_err(|e| KefuClientError::Api(e.to_string()))?;
        Ok(resp.url)
    }

    // --- Messaging (direct HTTP to avoid wxkefu-rs TLS issues) ---

    pub async fn sync_msg(
        &self,
        cursor: &str,
        event_token: Option<&str>,
        limit: u32,
    ) -> Result<SyncMsgResult, KefuClientError> {
        let access_token = self.access_token().await?;
        let url =
            format!("https://qyapi.weixin.qq.com/cgi-bin/kf/sync_msg?access_token={access_token}");
        let mut body = serde_json::json!({ "limit": limit, "voice_format": 0 });
        if !cursor.is_empty() {
            body["cursor"] = serde_json::Value::String(cursor.to_string());
        }
        if let Some(t) = event_token {
            body["token"] = serde_json::Value::String(t.to_string());
        }
        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| KefuClientError::Api(format!("sync_msg http: {e}")))?;
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| KefuClientError::Api(format!("sync_msg body: {e}")))?;
        let parsed: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| KefuClientError::Api(format!("sync_msg json: {e}")))?;
        let errcode = parsed.get("errcode").and_then(|v| v.as_i64()).unwrap_or(-1);
        if errcode != 0 {
            let errmsg = parsed
                .get("errmsg")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            return Err(KefuClientError::Api(format!(
                "sync_msg errcode={errcode}: {errmsg}"
            )));
        }
        Ok(SyncMsgResult {
            next_cursor: parsed
                .get("next_cursor")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            has_more: parsed.get("has_more").and_then(|v| v.as_u64()).unwrap_or(0) != 0,
            msg_list: parsed
                .get("msg_list")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default(),
        })
    }

    pub async fn send_text(
        &self,
        to: &str,
        open_kfid: &str,
        text: &str,
    ) -> Result<(), KefuClientError> {
        let access_token = self.access_token().await?;
        let url =
            format!("https://qyapi.weixin.qq.com/cgi-bin/kf/send_msg?access_token={access_token}");
        let body = serde_json::json!({
            "touser": to,
            "open_kfid": open_kfid,
            "msgid": uuid::Uuid::new_v4().to_string(),
            "msgtype": "text",
            "text": { "content": text },
        });
        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| KefuClientError::Api(format!("send_msg http: {e}")))?;
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| KefuClientError::Api(format!("send_msg body: {e}")))?;
        let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
        let errcode = parsed.get("errcode").and_then(|v| v.as_i64()).unwrap_or(-1);
        if errcode != 0 {
            return Err(KefuClientError::Api(format!(
                "send_msg errcode={}: {}",
                errcode,
                parsed
                    .get("errmsg")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
            )));
        }
        Ok(())
    }
}

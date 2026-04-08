//! Wire types for the WeChat iLink Bot API.
//!
//! This is a Rust port of the TypeScript reference implementation found in
//! `~/.openclaw/extensions/openclaw-weixin/src/api/types.ts`. Field names
//! match the JSON schema exactly so serde can serialize/deserialize without
//! `#[serde(rename = ...)]` boilerplate (we use `snake_case` everywhere).
//!
//! All `bytes` fields in the protocol are exchanged as base64 strings in JSON,
//! so we model them as `String` here and only decode where the bytes matter.

use serde::{Deserialize, Serialize};

/// Common request metadata attached to every CGI request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BaseInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_version: Option<String>,
}

// ── Enum tags ────────────────────────────────────────────────────────

/// `proto: UploadMediaType` — kept as `i32` constants matching the wire.
pub mod upload_media_type {
    pub const IMAGE: i32 = 1;
    pub const VIDEO: i32 = 2;
    pub const FILE: i32 = 3;
    pub const VOICE: i32 = 4;
}

/// `proto: MessageType`. Inbound messages are usually `USER`,
/// bot replies are `BOT`.
pub mod message_type {
    pub const NONE: i32 = 0;
    pub const USER: i32 = 1;
    pub const BOT: i32 = 2;
}

/// `proto: MessageItemType` — what's inside a `MessageItem`.
pub mod message_item_type {
    pub const NONE: i32 = 0;
    pub const TEXT: i32 = 1;
    pub const IMAGE: i32 = 2;
    pub const VOICE: i32 = 3;
    pub const FILE: i32 = 4;
    pub const VIDEO: i32 = 5;
}

/// `proto: MessageState`. We send `FINISH` for completed bot replies.
pub mod message_state {
    pub const NEW: i32 = 0;
    pub const GENERATING: i32 = 1;
    pub const FINISH: i32 = 2;
}

/// Typing indicator status.
pub mod typing_status {
    pub const TYPING: i32 = 1;
    pub const CANCEL: i32 = 2;
}

// ── Item types ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TextItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// CDN media reference. `aes_key` is base64-encoded bytes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CdnMedia {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypt_query_param: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aes_key: Option<String>,
    /// 0 = encrypt fileid only, 1 = encrypt thumb/middle/large fileids together
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypt_type: Option<i32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImageItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<CdnMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_media: Option<CdnMedia>,
    /// Raw AES-128 key as hex string (16 bytes); used for inbound decryption.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aeskey: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mid_size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_height: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_width: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hd_size: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VoiceItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<CdnMedia>,
    /// Encoding: 1=pcm 2=adpcm 3=feature 4=speex 5=amr 6=silk 7=mp3 8=ogg-speex
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encode_type: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bits_per_sample: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<i32>,
    /// Voice length in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playtime: Option<i64>,
    /// Server-side speech-to-text result, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<CdnMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub md5: Option<String>,
    /// Wire type is string in this protocol (not int).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub len: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VideoItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<CdnMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play_length: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_md5: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_media: Option<CdnMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_height: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_width: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RefMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_item: Option<Box<MessageItem>>,
    /// Quoted message preview/title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageItem {
    /// See `message_item_type` constants (1=text, 2=image, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_completed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_msg: Option<RefMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_item: Option<TextItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_item: Option<ImageItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_item: Option<VoiceItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_item: Option<FileItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_item: Option<VideoItem>,
}

/// `proto: WeixinMessage` — the unified message envelope used both inbound
/// (server → us) and outbound (us → server).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WeixinMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete_time_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    /// See `message_type` constants (1=user, 2=bot).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_type: Option<i32>,
    /// See `message_state` constants (0=new, 1=generating, 2=finish).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_state: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_list: Option<Vec<MessageItem>>,
    /// Critical: must echo this back when replying so the server can
    /// thread the conversation correctly. Never reuse a stale value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_token: Option<String>,
}

// ── Request / response envelopes ─────────────────────────────────────

/// `getUpdates` request body.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GetUpdatesReq {
    /// Cursor blob from the previous response. Send `""` on first request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub get_updates_buf: Option<String>,
    pub base_info: BaseInfo,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GetUpdatesResp {
    #[serde(default)]
    pub ret: Option<i32>,
    /// Server error code (e.g. -14 = session timeout).
    #[serde(default)]
    pub errcode: Option<i32>,
    #[serde(default)]
    pub errmsg: Option<String>,
    #[serde(default)]
    pub msgs: Option<Vec<WeixinMessage>>,
    /// Updated cursor; cache and send back next time.
    #[serde(default)]
    pub get_updates_buf: Option<String>,
    /// Server-recommended next long-poll timeout (ms).
    #[serde(default)]
    pub longpolling_timeout_ms: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SendMessageReq {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg: Option<WeixinMessage>,
    pub base_info: BaseInfo,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SendTypingReq {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ilink_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typing_ticket: Option<String>,
    /// 1 = typing (default), 2 = cancel typing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<i32>,
    pub base_info: BaseInfo,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GetConfigReq {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ilink_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_token: Option<String>,
    pub base_info: BaseInfo,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GetConfigResp {
    #[serde(default)]
    pub ret: Option<i32>,
    #[serde(default)]
    pub errmsg: Option<String>,
    /// Base64-encoded typing ticket. Required by `sendTyping`.
    #[serde(default)]
    pub typing_ticket: Option<String>,
}

// ── QR-code login response shapes ────────────────────────────────────

/// Response to `GET ilink/bot/get_bot_qrcode?bot_type=3`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QrCodeResponse {
    /// Server-side QR identifier (echoed back to the status poll endpoint).
    #[serde(default)]
    pub qrcode: String,
    /// User-facing URL the bot must scan with the WeChat ClawBot plugin.
    /// Open this in a browser to render the QR code.
    #[serde(default)]
    pub qrcode_img_content: String,
}

/// Status returned by `GET ilink/bot/get_qrcode_status?qrcode=...` (long-poll).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QrStatusResponse {
    /// "wait" | "scaned" | "confirmed" | "expired"
    #[serde(default)]
    pub status: Option<String>,
    /// Bearer token, present when status == "confirmed".
    #[serde(default)]
    pub bot_token: Option<String>,
    /// Bot account identifier (e.g. "abc123@im.bot").
    #[serde(default)]
    pub ilink_bot_id: Option<String>,
    /// Server suggests we use this baseUrl for subsequent calls (may differ from default).
    #[serde(default)]
    pub baseurl: Option<String>,
    /// Identifier of the WeChat user who scanned the QR.
    #[serde(default)]
    pub ilink_user_id: Option<String>,
}

// ── On-disk persisted account data ───────────────────────────────────

/// Mirrors the JSON OpenClaw writes to
/// `~/.openclaw/openclaw-weixin/accounts/{accountId}.json`. We persist
/// our own copy to `~/.warwolf/wechat/accounts/{accountId}.json` so we
/// don't conflict with OpenClaw's instance.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WeixinAccountData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// ISO 8601 timestamp of last save.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved_at: Option<String>,
    /// API base URL (defaults to `DEFAULT_BASE_URL`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// WeChat user id of the person who scanned the QR.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

// ── Constants ────────────────────────────────────────────────────────

pub const DEFAULT_BASE_URL: &str = "https://ilinkai.weixin.qq.com";
pub const CDN_BASE_URL: &str = "https://novac2c.cdn.weixin.qq.com/c2c";

/// Default `bot_type` parameter for QR-code generation. The reference
/// implementation always uses "3" — bots intended for personal WeChat use.
pub const DEFAULT_ILINK_BOT_TYPE: &str = "3";

/// Channel version reported in `base_info.channel_version`. We mimic the
/// official client's value rather than declaring our own — the server may
/// use this for compat-checking and we want to look like a known client.
pub const CHANNEL_VERSION: &str = "1.0.3";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_base_info_serializes_with_channel_version() {
        let base = BaseInfo {
            channel_version: Some(CHANNEL_VERSION.to_string()),
        };
        let json = serde_json::to_string(&base).expect("serialize");
        assert!(json.contains("\"channel_version\":\"1.0.3\""));
    }

    #[test]
    fn empty_base_info_omits_channel_version() {
        let base = BaseInfo::default();
        let json = serde_json::to_string(&base).expect("serialize");
        assert_eq!(json, "{}");
    }

    #[test]
    fn weixin_message_round_trip_text() {
        let msg = WeixinMessage {
            from_user_id: Some("user@im.wechat".to_string()),
            to_user_id: Some("bot@im.bot".to_string()),
            message_type: Some(message_type::USER),
            context_token: Some("AARzJWAF...".to_string()),
            item_list: Some(vec![MessageItem {
                r#type: Some(message_item_type::TEXT),
                text_item: Some(TextItem {
                    text: Some("hello".to_string()),
                }),
                ..Default::default()
            }]),
            ..Default::default()
        };
        let json = serde_json::to_string(&msg).expect("serialize");
        let parsed: WeixinMessage = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.from_user_id.as_deref(), Some("user@im.wechat"));
        assert_eq!(parsed.context_token.as_deref(), Some("AARzJWAF..."));
        let blocks = parsed.item_list.as_ref().expect("item_list");
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            blocks[0].text_item.as_ref().and_then(|t| t.text.as_deref()),
            Some("hello")
        );
    }

    #[test]
    fn get_updates_resp_parses_minimal_payload() {
        let json = r#"{"ret":0,"msgs":[],"get_updates_buf":"abc"}"#;
        let resp: GetUpdatesResp = serde_json::from_str(json).expect("deserialize");
        assert_eq!(resp.ret, Some(0));
        assert_eq!(resp.get_updates_buf.as_deref(), Some("abc"));
        assert!(resp.msgs.is_some());
    }

    #[test]
    fn get_updates_resp_parses_error_payload() {
        let json = r#"{"errcode":-14,"errmsg":"session expired"}"#;
        let resp: GetUpdatesResp = serde_json::from_str(json).expect("deserialize");
        assert_eq!(resp.errcode, Some(-14));
        assert_eq!(resp.errmsg.as_deref(), Some("session expired"));
        assert!(resp.msgs.is_none());
    }

    #[test]
    fn qr_status_response_parses_confirmed() {
        let json = r#"{
            "status": "confirmed",
            "bot_token": "abc@im.bot:tok",
            "ilink_bot_id": "abc@im.bot",
            "baseurl": "https://ilinkai.weixin.qq.com",
            "ilink_user_id": "user@im.wechat"
        }"#;
        let resp: QrStatusResponse = serde_json::from_str(json).expect("deserialize");
        assert_eq!(resp.status.as_deref(), Some("confirmed"));
        assert!(resp.bot_token.is_some());
        assert!(resp.ilink_bot_id.is_some());
    }
}

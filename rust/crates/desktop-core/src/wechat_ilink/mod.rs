//! WeChat iLink Bot API integration.
//!
//! Implements the protocol used by the WeChat ClawBot plugin (`@tencent-weixin/openclaw-weixin`)
//! so this project can directly receive WeChat private messages and reply to
//! them via the official iLink relay (`https://ilinkai.weixin.qq.com`),
//! without depending on OpenClaw.
//!
//! Layout:
//! - [`types`]   — wire types ported from `openclaw-weixin/src/api/types.ts`
//! - [`client`]  — authenticated HTTP client (`get_updates`, `send_message`, ...)
//! - [`login`]   — QR-code login flow (anonymous endpoints)
//! - [`account`] — token persistence under `~/.warwolf/wechat/`
//!
//! See `docs/wechat-ilink.md` for the high-level architecture and the
//! `tools/wechat-bridge` directory for the smoke-test harness.

pub mod account;
pub mod client;
pub mod login;
pub mod types;

pub use account::{
    clear_account, list_account_ids, load_account, load_context_tokens, load_sync_buf,
    save_account, save_sync_buf, upsert_context_token, AccountError,
};
pub use client::{IlinkClient, IlinkError, SESSION_EXPIRED_ERRCODE};
pub use login::{LoginConfirmation, LoginError, LoginStatus, QrLoginSession};
pub use types::{
    BaseInfo, GetUpdatesResp, MessageItem, QrCodeResponse, QrStatusResponse, TextItem,
    WeixinAccountData, WeixinMessage, CHANNEL_VERSION, DEFAULT_BASE_URL,
};

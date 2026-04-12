---
title: ClaudeWiki 微信客服官方 API 接入设计
doc_type: spec
status: active
owner: desktop-shell
last_verified: 2026-04-10
related:
  - docs/desktop-shell/specs/2026-04-10-claudewiki-wechat-customer-service-ingress-design.md
---

# ClaudeWiki 微信客服官方 API 接入设计

## 目标

在 ClaudeWiki 中创建名为 **"ClaudeWiki助手"** 的微信客服账号，通过官方微信客服 API 实现：

1. 用户在微信 `转发给朋友 → 客服消息 → ClaudeWiki助手` 直接对话
2. 支持文本、图片、语音、视频、文件等全消息类型
3. 消息自动进入 ClaudeWiki turn pipeline + wiki ingest
4. 无需依赖 QClaw 或任何第三方私有协议

## 前置条件

| 项目 | 来源 | 说明 |
|------|------|------|
| `corpid` | 企业微信管理后台 → 我的企业 → 企业ID | 当前值: ClawClub 企业 |
| `secret` | kf.weixin.qq.com → 开发配置 → API Secret | 微信客服专用 secret |
| `token` | kf.weixin.qq.com → 开发配置 → 随机获取 | 回调签名验证 |
| `encoding_aes_key` | kf.weixin.qq.com → 开发配置 → 随机获取 | 回调消息解密 |

## 架构总览

```
微信用户
  │
  │ (1) 扫码/点击客服链接
  ↓
WeChat 客服平台 (qyapi.weixin.qq.com)
  │
  │ (2) 回调通知 POST → ClaudeWiki 回调端点
  │     (仅通知 "有新消息"，不含消息内容)
  ↓
ClaudeWiki desktop-server
  │
  │ (3) POST /cgi-bin/kf/sync_msg → 拉取消息内容
  │     (cursor 增量拉取，支持文本/图片/语音/视频/文件)
  ↓
  │ (4) 消息分发
  ├── DesktopAgentHandler → turn pipeline → 生成回复
  └── wiki_store → raw entry + inbox
  │
  │ (5) POST /cgi-bin/kf/send_msg → 发送回复
  ↓
微信用户 (收到 ClaudeWiki助手 回复)
```

## 官方 API 端点清单

### 认证

| 端点 | 方法 | 说明 |
|------|------|------|
| `/cgi-bin/gettoken?corpid=ID&corpsecret=SECRET` | GET | 获取 access_token (2h 有效) |

### 客服账号管理

| 端点 | 方法 | 说明 |
|------|------|------|
| `/cgi-bin/kf/account/add` | POST | 创建客服账号 → 返回 `open_kfid` |
| `/cgi-bin/kf/account/list` | POST | 列出所有客服账号 |
| `/cgi-bin/kf/account/update` | POST | 更新名称/头像 |
| `/cgi-bin/kf/account/del` | POST | 删除客服账号 |
| `/cgi-bin/kf/add_contact_way` | POST | 生成客服链接 URL (转 QR 码供用户扫) |

### 消息收发

| 端点 | 方法 | 说明 |
|------|------|------|
| `/cgi-bin/kf/sync_msg` | POST | 增量拉取消息 (cursor 分页) |
| `/cgi-bin/kf/send_msg` | POST | 发送回复消息 |
| `/cgi-bin/kf/send_msg_on_event` | POST | 发送欢迎语 |

### 消息类型支持

| 类型 | 接收 | 发送 | 说明 |
|------|------|------|------|
| text | Y | Y | 纯文本，最大 2048 字节 |
| image | Y | Y | 图片，需 media_id |
| voice | Y | Y | 语音，需 media_id |
| video | Y | Y | 视频，需 media_id |
| file | Y | Y | 文件，需 media_id |
| location | Y | N | 位置消息 |
| link | Y | Y | 图文链接 |
| miniprogram | N | Y | 小程序卡片 |

## 后端设计 (Rust)

### 模块结构

```
rust/crates/desktop-core/src/wechat_kefu/
├── mod.rs              # 模块声明
├── types.rs            # 官方 API 请求/响应类型
├── client.rs           # HTTP 客户端 (access_token 管理 + API 调用)
├── account.rs          # 磁盘持久化 (~/.warwolf/wechat-kefu/)
├── crypto.rs           # 回调签名验证 + AES 解密
├── callback.rs         # 回调 HTTP handler (axum)
├── monitor.rs          # sync_msg 轮询循环
├── desktop_handler.rs  # 消息 → turn pipeline 桥接
└── media.rs            # 媒体上传/下载 (图片/语音/视频/文件)
```

### 核心类型 (`types.rs`)

```rust
/// 持久化的客服配置
pub struct KefuConfig {
    pub corpid: String,
    pub secret: String,           // 微信客服专用 secret
    pub token: String,            // 回调验证 token
    pub encoding_aes_key: String, // 回调消息 AES key
    pub open_kfid: Option<String>,// 创建后获得
    pub contact_url: Option<String>, // 客服链接 URL
    pub saved_at: Option<String>,
}

/// sync_msg 请求
pub struct SyncMsgRequest {
    pub cursor: Option<String>,
    pub token: Option<String>,    // 回调事件中的 token (非 access_token)
    pub limit: Option<u32>,       // 最大 1000
    pub voice_format: Option<u32>,// 0=amr, 1=silk
    pub open_kfid: Option<String>,
}

/// sync_msg 响应
pub struct SyncMsgResponse {
    pub errcode: i32,
    pub errmsg: String,
    pub next_cursor: Option<String>,
    pub has_more: Option<bool>,
    pub msg_list: Option<Vec<KefuMessage>>,
}

/// 客服消息 (接收)
pub struct KefuMessage {
    pub msgid: String,
    pub open_kfid: String,
    pub external_userid: String,
    pub send_time: u64,
    pub origin: u32,              // 3=微信客户, 4=系统, 5=接待人员
    pub msgtype: String,          // "text", "image", "voice", etc.
    // 各类型消息体
    pub text: Option<TextContent>,
    pub image: Option<MediaContent>,
    pub voice: Option<VoiceContent>,
    pub video: Option<MediaContent>,
    pub file: Option<FileContent>,
    pub location: Option<LocationContent>,
    pub link: Option<LinkContent>,
    pub event: Option<EventContent>,
}

/// send_msg 请求
pub struct SendMsgRequest {
    pub touser: String,           // external_userid
    pub open_kfid: String,
    pub msgid: Option<String>,    // 去重 ID
    pub msgtype: String,
    // 各类型消息体 (同上)
}
```

### HTTP 客户端 (`client.rs`)

```rust
pub struct KefuClient {
    http: reqwest::Client,
    corpid: String,
    secret: String,
    // 缓存 access_token + 过期时间
    token_cache: Arc<RwLock<Option<(String, Instant)>>>,
}

impl KefuClient {
    /// 自动刷新 access_token (过期前 5 分钟刷新)
    pub async fn access_token(&self) -> Result<String, KefuError>;

    // 客服账号
    pub async fn create_account(&self, name: &str) -> Result<String, KefuError>; // → open_kfid
    pub async fn list_accounts(&self) -> Result<Vec<KefuAccount>, KefuError>;
    pub async fn delete_account(&self, open_kfid: &str) -> Result<(), KefuError>;
    pub async fn get_contact_url(&self, open_kfid: &str) -> Result<String, KefuError>;

    // 消息收发
    pub async fn sync_msg(&self, cursor: &str, limit: u32) -> Result<SyncMsgResponse, KefuError>;
    pub async fn send_text(&self, to: &str, open_kfid: &str, text: &str) -> Result<(), KefuError>;
    pub async fn send_image(&self, to: &str, open_kfid: &str, media_id: &str) -> Result<(), KefuError>;

    // 媒体
    pub async fn upload_media(&self, media_type: &str, data: &[u8], filename: &str) -> Result<String, KefuError>;
    pub async fn download_media(&self, media_id: &str) -> Result<Vec<u8>, KefuError>;
}
```

### 回调处理 (`callback.rs`)

```rust
/// 回调验证 (GET) — kf.weixin.qq.com 配置页面验证通过后只需一次
/// GET /api/desktop/wechat-kefu/callback?msg_signature=..&timestamp=..&nonce=..&echostr=..
pub async fn callback_verify_handler(
    State(state): State<AppState>,
    Query(params): Query<CallbackVerifyParams>,
) -> Result<String, StatusCode>;

/// 回调通知 (POST) — 收到新消息时微信推送
/// POST /api/desktop/wechat-kefu/callback
/// Body: XML (加密的事件通知)
pub async fn callback_event_handler(
    State(state): State<AppState>,
    Query(params): Query<CallbackEventParams>,
    body: String,
) -> Result<String, StatusCode>;
```

**回调事件类型：**
- `enter_session` — 用户进入客服会话
- `msg_receive` — 收到新消息 (触发 `sync_msg` 拉取)
- `servicer_status_change` — 接待人员状态变更

### 消息轮询 (`monitor.rs`)

```rust
/// 双模式消息获取:
/// - 有回调: 收到 msg_receive 事件后立即 sync_msg
/// - 无回调 (降级): 每 3 秒轮询 sync_msg
pub async fn run_kefu_monitor(config: KefuMonitorConfig, status_tx: watch::Sender<MonitorStatus>);
```

**轮询逻辑 (与 iLink monitor 对称):**
1. 启动时从磁盘加载 cursor
2. 调用 `sync_msg(cursor, limit=100)` 拉取消息
3. 保存新 cursor 到磁盘 (crash-safe)
4. 对每条消息调用 `handler.on_message()`
5. 失败退避: 2s → 30s (与 iLink 相同策略)

### DesktopState 集成

```rust
impl DesktopState {
    // 配置管理
    pub async fn save_kefu_config(&self, config: KefuConfig) -> Result<(), String>;
    pub async fn load_kefu_config(&self) -> Result<Option<KefuConfig>, String>;

    // 客服账号
    pub async fn create_kefu_account(&self, name: &str) -> Result<String, String>;
    pub async fn get_kefu_contact_url(&self) -> Result<String, String>;

    // Monitor
    pub async fn spawn_kefu_monitor(&self) -> Result<(), String>;
    pub async fn stop_kefu_monitor(&self);
    pub async fn kefu_monitor_status(&self) -> Option<MonitorStatus>;
}
```

### 磁盘持久化

```
~/.warwolf/wechat-kefu/
├── config.json              # KefuConfig (corpid, secret, token, aes_key, open_kfid)
├── cursor.json              # { "cursor": "..." }
└── session-map.json         # { "external_userid": "desktop_session_id" }
```

### 回调端点暴露方案

由于 ClaudeWiki 是桌面应用，没有公网 IP，回调 URL 需要一个公网入口。三种方案：

| 方案 | 复杂度 | 说明 |
|------|--------|------|
| **A: 纯轮询 (无回调)** | 最低 | 不配置回调 URL，每 3s 轮询 `sync_msg`。延迟 3s，但零公网依赖 |
| **B: Cloudflare Tunnel** | 低 | `cloudflared tunnel` 将本机端口暴露到公网，免费 |
| **C: 中继服务器** | 中 | 轻量 VPS 接收回调，通过 WebSocket 转发到桌面端 |

**重要发现：** `sync_msg` 的 `token` 参数来自回调事件的解密载荷（有效期 10 分钟）。不带 `token` 调用 `sync_msg` 会被**严格限流**，纯轮询模式不可靠。

**建议：采用方案 B (Cloudflare Tunnel) 作为默认方案。**

`cloudflared tunnel` 可将本机端口免费暴露到公网，一行命令即可：
```bash
cloudflared tunnel --url http://localhost:19280
# → 得到 https://xxxx.trycloudflare.com
# 将此 URL 填入 kf.weixin.qq.com 回调配置
```

消息获取流程变为（官方推荐的两阶段拉取）：
1. 微信 POST 回调 → Cloudflare Tunnel → ClaudeWiki `/api/desktop/wechat-kefu/callback`
2. ClaudeWiki 从回调事件中提取 `token`
3. 立即调用 `sync_msg(cursor, token)` 拉取完整消息（无限流限制）

方案 A (纯轮询) 仅作为调试/降级模式保留。

**参考实现：** 已有开源 Rust 库 [wxkefu-rs](https://github.com/ooiai/wxkefu-rs) 实现了完整的回调验证 + AES 解密 + token 缓存，可作为实现参考。

## 前端设计 (React)

### API 客户端 (`client.ts`)

```typescript
// 配置管理
export async function saveKefuConfig(config: KefuConfigRequest): Promise<{ ok: boolean }>;
export async function loadKefuConfig(): Promise<KefuConfigResponse>;

// 客服账号
export async function createKefuAccount(name: string): Promise<{ open_kfid: string }>;
export async function getKefuContactUrl(): Promise<{ url: string }>;
export async function getKefuStatus(): Promise<KefuStatusResponse>;

// Monitor
export async function startKefuMonitor(): Promise<{ ok: boolean }>;
export async function stopKefuMonitor(): Promise<{ ok: boolean }>;
```

### HTTP 路由

```
POST /api/desktop/wechat-kefu/config          # 保存配置 (corpid, secret, token, aes_key)
GET  /api/desktop/wechat-kefu/config          # 读取配置 (secret 脱敏)
POST /api/desktop/wechat-kefu/account/create  # 创建 "ClaudeWiki助手" 客服账号
GET  /api/desktop/wechat-kefu/contact-url     # 获取客服链接 (转 QR 码)
GET  /api/desktop/wechat-kefu/status          # Monitor 状态
POST /api/desktop/wechat-kefu/monitor/start   # 启动消息轮询
POST /api/desktop/wechat-kefu/monitor/stop    # 停止消息轮询
GET  /api/desktop/wechat-kefu/callback        # 回调验证 (GET)
POST /api/desktop/wechat-kefu/callback        # 回调事件 (POST)
```

### UI 设计 (`WeChatBridgePage.tsx` 新增区块)

```
┌─────────────────────────────────────────────────────────┐
│  ClaudeWiki 助手 · 微信客服                Channel B    │
│  ─────────────────────────────────────────────────────  │
│                                                         │
│  ┌─── 配置 ─────────────────────────────────────────┐  │
│  │ corpid:   [ww1234567890abcd        ]              │  │
│  │ secret:   [••••••••••••••••••      ] [显示/隐藏]  │  │
│  │ token:    [随机生成的 token         ] [随机获取]   │  │
│  │ AES key:  [随机生成的 AES key       ] [随机获取]   │  │
│  │                                                    │  │
│  │           [保存配置]                               │  │
│  └────────────────────────────────────────────────────┘  │
│                                                         │
│  ┌─── 客服账号 ──────────────────────────────────────┐  │
│  │ 状态: ● 已创建                                     │  │
│  │ 名称: ClaudeWiki助手                               │  │
│  │ open_kfid: wkXXXXXXXXXX                            │  │
│  │                                                    │  │
│  │ [创建客服账号]  (如果未创建)                         │  │
│  └────────────────────────────────────────────────────┘  │
│                                                         │
│  ┌─── 客服链接二维码 ────────────────────────────────┐  │
│  │                                                    │  │
│  │     ┌──────────────┐                               │  │
│  │     │   QR CODE    │  扫码接入 ClaudeWiki助手      │  │
│  │     │   (192x192)  │  扫码后可在微信中直接对话      │  │
│  │     │              │                               │  │
│  │     └──────────────┘  用户在微信 "转发给朋友" 中    │  │
│  │                       可看到 "ClaudeWiki助手"       │  │
│  │                                                    │  │
│  │     [复制链接]  [刷新二维码]                         │  │
│  └────────────────────────────────────────────────────┘  │
│                                                         │
│  ┌─── Monitor 状态 ──────────────────────────────────┐  │
│  │ ● Running  (轮询模式, 每3s)                        │  │
│  │ 最后拉取: 2026-04-10 17:42:00                      │  │
│  │ 最后消息: 2026-04-10 17:41:55                      │  │
│  │ 连续失败: 0                                        │  │
│  │                                                    │  │
│  │ [停止]  [重启]                                     │  │
│  └────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

### UI 流程

1. **首次配置**: 用户从 kf.weixin.qq.com 复制 corpid, secret, token, AES key → 粘贴到配置表单 → 保存
2. **创建客服**: 点击 "创建客服账号" → 后端调用 `kf/account/add` → 获得 `open_kfid`
3. **生成 QR**: 后端调用 `kf/add_contact_way` → 获得客服链接 URL → 前端渲染为 QR 码
4. **启动监听**: 自动启动 `sync_msg` 轮询循环
5. **用户扫码**: 微信用户扫 QR → 打开客服对话 → 消息到达 ClaudeWiki

## 实施计划

### Task 1: 基础类型 + HTTP 客户端 + access_token 管理
- `types.rs`, `client.rs`, `account.rs`
- access_token 自动刷新缓存
- `cargo check`

### Task 2: sync_msg 轮询 Monitor
- `monitor.rs`, `desktop_handler.rs`
- 复用 iLink handler 的 run_turn + markdown_split 逻辑
- `cargo check`

### Task 3: 回调签名验证 + AES 解密
- `crypto.rs`, `callback.rs`
- SHA1 签名验证 + AES-256-CBC 解密
- 单元测试
- `cargo check`

### Task 4: DesktopState 集成 + HTTP 路由
- `lib.rs` 新增 kefu 方法
- `desktop-server/src/lib.rs` 新增路由
- `main.rs` 启动时自动启动 monitor
- `cargo check --workspace`

### Task 5: 前端 UI
- `client.ts` API 函数
- `WeChatBridgePage.tsx` 配置表单 + QR 码 + Monitor 状态
- `tsc --noEmit`

### Task 6: 端到端测试
- 在 kf.weixin.qq.com 配置回调 URL (或跳过, 用纯轮询)
- 创建 "ClaudeWiki助手" 客服账号
- 生成客服链接 QR
- 微信扫码 → 发消息 → ClaudeWiki 收到 → 回复

## 与 QClaw 方案对比

| 维度 | QClaw AGP (已清除) | 官方 API (本方案) |
|------|-------------------|------------------|
| 品牌 | 固定 "QClaw客服" | 自定义 "ClaudeWiki助手" |
| 消息类型 | 仅文本 | 文本+图片+语音+视频+文件+链接 |
| 稳定性 | 依赖腾讯内部协议 | 官方 API，有 SLA |
| 服务器 | 无需 (WebSocket) | 方案A无需 (轮询) |
| 客户限制 | 无 | 100人 (未认证) |
| 协议 | AGP WebSocket (私有) | HTTP JSON (官方) |

## 约束与风险

1. **100 客户限制**: 企业未认证，累计仅可接待 100 位客户。初期够用，认证后解除。
2. **轮询延迟**: 纯轮询模式有 ~3s 延迟。用户体验可接受，后续可升级回调模式。
3. **access_token 刷新**: 2h 过期，需后台自动刷新。不可返回给前端。
4. **回调 URL**: 桌面应用无公网 IP。方案 A 纯轮询规避此限制。

## 决策

请团队评审以下要点：

1. 客服账号名称: "ClaudeWiki助手" 是否合适？
2. 消息获取方式: 方案 B (Cloudflare Tunnel + 回调) 作为默认，方案 A (纯轮询) 作为降级？
3. 100 客户限制是否可接受？是否需要立即企业认证？
4. 实施优先级: 先做文本消息，富媒体 (图片/文件) 作为后续里程碑？
5. 是否直接引用 [wxkefu-rs](https://github.com/ooiai/wxkefu-rs) 作为依赖，还是自行实现？

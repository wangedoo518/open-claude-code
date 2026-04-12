---
title: ClaudeWiki 微信客服自托管中继方案（wrangler CLI 部署）
doc_type: spec
status: active
owner: desktop-shell
last_verified: 2026-04-11
---

# ClaudeWiki 微信客服自托管中继方案

## 产品目标

每个 ClaudeWiki 用户的微信客服中继完全运行在**用户自己的 Cloudflare 账号**中，通过 wrangler CLI 自动部署。

## 部署工具选型：wrangler CLI

| 维度 | REST API | wrangler CLI ✓ |
|------|----------|----------------|
| Durable Object 部署 | 需手动处理 migration | `wrangler deploy` 自动处理 |
| Secret 管理 | 逐个 PUT multipart | `wrangler secret bulk` 一次设置 |
| 错误处理 | 原始 HTTP JSON | 友好错误信息 + 重试 |
| DO class 迁移 | 不支持 | 自动迁移 |
| 可靠性 | 边界情况多 | 官方工具, 全场景覆盖 |

## 架构图

```
用户的 Cloudflare 账号
┌──────────────────────────────────────────────────────────────┐
│  Worker: claudewiki-kefu-relay                                │
│  URL: claudewiki-kefu-relay.{subdomain}.workers.dev           │
│                                                               │
│  ┌──────────────────────────────────────────────────────────┐│
│  │ Durable Object: RelayDO                                  ││
│  │ - GET  /callback → echostr 验证 (SHA1 + AES 解密)       ││
│  │ - POST /callback → 验签 → 通过 WebSocket 推送给桌面端    ││
│  │ - GET  /ws       → WebSocket 升级 (桌面端连接)            ││
│  │ - 离线消息缓冲 (≤100条, 上线后推送)                       ││
│  └──────────────────────────────────────────────────────────┘│
│                                                               │
│  Secrets: AUTH_TOKEN, CALLBACK_TOKEN, ENCODING_AES_KEY, CORPID│
└───────────────┬──────────────────────┬───────────────────────┘
                │                      │
        HTTP POST 回调          出站 WebSocket
                │                      │
┌───────────────┴────┐   ┌─────────────┴─────────────────────────┐
│ 微信客服平台        │   │ ClaudeWiki 桌面端                      │
│                    │   │                                        │
│ 回调 URL:           │   │  部署流程 (一次性):                     │
│ ...workers.dev     │   │  Rust → std::process::Command          │
│ /callback          │   │  → npx wrangler deploy                 │
│                    │   │  → npx wrangler secret bulk             │
│                    │   │                                        │
│                    │   │  运行时:                                │
│                    │   │  relay_client.rs (WebSocket)            │
│                    │   │  → callback 解密 → sync_msg → 回复     │
└────────────────────┘   └────────────────────────────────────────┘
```

## 用户操作流程 (4 步)

### Step 1: 准备环境 (一次性)

```
ClaudeWiki 引导页:

  ┌─ 环境检查 ────────────────────────────────────┐
  │                                                │
  │  Node.js:  ✓ v22.1.0                          │
  │  npx:      ✓ 可用                              │
  │  wrangler: ✓ 可用 (npx wrangler --version)    │
  │                                                │
  │  Cloudflare 账号:                               │
  │  [注册免费账号] (https://dash.cloudflare.com)   │
  │                                                │
  │  API Token:                                    │
  │  [创建 Token 教程] → 选择 "Edit Workers" 模板   │
  │                                                │
  │  Token: [________________________________]     │
  │                                                │
  │  [验证 Token]                                  │
  └────────────────────────────────────────────────┘
```

ClaudeWiki 自动检测：
```rust
// 环境检测
fn check_prerequisites() -> PrereqStatus {
    let node = Command::new("node").arg("--version").output();
    let npx = Command::new("npx").arg("--version").output();
    PrereqStatus { node_ok, npx_ok }
}
```

### Step 2: 一键部署 (自动, ~60 秒)

```
  ┌─ 部署中继 ─────────────────────────────────────┐
  │                                                  │
  │  企业 ID (corpid): [wwdd403e3dd8bc27ee    ]     │
  │  客服 Secret:      [••••••••••••••••••    ]     │
  │                                                  │
  │  [一键部署]                                      │
  │                                                  │
  │  ⠋ 生成项目模板...                               │
  │  ✓ 项目生成完成                                   │
  │  ⠋ wrangler deploy...                            │
  │  ✓ Worker 部署成功                                │
  │  ⠋ 设置密钥...                                   │
  │  ✓ 4 个 Secret 已设置                             │
  │  ✓ 健康检查通过                                   │
  │                                                  │
  │  部署成功!                                       │
  └──────────────────────────────────────────────────┘
```

### Step 3: 配置微信回调 (手动, 2 分钟)

```
  ┌─ 微信客服回调配置 ──────────────────────────────┐
  │                                                  │
  │  请在 kf.weixin.qq.com "开发配置" 中填入:         │
  │                                                  │
  │  回调 URL:       https://claudewiki-kefu-relay   │
  │                  .abc123.workers.dev/callback     │
  │                                        [复制]    │
  │                                                  │
  │  Token:          RTFJynmyrgHKc          [复制]   │
  │                                                  │
  │  EncodingAESKey: k22pqLXRowo8Y...       [复制]   │
  │                                                  │
  │  [打开 kf.weixin.qq.com]                         │
  │                                                  │
  │  填写完成后点击 "完成" 验证通过后:                  │
  │  [回调已验证通过 ✓]                               │
  └──────────────────────────────────────────────────┘
```

### Step 4: 创建客服 + 扫码

```
  ┌─ 客服账号 ──────────────────────────────────────┐
  │                                                  │
  │  [创建 "ClaudeWiki助手" 客服账号]                 │
  │                                                  │
  │  ✓ 已创建: wkbeQYRgAAj0EhLjS79mmsCLF0msXXdg     │
  │                                                  │
  │  ┌──────────┐  扫码开始对话                      │
  │  │  QR CODE │  微信 "转发给朋友 → 客服消息"       │
  │  │          │  可看到 "ClaudeWiki助手"            │
  │  └──────────┘                                    │
  │                                                  │
  │  ● Monitor 运行中 (WebSocket 已连接)              │
  └──────────────────────────────────────────────────┘
```

## 部署实现

### 项目模板 (内嵌于 ClaudeWiki 二进制)

ClaudeWiki 在 `~/.warwolf/wechat-kefu/relay/` 生成一个完整的 wrangler 项目：

```
~/.warwolf/wechat-kefu/relay/
├── wrangler.toml          # wrangler 配置
├── src/
│   └── index.js           # Worker + Durable Object 代码
└── package.json           # 最小依赖声明
```

#### `wrangler.toml`

```toml
name = "claudewiki-kefu-relay"
main = "src/index.js"
compatibility_date = "2024-12-01"

[[durable_objects.bindings]]
name = "RELAY"
class_name = "RelayDO"

[[migrations]]
tag = "v1"
new_classes = ["RelayDO"]
```

#### `package.json`

```json
{
  "name": "claudewiki-kefu-relay",
  "version": "1.0.0",
  "private": true
}
```

#### `src/index.js` (~100 行)

```javascript
export default {
  async fetch(request, env) {
    const url = new URL(request.url);

    if (url.pathname === '/health') {
      return new Response('ok');
    }

    if (url.pathname === '/callback' || url.pathname === '/ws') {
      const id = env.RELAY.idFromName('default');
      const obj = env.RELAY.get(id);
      return obj.fetch(request);
    }

    return new Response('not found', { status: 404 });
  }
};

export class RelayDO {
  constructor(state, env) {
    this.state = state;
    this.env = env;
    this.buffer = [];
  }

  async fetch(request) {
    const url = new URL(request.url);

    // 桌面端 WebSocket 连接
    if (url.pathname === '/ws') {
      if (request.headers.get('Upgrade') !== 'websocket') {
        return new Response('expected websocket', { status: 426 });
      }
      const auth = url.searchParams.get('auth');
      if (auth !== this.env.AUTH_TOKEN) {
        return new Response('unauthorized', { status: 401 });
      }
      const pair = new WebSocketPair();
      this.state.acceptWebSocket(pair[1]);
      // 推送缓冲的离线消息
      for (const msg of this.buffer) {
        pair[1].send(msg);
      }
      this.buffer = [];
      return new Response(null, { status: 101, webSocket: pair[0] });
    }

    // GET /callback — 微信 echostr 验证
    if (request.method === 'GET' && url.pathname === '/callback') {
      // 透传给桌面端验证 (Worker 不做 AES 解密, 密钥不上传)
      // 但桌面端可能离线, 所以 Worker 需要自行验证
      // 方案: echostr 验证在 Worker 侧完成 (token + aes_key 在 secrets 中)
      const params = Object.fromEntries(url.searchParams);
      const relayMsg = JSON.stringify({
        type: 'verify',
        params,
        ts: Date.now()
      });
      // 尝试让桌面端处理
      const clients = this.state.getWebSockets();
      if (clients.length > 0) {
        // 同步等待桌面端返回验证结果 (通过 WebSocket request/response)
        // 简化方案: Worker 自行做签名验证 + AES 解密
      }
      return await this.handleVerify(url.searchParams);
    }

    // POST /callback — 微信事件通知 → 推送 WebSocket
    if (request.method === 'POST' && url.pathname === '/callback') {
      const body = await request.text();
      const relayMsg = JSON.stringify({
        type: 'callback',
        params: url.search,
        body,
        ts: Date.now()
      });

      const clients = this.state.getWebSockets();
      if (clients.length > 0) {
        for (const ws of clients) {
          ws.send(relayMsg);
        }
      } else {
        // 桌面离线 → 缓冲
        this.buffer.push(relayMsg);
        if (this.buffer.length > 100) this.buffer.shift();
      }
      return new Response('success');
    }

    return new Response('not found', { status: 404 });
  }

  async handleVerify(params) {
    const msgSig = params.get('msg_signature') || '';
    const timestamp = params.get('timestamp') || '';
    const nonce = params.get('nonce') || '';
    const echostr = params.get('echostr') || '';

    // SHA1 签名验证
    const token = this.env.CALLBACK_TOKEN;
    const parts = [token, timestamp, nonce, echostr].sort();
    const hash = await sha1(parts.join(''));
    if (hash !== msgSig) {
      return new Response('signature mismatch', { status: 403 });
    }

    // AES-256-CBC 解密 echostr
    const aesKey = this.env.ENCODING_AES_KEY;
    const corpid = this.env.CORPID;
    const plaintext = await decryptEchostr(aesKey, echostr, corpid);
    return new Response(plaintext);
  }

  webSocketMessage(ws, msg) {
    // 心跳
    try {
      const data = JSON.parse(msg);
      if (data.type === 'ping') {
        ws.send(JSON.stringify({ type: 'pong', ts: Date.now() }));
      }
    } catch {}
  }

  webSocketClose(ws, code, reason) {}
  webSocketError(ws, error) {}
}

// --- SHA1 (Web Crypto API) ---
async function sha1(data) {
  const buf = await crypto.subtle.digest('SHA-1',
    new TextEncoder().encode(data));
  return [...new Uint8Array(buf)]
    .map(b => b.toString(16).padStart(2, '0')).join('');
}

// --- AES-256-CBC 解密 ---
async function decryptEchostr(encodingAesKey, cipherB64, corpid) {
  // Base64 decode AES key (43 chars → 32 bytes)
  const keyBytes = Uint8Array.from(atob(encodingAesKey + '='),
    c => c.charCodeAt(0));
  const iv = keyBytes.slice(0, 16);

  const key = await crypto.subtle.importKey('raw', keyBytes,
    { name: 'AES-CBC' }, false, ['decrypt']);

  const cipherBytes = Uint8Array.from(atob(cipherB64),
    c => c.charCodeAt(0));

  const plainBuf = await crypto.subtle.decrypt(
    { name: 'AES-CBC', iv }, key, cipherBytes);
  const plain = new Uint8Array(plainBuf);

  // 跳过 16 字节随机数, 读取 4 字节消息长度, 提取消息内容
  const msgLen = (plain[16] << 24) | (plain[17] << 16) |
                 (plain[18] << 8) | plain[19];
  const msg = new TextDecoder().decode(plain.slice(20, 20 + msgLen));
  return msg;
}
```

### Rust 部署模块: `deployer.rs`

```rust
/// wrangler CLI 自动部署器。
/// 在用户的 Cloudflare 账号中部署 Worker + Durable Object。
pub struct WranglerDeployer {
    cf_api_token: String,
    project_dir: PathBuf,  // ~/.warwolf/wechat-kefu/relay/
}

pub struct DeployResult {
    pub worker_url: String,      // https://claudewiki-kefu-relay.xxx.workers.dev
    pub callback_url: String,    // .../callback
    pub ws_url: String,          // wss://.../ws
    pub auth_token: String,
    pub callback_token: String,
    pub encoding_aes_key: String,
}

impl WranglerDeployer {
    /// 检测 Node.js 和 npx 是否可用
    pub fn check_prerequisites() -> PrereqStatus {
        let node = Command::new("node").arg("--version")
            .output().ok().map(|o| o.status.success()).unwrap_or(false);
        let npx = Command::new("npx").arg("--version")
            .output().ok().map(|o| o.status.success()).unwrap_or(false);
        PrereqStatus { node_ok: node, npx_ok: npx }
    }

    /// 生成项目模板到 ~/.warwolf/wechat-kefu/relay/
    fn scaffold_project(&self, corpid: &str) -> Result<(), DeployError> {
        let dir = &self.project_dir;
        fs::create_dir_all(dir.join("src"))?;

        // 写入 wrangler.toml
        fs::write(dir.join("wrangler.toml"), WRANGLER_TOML)?;

        // 写入 Worker 代码 (编译时嵌入)
        fs::write(dir.join("src/index.js"), WORKER_JS)?;

        // 写入 package.json
        fs::write(dir.join("package.json"), PACKAGE_JSON)?;

        Ok(())
    }

    /// 完整部署流程
    pub fn deploy(&self, corpid: &str) -> Result<DeployResult, DeployError> {
        // 1. 生成项目模板
        self.scaffold_project(corpid)?;

        // 2. wrangler deploy
        let deploy_output = Command::new("npx")
            .args(["wrangler", "deploy"])
            .current_dir(&self.project_dir)
            .env("CLOUDFLARE_API_TOKEN", &self.cf_api_token)
            .output()?;

        if !deploy_output.status.success() {
            return Err(DeployError::WranglerFailed(
                String::from_utf8_lossy(&deploy_output.stderr).to_string()
            ));
        }

        // 从 stdout 解析 Worker URL
        let worker_url = parse_worker_url(&deploy_output.stdout)?;

        // 3. 生成随机密钥
        let auth_token = generate_random_hex(32);
        let callback_token = generate_random_alphanum(13);
        let encoding_aes_key = generate_aes_key_43();

        // 4. 批量设置 Secrets
        let secrets = serde_json::json!({
            "AUTH_TOKEN": auth_token,
            "CALLBACK_TOKEN": callback_token,
            "ENCODING_AES_KEY": encoding_aes_key,
            "CORPID": corpid,
        });
        let secrets_json = serde_json::to_string(&secrets)?;

        let secret_output = Command::new("npx")
            .args(["wrangler", "secret", "bulk"])
            .current_dir(&self.project_dir)
            .env("CLOUDFLARE_API_TOKEN", &self.cf_api_token)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                child.stdin.take().unwrap()
                    .write_all(secrets_json.as_bytes())?;
                child.wait_with_output()
            })?;

        if !secret_output.status.success() {
            return Err(DeployError::SecretFailed(
                String::from_utf8_lossy(&secret_output.stderr).to_string()
            ));
        }

        // 5. 健康检查
        // (异步, 由调用方执行)

        Ok(DeployResult {
            worker_url: worker_url.clone(),
            callback_url: format!("{worker_url}/callback"),
            ws_url: worker_url.replace("https://", "wss://") + "/ws",
            auth_token,
            callback_token,
            encoding_aes_key,
        })
    }

    /// 升级 Worker 代码 (ClaudeWiki 更新时)
    pub fn upgrade(&self) -> Result<(), DeployError> {
        // 更新 src/index.js → wrangler deploy
        fs::write(
            self.project_dir.join("src/index.js"),
            WORKER_JS,
        )?;
        let output = Command::new("npx")
            .args(["wrangler", "deploy"])
            .current_dir(&self.project_dir)
            .env("CLOUDFLARE_API_TOKEN", &self.cf_api_token)
            .output()?;
        if !output.status.success() {
            return Err(DeployError::WranglerFailed(
                String::from_utf8_lossy(&output.stderr).to_string()
            ));
        }
        Ok(())
    }

    /// 删除 Worker
    pub fn undeploy(&self) -> Result<(), DeployError> {
        Command::new("npx")
            .args(["wrangler", "delete", "--name", "claudewiki-kefu-relay"])
            .current_dir(&self.project_dir)
            .env("CLOUDFLARE_API_TOKEN", &self.cf_api_token)
            .output()?;
        Ok(())
    }
}

/// 编译时嵌入的模板文件
const WRANGLER_TOML: &str = include_str!("relay_template/wrangler.toml");
const WORKER_JS: &str = include_str!("relay_template/src/index.js");
const PACKAGE_JSON: &str = include_str!("relay_template/package.json");
```

### 文件布局

```
rust/crates/desktop-core/src/wechat_kefu/
├── mod.rs
├── types.rs
├── client.rs              # 微信客服 API (现有)
├── callback.rs            # 回调签名验证 (现有)
├── monitor.rs             # 消息监听 (改用 relay_client)
├── desktop_handler.rs     # turn pipeline 桥接 (现有)
├── account.rs             # 持久化 (现有)
├── deployer.rs            # ← 新: wrangler CLI 部署器
├── relay_client.rs        # ← 新: WebSocket 中继客户端
└── relay_template/        # ← 新: Worker 项目模板
    ├── wrangler.toml
    ├── package.json
    └── src/
        └── index.js
```

### 持久化配置

```json
// ~/.warwolf/wechat-kefu/config.json
{
  // 微信客服
  "corpid": "wwdd403e3dd8bc27ee",
  "secret": "yTZy3BAEsgxkMd5AyJ0z9CJ...",
  "open_kfid": "wkbeQYRgAAj0EhLjS79mmsCLF0msXXdg",
  "contact_url": "https://work.weixin.qq.com/kfid/kfc...",
  "account_name": "ClaudeWiki助手",

  // Cloudflare 部署
  "cf_api_token": "用户的 CF API Token",
  "worker_url": "https://claudewiki-kefu-relay.abc123.workers.dev",
  "relay_ws_url": "wss://claudewiki-kefu-relay.abc123.workers.dev/ws",
  "relay_auth_token": "随机生成",

  // 微信回调 (自动生成, 用户复制到 kf.weixin.qq.com)
  "callback_url": "https://claudewiki-kefu-relay.abc123.workers.dev/callback",
  "callback_token": "RTFJynmyrgHKc",
  "encoding_aes_key": "k22pqLXRowo8YCt4cUKSs6A1LrEcNLTTUsoorC1PvRh"
}
```

## 安全设计

| 层 | 措施 |
|----|------|
| CF API Token | 仅存用户本地 (0o600), 不上传任何服务器 |
| 微信 Secret | 仅存桌面端, Worker 中无此密钥 |
| Worker Secrets | 通过 `wrangler secret bulk` 安全传输, Worker 代码不可读 |
| WebSocket Auth | 随机 auth_token, Worker 校验后才允许 upgrade |
| 回调验签 | Worker 做 SHA1 + AES 解密 (密钥在 CF Secrets 中) |
| 数据隔离 | 每个用户独立 CF 账号, 物理隔离 |

## 产品升级路径

```
ClaudeWiki 新版本发布
    ↓
内嵌新的 relay_template/src/index.js
    ↓
首次启动检测: 本地模板版本 vs 已部署版本
    ↓
如有更新: deployer.upgrade()
    ↓
npx wrangler deploy → Worker 自动更新
    ↓
用户无感知
```

## 实施计划

| Phase | 工作量 | 内容 |
|-------|--------|------|
| 1. relay_template/ | 2h | wrangler.toml + package.json + src/index.js (~100行) |
| 2. deployer.rs | 3h | scaffold + wrangler deploy + secret bulk + upgrade/undeploy |
| 3. relay_client.rs | 2h | WebSocket 客户端 (心跳/重连/去重/缓冲消息接收) |
| 4. 集成 | 2h | monitor.rs 对接 relay, DesktopState + HTTP 路由 |
| 5. 前端 UI | 3h | 4 步引导 (环境检查 → 部署 → 回调配置 → 扫码) |
| 6. 端到端测试 | 1h | 完整流程 |
| **总计** | **~13h** | |

## 决策点

| # | 决策 | 建议 |
|---|------|------|
| 1 | Node.js 依赖 | 引导用户安装, 或检测 Homebrew 自动装 |
| 2 | wrangler 版本 | `npx wrangler` 自动拉最新, 不锁定版本 |
| 3 | Worker 语言 | JavaScript (简单, ~100行, 无编译) |
| 4 | 回调验证位置 | Worker 做签名验证+AES解密 (桌面离线时仍可通过验证) |
| 5 | 离线缓冲 | DO 内存缓冲 100 条, 桌面上线后自动推送 |
| 6 | 自动升级 | ClaudeWiki 启动时比较模板版本, 有变化自动 `wrangler deploy` |

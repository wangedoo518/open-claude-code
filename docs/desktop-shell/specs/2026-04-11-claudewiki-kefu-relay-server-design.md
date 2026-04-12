---
title: ClaudeWiki 微信客服中继服务器设计
doc_type: spec
status: active
owner: desktop-shell
last_verified: 2026-04-11
---

# ClaudeWiki 微信客服中继服务器设计

## 背景

ClaudeWiki 桌面端没有公网 IP，无法直接接收微信客服的 HTTP 回调。之前尝试的 Cloudflare Tunnel 方案有两个问题：

1. **`sync_msg` 无 token 轮询被限流** — errcode=45009 (api freq out of limit)
2. **Tunnel URL 不稳定** — 免费 Tunnel 每次重启 URL 变化，需重新配置回调

QClaw 的解决方案：部署一台公网服务器作为中继，桌面端通过 **出站 WebSocket** 连接中继，中继接收微信回调后通过 WebSocket 推送给桌面端。

## QClaw 架构参考

```
微信用户 → WeChat 平台 → HTTP POST 回调 → AGP 中继服务器 → WebSocket 推送 → QClaw 桌面端
                                          (wss://mmgrcalltoken.3g.qq.com/agentwss)
                                          - 腾讯运营
                                          - token 路由: token → WebSocket 连接
                                          - 桌面端出站连接 (穿透 NAT)
```

核心设计：
- 桌面端发起**出站** WebSocket 连接（穿透 NAT/防火墙）
- 中继服务器保持**入站** HTTP 端点（接收微信回调）
- 通过 `channel_token` 路由：token → 对应的 WebSocket 连接
- 心跳保活（20s ping）+ 指数退避重连（3s-25s）

## ClaudeWiki 中继服务器方案

### 方案对比

| 方案 | 复杂度 | 成本 | 延迟 | 桌面需常驻? | 可靠性 |
|------|--------|------|------|------------|--------|
| **A: Cloudflare Worker + Durable Object** | 低 | 免费 | ~50ms | 否(可缓冲) | 高 |
| B: Fly.io + Rust Axum | 中 | 免费 | ~50ms | 否(可缓冲) | 高 |
| C: 自有 VPS + Rust | 中 | ~$5/月 | ~30ms | 否(可缓冲) | 中 |
| D: cloudflared Tunnel | 最低 | 免费 | ~20ms | 是 | 低(URL变化) |

**推荐方案 A: Cloudflare Worker + Durable Object**

理由：
- 免费额度充足（100k 请求/天）
- Durable Object 天然支持 WebSocket（Hibernation API 节省资源）
- 固定 URL（`kefu-relay.your-domain.workers.dev`）
- 全球边缘节点，延迟低
- 零运维（无需管理服务器/证书/升级）

### 架构图

```
                    Cloudflare Worker (公网)
                    kefu-relay.clawclub.workers.dev
┌─────────────────────────────────────────────────────────┐
│                                                         │
│  GET/POST /callback                                     │
│  ├── GET  → 验证 echostr (AES 解密) → 返回明文          │
│  └── POST → 验证签名 → 解密 XML → 提取 Token            │
│            → 通过 WebSocket 推送给桌面端                 │
│                                                         │
│  GET /ws?auth_token=<secret>                            │
│  └── 升级为 WebSocket (Durable Object)                   │
│      └── 保持连接，接收推送                               │
│                                                         │
│  Durable Object "RelayDO"                               │
│  ├── 维护 WebSocket 连接列表                              │
│  ├── 接收回调 → 广播给已连接客户端                        │
│  ├── WebSocket Hibernation (空闲不计费)                   │
│  └── 消息缓冲 (桌面离线时暂存, 上线后推送)                │
│                                                         │
└───────────────┬─────────────────────────────────────────┘
                │
                │ 微信 HTTP POST 回调
                │ (msg_signature + timestamp + nonce + 加密XML)
                │
┌───────────────┴─────────────────┐
│       微信客服平台               │
│  qyapi.weixin.qq.com            │
└─────────────────────────────────┘

                │
                │ 出站 WebSocket (穿透 NAT)
                │ wss://kefu-relay.clawclub.workers.dev/ws
                │
┌───────────────┴─────────────────────────────────────────┐
│              ClaudeWiki 桌面端                           │
│                                                         │
│  wechat_kefu/relay_client.rs                            │
│  ├── WebSocket 连接到中继                                │
│  ├── 接收回调事件 → 解析为 CallbackEvent                 │
│  ├── 触发 sync_msg(cursor, token) 拉取消息              │
│  ├── 心跳 (30s ping) + 重连 (指数退避)                   │
│  └── 传入 monitor.rs 的 callback_rx channel             │
│                                                         │
│  消息处理流程 (已有, 不变):                               │
│  sync_msg → KefuDesktopHandler → turn pipeline → send_msg│
└─────────────────────────────────────────────────────────┘
```

### 消息流程

```
1. 微信用户发消息 → 微信平台

2. 微信平台 POST → Worker /callback
   Query: msg_signature, timestamp, nonce
   Body: <xml><Encrypt>...</Encrypt></xml>

3. Worker 验证签名 → 解密 XML → 提取 Token
   → 构造 JSON: { "type": "callback", "token": "ENC...", "timestamp": "..." }

4. Worker → Durable Object → WebSocket push → 桌面端

5. 桌面端 relay_client.rs 接收 → 解析为 CallbackEvent::MsgReceive { token }
   → 写入 callback_rx channel

6. monitor.rs 从 callback_rx 接收 → 调用 sync_msg(cursor, token)
   → 获取消息列表

7. desktop_handler.rs 处理消息 → turn pipeline → 生成回复

8. client.rs 调用 send_msg → 微信用户收到回复

Worker 返回 "success" 给微信 (步骤 3 完成后立即返回, 不等桌面处理)
```

### Worker 实现 (~80 行 TypeScript)

```typescript
// wrangler.toml
// name = "claudewiki-kefu-relay"
// [durable_objects]
// bindings = [{ name = "RELAY", class_name = "RelayDO" }]

interface Env {
  RELAY: DurableObjectNamespace;
  AUTH_TOKEN: string;        // 桌面端认证密钥
  CALLBACK_TOKEN: string;    // 微信回调验证 Token
  ENCODING_AES_KEY: string;  // 微信回调 AES Key
  CORPID: string;            // 企业 ID
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);

    if (url.pathname === '/callback') {
      // 微信回调 → 转发给 Durable Object
      const id = env.RELAY.idFromName('default');
      const obj = env.RELAY.get(id);
      return obj.fetch(request);
    }

    if (url.pathname === '/ws') {
      // 桌面端 WebSocket 连接
      const auth = url.searchParams.get('auth_token');
      if (auth !== env.AUTH_TOKEN) {
        return new Response('unauthorized', { status: 401 });
      }
      const id = env.RELAY.idFromName('default');
      const obj = env.RELAY.get(id);
      return obj.fetch(request);
    }

    if (url.pathname === '/health') {
      return new Response('ok');
    }

    return new Response('not found', { status: 404 });
  }
};

export class RelayDO {
  state: DurableObjectState;
  env: Env;
  messageBuffer: string[] = [];  // 离线消息缓冲

  constructor(state: DurableObjectState, env: Env) {
    this.state = state;
    this.env = env;
  }

  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);

    // WebSocket 升级 (桌面端连接)
    if (request.headers.get('Upgrade') === 'websocket') {
      const pair = new WebSocketPair();
      this.state.acceptWebSocket(pair[1]);

      // 推送缓冲的离线消息
      for (const msg of this.messageBuffer) {
        pair[1].send(msg);
      }
      this.messageBuffer = [];

      return new Response(null, { status: 101, webSocket: pair[0] });
    }

    // GET /callback → 微信验证 echostr
    if (request.method === 'GET' && url.pathname === '/callback') {
      const echostr = url.searchParams.get('echostr') || '';
      const msgSig = url.searchParams.get('msg_signature') || '';
      const timestamp = url.searchParams.get('timestamp') || '';
      const nonce = url.searchParams.get('nonce') || '';
      // 验证签名 + 解密 echostr (需实现 SHA1 + AES-CBC)
      const decrypted = verifyAndDecrypt(
        this.env.CALLBACK_TOKEN, this.env.ENCODING_AES_KEY,
        this.env.CORPID, msgSig, timestamp, nonce, echostr
      );
      return new Response(decrypted);
    }

    // POST /callback → 微信事件通知
    if (request.method === 'POST' && url.pathname === '/callback') {
      const body = await request.text();
      const params = url.search;

      // 构造中继消息
      const relayMsg = JSON.stringify({
        type: 'wechat_callback',
        params,
        body,
        timestamp: Date.now(),
      });

      // 推送给所有已连接的桌面端
      const clients = this.state.getWebSockets();
      if (clients.length > 0) {
        for (const ws of clients) {
          ws.send(relayMsg);
        }
      } else {
        // 桌面离线 → 缓冲 (最多 100 条)
        this.messageBuffer.push(relayMsg);
        if (this.messageBuffer.length > 100) {
          this.messageBuffer.shift();
        }
      }

      return new Response('success');
    }

    return new Response('not found', { status: 404 });
  }

  webSocketMessage(ws: WebSocket, msg: string | ArrayBuffer) {
    // 桌面端发来的消息 (心跳 pong 等)
    try {
      const data = JSON.parse(msg as string);
      if (data.type === 'ping') {
        ws.send(JSON.stringify({ type: 'pong' }));
      }
    } catch {}
  }

  webSocketClose(ws: WebSocket) {
    // 清理连接
  }
}
```

### 桌面端 relay_client.rs

```rust
/// 连接到中继服务器的 WebSocket 客户端。
/// 接收中继转发的微信回调事件, 传入 monitor 的 callback_rx。
pub struct RelayClient {
    relay_url: String,     // "wss://kefu-relay.clawclub.workers.dev/ws"
    auth_token: String,    // 中继认证密钥
    cancel: CancellationToken,
}

impl RelayClient {
    /// 主循环: connect → read → dispatch → reconnect
    pub async fn run(&self, callback_tx: mpsc::Sender<CallbackEvent>) {
        // 与 QClaw AGP 客户端相同的模式:
        // - 心跳 30s ping
        // - 指数退避重连 3s-25s
        // - 消息去重
        // 
        // 收到中继消息后:
        // 1. 解析 JSON { type: "wechat_callback", params, body }
        // 2. 用 KefuCallback 验签 + 解密
        // 3. 提取 CallbackEvent::MsgReceive { token }
        // 4. 发送到 callback_tx
    }
}
```

### 配置结构

```rust
pub struct KefuConfig {
    // ... 现有字段 ...

    /// 中继服务器 WebSocket URL
    pub relay_url: Option<String>,
    /// 中继认证密钥 (与 Worker 的 AUTH_TOKEN 匹配)
    pub relay_auth_token: Option<String>,
}
```

### 持久化布局

```
~/.warwolf/wechat-kefu/
├── config.json         # KefuConfig (含 relay_url, relay_auth_token)
├── cursor.json         # sync_msg cursor
└── session-map.json    # external_userid → desktop session
```

### kf.weixin.qq.com 回调配置

回调 URL 指向 Worker 的固定地址:
```
https://kefu-relay.clawclub.workers.dev/callback
```

这个 URL **永远不变**, 不像 cloudflared Tunnel 每次重启都变。

## 实施计划

### Phase 1: Worker 部署 (30 分钟)

1. `npm create cloudflare@latest claudewiki-kefu-relay`
2. 实现 Worker + Durable Object (~80 行)
3. 配置 secrets: `AUTH_TOKEN`, `CALLBACK_TOKEN`, `ENCODING_AES_KEY`, `CORPID`
4. `wrangler deploy`
5. 在 kf.weixin.qq.com 更新回调 URL → Worker URL
6. 验证回调通过

### Phase 2: relay_client.rs (2 小时)

1. 新增 `wechat_kefu/relay_client.rs`
2. tokio-tungstenite WebSocket 客户端 (参考已删除的 agp_client.rs)
3. 心跳 + 重连 + 消息去重
4. 接收中继消息 → KefuCallback 解密 → callback_tx
5. `cargo check`

### Phase 3: 集成 (1 小时)

1. 修改 `monitor.rs` — relay_client 替代 HTTP callback handler
2. 修改 `DesktopState` — 启动时连接中继
3. 修改前端 — relay 配置表单
4. `cargo check --workspace && npx tsc --noEmit`

### Phase 4: 端到端测试

1. 微信发消息 → Worker 收到 → WebSocket 推送 → 桌面收到 → sync_msg → 回复
2. 桌面断开重连测试
3. 桌面离线 → 上线后收到缓冲消息

## 与 QClaw 方案的对比

| 维度 | QClaw AGP 中继 | ClaudeWiki Worker 中继 |
|------|---------------|----------------------|
| 运营者 | 腾讯 | ClawClub (自有) |
| 协议 | AGP (私有, 含消息解析) | 透传 (原始回调 body) |
| 消息处理 | 中继解析消息内容 | 中继只转发, 桌面端解析 |
| 品牌 | 固定 "QClaw客服" | 自有 "ClaudeWiki助手" |
| 消息类型 | 仅文本 | 全类型 (文本/图片/链接/文件) |
| 离线缓冲 | 无 (需重连后重新拉取) | 有 (Worker 缓冲 100 条) |
| 成本 | 免费 (腾讯运营) | 免费 (Cloudflare 免费额度) |
| 可控性 | 不可控 | 完全自主 |

## 关键设计差异

**QClaw 中继 = 厚中继 (消息解析 + 路由 + 会话管理)**
- 中继服务器理解 AGP 协议
- 解析微信消息为 session.prompt
- 管理 session_id, prompt_id
- 实现消息队列和重试

**ClaudeWiki 中继 = 薄中继 (纯透传)**
- Worker 只做: 接收 HTTP → 转发 WebSocket
- 不解析微信消息内容
- 不管理会话状态
- 解密和业务逻辑全在桌面端

薄中继优势:
1. Worker 代码极简 (~80 行), 不易出 bug
2. 微信 API 变化只需更新桌面端, 不需重新部署 Worker
3. 安全: 密钥 (secret, AES key) 可以只存桌面端 (Worker 只做签名验证)

## 决策点

1. **Worker 域名**: `kefu-relay.clawclub.workers.dev` 还是绑定自有域名?
2. **签名验证在哪做**: Worker 侧 (防止伪造回调) vs 桌面端 (Worker 纯透传)?
   - 建议: Worker 做基本签名验证 (防止垃圾请求), 桌面端做完整解密
3. **离线消息策略**: 缓冲多少条? 缓冲多长时间?
   - 建议: 100 条, 1 小时过期
4. **多设备支持**: 是否允许多台桌面同时连接同一个 Worker?
   - 建议: 广播给所有连接 (去重由桌面端处理)

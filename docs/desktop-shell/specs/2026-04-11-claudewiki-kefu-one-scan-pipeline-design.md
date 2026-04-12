---
title: ClaudeWiki 微信客服一键扫码接入 Pipeline 设计 (OpenCLI 方案)
doc_type: spec
status: active
owner: desktop-shell
last_verified: 2026-04-11
---

# ClaudeWiki 微信客服一键扫码 Pipeline (基于 OpenCLI)

## 产品愿景

用户在 ClaudeWiki 中点击 "一键接入微信客服"，**只需企业微信扫一次码**，全部自动完成。

## 工具选型：OpenCLI 替代 Selenium

| 维度 | Selenium (legacy automation) | OpenCLI ✓ |
|------|------------------------|-----------|
| 浏览器控制 | Headless + undetected-chromedriver | 复用真实 Chrome (Browser Bridge) |
| 反检测 | 绕过机制 (易失效) | 无需绕过 — 用户自己的浏览器 |
| 登录态 | 每次重新登录 | 复用已有 Cookie/Session |
| 依赖 | Python + Selenium + ChromeDriver (~200MB) | Node.js + opencli (~20MB) |
| 可靠性 | ChromeDriver 版本兼容频繁 break | 稳定 — 真实 Chrome |
| Pipeline | 手写 Python 编排 | 内置 YAML Pipeline 引擎 |
| WeCom | 无 | wecom-cli 集成 |

## 架构总览

```
┌─────────────────────────────────────────────────────────────────┐
│                    ClaudeWiki 桌面端                              │
│                                                                  │
│  Pipeline 编排器 (Rust → 调用 opencli)                           │
│                                                                  │
│  Phase 1 ──→ Phase 2 ──→ Phase 3 ──→ Phase 4 ──→ Phase 5       │
│  CF 注册     Worker 部署  企微扫码     回调配置    客服创建       │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  OpenCLI (Node.js)                                       │   │
│  │  ├── opencli browser (Chrome Bridge 浏览器自动化)         │   │
│  │  ├── opencli wecom-cli (企业微信 CLI)                     │   │
│  │  ├── npx wrangler (Cloudflare Worker 部署)                │   │
│  │  └── Pipeline YAML 编排                                   │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │  临时邮箱服务 (复用既有自动化思路)                        │   │
│  │  POST /api/new_address → 临时邮箱                         │   │
│  │  GET  /api/messages    → 验证码                           │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

## 5 阶段 Pipeline 详细设计

### Phase 1: Cloudflare 账号自动注册 (~30s)

**工具: `opencli browser` + 临时邮箱 API**

```bash
# 1. 生成临时邮箱
curl -s -X POST https://mail-api.example.com/api/new_address \
  → {email: "abc123@example.test", jwt: "xxx"}

# 2. 浏览器自动填写注册表单
opencli browser open "https://dash.cloudflare.com/sign-up"
opencli browser type "#email" "abc123@example.test"
opencli browser type "#password" "$(openssl rand -base64 16)"
opencli browser click "button[type=submit]"

# 3. 等待验证邮件 → 提取验证链接
curl -s https://mail-api.example.com/api/messages?jwt=xxx
  → 轮询 3s 间隔, 120s 超时
  → 提取验证链接

# 4. 点击验证链接
opencli browser open "${verify_link}"

# 5. 创建 API Token
opencli browser open "https://dash.cloudflare.com/profile/api-tokens"
opencli browser click "a:text('Create Token')"
opencli browser click "button:text('Use template')"  # Edit Workers 模板
opencli browser click "button:text('Continue to summary')"
opencli browser click "button:text('Create Token')"
# 提取 token 值
opencli browser get value "input.token-value"
  → cf_api_token
```

**Rust 编排:**

```rust
pub async fn phase1_cf_register(
    &self,
) -> Result<CfCredentials, PipelineError> {
    // 1. 生成临时邮箱
    let email_resp = self.http.post("https://mail-api.example.com/api/new_address")
        .send().await?;
    let (email, jwt) = parse_email_resp(email_resp);

    // 2-5. OpenCLI 浏览器自动化
    let password = generate_password(16);
    self.opencli(&["browser", "open", "https://dash.cloudflare.com/sign-up"]).await?;
    self.opencli(&["browser", "type", "#email", &email]).await?;
    self.opencli(&["browser", "type", "#password", &password]).await?;
    self.opencli(&["browser", "click", "button[type=submit]"]).await?;

    // 等待验证邮件
    let verify_link = self.poll_email_for_link(&jwt, 120).await?;
    self.opencli(&["browser", "open", &verify_link]).await?;

    // 创建 API Token
    self.opencli(&["browser", "open",
        "https://dash.cloudflare.com/profile/api-tokens"]).await?;
    // ... (点击创建流程)
    let token = self.opencli_get(&["browser", "get", "value", "input.token-value"]).await?;

    Ok(CfCredentials { email, password, api_token: token })
}

/// 统一 OpenCLI 调用入口
fn opencli(&self, args: &[&str]) -> Result<String, PipelineError> {
    let output = Command::new("opencli")
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(PipelineError::OpenCli(
            String::from_utf8_lossy(&output.stderr).to_string()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
```

### Phase 2: Worker 中继部署 (~30s)

**工具: `npx wrangler` (通过 OpenCLI 注册为外部 CLI)**

```bash
# 注册 wrangler 为 opencli 外部命令 (一次性)
opencli register wrangler --binary "npx wrangler"

# 生成项目 → 部署 → 设置密钥
cd ~/.warwolf/wechat-kefu/relay
CLOUDFLARE_API_TOKEN=xxx npx wrangler deploy
echo '{"AUTH_TOKEN":"...","CALLBACK_TOKEN":"...","ENCODING_AES_KEY":"..."}' | \
  CLOUDFLARE_API_TOKEN=xxx npx wrangler secret bulk
```

**Rust 编排 (复用之前的 deployer.rs):**

```rust
pub async fn phase2_deploy_worker(
    &self,
    cf_token: &str,
    corpid: &str,
) -> Result<DeployResult, PipelineError> {
    self.scaffold_project()?;
    let output = Command::new("npx")
        .args(["wrangler", "deploy"])
        .current_dir(&self.relay_dir)
        .env("CLOUDFLARE_API_TOKEN", cf_token)
        .output()?;
    // ... 解析 URL + 设置 secrets
    Ok(deploy_result)
}
```

### Phase 3: 企业微信扫码授权 (~10s, 用户唯一操作)

**方案 A: 第三方应用授权 (需产品方注册服务商)**

```
ClaudeWiki 展示企微授权 QR:
  https://open.work.weixin.qq.com/3rdapp/install
    ?suite_id=SUITE_ID
    &pre_auth_code=PRE_AUTH_CODE
    &redirect_uri=https://example.com/wecom/callback

用户企微扫码 → 确认 → 回调返回 auth_code
→ 交换 permanent_code + corpid + access_token
```

**方案 B: wecom-cli 扫码 (无需注册服务商, 更快落地)**

```bash
# 使用 wecom-cli 获取企微凭证
opencli wecom-cli auth login --scan
  → 显示 QR 码
  → 用户扫码确认
  → 获得 corpid + access_token

# 获取客服 Secret
opencli wecom-cli kefu secret get
  → kefu_secret
```

**Rust 编排:**

```rust
pub async fn phase3_wecom_auth(&self) -> Result<WecomCredentials, PipelineError> {
    // 显示 QR 码让用户扫码
    let output = self.opencli(&[
        "wecom-cli", "auth", "login", "--scan", "--format", "json"
    ]).await?;
    let creds: WecomCredentials = serde_json::from_str(&output)?;
    Ok(creds)
}
```

### Phase 4: kf.weixin.qq.com 回调配置 (~15s)

**工具: `opencli browser` (复用用户已登录的企微管理后台)**

```bash
# 用户在 Phase 3 扫码后, 浏览器可能已有企微管理后台 session

# 1. 打开客服配置页
opencli browser open "https://kf.weixin.qq.com/kf/frame#/config/api_setting"

# 2. 如需登录 → 等待企微扫码
opencli browser wait "input[placeholder*='URL']" --timeout 30

# 3. 填写回调配置
opencli browser type "input[placeholder*='URL']" "${callback_url}"
opencli browser type "input[placeholder*='Token']" "${callback_token}"
opencli browser type "input[placeholder*='AESKey']" "${encoding_aes_key}"

# 4. 点击 "完成"
opencli browser click "button:text('完成')"

# 5. 等待验证通过
opencli browser wait "span:text('验证通过')" --timeout 10
```

**注意:** kf.weixin.qq.com 可能需要单独的企微扫码登录。如果 Phase 3 的登录态不能复用，此步骤需要第二次扫码。优化方案：先让用户在 Chrome 中登录 kf.weixin.qq.com，然后 OpenCLI Browser Bridge 复用该 session。

**Rust 编排:**

```rust
pub async fn phase4_configure_callback(
    &self,
    callback_url: &str,
    callback_token: &str,
    encoding_aes_key: &str,
) -> Result<String, PipelineError> {
    self.opencli(&["browser", "open",
        "https://kf.weixin.qq.com/kf/frame#/config/api_setting?isfirst=1"]).await?;

    // 等待表单加载
    self.opencli(&["browser", "wait", "input", "--timeout", "30"]).await?;

    // 填写
    self.opencli(&["browser", "type", "input:nth(1)", callback_url]).await?;
    self.opencli(&["browser", "type", "input:nth(2)", callback_token]).await?;
    self.opencli(&["browser", "type", "input:nth(3)", encoding_aes_key]).await?;

    // 提交
    self.opencli(&["browser", "click", "button:text('完成')"]).await?;

    // 等待验证
    self.opencli(&["browser", "wait", "text('Secret')", "--timeout", "15"]).await?;

    // 提取 Secret
    let secret = self.opencli_get(&["browser", "get", "text", ".secret-value"]).await?;
    Ok(secret)
}
```

### Phase 5: 客服账号创建 + 链接生成 (~5s)

**工具: 直接 HTTP API (wxkefu-rs 或 reqwest)**

```rust
pub async fn phase5_create_kefu(
    &self,
    corpid: &str,
    secret: &str,
) -> Result<KefuResult, PipelineError> {
    let client = KefuClient::new(corpid, secret);

    // 上传头像 → 创建账号 → 生成链接
    let media_id = upload_avatar(&client).await?;
    let open_kfid = client.create_account_with_media("ClaudeWiki助手", &media_id).await?;
    let contact_url = client.get_contact_url(&open_kfid).await?;

    // 启动 WebSocket 中继
    // (自动连接到 Phase 2 部署的 Worker)

    Ok(KefuResult { open_kfid, contact_url })
}
```

## 完整 Pipeline 编排

```rust
/// 一键接入 Pipeline: 用户只需企微扫码一次
pub async fn run_full_pipeline(&self) -> Result<PipelineResult, PipelineError> {
    // 前置检查
    self.check_prerequisites().await?; // Node.js, opencli, Chrome

    // Phase 1: Cloudflare 注册
    self.update_status(Phase::CfRegister, Status::Running).await;
    let cf = self.phase1_cf_register().await?;
    self.update_status(Phase::CfRegister, Status::Done).await;

    // Phase 2: Worker 部署
    self.update_status(Phase::WorkerDeploy, Status::Running).await;
    let deploy = self.phase2_deploy_worker(&cf.api_token, "pending").await?;
    self.update_status(Phase::WorkerDeploy, Status::Done).await;

    // Phase 3: 企微扫码 (← 用户唯一操作)
    self.update_status(Phase::WecomAuth, Status::WaitingScan).await;
    let wecom = self.phase3_wecom_auth().await?;
    self.update_status(Phase::WecomAuth, Status::Done).await;

    // Phase 4: 回调配置
    self.update_status(Phase::CallbackConfig, Status::Running).await;
    let secret = self.phase4_configure_callback(
        &deploy.callback_url,
        &deploy.callback_token,
        &deploy.encoding_aes_key,
    ).await?;
    self.update_status(Phase::CallbackConfig, Status::Done).await;

    // Phase 5: 客服创建
    self.update_status(Phase::KefuCreate, Status::Running).await;
    let kefu = self.phase5_create_kefu(&wecom.corpid, &secret).await?;
    self.update_status(Phase::KefuCreate, Status::Done).await;

    // 持久化所有凭证
    self.save_config(cf, deploy, wecom, secret, kefu).await?;

    // 启动 WebSocket 中继
    self.start_relay_client().await?;

    Ok(PipelineResult { contact_url: kefu.contact_url })
}
```

## 前端 UI

```
┌────────────────────────────────────────────────────────────────┐
│  ClaudeWiki助手 · 一键接入微信客服                               │
│ ───────────────────────────────────────────────────────────── │
│                                                                │
│  ┌─── Pipeline 进度 ──────────────────────────────────────┐   │
│  │                                                         │   │
│  │  ✓ Phase 1: Cloudflare 账号       已注册                │   │
│  │  ✓ Phase 2: 中继服务器             已部署                │   │
│  │  ◉ Phase 3: 企业微信授权           ← 请扫码              │   │
│  │  ○ Phase 4: 回调配置               等待中                │   │
│  │  ○ Phase 5: 客服创建               等待中                │   │
│  │                                                         │   │
│  │  ┌──────────────┐                                      │   │
│  │  │              │  请使用企业微信                        │   │
│  │  │  企微授权     │  扫描左侧二维码                       │   │
│  │  │  二维码       │                                      │   │
│  │  │              │  授权后将自动完成                      │   │
│  │  │              │  全部剩余配置                          │   │
│  │  └──────────────┘                                      │   │
│  │                                                         │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                │
│  已有 Cloudflare 账号?  [跳过 Phase 1, 手动输入 Token]          │
│  已配置过回调?          [跳过 Phase 4]                          │
│                                                                │
└────────────────────────────────────────────────────────────────┘
```

## 文件结构

```
新增/修改:

rust/crates/desktop-core/src/wechat_kefu/
├── pipeline.rs              # Pipeline 编排 (Rust 调用 opencli)
├── deployer.rs              # Phase 2: wrangler 部署
├── relay_client.rs          # WebSocket 中继客户端
├── relay_template/          # Worker 代码模板
│   ├── wrangler.toml
│   ├── package.json
│   └── src/index.js
└── ... (现有模块)

opencli 自定义适配器:
~/.opencli/clis/
├── cloudflare/
│   └── register.yaml        # CF 注册 Pipeline 定义
├── kefu/
│   ├── config-callback.yaml  # 回调配置 Pipeline
│   └── create-account.yaml   # 客服创建 Pipeline
```

## 依赖链

```
ClaudeWiki 桌面端 (Rust/Tauri)
  └── 调用 OpenCLI (Node.js CLI)
        ├── opencli browser (Chrome Bridge)
        │   └── Chrome 浏览器 + OpenCLI 扩展
        ├── opencli wecom-cli (企微 CLI)
        │   └── @wecom/cli (npm)
        └── npx wrangler (CF Worker CLI)
            └── @cloudflare/wrangler (npm)

邮件服务:
  └── mail-api.example.com (示例临时邮箱服务)
```

**安装链 (首次使用时自动):**

```bash
# 1. 确保 Node.js (参考 clawhub123 的 linux 构建脚本)
brew install node  # macOS
# 或引导用户安装

# 2. 安装 OpenCLI
npm install -g @jackwener/opencli

# 3. 安装 OpenCLI Chrome 扩展
opencli doctor  # 检查扩展状态

# 4. 注册外部 CLI
opencli register wrangler --binary "npx wrangler"
opencli install @wecom/cli
```

## 实施计划

| Phase | 工作量 | 内容 |
|-------|--------|------|
| 0. 环境搭建 | 1h | 安装 OpenCLI + 扩展 + wecom-cli + wrangler |
| 1. CF 注册适配器 | 3h | OpenCLI browser 自动化 CF 注册 + Token 创建 |
| 2. Worker 模板 | 2h | relay_template/ (JS, ~100行) + deployer.rs |
| 3. 企微授权 | 3h | wecom-cli 集成 或 第三方应用授权流程 |
| 4. 回调配置适配器 | 2h | OpenCLI browser 自动填写 kf.weixin.qq.com |
| 5. 客服创建 | 1h | REST API (复用现有 client.rs) |
| 6. Pipeline 编排 | 3h | pipeline.rs (5 阶段状态机 + 错误恢复) |
| 7. 前端 UI | 2h | Pipeline 进度 + QR 显示 + 状态面板 |
| 8. 端到端测试 | 2h | 完整流程验证 |
| **总计** | **~19h** | |

## 决策点

| # | 决策 | 建议 |
|---|------|------|
| 1 | Phase 3 实现方式 | A: wecom-cli 扫码 ← **推荐 (最快落地)** |
|   |                  | B: 第三方应用授权 (需注册服务商, 更彻底) |
| 2 | Phase 4 登录态 | A: 复用 Phase 3 的企微 session ← **优先尝试** |
|   |                 | B: 用户需在 Chrome 中先登录 kf.weixin.qq.com |
| 3 | OpenCLI 扩展安装 | 引导用户安装 Chrome 扩展 (Browser Bridge 必需) |
| 4 | 错误恢复 | Pipeline 支持断点续传 — 每个 Phase 完成后持久化状态 |
| 5 | 跳过已完成步骤 | 用户已有 CF 账号/已配置回调 → 可跳过对应 Phase |

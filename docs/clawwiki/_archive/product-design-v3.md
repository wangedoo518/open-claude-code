# ClawWiki 产品设计方案 · v3 （Wiki-first · CCD 模式提取 · Broker 内聚）

> 源起：`Clippings/当知识开始自己生长：Karpathy开源个人LLM Wiki.md`
> 载体：`Warwolf/claudewiki`（`apps/desktop-shell` + `rust/crates/{desktop-core,desktop-server}`）
> 前代文档（与本文**并存**，供团队 diff 评审）：
>
> - [`product-design-v1.md`](./product-design-v1.md) · v1 · "不复刻 CCD"——被用户否决
> - [`product-design.md`](./product-design.md) · middle-path · "保留 Ask 页氛围，替换 TabBar/apps/code"
> - [`product-design-v2.md`](./product-design-v2.md) · v2 canonical · "完整保留 CCD + 加 Wiki + Broker 对外"
> - **本文 `product-design-v3.md` · v3 canonical** · **Wiki-first + 提取 CCD 模式 + Broker 只服务自己**

---

## 0. 本次指令 → 三条硬指标

> **产品定位**：ClawWiki 定位在 Wiki 管理。让用户在**微信的对话框中**结合 ClawWiki 客户端实现"认知资产操作系统"。
>
> **核心用户体验**：用微信强大的内容生态与用户的交互体验，通过 ClawWiki 实现 **Karpathy 式的认知复利系统**。
>
> **三个聚焦**（逐字翻译）：
>
> 1. **认知资产 OS**：把微信文章、语音、PPT、视频、网页、笔记等持续沉淀成可维护的 Wiki。
> 2. **桌面控制台**：**提取** Claude Code Desktop 式的工作台交互、流式会话、权限确认、任务审阅体验，来**专注打造 Wiki**。
> 3. **Codex Token Broker**：把平台分配的 Codex 账号稳定供给 **ClawWiki 自己**。其他外部 Claude Code Desktop / CLI / Cursor 等客户端**都去掉**。

### 三条硬指标翻译表

| 指令 | v2 的做法（被否决） | v3 的做法（本文） |
|---|---|---|
| 定位 | "保留 CCD 全部 + 加 Wiki 一级 Tab" | Wiki 是**唯一产品**，CCD 只是**模式来源** |
| CCD 双行 TabBar | 零改动保留 + 加 Wiki Tab | **删除**。换 Wiki 优先的 DeepTutor-style 单侧栏 |
| `/apps` MinApps 画廊 | 保留 + 追加 WeChat Inbox MinApp | **删除**。WeChat 合进 `/wiki/wechat` 一级页 |
| `/code` CLI 启动器 | 保留 + 加 "Launch CCD" 选项 | **删除**。不支持拉起任何外部 CLI |
| SessionWorkbench 组件 | 零改动 + mode 下拉（Code/Wiki） | 拆开成**5 个独立组件**沿用交互语法，只留 Wiki mode |
| Token Broker 消费者 | Ask + maintainer + **外部 CCD/Cursor/claw-cli** | **只** Ask + maintainer，**无对外 /v1 代理** |
| "Launch Claude Code Desktop" 按钮 | 有 | **没有** |
| 视觉 | 壳冷（CCD 灰黑蓝紫）+ 内容暖（DeepTutor 橙） | **全暖色**（DeepTutor 橙一以贯之） |

---

## 1. 核心用户故事（必须先讲清楚）

> **用户**：打开微信，看到一篇感兴趣的公众号文章、一段别人发的语音、一份会议 PPT、一个产品 demo 视频。
>
> **动作**：长按 → 转发 → 发给 "ClawWiki 小助手"（企业微信外联机器人）。
>
> **3 秒后**：桌面端 ClawWiki 在 **Inbox** 里收到一条卡片，显示 "已入库 raw/00248"。
>
> **1 分钟后**：卡片变成 "maintainer 已完成 5 件事——新建 `concept/xxx`、更新 `compare/yyy`、加了 4 条 backlink、追加了今日 changelog"。
>
> **用户**：点 Inbox 里的 "查看" → 进到 `/wiki/pages/concept/xxx` → Lora 衬线正文、右栏 backlinks、来源追溯到微信里的那篇文章。
>
> **两周后**：用户在 Dashboard 看到 "你今天读过的内容，和 10 天前某份 PPT 有一条冲突"，点进去 → AI 已经在 Inbox 里提好了合并草案，用户 1 click resolve。
>
> **一个月后**：用户开 Ask 问 "我最近关注的 AI memory 这一块，主要的几条论据是什么？"—— AI 基于**你自己 30 天里喂进去的 120 份材料**，而不是重新爬一遍互联网，给出一个只属于你的结构化回答。

这就是 Karpathy 说的**认知复利**——过去留下的判断越多，未来的回答越值钱。

---

## 2. 产品结构

ClawWiki 只有三条主线：

| 主线 | 角色 | 用户感知 |
|---|---|---|
| **A · 认知资产层** | Wiki 是产品本体 | 用户每天打开看 `/wiki/*`，读 / 问 / 复盘 |
| **B · 微信接入层** | 唯一的用户输入通道（MVP） | 用户在微信里转发 → 自动落地 |
| **C · Codex 供给层** | 基础设施，对用户透明 | 用户只看到"订阅池还有多少 token"一个数字 |

**所有其它都不做**：不做 MinApps 画廊、不做 `/code` CLI 启动器、不做外部 Broker、不做 Obsidian 插件、不做网页版、不做团队协作、不做移动端 app（MVP）。

---

## 3. 从 Claude Code Desktop 提取什么

用户的话是 "**提取** Claude Code Desktop 式的工作台交互、流式会话、权限确认、任务审阅体验"——这四样是交互**模式**，不是 UI 容器。v3 的做法是把这些模式做成**5 个独立组件**，注入到 Wiki 页面里。

### 3.1 提取清单（✅ 沿用模式语法，重新皮肤）

| # | CCD 原组件 | 路径 | ClawWiki 中的新用途 | 新路径 |
|---|---|---|---|---|
| 1 | `SessionWorkbenchTerminal` 的消息流 | `features/session-workbench/SessionWorkbenchTerminal.tsx` | Ask 页面的主对话流 + Maintainer 后台任务的流式进度 | `features/ask/AskStream.tsx` |
| 2 | `MessageItem` + `VirtualizedMessageList` | `features/session-workbench/MessageItem.tsx` | Ask 页消息气泡 + Inbox 里 Maintainer 的"思考过程"展开 | `features/ask/Message.tsx` · `features/inbox/MaintainerThought.tsx` |
| 3 | `InputBar` 多行 + 附件 + @mention + 斜杠命令 | `features/session-workbench/InputBar.tsx` | Ask 页输入器 + Dashboard "快捷问答"输入器（裁剪版） | `features/ask/Composer.tsx` · `features/dashboard/QuickAsk.tsx` |
| 4 | `PermissionDialog` low/medium/high + Always allow | `features/session-workbench/PermissionDialog.tsx` | Maintainer 的每一个 `write_page` / `deprecate_page` / `ingest_source` 都弹它 | `features/permission/WikiPermissionDialog.tsx` |
| 5 | `StatusLine` | `features/session-workbench/StatusLine.tsx` | Ask 页底部、Inbox 详情页底部、Dashboard 的"系统状态条"都用同一个 | `features/common/StatusLine.tsx` |

**任务审阅体验**——对应 CCD 的 `SubagentPanel`（`features/session-workbench/SubagentPanel.tsx`）：

| CCD 原组件 | ClawWiki 用途 |
|---|---|
| `SubagentPanel` 的 task 树 / 展开收起 / 状态标签 | `/wiki/inbox` 里每条 Maintainer 任务的展开详情：显示它调了哪些工具、读了哪些 raw/page、产出了哪些 diff、哪些被用户批准 |

→ 新路径 `features/inbox/MaintainerTaskTree.tsx`。

### 3.2 不提取的（❌ 删除或不碰）

| CCD 原组件 | 处理 |
|---|---|
| `shell/TabBar.tsx`（双行顶栏） | **删除**。v3 用 DeepTutor-style 左侧栏 |
| `shell/TabItem.tsx` | **删除** |
| `shell/AppShell.tsx` 里的 Session tab row 2 | **删除**。Ask 会话用常规 URL 路由（`/ask/:sessionId`）而不是 tab 条 |
| `features/apps/*`（MinApps 画廊 + MinAppDetailPage） | **全部删除**。MinApps 概念不进入 v3 |
| `features/code-tools/*`（`/code` 一级页 + `runCodeTool`） | **全部删除**。v3 不拉起任何外部 CLI |
| `features/workbench/HomePage.tsx`（Search/Scheduled/Dispatch/Customize）| **拆解**。Search 作为全局 Cmd+K；Scheduled 留在 Settings；Dispatch/Customize 不进 v3 |
| `features/workbench/OpenClawPage.tsx` | **删除** |
| `features/billing/CloudAccountsPanel.tsx` 的"前端明文 JSON"路径 | **删除**。走 Rust endpoint（从 v1 就规划但一直没做的活，v3 必须做） |

### 3.3 复用但重命名的

| v2 名字 | v3 名字 | 说明 |
|---|---|---|
| `session-workbench` | `ask` | 产品语言从"会话"换成"提问" |
| `workbench/HomePage` | `dashboard/DashboardPage` | 从"工作台首页"换成"认知资产仪表盘" |
| `code-tools/CodeToolsPage` | **删除** | |
| `apps/AppsGalleryPage` | **删除** | |

---

## 4. Token Broker 收窄到"内聚组件"

### 4.1 v2 vs v3

| 维度 | v2（对外） | v3（内聚） |
|---|---|---|
| 消费者 | ClawWiki Ask + maintainer + 外部 CCD / Cursor / claw-cli | **只** ClawWiki Ask + maintainer |
| HTTP 暴露 | `/v1/chat/completions` · `/v1/messages` · `/v1/models` + `/api/broker/status` + `/api/broker/launch-client` | **无 HTTP 暴露**。Broker 变成 `desktop-core` 的一个 Rust 模块，内部 trait 调用 |
| 端口 | 127.0.0.1:4357 对外 | 不绑端口。`desktop-server` 依然占 4357 但不开 `/v1` 路由 |
| "Launch Claude Code Desktop" 按钮 | 有 | **没有** |
| 环境变量注入 | 注入给外部 CCD | 无 |
| Settings 中的 "Token Broker" 页 | "外部客户端 hookup" 为主 | 改名 **"Subscription & Codex Pool"**，只展示内部状态（账号数 / 配额 / 刷新 / 消费曲线） |

### 4.2 内聚 Broker 的形态

```rust
// rust/crates/desktop-core/src/codex_broker.rs （新）
pub struct CodexBroker {
    pool: Arc<RwLock<Vec<DesktopCodexInstallationRecord>>>,
    managed_auth: Arc<DesktopManagedAuthRuntimeClient>,
    http: reqwest::Client,
    rr_counter: AtomicUsize,
}

impl CodexBroker {
    /// 供 Ask 会话和 wiki-maintainer 调用的唯一入口
    pub async fn chat_completion(&self, req: ChatRequest) -> Result<ChatStream>;

    /// 由 trade-service WS 推送触发
    pub async fn sync_cloud_accounts(&self, accounts: Vec<CloudAccountInput>);

    /// 用户订阅到期 / 主动退订
    pub async fn clear_cloud_accounts(&self);

    /// 健康状态（给 Settings 页显示用）
    pub fn status(&self) -> BrokerStatus;
}
```

`wiki-maintainer` 和 `ask-runtime` 两个 crate **直接** `use codex_broker::CodexBroker`，不走 HTTP。

### 4.3 为什么这是更好的选择

1. **攻击面缩小**：本机 4357 不再监听 `/v1`，就不会被任何本机恶意进程（或跨源请求）冒充 ClawWiki 去消耗你的 Codex 额度
2. **产品叙事更干净**：ClawWiki 不再是"一个 wiki 产品 + 一个 CCD 供给商"，它只是**一个 wiki 产品**。用户买订阅是为了自己的 wiki 被 AI 维护，不是为了给其它客户端续命
3. **Rust 侧性能提升**：少一层 HTTP 序列化/反序列化，`wiki-maintainer` 直接拿到 `ChatStream` Futures
4. **合规边界更清楚**：`docs/desktop-shell/cloud-managed-integration.md` 的隐私论述（token 不出 Rust 层）天然成立——因为根本没有"出 Rust 层"的 API
5. **Settings 页视觉更简单**：不用展示"Hook up external clients" 代码块、不用做 "Launch CCD" 按钮、不用画 "External clients seen" 面板

### 4.4 砍外部 Broker 后的"遗憾清单"

诚实列一下：

- ❌ 用户不能用自己订阅的 Codex 账号同时撑 CCD 和 ClawWiki ← 这是最大代价
- ❌ 如果用户原本期望用 ClawWiki 当"Codex 本机代理"来绕过 IP 限制 / 代理外部 CCD，这条路径没了
- ⚠️ Token 的单位经济性可能会被质疑："我买 ClawWiki 订阅，结果只能用来维护我自己的笔记？" —— **产品叙事必须把这个讲清楚：Codex 是"订阅的赠品"，你买的是 wiki 维护能力本身**

这几点列进 §11 评审清单。

---

## 5. 信息架构（Wiki-first · DeepTutor 侧栏）

```
ClawWiki 桌面壳 (Tauri)
│
├── 顶部 chrome (28px, 只留 traffic light + 应用名)
│
├── 左侧 Sidebar (220px, 可折叠到 56px)
│   ├── Logo + "ClawWiki"
│   ├── ─ Primary ─
│   ├── 📊 Dashboard           /dashboard
│   ├── 📥 Raw Library          /raw
│   ├── 📖 Wiki Pages           /wiki
│   ├── 💬 Ask                  /ask[/:sessionId]
│   ├── 🕸  Graph                /graph
│   ├── 📐 Schema               /schema
│   ├── 📨 Inbox  [badge]       /inbox
│   ├── ─ Bridge ─
│   ├── 🔗 WeChat Bridge        /wechat
│   ├── ─
│   └── ⚙️  Settings             /settings
│        ├── Account
│        ├── Subscription & Codex Pool   ← 原 Token Broker，收窄
│        ├── WeChat Bridge settings
│        ├── Wiki Storage
│        ├── Permissions
│        ├── Providers (Codex 一条)
│        ├── Shortcuts
│        └── About
│
└── 主区 (剩余宽度)
     ├── 上方 Page Head (56px) · h3 + sub + 右侧动作
     ├── 中间 Body (flex-1, overflow auto)
     └── 下方 StatusLine (28px, 仅在 Ask / Inbox / 任务在跑时显示)
```

Sidebar 折叠到 56px 时只显示图标 + 数字 badge。全局 `Cmd+K` 打开搜索 palette（替代 CCD 的 Search 侧栏）。全局 `Cmd+/` 打开快捷 Ask。

---

## 6. 视觉系统

**一以贯之用 DeepTutor 暖色**，不再搞 v2 那个"壳冷内容暖"的双主题。

| Token | Light | Dark |
|---|---|---|
| `--background` | `#FAF9F6` | `#1A1918` |
| `--foreground` | `#2D2B28` | `#E8E4DE` |
| `--card` | `#FFFFFF` | `#242220` |
| `--muted` | `#F0EDE7` | `#2A2725` |
| `--muted-fg` | `#8B8580` | `#9B9590` |
| `--border` | `#E8E4DE` | `#3A3634` |
| `--primary` | `#C35A2C` | `#D4734B` |
| `--accent` | `#8B5CF6` | `#A78BFA`（只用于 Maintainer AI 标识色） |
| Font UI | Plus Jakarta Sans | Plus Jakarta Sans |
| Font 阅读 | **Lora** 衬线 | **Lora** |
| Font mono | JetBrains Mono | JetBrains Mono |

`.surface-card` = `rounded-2xl border bg-card shadow-sm`
圆角：Card 16 / Input 10 / Button 8 / Chip 9999

---

## 7. 微信接入 · 继承 v2 pipeline

微信入口的技术链路和 v2 完全一样，因为 v2 那部分本来就不依赖 CCD 壳。只把"/apps/wechat-inbox MinApp"改名成"/wechat 一级页"。这里只列 v3 相对 v2 的改动点：

| 动作 | 改动 |
|---|---|
| 入口位置 | MinApp → Sidebar 一级项 `🔗 WeChat Bridge` |
| Inbox 展示 | 不再是 MinApp tray，而是一个带左右分栏的正式页面：左列历史事件、右列选中事件的 pipeline 5 步时间线 |
| 权限弹窗 | Maintainer 写 `write_page` 时弹的是**提取版的 `PermissionDialog`**（第 3.1 节的 `WikiPermissionDialog.tsx`） |
| 代码路径 | `features/wiki/ingest/*`（v2 规划）→ `features/ingest/*`（v3 提升到 features 一级） |
| `defuddle` fork + `wechat.ts` extractor | **不变**，依然要做 |
| `obsidian-clipper/api::clip()` | **不变**，继续在 WebView 里直接调用 |
| 10 种素材类型适配 | **不变**（mp.weixin URL / 普通 URL / 语音 / 图 / PPT / PDF / DOCX / 视频 / 小程序卡 / 聊天记录） |
| 云端 `wechat-ingest:8904` 新服务 | **不变** |
| WebSocket `/ws/wechat-inbox` | **不变** |

v2 §3 D4 的所有具体代码（`adapters/wechat-article.ts` · `templates/wechat-clip.json` · defuddle 的 `wechat.ts` 骨架）全部有效，直接迁移即可。

---

## 8. schema/CLAUDE.md（继承 v2 §3 D7，一字不改）

`~/.clawwiki/schema/CLAUDE.md` 保持 v2 里已经定好的版本。核心部分：

- **角色**：wiki-maintainer
- **分层合同**：raw/ 只读，wiki/ 可写但要通过 schema v1 frontmatter 校验，schema/ 纯手工
- **5 件维护动作**：summarise / update affected pages / bidirectional backlinks / mark conflict / append changelog
- **权限分级**：low (read) · medium (write_page / patch_page / link_pages / touch_changelog) · high (ingest_source / deprecate_page / mark_conflict)
- **硬禁**：never rewrite raw · never silently merge · never quote > 15 words · never touch schema/

---

## 9. 代码改造清单

### 9.1 `apps/desktop-shell/src/` · 前端

#### 删除

```
shell/TabBar.tsx
shell/TabItem.tsx
shell/AppShell.tsx                 （重写为侧栏版）
features/apps/*                    整个目录
features/code-tools/*              整个目录
features/workbench/HomePage.tsx
features/workbench/OpenClawPage.tsx
features/workbench/DispatchPage.tsx
features/workbench/CustomizePage.tsx
features/workbench/ScheduledPage.tsx   （挪进 Settings 的 Schedule 子页作为 backlog）
features/workbench/SearchPage.tsx      （挪进全局 Cmd+K palette）
features/session-workbench/         整个目录（组件被拆分到 features/ask + features/common + features/permission + features/inbox）
state/minapps-store.ts
state/tabs-store.ts
state/permissions-store.ts         （逻辑迁移到 wiki-maintainer-store）
```

#### 保留并改名

```
features/auth/*                    → 不动
features/billing/*                 → 保留 plans/orders；CloudAccountsPanel 改成只读 "Subscription & Codex Pool" 面板
features/settings/*                → 砍子页：删除 Provider (只留 Codex 一条) · MCP · Data · About 保留
state/auth-store.ts                → 不动
state/billing-store.ts             → 不动
state/settings-store.ts            → 不动
state/streaming-store.ts           → 迁移到 state/ask-store.ts
lib/cloud/*                        → 不动
lib/tauri.ts                       → 删掉 code-tools 相关的 type 和 invoke
```

#### 新增

```
shell/AppShell.tsx (rewrite)       侧栏版 AppShell
shell/Sidebar.tsx                  DeepTutor-style 220/56px 可折叠
shell/StatusLine.tsx               从 session-workbench/StatusLine 迁移 + 重新皮肤

features/dashboard/DashboardPage.tsx
features/dashboard/QuickAsk.tsx    提取版 InputBar

features/raw/RawLibraryPage.tsx
features/raw/RawDetailPage.tsx

features/wiki/WikiExplorerPage.tsx
features/wiki/WikiPageDetail.tsx
features/wiki/WikiCategoryTabs.tsx

features/ask/AskPage.tsx
features/ask/AskStream.tsx         从 SessionWorkbenchTerminal 提取
features/ask/Message.tsx           从 MessageItem 提取
features/ask/VirtualizedMessageList.tsx   从原文件迁移
features/ask/Composer.tsx          从 InputBar 提取
features/ask/useAskLifecycle.ts    从 useSessionLifecycle 提取

features/graph/GraphPage.tsx
features/schema/SchemaEditorPage.tsx
features/schema/PolicyProposal.tsx

features/inbox/InboxPage.tsx
features/inbox/MaintainerTaskCard.tsx
features/inbox/MaintainerTaskTree.tsx     从 SubagentPanel 提取
features/inbox/MaintainerThought.tsx      从 MessageItem 提取

features/permission/WikiPermissionDialog.tsx   从 PermissionDialog 提取

features/wechat/WeChatBridgePage.tsx
features/wechat/WeChatBotCard.tsx
features/wechat/WeChatInboxList.tsx
features/wechat/PipelineStatus.tsx

features/ingest/pipeline.ts
features/ingest/adapters/*.ts       （10 个 adapter，继承 v2 §3 D4）
features/ingest/templates/wechat-clip.json
features/ingest/persist.ts

state/wiki-store.ts                 Wiki 当前页、Inbox 未读、maintainer 任务队列
state/ask-store.ts                  ask 会话、streaming 缓冲
state/ingest-store.ts               ingest pipeline 进度
```

### 9.2 `rust/crates/*` · Rust

#### 删除

```
desktop-server::code_tools_bridge        整个模块
desktop-core::code_tools                 如果存在
```

#### 保留并改造

```
desktop-core::managed_auth               加 CloudManaged source（v1/v2 都规划过）
desktop-core::codex_auth                 不变
desktop-server::*                        删除 /api/desktop/code-tools/*；删除 /v1/*（没做就更好）
```

#### 新增

```
desktop-core::codex_broker               内聚 Broker（trait + 实现）
desktop-core::wiki_store                 ~/.clawwiki/ 文件系统后端
desktop-core::wiki_maintainer            触发型 agent loop
desktop-core::wechat_bridge              WebSocket 客户端订阅 trade-service

desktop-server 新增路由:
  POST /api/desktop/cloud/codex-accounts/sync
  GET  /api/desktop/cloud/codex-accounts
  POST /api/desktop/cloud/codex-accounts/clear
  GET  /api/broker/status               仅返回数字给 Settings 页面；不代理 LLM
  POST /api/wiki/raw/ingest
  POST /api/wiki/ingest/voice
  POST /api/wiki/ingest/image
  POST /api/wiki/ingest/pptx
  POST /api/wiki/ingest/docx
  POST /api/wiki/ingest/video
  POST /api/wiki/fetch                  代理外链 fetch（绕 CORS）
  GET  /api/wiki/pages/:slug
  PUT  /api/wiki/pages/:slug
  PATCH /api/wiki/pages/:slug
  GET  /api/wiki/raw
  GET  /api/wiki/raw/:id
  GET  /api/wiki/search
  GET  /api/wiki/graph
  GET  /api/wiki/inbox
  POST /api/wiki/inbox/:id/resolve
  GET  /api/wechat/events
  WS   /ws/wechat-inbox                 本机桥接，订阅云 trade-service
```

### 9.3 云端

和 v2 一样：新增 `wechat-ingest:8904` 微服务（POST webhook / GET inbox / GET blob / WS）。

---

## 10. MVP 路线（7 周，v3 精简版）

| Sprint | 周 | 交付 | 成功标准 |
|---|---|---|---|
| **S1** | W1 | 删除 shell 老代码 + 新 `Sidebar` + `DashboardPage` 空壳 + `~/.clawwiki/` 布局 + `schema/CLAUDE.md` 初稿 + `wiki_store` crate | 桌面开起来是 Wiki-first 布局，sidebar 有全部 8 个入口，`/dashboard` 能显示"0 pages"空态 |
| **S2** | W2 | fork defuddle + 写 `wechat.ts` extractor + 前端 `features/ingest/` + `Ask` 页骨架（复用 `MessageItem` 样式） | 粘贴 mp.weixin URL 到 Dashboard 的 QuickAsk → 10 秒内 raw/ 多一个格式良好 md + Ask 页能看到流式响应 |
| **S3** | W3 | `codex_broker` 内聚 + `managed_auth::CloudManaged` source + `cloud-accounts-sync` 改走 Rust + `Settings → Subscription & Codex Pool` 只读面板 | Dashboard 能显示"订阅池: 5 accounts · 4,521 req/day · ¥0"；Ask 对话走通 |
| **S4** | W4 | `wiki-maintainer` MVP（engram 风格单次 LLM 调用）+ `WikiPermissionDialog` + `Inbox` 页面（包含 MaintainerTaskTree） | 一次 ingest 自动生成 1-3 wiki/page，所有 `write_page` 都过 permission，Inbox 能看到完整任务树 |
| **S5** | W5 | `wechat-ingest` 云服务（文本 + URL）+ `wechat_bridge` Rust client + `WeChatBridgePage` 页面 | 微信发 mp 链接，3 秒内桌面 Inbox 出现卡片并自动走完 pipeline |
| **S6** | W6 | 语音 / 图片 / PPT / PDF / 视频 adapter + 对应 Rust endpoint + `GraphPage` 空实现 + `SchemaEditorPage` 只读 | 微信发语音/PPT/视频，raw 出 md 带转写/slides/关键帧 caption |
| **S7** | W7 | 陈旧度 lint + 冲突检测 + log.md/index.md 自动重建 + SchemaEditorPage 支持 proposal | Inbox 有真实告警，点击能触发 maintainer 一键修复 |

**Backlog（v4+）**：sage-wiki 5-pass compiler · prompt cache · Batch API · FTS5 · 向量 · MCP server · Obsidian Vault overlay · 个人微信桥接 opt-in · 团队多用户。

---

## 11. 评审清单（给团队逐条打勾）

### 11.1 产品边界决策（需要明确 yes/no）

| # | 问题 | 建议 | 谁拍板 |
|---|---|---|---|
| R1 | 彻底删除 `/apps` MinApps 画廊？（v2 规划过 WeChat Inbox MinApp） | **删** | PM |
| R2 | 彻底删除 `/code` CLI 启动器？（v2 规划过"Launch CCD"） | **删** | PM |
| R3 | Broker 完全内聚 / 不暴露 HTTP `/v1`？ | **是** | Tech Lead |
| R4 | Settings 里的 "Token Broker" 改名 "Subscription & Codex Pool"？ | **是** | PM + Design |
| R5 | 双层 TabBar 彻底删除，换 DeepTutor 侧栏？ | **是** | Design |
| R6 | 视觉放弃"壳冷内容暖"双主题，统一 DeepTutor 暖色？ | **是** | Design |
| R7 | `features/session-workbench/*` 整个目录删除，拆成 5 个独立组件？ | **是**，但要一次性拆完，不能半拆 | Tech Lead |

### 11.2 产品叙事 / 订阅价值（营销关切）

| # | 问题 | 风险 | 缓解方向 |
|---|---|---|---|
| R8 | 砍掉"Codex 供给给外部 CCD"后，订阅价值如何表述？ | 用户可能觉得"订阅只为了一个 wiki 太贵" | 把订阅包装成"雇一个全职维护员 + 海量 GPT-5.4 调用"，而不是"一个 token 卖场" |
| R9 | 免费版是否保留？如果保留，GPT-5.4 token 从哪来？ | 体验断层 | MVP 暂不做免费版，所有用户必须登录 + 有 active subscription |
| R10 | 微信渠道合规 vs 账号风险如何兜底？ | 企微需要企业资质；个微封号 | MVP 只做企微外联机器人；个微桥放 v4 作为 opt-in 高级功能 + 红字警告 |

### 11.3 技术可行性（工程关切）

| # | 问题 | 状态 |
|---|---|---|
| R11 | `defuddle` fork 的 wechat extractor 对 mp.weixin DOM 漂移的鲁棒性 | S2 末必须跑过 10 篇不同号的 fixture |
| R12 | `obsidian-clipper/api::clip()` 在 Tauri WebView 里的 bundle 大小 | ~200KB gzip，已 PoC 过，OK |
| R13 | `wiki-maintainer` 单次 LLM 调用返回的 JSON 是否能稳定过 schema 校验 | 需要写 few-shot examples 压测 |
| R14 | Codex 批处理 API 在订阅账号上是否可用 | 未知，需 spike |
| R15 | `managed_auth::CloudManaged` 需要的 `claw-code-parity` upstream 改动是否可控 | 这是 v1/v2 都欠的债，**S3 必须还** |

### 11.4 设计细节（设计关切）

| # | 问题 | 建议决策 |
|---|---|---|
| R16 | Sidebar 默认展开还是折叠？ | **展开**（220px）；记住用户偏好到 `settings-store` |
| R17 | Ask 会话是否持久化成标签页？ | **不**。会话列表进 Ask 页左栏；URL 是 `/ask/:id` |
| R18 | Dashboard 的 QuickAsk 是否走独立会话？ | 是。每次 QuickAsk 都开新 session，避免污染长会话上下文 |
| R19 | Inbox 里 Maintainer 的"思考过程"默认收起还是展开？ | **收起**，hover 显示小预览，点击展开完整 tool call 树（仿 CCD SubagentPanel） |
| R20 | 微信入库时是否默认 auto-approve 所有 low/medium 操作？ | **Yes**，否则用户会被 permission dialog 淹没；high 级别（deprecate / mark_conflict）永远要 confirm |

### 11.5 MVP 范围 trade-offs

| # | 问题 | 建议 |
|---|---|---|
| R21 | `GraphPage` 要不要进 MVP？ | 进 S6 但只做**空壳**（节点+边渲染），力导向布局放 v4 |
| R22 | `SchemaEditorPage` 要不要进 MVP？ | 进 S7 但只做**只读**（显示当前 CLAUDE.md 内容 + AI 提的 diff），手动编辑放 v4 |
| R23 | MCP server 是否进 MVP？ | **不进**。MCP 是"对外让其它 agent 读我们的 wiki"，和 v3 的"Broker 内聚"方向矛盾 |
| R24 | Obsidian Vault overlay 是否进 MVP？ | **不进**。作为导出功能放 v4 |
| R25 | 英文国际化是否进 MVP？ | **不进**。MVP 只中文；v4 再做 i18n |

---

## 12. 线框图索引

HTML 文件：[`./wireframes-v3.html`](./wireframes-v3.html) · 10 张屏。

| # | 屏 | 主要演示 |
|---|---|---|
| 01 | Dashboard | 认知资产复利指标 · 今日微信进账 · 维护活动时间线 · QuickAsk（复用 InputBar 样式）· StatusLine |
| 02 | Raw Library | 1326 源 · 92% 来自微信 · 支持按类型/时间/状态过滤 · 每条显示"已触发 N 次维护" |
| 03 | Wiki Pages · Explorer | Concepts/People/Topics/Compare/Changelog 分类 tab · 卡片网格 · 右上新建 |
| 04 | Wiki Page Detail | Lora 衬线正文 · 右栏 backlinks（来源追到微信那篇）/ sources / maintenance history |
| 05 | Ask · Wiki 会话 | 流式消息流（复用 MessageItem 视觉）· Composer（复用 InputBar 的 @mention + 附件）· **PermissionDialog** 拦截 `write_page` · StatusLine 底部 |
| 06 | Inbox · 维护任务审阅 | 左列任务列表 · 右列选中任务的 **MaintainerTaskTree**（复用 SubagentPanel 交互）：tool call 树、每一步 diff、approve/deny 按钮 |
| 07 | WeChat Bridge | 企微 bot 绑定卡 · 5 步 pipeline 状态灯 · 今日收件箱 · 合规说明 |
| 08 | Graph | 节点=页面 · 颜色=fresh/stale/conflict · 右上 legend · 点击节点跳 Page Detail |
| 09 | Schema Editor | 左文件树（CLAUDE.md/AGENTS.md/templates/policies）· 右 Monaco dark · 演示 AI 提的 schema proposal diff（绿底 add 行） |
| 10 | Settings · Subscription & Codex Pool | 订阅状态 · 5 个 cloud-managed 账号只读列表 · 今日消费曲线 · **没有任何"外部客户端"相关内容** · 退订按钮 |

---

## 13. 后续工作 & 留白

1. `schema/CLAUDE.md` 继承 v2，不变
2. `schema/AGENTS.md` 需要定义多 agent 分工（Maintainer / Reviewer / Compressor），MVP 先只实现 Maintainer
3. 订阅包的 SKU 定义（几档、每档多少 accounts、价格）由 Billing 团队定
4. 微信合规法务审查（企业微信外联机器人的用户数据合规性）需要单独 kickoff
5. **v3 相对 middle-path 的关系**：middle-path 的 §8.1（保留 CCD 熟悉感资产）和 §8.2（替换掉双层 TabBar / apps / code）和 §11（Token Broker 设计）的大部分判断在 v3 被继承 —— 所以可以把 middle-path 视为 **v3 的 0.5 版本**，而 v2 是一个"full-CCD"的 spike。team 评审时可以让 middle-path 的作者先 +1 v3 再继续讨论。

---

## 附：一句话总结

> **v3 = middle-path 的"信息架构重做" + v2 的"微信 pipeline 细节" + 一个新增的"Broker 内聚"决定**。
>
> 产品不再是一个装 Wiki 功能的 Claude Code Desktop 克隆，也不是一个对外供 Codex 的 broker 厂。它就是一个**用微信喂料 + 用 AI 自动维护的个人 Wiki**，CCD 在它里面只留下 5 个组件的肌肉记忆。

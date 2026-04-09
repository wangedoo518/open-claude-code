# ClawWiki 产品设计方案

> 源起：@docs/Clippings/当知识开始自己生长：Karpathy开源个人LLM Wiki.md
> 载体：本仓库 `Warwolf/claudewiki`（`apps/desktop-shell` + `rust/crates/{desktop-core,desktop-server}`）
> 参考：`Warwolf/DeepTutor`（Next.js 的 SidebarShell / 统一 Chat Workspace / Knowledge 架构）

## 0. TL;DR

1. **把"Claude Code Desktop 复刻器"降格为内部工具，把"LLM Wiki"升格为产品第一身位**。ClawWiki 的产品叙事是 Karpathy 式的认知复利系统，不是又一个 Claude Code 壳子。
2. **Token 供给解耦**。当前 `features/billing/*` 已把 Codex 订阅 token 拉到前端本地 JSON，但还没进到 `rust/desktop-core` 的 `managed_auth` registry。要把这一步做完，并在 `desktop-server` 旁边开一个 **本地 Token Broker（127.0.0.1:4357 /v1）**，供外部 Claude Code Desktop / claw-cli / Cursor 直接消费，而不是继续把 CLI 塞进 ClawWiki 自己的终端面板。
3. **不严格复刻 Claude Code Desktop UI**。只保留 "Ask" 这一条对话式维护器（复用现有 `session-workbench/*`），其余导航按 Karpathy 的三层结构（Raw / Wiki / Schema）重塑。
4. 视觉系统直接借 **DeepTutor** 的暖色系 + 侧栏 Shell：`--background:#FAF9F6`、`--primary:#C35A2C`、Plus Jakarta Sans + Lora，220px / 56px 可折叠 sidebar，`surface-card` 圆角卡片。
5. 十张 HTML 线框图落在 [`docs/clawwiki/wireframes.html`](./wireframes.html)，与本文档同目录。

---

## 1. 现状盘点（claudewiki 源码）

| 模块 | 位置 | 能力 | 问题 |
|---|---|---|---|
| 桌面壳层 | `apps/desktop-shell` | Tauri2 + React + HashRouter，双层 TabBar 模仿 Claude Code Desktop，主路由 `/home,/apps,/apps/:id,/code` | 产品叙事与 Wiki 无关 |
| 工作台 | `features/workbench/HomePage.tsx` | Sidebar + Session list + Settings/Search/Scheduled/Dispatch/Customize 子页 | 本质是一个 Claude Code 会话工作台 |
| 会话终端 | `features/session-workbench/*` | ContentHeader + VirtualizedMessageList + InputBar + PermissionDialog + StatusLine + SubagentPanel | 质量很好，值得复用 |
| Code Tools | `features/code-tools/CodeToolsPage.tsx` + `/code` | 选择 CLI + 模型 + 工作目录 + 终端 + env，调用 `runCodeTool` 通过 Rust 拉起外部 CLI（bun + codex/qwen-code/claw）| 当前是把其它 CLI 塞进 ClawWiki 内部 terminal，无 Broker 角色 |
| 登录 | `features/auth/*` + `state/auth-store.ts` | Google loopback OAuth → `user-service` (127.0.0.1:8902)，`setCloudAuthBindings` 注入全局 transport | 已可复用 |
| 套餐 / 订单 | `features/billing/*` + `state/billing-store.ts` | `/api/v1/plans`、`/orders`、`/payments/{alipay,wechat}`、`/subscriptions/me`、`/codex-accounts/me`；`CloudAccountsPanel` 显示明文 token，读写 Tauri plugin-store `~/.warwolf/cloud-accounts.json` | **token 只在前端 JSON，没进 Rust 的 managed_auth** |
| Rust 集成 | `rust/crates/desktop-core` | `managed_auth.rs` 定义 `DesktopManagedAuthProvider { Codex, QwenCode }` + `DesktopManagedAuthAccount`；`codex_auth.rs` 管 token 刷新 | 缺 `CloudManaged` source、没有 HTTP Broker |
| 本地 HTTP | `rust/crates/desktop-server` | 默认 `127.0.0.1:4357`，`/api/desktop/*`、`code_tools_bridge` | 缺 `/api/desktop/cloud/codex-accounts/*`、缺 `/v1` 代理 |
| 集成设计 | `docs/desktop-shell/cloud-managed-integration.md` | 已经写好 `DesktopCodexAuthSource::CloudManaged`、3 个 HTTP 路由、前端切换方案 | **只写了，没落地**——这次要做完 |

## 2. 源文档要点映射（Karpathy → ClawWiki）

| 原文理念 | 落地到 ClawWiki |
|---|---|
| 三层结构：Raw / Wiki / Schema | IA 直接用这三个顶级导航 |
| 人策展、AI 维护 | 所有 Wiki 页面都是 **LLM 主笔 + 人复核**。每次入库自动触发维护 session（读源 → 写摘要/概念/人物/对比 → 更新索引 → 打冲突 → 写 changelog） |
| Wiki 层不是临时检索，是**持续演化** | 维护任务沉淀到 **Inbox**（一个 TaskList）供用户审阅 |
| Schema 是真正决定质量的东西 | 暴露 `schema/` 编辑器：`CLAUDE.md`、`AGENTS.md`、`templates/*.md`、`policies/*.md` |
| "认知复利" = 个人资产 | `Graph` 页呈现概念之间的连接、热度、陈旧度；"我的认知资产"在仪表盘量化 |
| 认知操作系统升级 | 对外的入口 = **Ask**：提问即生成/更新页面；AI 的输出最终落地为页面 asset，不是一次性回答 |

## 3. 产品定位

> **ClawWiki = 认知资产 OS + Codex Token Broker**
>
> - 一份由我策展、由 Claude/Codex 持续维护、越用越强的**个人/小团队 Wiki**；
> - 同时是一个 **本地 Token Broker**，把订阅得到的 Codex 账号稳定供给 Claude Code Desktop 和其它兼容客户端。

两条主线互相独立，但共享一个 Tauri 应用壳：
- 主线 A（**Wiki**）：用户日常"喂资料—提问—审计维护"的地方
- 主线 B（**Broker**）：用户的订阅 + 本地代理 + 外部 CLI 拉起

## 4. 关键决策

### 决策 1 — **不**严格复刻 Claude Code Desktop

现状 `desktop-shell/src/shell/TabBar.tsx` 明确写着 "Dual-row top bar — Claude Code desktop style"，产品叙事靠死了复刻。但是：

1. 用户手里本来就有 Claude Code Desktop。再给一份等价 UI，是"多一个入口 × 少一个理由"的反向操作。
2. Karpathy 文档强调的是 **维护** 而不是 **编程会话**。Wiki 的 core loop 是 `curate → question → maintain`，不是 `read → edit → bash`。死抠 Claude Code Desktop 的导航反而把用户逼回会话范式。
3. Token Broker 的目标客户 = 外部 Claude Code Desktop。如果我们自己也 replicate，会出现"用户同时打开两个长得一样的应用"的荒诞场面。

**保留的那 10%**：把 `session-workbench/*`（`SessionWorkbenchTerminal` + `InputBar` + `MessageItem` + `PermissionDialog` + `StatusLine` + `SubagentPanel`）整体搬到 **Ask** 页面，重命名为 "Ask Claude"。提问器的视觉骨架、权限弹窗、流式 message 全部继承，但挂载的 tool 集合从 `Bash/Edit/Read/Write` 换成 Wiki 定制 tool（`read_source`、`write_page`、`link_pages`、`mark_conflict`、`deprecate_page`、`touch_changelog`）。

### 决策 2 — Token 供给走 **Broker**，不再走"内嵌 CLI 面板"

- 废除 `/code` 路由作为顶级一级页；
- `CodeToolsPage` 的功能并入 `Settings → Token Broker` 子页，并改成"面向外部客户端的启动器"：
  - 顶部 "Broker Running ⬤ http://127.0.0.1:4357/v1"
  - 中间 Accounts 表（cloud-managed 只读 + user-imported 可编辑）
  - 下面 "Launch Claude Code Desktop with injected env" 按钮（仍然调 `runCodeTool`，但 cliTool 固定为 `claude-code`，注入环境变量 `ANTHROPIC_BASE_URL=http://127.0.0.1:4357/v1` + `ANTHROPIC_AUTH_TOKEN=<broker short-lived jwt>`）
  - 提示 "You can also point any OpenAI/Anthropic-compatible client at the local URL"，给出复制按钮

### 决策 3 — 完成 `cloud-managed-integration.md` 里规划的 Rust 工作

这是决策 2 的前置条件。需要在 **`claw-code-parity`** 上游或者 downstream `rust/crates/desktop-core` 新加：

```rust
// rust/crates/desktop-core/src/managed_auth.rs
pub enum DesktopCodexAuthSource { LocalOAuth, ImportedFile, CloudManaged }

pub struct DesktopCodexInstallationRecord {
    // ... 现有字段
    pub cloud_subscription_id: Option<i64>,
    pub cloud_account_id: Option<i64>,
}

impl DesktopManagedAuthRuntimeClient {
    pub async fn import_cloud_accounts(&self, accounts: Vec<CloudAccountInput>);
    pub async fn list_cloud_accounts(&self) -> Vec<DesktopCodexInstallationRecord>;
    pub async fn clear_cloud_accounts(&self);
}
// delete_codex_profile / remove_account 拒绝 source == CloudManaged
```

然后 `desktop-server` 新增：

```
POST /api/desktop/cloud/codex-accounts/sync
GET  /api/desktop/cloud/codex-accounts
POST /api/desktop/cloud/codex-accounts/clear

# 新增 Broker 代理
POST /v1/chat/completions          -> round-robin cloud accounts, refresh via trade
POST /v1/messages                  -> Anthropic 兼容形态
GET  /v1/models                    -> 聚合 provider 目录
GET  /api/broker/status            -> 健康 / 配额 / 最后刷新时间
POST /api/broker/launch-client     -> 拉起外部 CLI + 注入 env，等价旧 runCodeTool
```

前端 `features/billing/cloud-accounts-sync.ts` 从"自己写 JSON"改成 `desktopTransport.post('/api/desktop/cloud/codex-accounts/sync', {...})`；`CloudAccountsPanel.tsx` 变成一个读态面板（数据源切换为 Rust）。

## 5. 信息架构（IA）

```
ClawWiki
├── Home                Dashboard：资产量、陈旧页、冲突告警、今日进账、Ask 快捷输入
├── Raw                 原始资料（上传 / 列表 / 源详情 / 再索引）
├── Wiki                LLM 维护的页面网络
│   ├── Concepts          概念页
│   ├── People            人物页
│   ├── Topics            主题/专题页
│   ├── Compare           对比分析页
│   └── Changelog         变更日志
├── Ask                 对话式维护器（复用 session-workbench）
├── Graph               知识图谱：节点=页面，边=backlink；热度/陈旧度染色
├── Schema              规则编辑器：CLAUDE.md / AGENTS.md / templates / policies
├── Inbox               待办维护任务（AI 建议 + 用户手动建）
└── Settings
    ├── Account              个人信息 / 登出
    ├── Billing              套餐 + 订单 + 订阅状态
    ├── Token Broker         本 App 的 B 线主入口：Broker 状态 + Accounts + Launch CCD
    ├── Providers            外部 Provider 目录（Codex / Qwen）
    ├── MCP                  MCP 服务列表
    ├── Permissions          桌面权限
    ├── Data                 数据目录 / 导出 / 重置
    └── About
```

顶部不再有双层 TabBar，只保留一行 36px 的 window chrome（traffic light + workspace 名 + theme toggle）。左侧 220px Sidebar（DeepTutor 风格），折叠到 56px。

## 6. 视觉系统

| 维度 | 取值 | 出处 |
|---|---|---|
| 字体正文 | `Plus Jakarta Sans`, system-ui | DeepTutor `layout.tsx` |
| 字体阅读 | `Lora` (serif) — 用于 Wiki 页正文和 Schema 编辑器 | DeepTutor |
| 字体代码 | `JetBrains Mono` — Ask 终端、Schema 的 code block | 新增 |
| Light 背景 | `#FAF9F6` (bg) / `#FFFFFF` (card) / `#F0EDE7` (muted) | DeepTutor |
| Dark 背景 | `#1A1918` (bg) / `#242220` (card) / `#2A2725` (muted) | DeepTutor |
| Primary | `#C35A2C` (light) / `#D4734B` (dark) — "烧陶橙" | DeepTutor |
| Border | `#E8E4DE` / `#3A3634` | DeepTutor |
| 圆角 | Card 16px，Chip 9999px，Button 8px | 微调 |
| 卡片规范 | `surface-card` = `rounded-2xl border bg-card shadow-sm` | DeepTutor |
| Icon 栈 | `lucide-react` | 两仓库共用 |

## 7. Token 供给链路（端到端）

```
┌───────────────────────┐
│ 用户在 ClawWiki 买套餐 │
└───────────┬───────────┘
            ▼
  trade-service 下发
  cloud codex accounts
            │
            ▼
 billing-store.loadCloudAccounts()  ─┐
                                     │
                                     ▼
                     desktopTransport.post
                     /api/desktop/cloud/codex-accounts/sync
                                     │
                                     ▼
                  rust desktop-core::managed_auth
                  (新 CloudManaged source, 持久化到 OS keychain)
                                     │
        ┌────────────────────────────┼────────────────────────────┐
        ▼                            ▼                            ▼
  Ask 页面 Tool Loop           Broker 本地代理             Launch Claude Code Desktop
  （ClawWiki 自己用）           127.0.0.1:4357/v1           注入 ANTHROPIC_BASE_URL
                                 轮询 / 刷新 / 配额          + ANTHROPIC_AUTH_TOKEN
                                     │
                                     ▼
                        兼容 Anthropic / OpenAI 的
                        外部客户端（CCD / Cursor / CLI）
```

关键点：
- **外部 Claude Code Desktop 永远看不到用户的原始 refresh_token**，只看到本地 Broker 的短期 session token；
- **Broker 只能本机访问**（127.0.0.1 bind），绝不暴露 LAN；
- **帐号退订/过期**：`billing-store.resetPaymentDialog` → `POST /cloud/codex-accounts/clear`，立刻从 managed_auth registry 踢出，Broker 下一轮请求就收到 401。

## 8. Ask 会话：Wiki 维护型工具集

沿用 `SessionWorkbenchTerminal` 骨架，但绑定的工具要换：

| Tool | 作用 | 落地方式 |
|---|---|---|
| `read_source(id)` | 读 Raw 层原始资料（PDF/markdown/clipping）| 走 `desktop-server /api/wiki/raw/:id` |
| `read_page(slug)` | 读 Wiki 页 | 同上 `/api/wiki/page/:slug` |
| `write_page(slug, frontmatter, body)` | 创建/覆盖 Wiki 页（按 Schema 校验）| `PUT /api/wiki/page/:slug` |
| `patch_page(slug, diff)` | 局部补丁 | `PATCH /api/wiki/page/:slug` |
| `link_pages(src, dst, relation)` | 写 backlink | `/api/wiki/links` |
| `mark_conflict(slug_a, slug_b, reason)` | 建冲突记录 | Inbox 入一条 task |
| `touch_changelog(entry)` | Append 变更日志 | `/api/wiki/changelog` |
| `deprecate_page(slug, replacement)` | 标记废弃 | 前端 Wiki list 显示删除线 |
| `search(query)` | 全文/向量检索 | `/api/wiki/search` |
| `ingest_source(path|url)` | 入库 + 触发自动维护 session | `/api/wiki/raw/ingest` |

权限分层依然用 `PermissionDialog`：
- Low：`read_*`, `search`
- Medium：`write_page`, `patch_page`, `link_pages`, `touch_changelog`
- High：`deprecate_page`, `ingest_source`（批量外部内容拉取）

## 9. Rust / 前端模块重映射

| 现有模块 | 动作 |
|---|---|
| `features/auth/*` | **保留** |
| `features/billing/*` | **保留**，`cloud-accounts-sync.ts` 改走 Rust endpoint |
| `features/session-workbench/*` | **搬迁** → `features/ask/*`，工具集换绑 |
| `features/code-tools/*` + `/code` | **并入** `features/settings/sections/TokenBrokerSettings.tsx`；`runCodeTool` 只剩 "启动外部 CCD" 一个用例 |
| `features/apps/*` (MinApp gallery) | **下架 MVP**，归档到 `features/_parked/apps/` |
| `features/workbench/HomePage.tsx` | **重写** 为 Wiki Dashboard |
| **新增** | `features/raw/*`、`features/wiki/*`、`features/schema/*`、`features/graph/*`、`features/inbox/*`、`features/broker/*`（作为 Settings 子页） |
| `rust/desktop-core::managed_auth` | **扩展** CloudManaged source + import/list/clear |
| `rust/desktop-core` | **新增** `wiki_store` (FS-backed page store, 类 `~/.clawwiki/wiki/`), `wiki_maintainer` (agent loop), `broker` (proxy + refresh) |
| `rust/desktop-server` | **新增** `/api/wiki/*`, `/api/desktop/cloud/codex-accounts/*`, `/v1/*` 代理, `/api/broker/*` |

## 10. MVP 迭代顺序

1. **Broker 基础**（1 个 sprint）
   - 打通 `cloud-managed-integration.md` 的 3 个 HTTP 路由
   - managed_auth 新 source 落地
   - `Settings → Token Broker` 只读面板
2. **Broker 代理**（1 个 sprint）
   - `/v1/chat/completions` + `/v1/messages` 代理
   - `launch-client` 把 CCD 拉起并注入 env
   - 健康面板
3. **Raw / Wiki 最小能力**（1~2 个 sprint）
   - 文件 / URL / markdown clipping 入库到 `~/.clawwiki/raw/`
   - 手写一个默认 CLAUDE.md / AGENTS.md / templates
   - 一次维护 session 由后端自动触发（不必漂亮，先能跑）
4. **Ask 页**（1 个 sprint）
   - 复用 `session-workbench`，换 tool 集
   - Permission 弹窗复用
5. **Schema 编辑器 + Inbox**（1 个 sprint）
6. **Graph 视图**（1 个 sprint）
7. **Dashboard 指标化**、陈旧度检测、冲突自动扫描（持续）

## 11. 风险 & 留白

- **Broker 代理的刷新语义**：必须在 `managed_auth.rs` 里保证 access_token 只在 Rust 内部刷新，不回吐前端；目前 `cloud-accounts-sync.ts` 还把明文 access/refresh token 放在前端 JSON，这一步要随迁移一起清理掉（安全收敛）。
- **Wiki store 的冲突机制**：Schema 层的 policies 还没定。第一版手写规则，之后再做 agent 协商。
- **与 `claw-code-parity` 的上下游关系**：`managed_auth` 改动到底放 upstream 还是 downstream，在 `docs/open-claude-code-parity-dependency-design.md` 的边界里权衡。建议 Cloud source 放 downstream（ClawWiki 特有），不污染 parity 的"通用 Claude Code 行为"。
- **不再复刻 Claude Code Desktop** 意味着当前 `PARITY.md` 里若干 parity gap（`/agents`, `/hooks`, `/plugin`, `/mcp` 之类）就彻底不是 ClawWiki 的目标了——只有 Ask 页会用到部分 runtime 能力，可以从 claw-code-parity 按需拉。

## 12. 线框图索引

HTML 文件：[`./wireframes.html`](./wireframes.html)

| # | Screen | 对应代码位置 |
|---|---|---|
| 01 | Login | `features/auth/LoginPage.tsx`（保留） |
| 02 | Home / Dashboard | `features/workbench/HomePage.tsx`（重写） |
| 03 | Raw Library | **新** `features/raw/RawLibraryPage.tsx` |
| 04 | Wiki Explorer | **新** `features/wiki/WikiExplorerPage.tsx` |
| 05 | Wiki Page Detail | **新** `features/wiki/WikiPageDetail.tsx` |
| 06 | Ask (Claude session) | `features/session-workbench/*`（搬到 `features/ask/*`） |
| 07 | Graph | **新** `features/graph/GraphPage.tsx` |
| 08 | Schema Editor | **新** `features/schema/SchemaEditorPage.tsx` |
| 09 | Inbox | **新** `features/inbox/InboxPage.tsx` |
| 10 | Settings → Token Broker | `features/code-tools/*` 重写 + `features/billing/CloudAccountsPanel.tsx` 并入 |

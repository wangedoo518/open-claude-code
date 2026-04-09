# ClaudeWiki 产品设计方案

> 面向仓库 `Warwolf/claudewiki` 的正式版产品方案
>
> 目标：把《当知识开始自己生长：Karpathy开源个人LLM Wiki》的核心思想，落成一个以 `Claude Code Desktop` 复刻交互为底座、以 `Wiki` 为主产品、以 `Codex Token Broker` 为基础设施的桌面产品。

## 1. 结论先行

### 1.1 产品定义

**ClaudeWiki 不是“另一个 Claude Code Desktop”。**

它应该是三件事的组合：

1. **认知资产操作系统**
   - 把微信文章、语音、PPT、视频、网页、笔记等持续沉淀成可维护的 Wiki。
2. **桌面控制台**
   - 保留 Claude Code Desktop 式的工作台交互、流式会话、权限确认、任务审阅体验。
3. **Codex Token Broker**
   - 把平台分配的 Codex 账号稳定供给外部 `Claude Code Desktop`、CLI、Cursor 等客户端。

### 1.2 是否严格复刻 Claude Code Desktop

**决策：不做“页面级严格复刻”，做“交互骨架保留 + 信息架构重做”。**

保留：

- 窗口 chrome、桌面壳氛围
- 左侧工作台导航密度
- 对话式 Ask 工作区
- 流式消息、权限弹窗、状态线、任务侧栏
- 桌面本地服务 + 本机账号/runtime 绑定模型

不保留：

- 以 `/home /apps /code` 为中心的 IA
- 双层 TabBar 作为全局主导航
- “应用画廊 / MinApp” 作为主产品叙事
- “内嵌 CLI 面板” 作为 Codex 供给的主要形态

**一句话解释**：用户要的是“Claude Code Desktop 的熟悉感”，不是“第二个一模一样的 Claude Code Desktop”。

## 2. 源码现状分析

## 2.1 `claudewiki` 当前到底是什么

从源码看，当前仓库更接近：

- 一个 **Claude Code Desktop 风格的桌面壳**
- 一个 **本地桌面服务 + OAuth/managed auth 运行时**
- 一个 **平台账号分发面板**

而不是一个真正的 Wiki 产品。

关键证据：

- `apps/desktop-shell/src/shell/AppShell.tsx`
  - 顶层主路由仍是 `/home`、`/apps`、`/apps/:id`、`/code`。
- `apps/desktop-shell/src/shell/TabBar.tsx`
  - 注释直接写明 `Dual-row top bar — Claude Code desktop style`。
- `apps/desktop-shell/src/features/workbench/HomePage.tsx`
  - 首页是会话、Search、Scheduled、Dispatch、Customize 的 Claude Code 工作台逻辑。
- `apps/desktop-shell/src/features/session-workbench/*`
  - 这一套消息流、输入、权限、状态、子代理 UI 已经很完整，适合复用为 Ask 页。
- `apps/desktop-shell/src/features/code-tools/CodeToolsPage.tsx`
  - 当前是在桌面内选择 CLI/模型/目录并 `runCodeTool` 拉外部工具，更像 CLI 启动器。
- `apps/desktop-shell/src/features/billing/api.ts`
  - 已接入 `/api/v1/codex-accounts/me`。
- `apps/desktop-shell/src/features/billing/cloud-accounts-sync.ts`
  - 当前把 cloud codex 账号明文写到 `plugin-store` / `localStorage`。
- `apps/desktop-shell/src/features/billing/CloudAccountsPanel.tsx`
  - 展示的是“云端托管的 Codex 账号”，但仍是前端层概念。
- `rust/crates/desktop-core/src/managed_auth.rs`
  - 当前 `DesktopManagedAuthSource` 只有 `imported_auth_json` / `browser_login` / `device_code`，没有 `cloud_managed`。
- `rust/crates/desktop-server/src/lib.rs`
  - 目前有 `/api/desktop/auth/providers/*` 和 code-tools bridge，但没有云账号同步路由，也没有通用 `/v1` broker。

### 2.2 这意味着什么

这说明现有仓库的可复用资产主要有三类：

1. **桌面壳与交互资产**
2. **本地认证 / runtime / provider 资产**
3. **会话式 agent 工作台资产**

真正缺失的是：

1. **Wiki 的产品信息架构**
2. **Raw → Wiki → Schema 的编译型知识流水线**
3. **WeChat 作为主入口的采集与标准化体系**
4. **把 Codex token 作为本机 Broker 暴露给外部客户端的闭环**

## 3. Karpathy 方法论落地要点

Karpathy 的 gist 真正重要的不是“用 Obsidian 存 markdown”，而是下面四条：

1. **Raw 是不可变事实层**
   - 原始文章、PDF、图片、音频、视频、PPT 都只读保存。
2. **Wiki 是持续演化层**
   - 每次新增资料，不只是新增摘要，而是重写相关页、索引页、比较页、冲突页。
3. **Schema 是纪律层**
   - `CLAUDE.md` / `AGENTS.md` / 模板 / 更新规则决定 Wiki 能否长期稳定。
4. **Query 不是一次性问答**
   - 好问题产出的答案，应该能回写成新的 Wiki 页面或变更记录。

ClaudeWiki 必须完整保留这四层，而不是只拿 “聊天 + 搜索”。

### 3.1 Karpathy 推文传达的产品信号

虽然 X 原帖不适合当实现文档，但它释放了两个非常明确的信号：

1. **这是产品机会，不只是个人技巧**
   - Karpathy 不是在讨论“如何更方便做笔记”，而是在指出“显式、可导航、文件优先的 agent memory”本身就是一条新产品线。
2. **文件优先、显式可审计，比黑盒画像更重要**
   - 本地 markdown wiki 让用户拥有可检查、可迁移、可重写的“自我知识层”，而不是把个性化交给平台在黑盒里慢慢猜。

这直接决定 ClaudeWiki 的产品路线应该是：

- **以本地文件和 markdown 为资产边界**
- **以 agent 可读、可维护、可审计为核心卖点**
- **以多模型 / 多客户端可复用为生态方向**

### 3.2 Farza 案例：从“研究/资料库”扩展到“人生数据编译”

Farza 的公开案例非常重要，因为它证明了这套方法不只适用于论文、文章，也适用于“生活级原始材料”。

从其公开落地页与公开转述信息看，Farza 做的事情大致是：

- 把约 **2500 条** 日记、Apple Notes 和 iMessage 对话喂给 LLM
- 生成约 **400 篇** 互相链接的个人 wiki 文章
- 覆盖朋友、创业经历、研究兴趣、个人偏好等主题
- 目标不是给人手工阅读，而是让 agent 可以沿着 `index.md` 和 backlinks 去爬取上下文

这对 ClaudeWiki 的启发非常直接：

1. **微信入口必须是第一类公民**
   - 在中文语境里，微信就是最接近日记、聊天、文章分享、语音碎片、文件流转的“生活数据总线”。
2. **Raw 不该只支持“知识文章”，也要支持“生活/工作过程材料”**
   - 语音、聊天片段、会议纪要、PPT、录屏都必须是原生对象。
3. **最终产品不只是“研究 wiki”，而是“个人/团队认知操作系统”**
   - 这和 Farzapedia 的产品化方向是一致的。

对 Farza 案例的判断可以总结成一句话：

> Karpathy 证明了方法论成立，Farza 证明了“生活级、人格级、长期记忆级”的产品想象也成立。

## 4. 参考项目分析与借鉴边界

## 4.1 `DeepTutor`：借视觉系统与壳层，不借产品叙事

`DeepTutor` 对本项目最有价值的不是业务能力，而是 UI 语言：

- `web/app/layout.tsx`
  - 使用 `Plus Jakarta Sans` + `Lora`
- `web/app/globals.css`
  - 暖色系 token：
    - `--background: #FAF9F6`
    - `--primary: #C35A2C`
    - `--card: #FFFFFF`
    - `--border: #E8E4DE`
  - 暗色也已经配套
  - `surface-card` 的圆角卡片体系成熟
- `web/components/sidebar/SidebarShell.tsx`
  - 220px/56px 的可折叠侧栏、轻密度导航、低压迫感，很适合知识类桌面产品

**借鉴结论**：

- ClaudeWiki 应直接采用 `DeepTutor` 的暖色 token、字体、卡片和侧栏结构。
- 不应该继续沿用“纯终端灰黑感”的 Claude Code Desktop 全页视觉。

## 4.2 `sage-wiki`：借“编译型 Wiki 系统”，不借“独立 wiki 应用替代桌面壳”

`sage-wiki` 的价值非常高，尤其适合 ClaudeWiki 的知识内核设计：

- `README.md`
  - 明确是 `sources in, wiki out`
  - 支持 vault overlay，可直接与 Obsidian 共用
  - 内置 hybrid search、graph、MCP、web UI
- `internal/compiler/pipeline.go`
  - 已经是完整的 compiler pipeline：diff → summarize → extract concepts → write
- `internal/extract/extract.go`
  - 已支持 markdown / PDF / Office / CSV / EPUB / email / image / code
- `internal/query/query.go`
  - 不是简单检索，而是 hybrid search + ontology traversal + synthesis
- `docs/guides/mcp-knowledge-capture.md`
  - 很好地说明了“对话中的知识如何被 capture 回 wiki”

**借鉴结论**：

- ClaudeWiki 的后端知识流水线可以直接按 `sage-wiki` 的范式来设计。
- “Obsidian 兼容 markdown + 可覆盖到 vault” 应成为第一原则。
- `MCP` 不是可选装饰，而应成为 Ask 页与外部 agent 的统一写入接口。

## 4.3 `engram`：借“轻量、持久、agent-first”，不借“能力过轻”

`engram` 的优势：

- `wiki/store.py`
  - 纯 markdown 文件管理、`index.md` 重建、`log.md` 追加都很清晰
- `core/ingest.py`
  - “保存 raw，再按当前 wiki 状态决定更新哪些页”的思路很对
- `sources/url.py`
  - 自带 SSRF 保护，说明外部 URL ingest 必须有安全边界

`engram` 的限制：

- 默认搜索仍偏轻量
- 文件类型支持没有 `sage-wiki` 宽
- 更像“agent memory CLI”，不是桌面产品

**借鉴结论**：

- ClaudeWiki 应保留 engram 的文件即资产、日志即时间线、slug 安全、URL 安全这些约束。
- 但检索、媒体处理、审阅工作台、多人/多来源 intake 需要比 engram 更重。

## 4.4 `defuddle` / `obsidian-clipper` / `obsidian-importer`

### `defuddle`

适合做：

- 微信公众号文章页、普通网页、博客文章的 HTML 清洗
- 标准化脚注、代码块、数学公式、callout
- 输出 clean HTML 或 Markdown

不适合独立承担：

- 微信消息总线
- 富媒体编排
- 附件下载与 vault 路径管理

### `obsidian-clipper`

适合借鉴：

- `defuddle + template compiler` 的剪藏链路
- 模板变量、frontmatter 生成、剪藏规则
- 作为桌面浏览器辅助入口，让用户把网页直接送进 ClaudeWiki raw 层

不适合直接复用为后端主链路：

- 它本质是浏览器扩展
- 假设大量状态存在浏览器环境
- 不承担微信场景下的服务端批处理

### `obsidian-importer`

适合借鉴：

- HTML → Markdown import
- 附件重写
- 从外部工具或历史资产批量导入到 vault

不适合直接做实时 intake：

- 它是“导入器”，不是“持续 ingestion pipeline”
- 更适合作为补充工具，而非 ClaudeWiki 的实时主引擎

### 工具链决策

**结论：可以接。**

但接法应该是：

1. **微信公众号 / 网页链接**
   - `fetch html` → `defuddle` → `normalized markdown`
2. **浏览器手动剪藏**
   - 可选接入 `obsidian-clipper` 或参考其模板能力
3. **历史资产 / HTML 导出 / 第三方知识库**
   - 通过 `obsidian-importer` 的格式转换能力做一次性批量导入

而不是：

- 直接把 `obsidian-clipper` 当成微信 ingest 服务
- 直接把 `obsidian-importer` 当成在线消息处理引擎

## 5. 微信入口设计：ClaudeWiki 的主 Raw 通道

## 5.1 产品原则

**微信不是聊天前端，而是 ClaudeWiki 的“随手投喂入口”。**

用户在微信里做的事情只有一类：

- 把值得长期积累的材料扔进系统

包括：

- 分享公众号文章
- 转发网页链接
- 发一段语音
- 上传 PPT / PDF / 文档
- 上传视频
- 转发聊天记录片段

ClaudeWiki 要做的事情是：

- 把这些材料收为 Raw
- 自动标准化
- 自动编译进 Wiki
- 再把待审阅结果回到桌面控制台

## 5.2 微信 Raw 分层

建议把 Raw 分为三层：

### Layer A：原始对象层

只读保存原件：

- 原始 HTML
- 原始音频
- 原始视频
- 原始 PPT/PDF
- 原始图片
- 原始消息 JSON

### Layer B：标准化表示层

统一转成可被 LLM 编译的中间表示：

- `article.md`
- `transcript.md`
- `slides.md`
- `video-notes.md`
- `asset-manifest.json`

### Layer C：编译任务层

为每个 raw item 生成一条 compile job：

- 目标页候选
- 预计受影响的 wiki 页面
- 冲突检测
- 建议标签 / 人物 / 主题
- 风险等级

## 5.3 各类素材的处理路径

### 公众号文章 / 普通网页

路线：

`share url` → `抓取 HTML` → `defuddle 清洗` → `markdown + metadata` → `入 raw/articles`

### 语音

路线：

`audio` → `ASR 转写` → `结构化 transcript` → `提取主题/结论/待验证点` → `入 raw/audio`

原则：

- GPT-5.4 负责判断、归纳、冲突比较、写 wiki
- ASR 交给专门转写阶段，不让通用大模型直接吃原始音频成为主链路

### PPT / PDF

路线：

`file` → `提取文本/备注/页结构/图片` → `slides.md` + `assets` → `入 raw/docs`

### 视频

路线：

`video` → `抽音轨 + ASR` + `关键帧抽样 + OCR` → `video-notes.md` → `入 raw/video`

### 微信聊天片段

路线：

`message bundle` → `去噪` → `按主题聚合` → `conversation-note.md` → `入 raw/conversations`

## 6. ClaudeWiki 正式产品定义

## 6.1 双层产品结构

### A. Wiki Control Plane

给用户看的主产品：

- Dashboard
- Raw Inbox
- Wiki Explorer
- Ask
- Graph
- Schema
- Inbox / Review
- Obsidian Sync

### B. Local Infrastructure Plane

用户依赖但不每天盯着看的基础设施：

- Token Broker
- Provider / Managed Auth
- Local storage
- Background compile workers
- Sync / export / MCP

## 6.2 用户心智模型

用户不需要理解“很多模块”。

只需要理解三个动作：

1. **丢材料进来**
   - 来自微信、网页、文件、聊天
2. **看系统怎么长**
   - 原始资料如何变成 wiki 页面与关联
3. **随时提问与修订**
   - Ask 页面让系统继续维护和推演

## 7. 产品信息架构

```text
ClaudeWiki
├── Dashboard
├── WeChat / Intake
├── Raw Library
├── Wiki
│   ├── Overview
│   ├── Concepts
│   ├── People
│   ├── Topics
│   ├── Compare
│   └── Changelog
├── Ask
├── Graph
├── Schema
├── Inbox
├── Obsidian
└── Settings
    ├── Account
    ├── Billing
    ├── Token Broker
    ├── Providers
    ├── MCP
    ├── Permissions
    ├── Data
    └── About
```

## 8. 桌面页面策略

## 8.1 要保留的 Claude Code Desktop 复刻交互

建议保留这些“熟悉感资产”：

- 桌面窗口 chrome
- 左侧紧凑导航
- Ask 页的对话区、输入区、权限确认、状态线
- 任务审阅 / 执行中 / 已完成的工作流节奏
- 本地服务驱动的账户和 provider 面板

## 8.2 必须替换掉的页面结构

建议替换：

- 顶部双层 TabBar
- `/apps` 应用画廊
- `/code` 一级页
- 以“会话列表”代替“知识资产总览”的首页

## 8.3 最终页面决策

最终方案应是：

- **首页像 DeepTutor 的知识工作台**
- **Ask 页像 Claude Code Desktop 的工作区**
- **Settings > Token Broker 像桌面开发者工具**

这比“所有页面都像 Claude Code Desktop”更合理。

## 9. 核心页面设计

## 9.1 Dashboard

展示：

- 今日新增 raw
- 今日编译结果
- 冲突页数量
- 待审阅任务
- 高价值主题
- 最近从微信进入的素材
- Ask 快捷入口

## 9.2 WeChat / Intake

展示：

- 最近收到的文章、语音、视频、文件
- 当前转换状态
- 失败原因
- 一键重试
- “送去编译” 与 “仅收为 raw”

## 9.3 Raw Library

展示：

- 所有原始资料
- 来源、格式、时间、标签、关联 wiki 页
- 标准化结果预览
- 原件与 markdown 对照

## 9.4 Wiki Explorer

展示：

- 目录树 / 分类视图
- 最近更新
- stale / conflict / needs review 标记
- 与 raw 的来源链路

## 9.5 Wiki Page Detail

展示：

- 页面正文
- 来源列表
- 最近变更
- 相关页
- 冲突提示
- Ask this page

## 9.6 Ask

这是 Claude Code Desktop 资产复用的核心页。

工具集从“编程工具”切成“知识维护工具”：

- `read_raw`
- `read_page`
- `write_page`
- `patch_page`
- `link_pages`
- `mark_conflict`
- `touch_changelog`
- `search_wiki`
- `queue_compile`

## 9.7 Graph

展示：

- 页面节点
- raw source 节点
- conflict 高亮
- stale 高亮
- 高活跃主题 cluster

## 9.8 Schema

展示并编辑：

- `CLAUDE.md`
- `AGENTS.md`
- page templates
- ingest policies
- review policies
- naming conventions

## 9.9 Inbox

聚合：

- AI 建议的待审阅修改
- 失败的 ingestion
- 冲突待确认
- stale page recheck
- 高价值问答待回写

## 9.10 Token Broker

展示：

- Broker 状态
- 本地 endpoint
- 已同步 cloud accounts
- provider 健康
- 启动 Claude Code Desktop / CLI 的入口
- 外部客户端环境变量复制

## 9.11 Obsidian 协同

展示：

- 当前 vault 路径
- overlay 模式 / 独立仓库模式
- 最近同步
- 打开到 Obsidian
- markdown 兼容检查

## 10. 数据结构建议

```text
ClaudeWiki/
├── raw/
│   ├── inbox/
│   │   ├── wechat/
│   │   ├── browser/
│   │   └── manual/
│   ├── articles/
│   ├── audio/
│   ├── video/
│   ├── docs/
│   ├── conversations/
│   └── assets/
├── normalized/
│   ├── articles/
│   ├── transcripts/
│   ├── slides/
│   └── video-notes/
├── wiki/
│   ├── concepts/
│   ├── people/
│   ├── topics/
│   ├── compare/
│   ├── outputs/
│   ├── index.md
│   └── log.md
├── schema/
│   ├── CLAUDE.md
│   ├── AGENTS.md
│   ├── templates/
│   └── policies/
├── inbox/
└── broker/
```

## 11. Token Broker 设计

## 11.1 目标

不是“让用户在 ClaudeWiki 里跑代码工具”，而是：

- **把 Codex 供给能力做成桌面本机基础设施**

## 11.2 推荐路由

本地服务新增：

- `POST /api/desktop/cloud/codex-accounts/sync`
- `GET /api/desktop/cloud/codex-accounts`
- `POST /api/desktop/cloud/codex-accounts/clear`
- `GET /api/broker/status`
- `POST /api/broker/launch-client`
- `GET /v1/models`
- `POST /v1/chat/completions`
- `POST /v1/messages`

## 11.3 安全原则

- 只绑定 `127.0.0.1`
- refresh token 不出 Rust 层
- 前端只拿摘要，不拿长期凭据
- cloud-managed 账号不可在 UI 里直接删除或编辑
- 退订后立刻 `clear`

## 12. Obsidian 与 ClaudeWiki 的关系

建议产品定位为：

- **Obsidian = 阅读与人工编辑 IDE**
- **ClaudeWiki = ingestion / compile / review / broker 控制台**

两者不是替代关系，而是协同关系。

推荐支持两种模式：

1. **Vault Overlay**
   - 直接把 `wiki/`、`schema/`、`log.md` 落在 Obsidian vault 中
2. **Managed Workspace**
   - ClaudeWiki 自己管理目录，但一键在 Obsidian 中打开

优先推荐 `Vault Overlay`，因为这最符合 Karpathy 的原始工作方式。

## 13. MVP 范围

### Phase 1：基础设施先行

- 云端 codex account 同步到 Rust managed auth
- Token Broker 本地 endpoint
- Settings 中的新 Broker 面板

### Phase 2：最小 Wiki 可用

- WeChat / URL / 文件进入 raw
- 文章用 `defuddle`
- 文档/PDF/PPT 基础抽取
- Ask 页改造成 Wiki 维护器

### Phase 3：知识网络成形

- Wiki Explorer
- Page Detail
- Schema Editor
- Inbox review

### Phase 4：长期复利能力

- Graph
- stale/conflict lint
- 高价值问答回写
- Obsidian overlay 深化

## 14. 线框图文件

正式版 HTML 线框图见：

- `docs/clawwiki/wireframes.html`

旧草稿保留为：

- `docs/clawwiki/product-design-v1.md`
- `docs/clawwiki/wireframes-v1.html`

## 15. 参考依据

外部方法论：

- Karpathy gist：`https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f`
- `sage-wiki`：`https://github.com/xoai/sage-wiki`
- `engram`：`https://github.com/emipanelliok/engram`
- `defuddle`：`https://github.com/kepano/defuddle`
- `obsidian-clipper`：`https://github.com/obsidianmd/obsidian-clipper`
- `obsidian-importer`：`https://github.com/obsidianmd/obsidian-importer`
- Farza landing：`https://farza.com/knowledge`

本地源码依据：

- `apps/desktop-shell/src/shell/AppShell.tsx`
- `apps/desktop-shell/src/shell/TabBar.tsx`
- `apps/desktop-shell/src/features/workbench/HomePage.tsx`
- `apps/desktop-shell/src/features/session-workbench/*`
- `apps/desktop-shell/src/features/code-tools/CodeToolsPage.tsx`
- `apps/desktop-shell/src/features/billing/*`
- `rust/crates/desktop-core/src/managed_auth.rs`
- `rust/crates/desktop-server/src/lib.rs`
- `docs/desktop-shell/cloud-managed-integration.md`
- `DeepTutor/web/app/layout.tsx`
- `DeepTutor/web/app/globals.css`
- `DeepTutor/web/components/sidebar/SidebarShell.tsx`

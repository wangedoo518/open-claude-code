# ClawWiki 产品设计方案 · canonical

> 产品哲学：**放弃做"瑞士军刀"，打造一把"手术刀"。**
>
> **ClawWiki = 你的外脑。微信喂料 → AI 审阅 → 认知资产沉淀。**

---

## 前言 · 这是第 4 代最终稿

过去四次迭代（`_archive/` 里完整保留）：

| 代 | 核心立场 | 结局 |
|---|---|---|
| v1 | 不复刻 CCD；独立 Wiki 产品 + 对外 Broker | ❌ 被否，叙事不锐 |
| middle-path | 保留 CCD 氛围，换掉双行 TabBar/apps/code | ⚠️ 半对半错 |
| v2 | 完整保留 CCD + 加 Wiki Tab + Broker 对外 | ⚠️ 违背"内聚"原则 |
| v3 | Wiki-first + CCD 5 组件提取 + Broker 内聚 | ⚠️ 仍保留了部分"兼容外部"的语言 |
| **canonical** | **手术刀三刃：定位刃 · 交互刃 · 价值刃** | **本文** |

### canonical 相对 v3 的 4 处锐化

| 维度 | v3 | canonical（锐化后） |
|---|---|---|
| 产品叙事一句话 | "Wiki-first 的认知资产 OS" | **"你的外脑"** —— 用具象的身体隐喻替代抽象术语 |
| CCD 提取粒度 | 5 个组件 | **4 件套灵魂**：工作台 / 流式会话 / 权限确认 / 任务审阅 —— 对齐用户原话 |
| WeChat 地位 | "主要入口之一" | **唯一漏斗** · MVP 阶段不做任何其它 ingestion 通道 |
| Token Broker 对外 HTTP | "不暴露" | **不存在**。Broker 不是一个"可以收起来的 HTTP 服务"，它根本不是一个 HTTP 服务——它是 `desktop-core` 里的一个 Rust 模块 |
| 战略断腕 | 分散在各节 | **单独一节"我们不做什么"**，作为团队评审的第一条议题 |

---

## 1. 一句话定位

> **ClawWiki 是你的"外脑"。**
>
> 你在微信里的每一次随手转发、每一段语音、每一份 PPT、每一个视频，都会经过 AI 审阅和你的权限确认，被**结构化进一个由 Karpathy 式三层架构（Raw / Wiki / Schema）组成的认知资产库**。
>
> 这个库不是工具箱。它是你未来两年里提问、写作、决策时，AI 能直接调用的"长期记忆"。

---

## 2. 核心用户故事（端到端）

**场景**：周二下午，changpeng 在地铁上刷微信。

**2:14 PM** — 看到朋友转发的 "Karpathy 开源个人 LLM Wiki"。
→ 长按 → 转发 → **发给"ClawWiki 小助手"**（企业微信外部联系人机器人）
→ 3 秒后手机震动，机器人回 "✓ 已入库，正在维护 5 个相关页面"

**2:15 PM** — 一个同事发来一段 47 秒的语音，讨论 RAG 的局限。
→ 长按 → 转发 → ClawWiki 小助手
→ 机器人回 "✓ 已转写（420 字），已跟 concept/rag 对齐"

**2:16 PM** — 另一个群里有人发了一份 PPT：AI Products 2026 Q2 Roadmap.pptx。
→ 长按 → 转发 → ClawWiki 小助手
→ 机器人回 "✓ 已抽取 32 slides，正在跟 topic/ai-products 合并"

**7:30 PM** — changpeng 到家，打开 ClawWiki 桌面端。

- **Dashboard**：今日进账 14 条 · 维护了 23 个页面 · 有 3 个需要你审阅（1 个冲突、2 个新页面）
- **Inbox**（CCD 任务审阅模式）：点开 "合并冲突：Agentic Loop v1 ↔ v2"，看到 Maintainer AI 完整的 tool call 树、每步 diff、一个 approve/reject 按钮。**这就是 CCD SubagentPanel 的灵魂**。
- **Ask**（CCD 工作台 + 流式会话模式）：打一句 "结合我今天入库的这三份材料，帮我写一段'AI Wiki vs 传统 RAG'的判断"。看到 AI 流式吐出答案，中间要写 `compare/rag-vs-llm-wiki` 时弹出权限确认窗。**这是 CCD PermissionDialog 的灵魂**。

**两周后** — changpeng 要给 CEO 汇报"AI memory 这块最近的认知"。
→ 打开 Ask 输入 "这两周我所有关于 AI memory 的判断，结构化一下"
→ AI 基于 **你自己 14 天里喂进去的 60 份材料**（而不是重新爬一遍互联网）给出一份结构化回答
→ 这是 Karpathy 说的 **认知复利**：过去 14 天的 WeChat 随手转发 → 现在 30 秒就能调用

---

## 3. 战略断腕清单 · 我们**不**做什么

**这是团队评审的第一议题。没有对这 11 条达成共识，后面所有设计都不成立。**

| # | 砍掉的东西 | 为什么砍 |
|---|---|---|
| ❌ 1 | **Claude Code Desktop 的完整外壳**（双行 TabBar / session tab row 2） | 用户不需要第二个 CCD；产品形态必须 Wiki-native |
| ❌ 2 | **`/apps` MinApps 画廊** + MinAppDetailPage | MinApps 是 Cherry Studio 的叙事，不是我们的叙事 |
| ❌ 3 | **`/code` CLI 启动器**（`runCodeTool` + 终端选择器） | 我们不拉起外部 CLI |
| ❌ 4 | **对外暴露的 Token Broker `/v1` HTTP 路由** | Broker 只在 Rust 层，不是一个可以被 Cursor/CLI 消费的服务 |
| ❌ 5 | **"Launch Claude Code Desktop"按钮** | 砍 #4 的直接后果 |
| ❌ 6 | **Provider 目录管理**（用户自己添加 API key） | 订阅用户只看到 "Codex Pool"，不选模型不选 provider |
| ❌ 7 | **Obsidian Vault overlay / 双向同步** | 我们就是 wiki 本身，不是 Obsidian 的插件 |
| ❌ 8 | **团队多用户 / 共享工作区** | MVP 只做单人 |
| ❌ 9 | **移动端 app**（手机上 ClawWiki 客户端） | 手机上已经有微信了，不需要另一个 app |
| ❌ 10 | **"能总结 / 能转录 / 能记笔记"的工具箱叙事** | 不提 "summarize tool"，提 "AI 在后台替你把知识长好" |
| ❌ 11 | **手动 ingestion 作为主流程**（桌面"上传"按钮） | 只在 Raw Library 里留一个次级 "手动上传" fallback；主流量必须从微信来 |

**核心断腕逻辑**：

> **ClawWiki 不是 "一个带 Wiki 功能的 AI 客户端"，而是 "微信转发的归宿"。**
>
> 所有的交互动词里，用户做得最多的只有一个——**在微信里按"转发"**。其他一切都是这个动作的后果。

---

## 4. 手术刀的三刃 · 我们做什么

### 刃一 · 定位刃 · Token 100% 内部消化

```
用户买的不是"Codex 代理服务"，是"一个会替自己长大的外脑"。
Codex 账号是订阅附赠的燃料，不是产品本身。
```

- `trade-service` 给每个 active 订阅下发 5 个 Codex 账号
- 账号进入 `rust/desktop-core::managed_auth` 的 `CloudManaged` source
- `CodexBroker` 是一个 Rust struct，**不是**一个 HTTP 服务
- 只有两个消费者：`AskSession`（用户提问）和 `WikiMaintainer`（后台自动维护）
- 前端从头到尾拿不到 access_token，只能看到"池子还剩多少配额"这个数字
- 外部应用（Cursor / CLI / CCD / 第三方）**无法**从 127.0.0.1:4357 拿到任何 token

### 刃二 · 交互刃 · CCD 4 件套灵魂注入

**CCD 是模式库，不是外壳。** 扒开 Claude Code Desktop，只留 4 个最硬核的交互原语，注入到 Wiki-native 界面里：

| CCD 灵魂 | Wiki 中的注入点 | 对应的 ClawWiki 组件（新命名） |
|---|---|---|
| 🧰 **工作台**（sidebar + main pane + status line 三段式） | `/ask` 页面的骨架 | `features/ask/AskWorkbench.tsx` |
| 🌊 **流式会话**（VirtualizedMessageList + MessageItem + tool call card + StatusLine 底部） | `/ask` 主区的流式消息流；`/inbox` 里 Maintainer 的实时任务流 | `features/ask/AskStream.tsx` · `features/inbox/MaintainerStream.tsx` |
| 🛡 **权限确认**（PermissionDialog low/medium/high + "always allow") | 每个 `write_page` / `patch_page` / `deprecate_page` / `mark_conflict` 都要过 | `features/permission/WikiPermissionDialog.tsx` |
| 🌲 **任务审阅**（SubagentPanel 的 task tree + tool call 展开 + diff 预览） | `/inbox` 选中一个 Maintainer 任务后的详情面板 | `features/inbox/MaintainerTaskTree.tsx` |

**注意**：我们提取的是**交互模式**不是**视觉样式**。皮肤全部重上 DeepTutor 暖色（`#FAF9F6` / `#C35A2C` / Lora 衬线）。

### 刃三 · 价值刃 · Karpathy 三层 + WeChat 漏斗

```
┌─────────────────────────┐
│    微信（信息输入流）    │   ← 中国最强的内容 + 交互生态
│                         │
│   随手转发 / 发语音 /   │
│   扔 PPT / 上传视频      │
└────────────┬────────────┘
             │
             ▼
     ClawWiki 漏斗
             │
  ┌──────────┴──────────┐
  ▼                     ▼
Raw 层               Schema 层
（只读事实）        （CLAUDE.md + AGENTS.md + templates + policies）
  │                     │
  └──────────┬──────────┘
             ▼
          Wiki 层
     （LLM 持续维护）
             │
  ┌──────────┼──────────┐
  ▼          ▼          ▼
概念页    人物页    对比/变更日志
```

这就是 Karpathy 2026 年 4 月开源的方法论落地。五件维护动作（每次 ingest 必做）：

1. Summarise（≤ 200 words，quote ≤ 15 words，版权安全）
2. Update affected concept / people / topic / compare pages
3. Add / update bidirectional backlinks
4. Detect conflicts → `mark_conflict` → Inbox（要人来审）
5. Append to `changelog/YYYY-MM-DD.md` + 重建 `wiki/index.md`

---

## 5. 信息架构（最终 · 7 个一级导航）

```
ClawWiki 桌面壳 (Tauri)
│
├── 顶部 chrome (28px, 只有 traffic light + "ClawWiki")
│
├── 左侧 Sidebar (220px, 可折叠到 56px, DeepTutor 风格)
│   ├── [Logo] ClawWiki · v3
│   │
│   ├── ─ PRIMARY ─
│   ├── 📊 Dashboard          /dashboard
│   ├── 💬 Ask                /ask[/:sessionId]       ← CCD 工作台+流式会话注入点
│   ├── 📨 Inbox  [badge]     /inbox                  ← CCD 权限确认+任务审阅注入点
│   ├── 📥 Raw Library        /raw
│   ├── 📖 Wiki Pages         /wiki[/:slug]
│   ├── 🕸  Graph              /graph
│   ├── 📐 Schema             /schema
│   │
│   ├── ─ FUNNEL ─
│   ├── 🔗 WeChat Bridge      /wechat                 ← 唯一入口的配置页
│   │
│   ├── [spacer]
│   └── ⚙️  Settings           /settings
│        ├── Account
│        ├── Subscription & Codex Pool   ← 原 Token Broker
│        ├── WeChat Bridge
│        ├── Wiki Storage
│        ├── Permissions
│        ├── Shortcuts
│        └── About
│
└── 主区 (剩余宽度)
     ├── 上方 Page Head (56px) · h3 + sub + 右侧 actions
     ├── 中间 Body (flex-1, overflow auto)
     └── 下方 StatusLine (28px, 只在 Ask / Inbox / 后台任务活跃时显示)
```

**为什么 Ask 和 Inbox 放在最前面**：这是 CCD 4 件套的主要栖息地。用户最高频的动作是"在微信发一条" → 然后"回到桌面看 Inbox → 点 Ask 接着挖"。其它页是支撑。

**为什么把 Graph / Schema 留着**：不是为 MVP，是为了让用户**一开始就看到完整的认知复利叙事**——Graph 证明你的知识在连接，Schema 证明纪律在生效。MVP 只做只读版本即可。

---

## 6. CCD 4 件套提取 · 精确映射

### 6.1 原码位置 → 新位置

| 原文件 | 原作用 | 新位置 | 新作用 |
|---|---|---|---|
| `apps/desktop-shell/src/features/session-workbench/SessionWorkbenchTerminal.tsx` | CCD 会话终端主容器 | `features/ask/AskWorkbench.tsx` | Ask 页骨架（sidebar sessions + 主流 + composer + status line） |
| `session-workbench/VirtualizedMessageList.tsx` | 虚拟滚动消息列表 | `features/ask/MessageList.tsx` + `features/inbox/TaskEventList.tsx` | Ask 流式消息；Inbox 里 Maintainer 的事件流 |
| `session-workbench/MessageItem.tsx` | 单条消息气泡（用户/AI/tool call） | `features/ask/Message.tsx` | Ask 消息气泡（皮肤换暖色） |
| `session-workbench/InputBar.tsx` | 多行输入 + 附件 + @mention + 斜杠 | `features/ask/Composer.tsx` | Ask 输入器（MVP 裁剪：只留 @mention 和多行，斜杠命令暂不要） |
| `session-workbench/PermissionDialog.tsx` | low/medium/high 权限弹窗 | `features/permission/WikiPermissionDialog.tsx` | 所有 Maintainer 写操作的 gate |
| `session-workbench/StatusLine.tsx` | 底部状态条 | `features/common/StatusLine.tsx` | Ask / Inbox / Dashboard 都用 |
| `session-workbench/SubagentPanel.tsx` | 子任务面板（task tree + 展开/收起 + 状态） | `features/inbox/MaintainerTaskTree.tsx` | Inbox 选中任务后的详情——**灵魂组件** |
| `session-workbench/ContentHeader.tsx` | 顶部会话信息条 | `features/ask/AskHeader.tsx` | Ask 页的会话信息 |
| `session-workbench/ProjectPathSelector.tsx` | 项目路径选择器 | **删除** | Wiki 只有一个路径 `~/.clawwiki/` |

### 6.2 用户会明确感知到的 4 个"似曾相识"时刻

1. **打开 Ask 页**：左侧 sessions 列表 + 主区消息流 + 底部输入器 + 状态条 → **"这是 CCD 工作台"**
2. **AI 在流式吐字的时候光标闪动，tool call 以卡片形式内嵌在消息里** → **"这是 CCD 流式会话"**
3. **AI 要写 `wiki/concept/llm-wiki.md` 前弹出一个 "Permission required" 窗，有 `Allow once` / `Always allow in Wiki` / `Deny` 三个按钮** → **"这是 CCD 权限确认"**
4. **打开 Inbox 点一个 Maintainer 任务，右边展开一个带时间线的 task tree，每个节点可以展开看 tool call 和 diff** → **"这是 CCD 任务审阅"**

---

## 7. WeChat 漏斗 · 唯一入口的技术链路

### 7.1 组件边界

```
┌───────────────────┐    webhook     ┌──────────────────────┐     WebSocket    ┌─────────────────────┐
│ 个人微信外联      │ ─────────────▶ │  wechat-ingest       │ ───────────────▶ │  desktop-shell       │
│ 机器人（主推）     │                 │  :8904 微服务        │                 │                      │
│                   │                 │                     │                 │  Tauri WebView       │
│ 公众号订阅号      │                 │  · 签名校验          │                 │                      │
│ （次选）          │                 │  · 分用户 / JWT      │                 │  收到事件:           │
│                   │                 │  · 附件转对象存储    │                 │   → 下载 blob        │
│ 个人微信桥接      │                 │  · 只中继不入库       │                 │   → defuddle        │
│ （v4+ opt-in）    │                 │  · 30 天 TTL         │                 │   → clipper/api     │
└───────────────────┘                 │  · AES-GCM 加密 blob │                 │   → POST 给 Rust     │
                                      └──────────────────────┘                 │   → 写 ~/.clawwiki  │
                                                                               │   → 触发 maintainer │
                                                                               └─────────────────────┘

🔒 原文永不经任何第三方 LLM
🔒 只经过用户自己订阅的 Codex 账号（CloudManaged source）
🔒 云侧只做 30 天中继，blob 加密后永不解密在云侧
```

### 7.2 10 种素材 → Raw 层的映射

| 微信输入 | 云侧处理 | 桌面侧 pipeline | Raw 产出 |
|---|---|---|---|
| 文本 | 直接转发 | 包成 md | `NNNNN_wechat_text_{slug}.md` |
| **mp.weixin.qq.com URL** | 只转发 URL | 桌面 fetch → **forked defuddle (+wechat extractor)** → **`obsidian-clipper/api::clip()`** → md + frontmatter | `NNNNN_wechat_article_{published}_{slug}.md` + attachments/ |
| 普通 URL | 只转发 URL | 同上，不经 wechat 分支 | `NNNNN_wechat_url_{slug}.md` |
| 语音 `.silk`/`.amr`/`.mp3` | 转对象存储 | ffmpeg → whisper.cpp 本地 或 Whisper API | `NNNNN_wechat_voice_{duration}.md` |
| 图片 `.jpg` | 转对象存储 | Codex GPT-5.4 Vision caption + OCR | `NNNNN_wechat_image_{sha}.md` + 原图 |
| **PPT `.pptx`** | 转对象存储 | Rust spawn `python-pptx` → 每 slide 一个 section | `NNNNN_wechat_pptx_{slug}.md` + slide 图 |
| PDF | 转对象存储 | 前端 `pdfjs-dist` 抽文本 + 页边图 | `NNNNN_wechat_pdf_{slug}.md` |
| DOCX | 转对象存储 | Rust spawn `mammoth` → defuddle 通用链路 | `NNNNN_wechat_docx_{slug}.md` |
| **视频 `.mp4`** | 转对象存储 | ffmpeg 抽音轨 + 10s 抽帧 → Whisper + Vision caption | `NNNNN_wechat_video_{duration}.md` + keyframes/ |
| 小程序卡片 | 解析 JSON | 反推落地 URL → URL pipeline；失败留 JSON | `NNNNN_wechat_card_{appid}.md` |
| 聊天记录片段 | 解析 JSON | 按发言人聚合、主题分段 | `NNNNN_wechat_chat_{count}.md` |

### 7.3 关键技术栈决定（继承自前代研究）

| 选择 | 决定 | 理由 |
|---|---|---|
| HTML → 干净 HTML | **defuddle** (fork + 自写 `wechat.ts` extractor) | 成熟、MIT、环境无关；`substack.ts` 现成模板可抄 |
| HTML → Markdown | **obsidian-clipper/api::clip()** | `src/api.ts` 零 chrome.* 依赖，已经把 defuddle 串起来了 |
| DOM Parser | Tauri WebView 原生 `DOMParser` | 不用 linkedom，不开 Node 子进程 |
| 不用 obsidian-importer | 硬绑 `app.vault.*`，抽出来成本 > 重写 | 已确认 |
| 不用 turndown 直接调 | 让 clipper 封装它 | clipper 已经做好了 |
| 维护 LLM | Codex GPT-5.4 | 用户订阅池；prompt cache 能省 50-90% |
| 维护范式 MVP | engram 式（一次 LLM 调用 + Pydantic 校验返回 JSON） | 代码量小；规模化后抄 sage-wiki 5-pass |

---

## 8. Schema 层 · `~/.clawwiki/schema/CLAUDE.md`

**这文件必须先于代码落地**。它决定 Maintainer AI 是"聊天机器人"还是"有纪律的维护员"。

```markdown
# CLAUDE.md · wiki-maintainer agent rules

## Role
You are the wiki-maintainer for ClawWiki — the user's "外脑" (external brain).
Sources arrive almost exclusively from the user's WeChat dialog box:
articles, voice messages, PPTs, videos, chat screenshots, etc.

Human curates (by forwarding in WeChat); you maintain (by writing wiki pages).
Never invert this.

## Layer contract
- raw/     read-only. Every file has unique sha256. Never mutate.
- wiki/    you write. Must pass Schema v1 frontmatter validation.
- schema/  human-only. You may PROPOSE changes via Inbox, never write directly.

## Triggers
Every `raw_ingested(source_id)` event MUST fire the 5 maintenance actions:
  1. summarise the new source (≤ 200 words, original wording; quote ≤ 15 words)
  2. update affected concept / people / topic / compare pages
     (create if absent, using templates/{type}.md)
  3. add / update backlinks (bidirectional: A→B implies B→A)
  4. detect conflicts with existing judgements → `mark_conflict` → Inbox
  5. append to `changelog/YYYY-MM-DD.md`: `## [HH:MM] ingest | {title}`
     and append to `log.md` with the same prefix

After all 5 actions, call `rebuild_index` once to refresh wiki/index.md.

## Frontmatter (schema v1, required)
type:          concept | people | topic | compare | changelog | raw
status:        canonical | draft | stale | deprecated | ingested
owner:         user | maintainer
schema:        v1
source:        wechat | upload | ask-session
source_url:    (when applicable)
published:     ISO-8601 date (for raw articles)
ingested_at:   ISO-8601 datetime
last_verified: ISO-8601 date

## Tool permissions (WikiPermissionDialog enforces)
low    : read_source · read_page · search_wiki · rebuild_index
medium : write_page · patch_page · link_pages · touch_changelog
high   : ingest_source · deprecate_page · mark_conflict

## Never do
- Never rewrite raw/ files
- Never silently merge conflicting pages — always mark_conflict
- Never deprecate a page without a replacement slug
- Never summarise in > 200 words
- Never quote > 15 consecutive words from raw/ (copyright)
- Never emit backlinks to non-existent pages (link_pages must precheck)
- Never touch schema/ — propose via Inbox instead

## When uncertain
Use `mark_conflict` with reason="uncertain: ${reason}" and move on.
The user will triage in Inbox.
```

---

## 9. Token Broker 内聚 · Rust 模块而非 HTTP 服务

### 9.1 形态对比

| | HTTP 形态（被砍） | **内聚形态（canonical）** |
|---|---|---|
| 接口 | `POST /v1/chat/completions` | `CodexBroker::chat_completion(req)` 函数 |
| 消费者 | 任何本机进程（Cursor/CLI/CCD 都可） | **仅** `ask-runtime` 和 `wiki-maintainer` 两个 Rust crate |
| Token 去向 | 要吐给调用方 → 有泄漏风险 | **永远不出 Rust 层** |
| Settings 页 | "Hook up your external clients" | "Subscription & Codex Pool" 只读状态 |
| 攻击面 | 127.0.0.1:4357 开放 | 0 |

### 9.2 Rust 草图

```rust
// rust/crates/desktop-core/src/codex_broker.rs
pub struct CodexBroker {
    pool: Arc<RwLock<Vec<CloudManagedAccount>>>,
    managed_auth: Arc<ManagedAuthRuntimeClient>,
    http: reqwest::Client,
    rr_counter: AtomicUsize,
}

impl CodexBroker {
    /// 唯一入口。只暴露给同 workspace 的其它 crate。
    pub async fn chat_completion(
        &self,
        req: ChatRequest,
    ) -> Result<ChatStream, BrokerError> { /* ... */ }

    /// 被 WeChatBridge 的 trade-service WS 触发
    pub async fn sync_cloud_accounts(&self, accounts: Vec<CloudAccountInput>);

    /// 订阅到期 / 主动退订
    pub async fn clear_cloud_accounts(&self);

    /// 给 Settings 页 GET /api/broker/status 用
    /// 只返回数字，不返回任何 token 字段
    pub fn public_status(&self) -> BrokerPublicStatus;
}

pub struct BrokerPublicStatus {
    pub pool_size: usize,
    pub fresh_count: usize,
    pub refreshing_count: usize,
    pub expired_count: usize,
    pub requests_today: u64,
    pub next_refresh_at: DateTime<Utc>,
    // NOT: any token / user_id / email
}
```

### 9.3 `desktop-server` 路由清单（没有 `/v1/*`）

```
# 云账号同步（前端 billing-store 的前端明文 JSON 路径改走这里）
POST /api/desktop/cloud/codex-accounts/sync
GET  /api/desktop/cloud/codex-accounts     → 只返回 alias + expires + status，没有 token
POST /api/desktop/cloud/codex-accounts/clear

# Broker 公共状态（给 Settings 页显示）
GET  /api/broker/status                    → BrokerPublicStatus

# Wiki 数据面
POST /api/wiki/raw/ingest                  ← 前端 persist.ts 调
POST /api/wiki/ingest/voice                ← 重活走 Rust
POST /api/wiki/ingest/image
POST /api/wiki/ingest/pptx
POST /api/wiki/ingest/docx
POST /api/wiki/ingest/video
POST /api/wiki/fetch                       ← 代理外链 fetch 绕 CORS
GET  /api/wiki/raw
GET  /api/wiki/raw/:id
GET  /api/wiki/pages
GET  /api/wiki/pages/:slug
PUT  /api/wiki/pages/:slug
PATCH /api/wiki/pages/:slug
GET  /api/wiki/search
GET  /api/wiki/graph
GET  /api/wiki/inbox
POST /api/wiki/inbox/:id/resolve
GET  /api/wiki/schema
PUT  /api/wiki/schema

# Ask 会话
POST /api/ask/sessions
GET  /api/ask/sessions/:id
POST /api/ask/sessions/:id/messages
GET  /api/ask/sessions/:id/events          ← SSE 流

# WeChat bridge
GET  /api/wechat/events
POST /api/wechat/events/:id/retry
WS   /ws/wechat-inbox                       ← 本机桥接，订阅云 trade-service

# 明确不存在的
# NOT: POST /v1/chat/completions
# NOT: POST /v1/messages
# NOT: GET  /v1/models
# NOT: POST /api/broker/launch-client
```

---

## 10. 数据层 · `~/.clawwiki/`

```
~/.clawwiki/
├── raw/                              # 不可变事实层（92%+ 来自微信）
│   ├── 00001_wechat_karpathy-llm-wiki_2026-04-08.md
│   ├── 00002_wechat_voice_2026-04-08_dur60s.md
│   ├── 00003_wechat_pptx_2026-04-08_slug.md
│   └── attachments/
│       ├── 00001/cover.jpg
│       └── 00003/slide-01.png
│
├── wiki/                             # LLM 持续维护的页面层
│   ├── index.md                      # Maintainer 自动重建
│   ├── log.md                        # append-only `## [YYYY-MM-DD HH:MM] ingest | ...`
│   ├── concepts/*.md
│   ├── people/*.md
│   ├── topics/*.md
│   ├── compare/*.md
│   └── changelog/YYYY-MM-DD.md
│
├── schema/                           # 规则层（人写 + AI 提议）
│   ├── CLAUDE.md                     # 见 §8
│   ├── AGENTS.md                     # 多 agent 分工
│   ├── templates/
│   │   ├── concept.md
│   │   ├── people.md
│   │   ├── topic.md
│   │   ├── compare.md
│   │   └── wechat-clip.clipper.json  # obsidian-clipper Template
│   └── policies/
│       ├── maintenance.md            # 5 件维护动作硬规则
│       ├── conflict.md
│       ├── deprecation.md
│       └── naming.md
│
├── .clawwiki/                        # 机器可读元数据
│   ├── manifest.json                 # 抄 sage-wiki：source/concept hash tracker
│   ├── compile-state.json            # 抄 sage-wiki：断点续传
│   └── ask-sessions.db               # SQLite：ask 会话持久化
│
└── .git/                             # git init，白送版本历史
```

---

## 11. 代码改造清单

### 11.1 `apps/desktop-shell/src/` · 前端

#### 🗑 删除（从现在开始，这些目录不存在）

```
shell/TabBar.tsx
shell/TabItem.tsx
features/apps/                         整个目录
features/code-tools/                   整个目录
features/workbench/                    整个目录（HomePage/OpenClaw/Dispatch/Customize/Scheduled/Search 全砍）
features/session-workbench/            整个目录（组件被拆散到 ask/inbox/permission/common）
state/minapps-store.ts
state/tabs-store.ts
state/permissions-store.ts             逻辑迁移到 wiki-maintainer-store
lib/tauri.ts                           删除 code-tools 相关类型和 invoke
```

#### ♻️ 保留并小改

```
shell/AppShell.tsx                     重写：HashRouter + Sidebar + 7 条路由
features/auth/*                        零改动（Google OAuth）
features/billing/api.ts                保留 plans/orders/subscription
features/billing/cloud-accounts-sync.ts 重写：改走 Rust endpoint（前端不再存 token）
features/billing/CloudAccountsPanel.tsx 改名为 CodexPoolPanel.tsx，只读
features/settings/SettingsPage.tsx     重写 MENU_ITEMS
state/auth-store.ts                    零改动
state/billing-store.ts                 小改（cloud-accounts 部分）
state/settings-store.ts                保留
state/streaming-store.ts               迁移到 state/ask-store.ts
lib/cloud/*                            零改动
```

#### ✨ 新增

```
shell/Sidebar.tsx                      220/56 可折叠 DeepTutor 风格

features/dashboard/DashboardPage.tsx
features/dashboard/QuickAsk.tsx        提取版 Composer

features/ask/AskPage.tsx
features/ask/AskWorkbench.tsx          从 SessionWorkbenchTerminal 提取
features/ask/AskHeader.tsx             从 ContentHeader 提取
features/ask/AskStream.tsx             从 VirtualizedMessageList 提取
features/ask/Message.tsx               从 MessageItem 提取
features/ask/Composer.tsx              从 InputBar 提取（裁剪版）
features/ask/useAskLifecycle.ts        从 useSessionLifecycle 提取

features/inbox/InboxPage.tsx
features/inbox/TaskListColumn.tsx
features/inbox/MaintainerTaskTree.tsx  从 SubagentPanel 提取（灵魂组件）
features/inbox/MaintainerStream.tsx    后台任务流
features/inbox/TaskDiffPreview.tsx

features/permission/WikiPermissionDialog.tsx   从 PermissionDialog 提取

features/common/StatusLine.tsx         从 session-workbench/StatusLine 提取

features/raw/RawLibraryPage.tsx
features/raw/RawDetailPage.tsx
features/raw/RawFilterBar.tsx

features/wiki/WikiExplorerPage.tsx
features/wiki/WikiPageDetail.tsx
features/wiki/CategoryTabs.tsx
features/wiki/BacklinksAside.tsx

features/graph/GraphPage.tsx           MVP 只做节点+边渲染

features/schema/SchemaEditorPage.tsx   MVP 只读 + 显示 proposal diff
features/schema/FileTree.tsx

features/wechat/WeChatBridgePage.tsx
features/wechat/BotBindingCard.tsx
features/wechat/PipelineStatusRow.tsx
features/wechat/EventTimeline.tsx

features/ingest/pipeline.ts            总入口按 kind 分派
features/ingest/adapters/text.ts
features/ingest/adapters/wechat-article.ts
features/ingest/adapters/url.ts
features/ingest/adapters/voice.ts
features/ingest/adapters/image.ts
features/ingest/adapters/pdf.ts
features/ingest/adapters/pptx.ts
features/ingest/adapters/docx.ts
features/ingest/adapters/video.ts
features/ingest/adapters/card.ts
features/ingest/adapters/chat.ts
features/ingest/templates/wechat-clip.json
features/ingest/persist.ts             调 /api/wiki/raw/ingest

state/ask-store.ts                     ask 会话、streaming 缓冲
state/wiki-store.ts                    Wiki 当前页、Inbox 未读
state/ingest-store.ts                  ingest pipeline 进度
state/wechat-store.ts                  wechat events

lib/defuddle/index.ts                  import @clawwiki/defuddle-fork
lib/clipper/index.ts                   import obsidian-clipper/api
```

#### 新依赖

```json
{
  "@clawwiki/defuddle-fork": "github:...",
  "obsidian-clipper": "^x",
  "pdfjs-dist": "^4",
  "ulid": "^2"
}
```

### 11.2 `rust/crates/` · Rust

#### 🗑 删除

```
desktop-server::code_tools_bridge      整个模块（/code 页没了）
```

#### ♻️ 扩展

```
desktop-core::managed_auth             新增 CloudManaged source + 3 个 import/list/clear API
desktop-core::codex_auth               兼容 CloudManaged
desktop-server::lib.rs                 新路由（见 §9.3）
```

#### ✨ 新增 crate

```
rust/crates/desktop-core/src/codex_broker.rs   §9.2 的内聚 Broker
rust/crates/wiki_store                          ~/.clawwiki/ 文件系统后端
rust/crates/wiki_maintainer                     触发型 agent loop
rust/crates/wiki_ingest                         voice/image/pptx/docx/video 后端处理
rust/crates/wechat_bridge                       WS 客户端订阅 trade-service
rust/crates/ask_runtime                         Ask 会话 runtime
```

### 11.3 云端

```
wechat-ingest          :8904 新微服务
  POST /api/v1/wechat/webhook        企微签名校验
  GET  /api/v1/wechat/inbox?user_id=
  GET  /api/v1/wechat/blob/:id       短时签名 URL
  WS   /ws/wechat-inbox?token=       本机桥接订阅点
```

---

## 12. MVP 路线（7 周 · 锐化版）

| Sprint | 周 | 交付 | 成功判据（能 demo 的） |
|---|---|---|---|
| **S0 斩断** | W1 前半 | 在一个 git branch 里**一次性**删除 shell/tabbar/apps/code-tools/workbench/session-workbench 六个目录。写 `~/.clawwiki/` 文件结构 + `schema/CLAUDE.md` 初稿 + `Sidebar.tsx` + 7 个空路由 stub | 能跑起来，Wiki-first 侧栏在，没任何功能但没编译错 |
| **S1 漏斗** | W1 后半 + W2 | fork defuddle + 写 `wechat.ts` extractor + 前端 `features/ingest/` 全套 + `RawLibraryPage` + 手动 "paste URL" 触发路径 | 桌面粘贴 mp.weixin URL，10 秒内 `~/.clawwiki/raw/` 多一份格式良好 md |
| **S2 Broker** | W3 | `codex_broker` Rust 模块 + `managed_auth::CloudManaged` + `billing/cloud-accounts-sync` 改走 Rust + `Settings > Subscription & Codex Pool` 只读面板 | Dashboard 能显示 "pool: 5 accounts · 今日 0 req · ¥0"，其它账号的任何 token 都拿不到 |
| **S3 刃二（CCD 提取）** | W4 | 拆 `session-workbench` → 把 `AskWorkbench / AskStream / Message / Composer / StatusLine / WikiPermissionDialog / MaintainerTaskTree` 7 个新组件建出来 · `AskPage` 通 Codex Broker · `InboxPage` 空壳 | 能在 Ask 里跟 Codex 对话、能看到流式字符、mock 一个 write_page 能触发 PermissionDialog |
| **S4 维护** | W5 | `wiki_maintainer` MVP（engram 式单次 LLM 调用）· Pydantic-style JSON 校验 · Inbox 显示 MaintainerTaskTree · 所有 write 过 PermissionDialog | 一次 ingest 后自动生成 1-3 个 wiki page + 追加 log.md + Inbox 能审阅 |
| **S5 微信主入口** | W6 | `wechat-ingest` 云服务（文本 + URL only） + `wechat_bridge` Rust client + `WeChatBridgePage` + WS 推送 | 微信给 bot 发 mp 链接，3 秒内桌面 Inbox 卡片出现并自动走完 pipeline |
| **S6 丰富素材** | W7 | 语音 / 图片 / PDF / PPT / 视频 5 个 adapter + 对应 Rust endpoint + `GraphPage` 空实现 + `SchemaEditorPage` 只读 | 微信发语音和 PPT，Raw 出 md 带转写 / slide 列表 |

**Backlog 明确不做**：sage-wiki 5-pass compiler · prompt cache · Batch API · FTS5 · 向量检索 · MCP server · 个人微信桥 · Obsidian vault · 团队多用户 · i18n · 移动端。

---

## 13. 团队评审 · 关键决策投票表

**评审方式**：每个决策逐条 +1 / -1 / ±0 / ?。任何 -1 票都必须提出替代方案。±0 视为 +1。议题 D1-D4 是战略级（一票否决），D5-D15 是战术级（投票决定）。

### D1-D4 战略级（一票否决）

| ID | 决策 | 投票 |
|---|---|---|
| **D1** | ClawWiki 的一句话定位是 **"你的外脑"**，不是 "AI 工具箱" / "Wiki 客户端" / "认知操作系统" | ⬜ +1  ⬜ -1  ⬜ ? |
| **D2** | MVP 只有 **一个用户输入通道：微信**。桌面"手动上传"降到 Raw Library 的次级功能 | ⬜ +1  ⬜ -1  ⬜ ? |
| **D3** | Token Broker **完全内聚**为 Rust 模块。砍掉对外部 Cursor / CLI / CCD 的任何兼容 | ⬜ +1  ⬜ -1  ⬜ ? |
| **D4** | 产品叙事是 **"把微信里的信息变成你的长期资产"**，不谈"帮你总结"、"帮你摘要"这种工具话术 | ⬜ +1  ⬜ -1  ⬜ ? |

### D5-D10 战术级（产品边界 + 技术）

| ID | 决策 | 建议 | 投票 |
|---|---|---|---|
| **D5** | 一次性删除 `shell/TabBar.tsx`, `features/{apps,code-tools,workbench,session-workbench}/`（六个目录） | 是 | ⬜ +1  ⬜ -1 |
| **D6** | CCD 只提取 4 件套灵魂（工作台 / 流式 / 权限 / 任务审阅），皮肤全换 DeepTutor 暖色 | 是 | ⬜ +1  ⬜ -1 |
| **D7** | Ask 和 Inbox 在 Sidebar 里排在 Dashboard 之后、Raw/Wiki 之前（高可见性位置） | 是 | ⬜ +1  ⬜ -1 |
| **D8** | HTML→MD 走 **defuddle + obsidian-clipper/api** 双栈，fork defuddle 自写 `wechat.ts` | 是 | ⬜ +1  ⬜ -1 |
| **D9** | 维护 Agent MVP 用 engram 风格（单次 LLM 调用返回 JSON），不上 sage-wiki 5-pass | 是 | ⬜ +1  ⬜ -1 |
| **D10** | 视觉系统一以贯之 DeepTutor 暖色（`#FAF9F6` bg + `#C35A2C` 烧陶橙 + Lora 衬线） | 是 | ⬜ +1  ⬜ -1 |

### D11-D15 叙事与定价

| ID | 问题 | 建议 | 投票 |
|---|---|---|---|
| **D11** | 砍掉外部 Broker 后的订阅叙事是什么？ | "订阅的是一个 7×24 替你读微信、整理成外脑的 AI 维护员" | ⬜ 同意 ⬜ 改 |
| **D12** | 免费版是否保留？ | 不保留，登录即必须有 active subscription；首月试用 7 天 | ⬜ +1 ⬜ -1 |
| **D13** | 微信合规方案 | 主推企业微信外联机器人；个微桥 v4+ opt-in 红字警告 | ⬜ +1 ⬜ -1 |
| **D14** | 如果 Codex GPT-5.4 的订阅配额不支持 Batch API 折扣，怎么办？ | 只靠 prompt cache 省钱；不行就加价 30% | ⬜ +1 ⬜ -1 |
| **D15** | 用户问 "为什么我不直接给 CCD 买 token" 时，销售话术是什么？ | "CCD 帮你写代码，ClawWiki 帮你记住你读过什么。两个产品不重合" | ⬜ 同意 ⬜ 改 |

### 设计与 MVP 范围

| ID | 问题 | 建议 | 投票 |
|---|---|---|---|
| **D16** | Graph / Schema Editor 是否进 MVP | 进 MVP，但只做只读/空壳 | ⬜ +1 ⬜ -1 |
| **D17** | Ask 会话是否持久化到 SQLite | 是，存 `~/.clawwiki/.clawwiki/ask-sessions.db` | ⬜ +1 ⬜ -1 |
| **D18** | Inbox 里 Maintainer 的 5 个 low/medium 动作是否默认 auto-approve | 是，否则用户被 dialog 淹没；high 级永远要手动 | ⬜ +1 ⬜ -1 |
| **D19** | Dashboard 的 QuickAsk 每次是否开新会话 | 是，避免污染长会话上下文 | ⬜ +1 ⬜ -1 |
| **D20** | Sidebar 默认展开还是折叠 | 展开 220px，记住用户偏好 | ⬜ +1 ⬜ -1 |

---

## 14. 风险 & 留白

| 风险 | 影响 | 缓解 |
|---|---|---|
| 微信合规不过 | 没有入口 = 没有产品 | MVP 只做企微外联；第三方合规审 S5 前必须完成 |
| mp.weixin DOM 漂移 | wechat extractor 悄悄坏 | 每周 fixture 回归（爬 5 篇不同号） |
| Codex 订阅配额不稳定 | maintainer 排队 / 失败 | 实现退避 + 失败队列 + Inbox 显式告警 |
| `wiki_maintainer` 返回的 JSON schema 不稳定 | Pydantic 校验失败率高 | S4 前必须跑 200 份 fixture 压测，失败率 < 2% |
| 用户一开始 raw 很少，Dashboard 空荡荡 | 冷启动体验差 | S7+ 做"示例工作区"：登录时自动 populate 10 篇 Karpathy gist 风格的示例 |
| 企业微信机器人消息有频率限制 | 批量转发时丢消息 | 客户端必须显示 "收到 / 处理中 / 完成" 三态，丢消息时用户能看到 |

---

## 15. 线框图

HTML 文件：[`./wireframes.html`](./wireframes.html) · 10 张屏，全 DeepTutor 暖色，单侧栏，CCD 4 件套灵魂注入。

| # | 屏 | 用来回答的问题 | CCD 灵魂展示 |
|---|---|---|---|
| 01 | Dashboard · 外脑主页 | "我的外脑今天长了多少" | QuickAsk (Composer 裁剪版) + StatusLine |
| 02 | WeChat Bridge · 漏斗主入口 | "微信怎么接进来的" | — |
| 03 | Raw Library · 事实层 | "我喂进去什么了" | — |
| 04 | Wiki Pages Explorer | "AI 帮我长出了什么" | — |
| 05 | Wiki Page Detail | "这一页怎么来的，怎么跟别的连的" | — |
| 06 | **Ask · CCD 工作台 + 流式会话** | "我怎么跟我的外脑对话" | **工作台 + 流式会话** |
| 07 | **Inbox · CCD 权限确认 + 任务审阅** | "AI 要动我的 wiki 前我怎么审" | **权限确认 + 任务审阅** |
| 08 | Graph · 认知网络 | "我的脑子里都连起来没" | — |
| 09 | Schema Editor | "AI 的纪律是什么" | — |
| 10 | Settings · Subscription & Codex Pool | "订阅能供给多少" | — |

---

## 附录 A · 命名

| 曾用名 | canonical 名 | 理由 |
|---|---|---|
| ClaudeWiki | **ClawWiki** | 四代都用这个，稳定 |
| Token Broker | **Subscription & Codex Pool** | 内聚后 "Broker" 是技术术语，用户不需要理解 |
| Session Workbench | **Ask Workbench** | 产品语言统一成"Ask" |
| Permission Dialog | **Wiki Permission Dialog** | 明确作用域 |
| SubagentPanel | **Maintainer Task Tree** | 更准确 |
| WeChat Inbox MinApp | **WeChat Bridge**（一级页） | 砍了 MinApp 概念后提级 |
| Code Tools Page | **（删除）** | 不存在 |

## 附录 B · 为什么是 "外脑" 不是 "Wiki"

普通用户不知道什么是 Wiki。但所有人都知道"我想记住这件事，但我记不住"。

- "ClawWiki 是一个 Wiki 工具" → 用户："Wiki 是啥？"
- "ClawWiki 是你的外脑" → 用户："我确实需要这个"

叙事优先级：**身体隐喻 > 行业术语**。技术文档里可以用 "Wiki / 认知资产 OS / Karpathy 三层"，但对外宣传、登录欢迎页、App 描述必须是 "外脑"。

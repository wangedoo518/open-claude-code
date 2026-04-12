# ClawWiki · 你的"外脑"

> 产品哲学：**放弃做"瑞士军刀"，打造一把"手术刀"。**
>
> **ClawWiki = 你的外脑。微信喂料 → AI 审阅 → 认知资产沉淀。**

你在微信里的每一次随手转发、每一段语音、每一份 PPT，都会经过 AI 审阅和你的权限确认，
被结构化进一个由 Karpathy 式三层架构（Raw / Wiki / Schema）组成的认知资产库。

这个库不是工具箱。它是你未来两年里提问、写作、决策时，AI 能直接调用的"长期记忆"。

📖 公共文档入口：[`docs/desktop-shell/README.md`](docs/desktop-shell/README.md)
🧭 开源边界与 API Key gateway 设计：[`docs/desktop-shell/specs/2026-04-12-desktop-shell-open-source-gateway-design.md`](docs/desktop-shell/specs/2026-04-12-desktop-shell-open-source-gateway-design.md)

---

## 核心用户故事

**周二下午** — changpeng 在地铁上刷微信。

- **2:14 PM** 看到 Karpathy 的 LLM Wiki 方法论 · 长按 → 转发 → "ClawWiki 小助手" · 3 秒后机器人回 "✓ 已入库，正在维护 5 个相关页面"
- **2:15 PM** 同事发来 47 秒语音，讨论 RAG 局限 · 转发 · "✓ 已转写 420 字，已跟 concept/rag 对齐"
- **2:16 PM** 群里有人发 PPT `AI Products 2026 Q2 Roadmap.pptx` · 转发 · "✓ 已抽取 32 slides，正在跟 topic/ai-products 合并"

**7:30 PM** — 到家打开 ClawWiki 桌面端：

- **Dashboard**：今日进账 14 条 · 维护了 23 个页面 · 3 个需要你审阅
- **Inbox**：点开 "合并冲突：Agentic Loop v1 ↔ v2"，看到 Maintainer AI 完整的 tool call 树、每步 diff、一个 approve/reject 按钮
- **Ask**：问 "结合今天的三份材料写一段 AI Wiki vs RAG 的判断"，AI 流式吐答案，写 `compare/rag-vs-llm-wiki` 时弹出权限确认窗

**两周后** — 要给 CEO 汇报 "AI memory 这块最近的认知"。打开 Ask 问一句 → AI 基于**你自己 14 天里喂进去的 60 份材料**给出结构化回答。

这就是 **Karpathy 说的认知复利**：过去两周的微信随手转发 → 现在 30 秒就能调用。

当前公开文档入口见 [`docs/desktop-shell/README.md`](docs/desktop-shell/README.md)。

---

## 手术刀的三刃

### 🗡 刃一 · 定位刃 · Token 100% 内部消化

> 用户买的不是"Codex 代理服务"，是"一个会替自己长大的外脑"。
> Codex 账号是订阅附赠的燃料，不是产品本身。

- 受管账号能力只应作为私有扩展存在于控制平面，不应成为公开仓库的默认依赖
- 任何 broker / pool 逻辑都应优先保持在 Rust 进程内，而不是暴露成公共 HTTP token 服务
- 只有两个消费者：`AskSession`（用户提问）和 `WikiMaintainer`（后台自动维护）
- 前端拿不到 `access_token`，只能看到"池子还剩多少配额"这个数字
- 外部应用（Cursor / CLI / CCD / 第三方）**无法**从 127.0.0.1:4357 拿到任何 token

### 🗡 刃二 · 交互刃 · CCD 是模式库不是外壳

扒开 Claude Code Desktop，只留 **4 件套交互灵魂**，注入到 Wiki-native 界面里：

| CCD 灵魂 | 原组件 | 注入点 | 新组件 |
|---|---|---|---|
| 🧰 **工作台**<br>(sidebar + main + status 三段式) | `SessionWorkbenchTerminal` | `/ask` 骨架 | `features/ask/AskWorkbench.tsx` |
| 🌊 **流式会话**<br>(消息列表 + tool call card) | `VirtualizedMessageList` + `MessageItem` | `/ask` 主流 + `/inbox` 任务流 | `features/ask/AskStream.tsx` + `features/inbox/MaintainerStream.tsx` |
| 🛡 **权限确认**<br>(low/medium/high + "always allow") | `PermissionDialog` | 每个 `write_page`/`patch_page`/`deprecate_page` 前必过 | `features/permission/WikiPermissionDialog.tsx` |
| 🌲 **任务审阅**<br>(task tree + 展开 + diff 预览) | `SubagentPanel` | `/inbox` 选中任务的详情 | `features/inbox/MaintainerTaskTree.tsx` |

皮肤全部重上 **DeepTutor 暖色**：`#FAF9F6` 背景 + `#C35A2C` 烧陶橙 + Lora 衬线。

### 🗡 刃三 · 价值刃 · Karpathy 三层 + WeChat 漏斗

```
微信（中国最强信息输入流）
      │
      ▼  随手转发 / 发语音 / 扔 PPT / 上传视频
ClawWiki 漏斗
      │
┌─────┴─────┐
▼           ▼
Raw 层    Schema 层
(只读)    (CLAUDE.md / AGENTS.md / templates / policies)
└─────┬─────┘
      ▼
   Wiki 层（LLM 持续维护）
      │
┌─────┼─────┐
▼     ▼     ▼
概念页 人物页 对比/变更日志
```

**Maintainer AI 的 5 件维护动作**（每次 ingest 必做）：

1. Summarise（≤ 200 words · quote ≤ 15 words · 版权安全）
2. Update affected concept / people / topic / compare pages
3. Add / update bidirectional backlinks
4. Detect conflicts → `mark_conflict` → Inbox（要人来审）
5. Append to `changelog/YYYY-MM-DD.md` + 重建 `wiki/index.md`

---

## 战略断腕清单 · 我们**不**做什么

**团队评审的第一议题**。没有对这 11 条共识，后面都不成立。

| # | 砍掉的 | 为什么 |
|---|---|---|
| ❌ 1 | Claude Code Desktop 的完整外壳（双行 TabBar / session tab row 2） | 用户不需要第二个 CCD |
| ❌ 2 | `/apps` MinApps 画廊 + MinAppDetailPage | 不是我们的叙事 |
| ❌ 3 | `/code` CLI 启动器 + `runCodeTool` + 终端选择器 | 我们不拉起外部 CLI |
| ❌ 4 | 对外暴露的 Token Broker `/v1` HTTP 路由 | Broker 只在 Rust 层 |
| ❌ 5 | "Launch Claude Code Desktop" 按钮 | 砍 #4 的直接后果 |
| ❌ 6 | Provider 目录管理（用户自己添加 API key） | 订阅用户只看 "Codex Pool" |
| ❌ 7 | Obsidian Vault overlay / 双向同步 | 我们就是 wiki 本身 |
| ❌ 8 | 团队多用户 / 共享工作区 | MVP 只做单人 |
| ❌ 9 | 移动端 app | 手机上已经有微信了 |
| ❌ 10 | "能总结 / 能转录 / 能记笔记" 的工具箱叙事 | 不讲工具话术 |
| ❌ 11 | 手动 ingestion 作为主流程（桌面"上传"按钮） | 主流量必须从微信来 |

> **ClawWiki 不是 "一个带 Wiki 功能的 AI 客户端"，而是 "微信转发的归宿"。**
>
> 所有的交互动词里，用户做得最多的只有一个——**在微信里按"转发"**。其他一切都是这个动作的后果。

---

## 信息架构 · 7 个一级导航

```
ClawWiki 桌面壳 (Tauri)
│
├── 顶部 chrome (28px, 只有 traffic light + "ClawWiki")
│
├── 左侧 Sidebar (220px, 可折叠到 56px, DeepTutor 风格)
│   ├── ─ PRIMARY ─
│   ├── 📊 Dashboard          /dashboard
│   ├── 💬 Ask                /ask[/:sessionId]   ← CCD 工作台+流式会话注入点
│   ├── 📨 Inbox  [badge]     /inbox              ← CCD 权限确认+任务审阅注入点
│   ├── 📥 Raw Library        /raw
│   ├── 📖 Wiki Pages         /wiki[/:slug]
│   ├── 🕸  Graph              /graph
│   ├── 📐 Schema             /schema
│   │
│   ├── ─ FUNNEL ─
│   ├── 🔗 WeChat Bridge      /wechat
│   │
│   └── ⚙️  Settings           /settings
│
└── 主区：Page Head (56px) + Body + StatusLine (28px)
```

**为什么 Ask 和 Inbox 排在前面**：这是 CCD 4 件套灵魂的主要栖息地。用户最高频的动作是"在微信发一条" → 然后"回到桌面看 Inbox → 点 Ask 接着挖"。其它页是支撑。

---

## WeChat 漏斗 · 唯一入口的技术链路

```
企业微信外联机器人 ──webhook──▶ wechat-ingest :8904 ──WS──▶ desktop-shell
     (主推 · 合规)                  (只中继 · 不入库)              │
                                   (30 天 TTL · AES-GCM)          ▼
                                                           下载 blob
                                                                  │
                                                                  ▼
                                                        defuddle (fork + wechat extractor)
                                                                  │
                                                                  ▼
                                                        obsidian-clipper/api::clip()
                                                                  │
                                                                  ▼
                                                          POST Rust → ~/.clawwiki/raw/
                                                                  │
                                                                  ▼
                                                          触发 wiki-maintainer (Codex GPT-5.4)

🔒 原文永不经任何第三方 LLM   🔒 只经过用户自己订阅的 Codex   🔒 云侧 blob 加密，不解密
```

### 10 种素材类型支持

| 微信输入 | 桌面侧 pipeline | Raw 产出 |
|---|---|---|
| 文本 | 包成 md | `NNNNN_wechat_text_{slug}.md` |
| **mp.weixin.qq.com URL** | fetch → forked defuddle + wechat extractor → clipper/api::clip() | `NNNNN_wechat_article_{pub}_{slug}.md` + `attachments/` |
| 普通 URL | 同上，不走 wechat 分支 | `NNNNN_wechat_url_{slug}.md` |
| 语音 `.silk/.amr/.mp3` | ffmpeg → whisper.cpp 本地 或 Whisper API | `NNNNN_wechat_voice_{dur}.md` |
| 图片 `.jpg` | Codex GPT-5.4 Vision caption + OCR | `NNNNN_wechat_image_{sha}.md` + 原图 |
| **PPT `.pptx`** | Rust spawn `python-pptx` → 每 slide 一个 section | `NNNNN_wechat_pptx_{slug}.md` + slide 图 |
| PDF | 前端 `pdfjs-dist` 抽文本 + 页边图 | `NNNNN_wechat_pdf_{slug}.md` |
| DOCX | Rust spawn `mammoth` → defuddle 通用链路 | `NNNNN_wechat_docx_{slug}.md` |
| **视频 `.mp4`** | ffmpeg 抽音轨 + 10s 抽帧 → Whisper + Vision caption | `NNNNN_wechat_video_{dur}.md` + `keyframes/` |
| 小程序卡片 | 反推落地 URL → URL pipeline | `NNNNN_wechat_card_{appid}.md` |
| 聊天记录片段 | 按发言人聚合 + 主题分段 | `NNNNN_wechat_chat_{count}.md` |

### 关键技术栈决定

| 选择 | 决定 | 理由 |
|---|---|---|
| HTML → 干净 HTML | **defuddle** (fork + 自写 `wechat.ts` extractor) | 成熟、MIT、环境无关 |
| HTML → Markdown | **obsidian-clipper/api::clip()** | `src/api.ts` 零 chrome.* 依赖，已经把 defuddle 串起来 |
| DOM Parser | Tauri WebView 原生 `DOMParser` | 不用 linkedom，不开 Node 子进程 |
| ❌ obsidian-importer | 硬绑 Obsidian Vault API，抽出来成本 > 重写 | 不用 |
| 维护 LLM | 兼容网关或直连模型提供方 | 公开仓库只承诺协议兼容，不承诺私有账号池能力 |
| 维护范式 MVP | engram 式（单次 LLM 调用 + Pydantic 校验返回 JSON） | 代码量小；规模化后抄 sage-wiki 5-pass |

---

## 数据层 · `~/.clawwiki/`

```
~/.clawwiki/
├── raw/                              # 不可变事实层（92%+ 来自微信）
│   └── attachments/
├── wiki/                             # LLM 持续维护
│   ├── index.md                      # Maintainer 自动重建
│   ├── log.md                        # append-only `## [YYYY-MM-DD HH:MM] ingest | ...`
│   ├── concepts/*.md
│   ├── people/*.md
│   ├── topics/*.md
│   ├── compare/*.md
│   └── changelog/YYYY-MM-DD.md
├── schema/                           # 规则层（人写 + AI 提议）
│   ├── CLAUDE.md                     # Maintainer 纪律（必须先于代码落地）
│   ├── AGENTS.md                     # 多 agent 分工
│   ├── templates/{concept,people,topic,compare}.md + wechat-clip.clipper.json
│   └── policies/{maintenance,conflict,deprecation,naming}.md
├── .clawwiki/                        # 机器可读元数据
│   ├── manifest.json                 # source/concept hash tracker
│   ├── compile-state.json            # 断点续传
│   └── ask-sessions.db               # SQLite：ask 会话持久化
└── .git/                             # git init，白送版本历史
```

---

## Public Docs

The repository no longer treats `docs/clawwiki/` as public
source-of-truth. Public readers should use:

- [`docs/desktop-shell/README.md`](docs/desktop-shell/README.md)
- [`docs/desktop-shell/architecture/overview.md`](docs/desktop-shell/architecture/overview.md)
- [`docs/desktop-shell/specs/README.md`](docs/desktop-shell/specs/README.md)
- [`docs/desktop-shell/plans/README.md`](docs/desktop-shell/plans/README.md)

---

## MVP 路线（7 周 · 锐化版）

| Sprint | 周 | 交付 | 成功判据 |
|---|---|---|---|
| **S0 · 斩断** | W1 前半 | 一次性删除 `shell/TabBar.tsx`、`features/{apps,code,code-tools,workbench,session-workbench}/` 六个目录 · 建 `~/.clawwiki/` + `CLAUDE.md` + `Sidebar.tsx` + 7 路由 stub | Wiki-first 壳跑起来不编译错 |
| **S1 · 漏斗** | W1 后半 + W2 | fork defuddle + 写 `wechat.ts` extractor + `features/ingest/` + `RawLibraryPage` + 手动 paste URL | 粘贴 mp.weixin URL，10s 内 raw 多一份格式良好 md |
| **S2 · Broker** | W3 | Rust 内部 broker 与受管账号边界方案 | 私有扩展可用，但不成为公开仓库默认 contract |
| **S3 · CCD 4 件套提取** | W4 | 拆 `session-workbench` → 7 个新组件 · AskPage 通 Broker · InboxPage 空壳 | 能 Ask 对话、看流式、mock write_page 触发 PermissionDialog |
| **S4 · 维护 Agent** | W5 | `wiki_maintainer` (engram 式单次调用) · JSON 校验 · Inbox 显 MaintainerTaskTree | ingest 后自动生成 1-3 wiki page + log.md + 能审阅 |
| **S5 · 微信主入口** | W6 | `wechat-ingest` 云服务 · Rust `wechat_bridge` · WeChatBridgePage + WS | 微信给 bot 发 mp 链接，3s 内 Inbox 卡片出现并走完 pipeline |
| **S6 · 丰富素材** | W7 | 语音/图片/PDF/PPT/视频 adapter + Rust endpoint · Graph + Schema 只读 | 微信发语音/PPT/视频，raw 出 md 带转写/slides |

**Backlog 明确不做**：sage-wiki 5-pass compiler · Batch API · FTS5 · 向量检索 · MCP server · 个人微信桥 · Obsidian vault · 团队多用户 · i18n · 移动端。

---

## 当前仓库结构

```text
claudewiki/
├── apps/
│   └── desktop-shell/                # Tauri 2 + React 桌面壳
│       └── src/
│           ├── shell/                # 🔨 S0 重写为 Sidebar + Router
│           ├── features/
│           │   ├── settings/         # ✅ Phase 6B/6C: WeChat + MultiProvider 已合并
│           │   ├── agents/
│           │   ├── apps/             # ❌ S0 删除
│           │   ├── code/             # ❌ S0 删除
│           │   ├── code-tools/       # ❌ S0 删除
│           │   ├── workbench/        # ❌ S0 删除
│           │   └── session-workbench/ # 🔨 S3 拆成 5 个新组件
│           └── lib/
├── rust/
│   └── crates/
│       ├── desktop-core/             # 桌面 domain
│       │   └── src/
│       │       ├── wechat_ilink/     # ✅ Phase 6B/6C: WeChat 账号接入（10 个文件）
│       │       ├── providers_config.rs
│       │       ├── managed_auth.rs   # 认证与兼容 provider runtime
│       │       ├── codex_auth.rs
│       │       └── agentic_loop.rs
│       ├── desktop-server/           # 本地 HTTP (127.0.0.1:4357)
│       ├── desktop-cli/              # 桌面侧 CLI 工具
│       └── server/                   # 轻量服务层
├── vendor/
│   └── api/                          # LLM provider 抽象（Anthropic / OpenAI 兼容）
├── docs/
│   ├── clawwiki/                     # legacy placeholders; not public source of truth
│   │   ├── README.md
│   │   ├── product-design.md         # public-safe placeholder
│   │   ├── wireframes.html           # public-safe placeholder
│   │   └── _archive/README.md        # legacy archive placeholder
│   └── desktop-shell/
└── assets/
```

---

## 开发环境

- Node.js 22+ · npm
- Rust stable · Cargo
- Tauri 2 本机依赖（[官方文档](https://tauri.app/start/prerequisites/)）

### 快速开始

```bash
# 桌面壳
cd apps/desktop-shell
npm install
npm run tauri:dev        # 会自动拉起 rust/crates/desktop-server

# Rust 服务层（独立启动）
cd rust
cargo run -p desktop-server
curl http://127.0.0.1:4357/healthz
```

### 验证命令

```bash
# Rust
cd rust
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Desktop shell
cd apps/desktop-shell
npm run build
cd src-tauri && cargo check
```

---

## 设计文档（必读）

按优先级：

| 文档 | 内容 |
|---|---|
| ⭐ [`docs/desktop-shell/README.md`](docs/desktop-shell/README.md) | **公共文档入口**。当前架构、tokens、operations、specs、plans 的正式入口 |
| ⭐ [`docs/desktop-shell/specs/2026-04-12-desktop-shell-open-source-gateway-design.md`](docs/desktop-shell/specs/2026-04-12-desktop-shell-open-source-gateway-design.md) | **开源边界 + API Key gateway 评审稿**。定义什么可以公开、什么必须保持私有，以及用户如何通过 `base_url + api_key` 接入兼容网关 |
| [`docs/clawwiki/README.md`](docs/clawwiki/README.md) | 兼容路径说明。该目录中的旧设计稿已退出公共 source-of-truth |
| [`docs/desktop-shell/cloud-managed-integration.md`](docs/desktop-shell/cloud-managed-integration.md) | 历史占位。原私有 cloud-managed 方案已退出公共 source-of-truth |
| [`CLAW.md`](CLAW.md) · [`AGENTS.md`](AGENTS.md) | 仓库级 Claude Code 使用约定 · 多 agent 分工 |
| [`rust/README.md`](rust/README.md) | Rust workspace 结构 |

---

## 设计边界

| 层 | 职责 |
|---|---|
| [`claw-code-parity`](https://github.com/wangedoo518/claw-code-parity) 上游 | Rust 核心能力（api / runtime / tools / plugins） |
| `rust/crates/desktop-core` | 桌面 domain：会话 · OAuth · WeChat · 权限 · providers |
| `rust/crates/desktop-server` | 本机 HTTP 服务，给前端提供 `/api/desktop/*` |
| `apps/desktop-shell` | 桌面交互、页面结构、宿主行为 |

如果要改 Rust 核心行为，**优先 upstream 到 `claw-code-parity`**。

---

## License

请结合仓库根目录现有文件与上游依赖（`claw-code-parity` 等）的许可证约束一起使用本项目。

---

<sub>
<b>ClawWiki</b> · 产品哲学："放弃做瑞士军刀，打造一把手术刀。" ·
public docs <code>docs/desktop-shell/README.md</code>
</sub>

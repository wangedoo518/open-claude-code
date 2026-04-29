---
title: Buddy Tolaria Deep Product Design
doc_type: spec
status: team-review
owner: desktop-shell
last_verified: 2026-04-29
source_of_truth: false
related:
  - docs/desktop-shell/README.md
  - docs/desktop-shell/architecture/overview.md
  - docs/desktop-shell/tokens/design-tokens.md
  - docs/desktop-shell/tokens/functional-tokens.md
  - docs/desktop-shell/operations/README.md
  - docs/desktop-shell/specs/README.md
  - docs/desktop-shell/specs/2026-04-28-buddy-tolaria-inspired-product-design.md
  - rust/README.md
---

# Buddy Tolaria Deep Product Design

本文用于团队评审：在代码、设计 token、交互设计三个层面对
`/Users/champion/Documents/develop/tolaria` 进行二次深读后，重新定义
Buddy 如何更彻底地借鉴 Tolaria。

本文不是当前实现真相，也不是直接实施计划。评审通过后，需要拆分为
`plan`、ADR、token 文档回填和代码任务。

## 0. 本轮确认项

2026-04-29 评审输入已确认以下方向，本文后续章节按这些约束展开：

1. Buddy 采用 Tolaria 式 main-only 研发工作流；质量闭环通过 spec/plan、TDD、质量门禁、原生 QA、文档同步兜住。
2. 接受 “Buddy Vault” 作为公开产品概念，不再只把它当实现细节。
3. Root `AGENTS.md` / `CLAUDE.md` shim 进入近期范围。
4. Buddy 允许用户直接编辑 `wiki/` 页面。
5. Git 成为一等能力。
6. 外部 AI 工具首期允许受控写入。
7. Rules Studio 替代当前独立 Schema Editor 页面。
8. Home/Pulse 成为默认首页，首屏偏“外脑健康体检”，强化“外脑今天是否健康、哪里需要处理”。
9. Purpose Lens 直接进入 frontmatter schema，首批包含 `writing / building / operating / learning / personal / research`。
10. 技术栈确认借鉴 Tolaria：Tauri + React/TS + Rust + Markdown/YAML + CodeMirror + MCP/CLI agent。
11. Tolaria 式研发纪律纳入 Buddy operations。
12. 信息架构继续降低使用门槛，并参考 `https://iamstarchild.com/` / `https://starchild.software/` 的低门槛叙事组织。
13. Knowledge / Rules 工作区默认展开 Tolaria-style 250px sidebar，其他工作区按上下文渐进披露。
14. Wiki 直接编辑允许修改 frontmatter 全量字段，但必须经过 schema validation、Git diff 和 lineage。
15. 外部 AI 受控写入允许覆盖 `schema/templates`，并区分“本次会话有效”和“永久规则”两种授权级别。
16. 新建 Buddy Vault 默认初始化 Git；用户可显式选择不启用，但这不是推荐路径。
17. Rules Studio 的 Advanced YAML / CodeMirror 默认折叠，避免新手被规则文件吓退。
18. 允许复制 Tolaria 源码，但必须接受并执行 AGPL 许可证义务、来源记录、许可证保留和质量门禁。

## 1. 执行结论

Buddy 不应复制 Tolaria 成为通用笔记编辑器。Buddy 的楔子仍然是：

> 微信里随手投喂，桌面端由 AI 维护成可追溯、可审阅、可问答的长期记忆。

Tolaria 最值得被 Buddy 深度引入的不是单个组件，而是三层系统：

1. **产品原则**：知识是有目的的。笔记不是为了存起来，而是为了帮助用户写作、构建、决策、运营和学习表达。
2. **工作台骨架**：左侧组织导航、中间队列/正文、右侧 Inspector/AI、底部 StatusBar，所有行为被 Command Palette 串起来。
3. **工程方法**：本地文件为真相、语义 token 先行、显式外部工具连接、TDD/回归/原生 QA/文档同步。

Buddy 的目标产品形态应升级为：

> WeChat-fed Purposeful Knowledge Workbench：一个本地优先、由微信捕获、由 AI 维护、人类审阅、以产出为目的的外脑工作台。

关键变化：

- 从“微信素材自动整理工具”升级为“有目的的知识生产系统”。
- 从“功能页面集合”升级为 Tolaria 式密集工作台。
- 从“Claude/ClawWiki 色彩移植”升级为 Tolaria 式语义 token 体系。
- 从“AI 问答页面”升级为“Agent/Inspector/StatusBar 串联的维护控制面板”。
- 从“schema 是规则文件”升级为“Rules Studio：用户教外脑如何整理”。

## 2. 分析依据

### 2.1 Tolaria 代码与设计输入

本次重点阅读：

- `src/index.css`：Tolaria 的语义 token、shadcn/Tailwind v4 映射、Markdown/AI 渲染样式。
- `src/theme.json`：编辑器排版、标题、列表、wikilink、blockquote、表格等内容 token。
- `src/App.tsx`、`src/App.css`、`src/hooks/useLayoutPanels.ts`：四栏工作台、可调整宽度、StatusBar、AI、Inspector、Welcome、Settings、Git/MCP/Agent 状态编排。
- `src/components/Sidebar.tsx`：Top nav、Favorites、Views、Types、Folder Tree、dnd-kit 键盘拖拽。
- `src/components/CommandPalette.tsx`：自研命令面板、fuzzy match、AI mode、键盘路由、拖入路径。
- `src/components/StatusBar.tsx` 与 `src/components/status-bar/*`：底部 30px 操作状态面板。
- `src/components/Inspector.tsx` 与 Inspector panels：frontmatter、properties、relationships、backlinks、referenced-by、git history。
- `src/components/AiPanel*.tsx`：右侧 AI Agent 面板、上下文条、wikilink 输入、流式状态。
- `design/design-full-layouts.pen`：1440x900 四栏布局、macOS titlebar、250px sidebar、300px note list、editor。
- `design/keyboard-first-nav.pen`：Note list 上下键/Enter、菜单快捷键、Inspector Tab+Enter。
- `design/ai-agent-panel-ui.pen`：36px trigger toolbar、320px AI panel、active streaming cards。
- `design/vault-agents-md.pen`：Vault root、AGENTS.md、Type sections、frontmatter/table/editor 视觉。
- `package.json`：Tauri v2、React 19、Tailwind v4、Radix/shadcn、dnd-kit、ReactMarkdown、CodeMirror、BlockNote、Vitest、Playwright。

### 2.2 Buddy 当前对照

本次重点阅读：

- `apps/desktop-shell/package.json`
- `apps/desktop-shell/src/globals.css`
- `apps/desktop-shell/src/shell/ClawWikiShell.tsx`
- `apps/desktop-shell/src/shell/Sidebar.tsx`
- `apps/desktop-shell/src/shell/clawwiki-routes.tsx`
- `apps/desktop-shell/src/features/palette/CommandPalette.tsx`
- `apps/desktop-shell/src/features/common/StatusLine.tsx`
- `apps/desktop-shell/src/features/ask/AskPage.tsx`
- `apps/desktop-shell/src/features/wiki/KnowledgeHubPage.tsx`
- `apps/desktop-shell/src/features/wiki/KnowledgeArticleView.tsx`
- `apps/desktop-shell/src/features/wiki/KnowledgePagesList.tsx`
- `docs/desktop-shell/architecture/overview.md`
- `docs/desktop-shell/tokens/design-tokens.md`
- `docs/desktop-shell/tokens/functional-tokens.md`
- `rust/README.md`

### 2.3 Starchild 信息架构输入

本轮补充分析：

- `https://iamstarchild.com/`：PWA / app shell，页面元信息为 “Starchild AI” 与 “The Agent OS. Launch your agent in seconds.”，manifest 将它描述为可 trading、building、automation 的 AI agent。静态资源显示其产品 IA 包含 Agent Hub、Task Detail、My Signals、My Fund Agent、Connections、agent market、login/wallet/google/telegram 等模块。
- `https://starchild.software/`：公开叙事页，按 “who it is -> first question -> how it listens -> growth map -> tiny steps -> privacy -> download” 组织，几乎不提前暴露复杂功能。

Starchild 对 Buddy 的可借鉴点不是视觉风格，而是 IA 降噪：

- 先给用户一个可理解的身份和第一步，再逐步暴露高级能力。
- 把复杂系统折叠成少数稳定意图：开始、成长地图、行动、隐私可信、下载/进入。
- 用 Home 讲“今天要做什么”，而不是让用户在功能列表里选择。
- 把信任能力放在主路径里解释，例如 private/local-first/encrypted，而不是藏在设置页。

### 2.4 源码复用与许可证边界

Tolaria 仓库声明 `AGPL-3.0-or-later`。本轮确认：Buddy 可以复制 Tolaria 源码，但这必须被当作正式源码复用，而不是随手粘贴。

执行边界：

- 团队接受 AGPL 对复制、修改、分发和网络交互场景可能带来的许可证义务后，才能把 Tolaria 代码合入 Buddy。
- 复制或派生的文件必须保留原许可证声明、版权声明和必要 attribution。
- 每次复制都要记录来源文件、来源 commit/ref、复制范围、修改说明、目标文件和评审人。
- `.pen` 设计资产、视觉资产、专有文案与源代码同等处理；无法确认授权时不得直接复制。
- 优先复制小而清晰的切片，例如 layout hook、token 映射、命令注册模式；复制后仍必须通过 Buddy 的 TDD、质量门禁、原生 QA 和文档同步。

## 3. Tolaria 的可迁移本质

### 3.1 产品本质：知识是有目的的

Tolaria 的根原则可以直接作为 Buddy 的产品北极星：

> 笔记存在是为了把事情做成。不是为了存起来留作某种“未来某天可能用到”。不是为了证明你有多有条理。是为了 do something。

这会改变 Buddy 的判断标准：

- 不是“是否成功把 raw 合并进 wiki”，而是“这条知识是否服务了一个可表达、可交付、可决策、可掌握的目的”。
- 不是“知识库页面数量”，而是“可复用知识单元、项目决策、流程改进、学习卡片、写作素材是否在持续产出”。
- 不是“AI 自动整理得多快”，而是“用户能否审阅、信任、追溯、复用”。

### 3.2 交互本质：工作台而非页面集合

Tolaria 的主界面不是传统网页路由，而是一个稳定工作台：

```text
macOS titlebar / native shell
  ↓
sidebar 250
  navigation / types / views / folders / root files
note list 300
  filtered queue / selected row / keyboard focus
editor flex
  markdown document / tabs / diff / raw editor
inspector or AI 280-320
  properties / relationships / backlinks / agent
status bar 30
  vault / sync / git / MCP / AI / zoom / settings
```

Buddy 当前有 `Home / Ask / Inbox / Wiki / WeChat / Raw / Graph / Schema / Settings`
等路由，也有 56px rail 和 Command Palette，但缺少 Tolaria 的“恒定工作台骨架”：

- 没有全局 StatusBar，只有 Ask/Inbox 可复用的局部 `StatusLine`。
- 没有全局右侧 Inspector/Agent 面板，Knowledge detail 只在文章页里局部展示 relations sidebar。
- Knowledge/Raw/Graph/Schema 仍是多个页面，而不是同一个知识工作台里的 lens。
- Rules 仍像开发者入口，没有被设计成用户调教外脑的 Studio。

### 3.3 Token 本质：语义角色先于品牌颜色

Tolaria 的 token 不是从品牌色开始，而是从 UI 角色开始：

- surfaces：`--surface-app/sidebar/panel/card/popover/input/button/dialog/editor/overlay`
- text：`--text-primary/secondary/tertiary/muted/faint/heading/inverse`
- border：`--border-default/subtle/strong/input/dialog/focus`
- state：`--state-hover/hover-subtle/selected/selected-strong/active/focus-ring/drag-target/disabled`
- accent：blue/green/orange/red/purple/yellow/teal/pink/gray
- feedback：info/success/warning/error text/bg/border
- syntax/diff：markdown、frontmatter、code highlight、diff added/removed/hunk
- aliases：shadcn/Tailwind v4 变量和旧 `--bg-*` 兼容别名

Buddy 当前 `globals.css` 已经有 warm ClawWiki v3 palette、Claude orange、focus blue、diff、state dot、shadcn aliases，但结构仍偏“品牌/历史兼容 token”。下一步应把 Tolaria 的语义角色层引入为 Buddy token v2，再把现有 Claude orange 变成用途明确的 action/maintainer/brand accent，而不是所有 active state 的默认颜色。

## 4. Buddy 产品定位重写

### 4.1 一句话

Buddy 是一个微信喂养的有目的知识工作台：它把你主动转发的内容沉淀为本地长期记忆，并通过可审阅维护队列、可追溯问答、可修改整理规则，把知识变成写作、项目、管理和学习表达的燃料。

### 4.2 用户承诺

- **我投喂，不整理**：微信转发、URL、文件、Ask 粘贴都先进入 raw，原始证据不丢。
- **AI 维护，我拍板**：结构化整理、合并、拆分、废弃、冲突处理先进入 Inbox。
- **每个结论有出处**：Wiki 页面、Ask 回答、Inbox diff 都能回到 raw/source/lineage。
- **知识服务产出**：每条知识尽量绑定写作、项目、责任、学习等目的。
- **规则我能教**：schema、templates、policies、guidance 都在 Rules Studio 可读可改。
- **数据在本地**：Buddy Vault 是普通文件夹，Git/remote/MCP/外部 AI 都是显式连接。

### 4.3 不做什么

- 不做通用 Tolaria/Obsidian 替代品。
- 不从空白笔记编辑器切入。
- 不把 AI 写入静默落盘。
- 不把私有云、微信中继或模型服务变成唯一真相。
- 不用营销式 landing page 包装主产品，桌面端第一屏就是工作台。

## 5. 目的模型：从 Capture 到 Express

Buddy 必须严格继承 Tolaria 的流动方向：

```text
capture -> organize -> express
```

对应 Buddy：

```text
capture
  WeChat / URL / file / text / Ask paste
  ↓
raw
  immutable source evidence
  ↓
organize
  maintainer proposal + Inbox review + Rules validation
  ↓
knowledge
  evergreen / topic / project / responsibility / procedure / decision / learning-card
  ↓
express
  cited answer / article outline / project memo / decision note / review report / study card
```

### 5.1 Purpose Lens 与扩展值

多数用户是多种角色的混合体，所以 Buddy 不应把用户锁进单一 Persona，而应提供 Purpose Lens：

| Lens | 用户是谁 | 产出 | Capture 服务什么 | Buddy 组织什么 | Express 形态 |
|---|---|---|---|---|---|
| Writing | 写作者、内容创作者 | 文章、随笔、帖子、报告 | 微信文章、高亮、语音观点、案例素材 | Evergreen、Topic、Compare、Quote | 大纲、论点、段落、长文草稿 |
| Building | 构建者、项目驱动者 | 产品、代码、方案、项目决策 | 技术资料、竞品、会议判断、需求片段 | Project、Decision、Procedure、Topic | PRD、技术方案、任务拆解、决策备忘 |
| Operating | 运营者、管理者 | 系统、流程、KPI、组织决策 | 复盘、客户反馈、指标变化、团队流程 | Responsibility、Procedure、People、Decision | 周报、复盘、会议准备、流程优化 |
| Learning | 学习者 | 掌握与表达 | 学科资料、课程笔记、论文、兴趣内容 | Learning Card、Topic、Project、Evergreen | 答题、讲解、研究提纲、申请材料 |
| Personal | 个人成长与生活管理者 | 反思、习惯、生活决策、个人项目 | 日记片段、灵感、情绪记录、生活问题、健康/财务/关系材料 | Personal Note、Decision、Responsibility、Project | 复盘、行动计划、个人决策备忘 |
| Research | 研究者、分析者、深度学习者 | 研究地图、文献综述、假设、分析 memo | 论文、访谈、数据点、引用、实验记录 | Source Note、Topic、Evergreen、Project、Decision | 研究提纲、文献矩阵、假设检验、分析报告 |

### 5.2 Purpose Lens 在 UI 中的角色

Purpose Lens 不应只是 schema 字段，而要进入主工作流：

- Home 显示 “本周每个目的吸收了什么、还能表达什么”。
- Inbox 审阅时必须能选择或确认目的。
- Knowledge 可按目的过滤，而不是只按页面类型过滤。
- Ask 可以限定目的，比如“基于 Building lens 回答这个技术决策”。
- Rules 可定义每个目的下的模板和维护策略。

Purpose Lens 已确认直接进入 frontmatter schema。建议首批稳定字段：

```yaml
purpose:
  - writing
  - research
type: evergreen
status: active
source_refs:
  - raw:2026-04-29-...
expressed_in:
  - ask:session-id
  - doc:project-memo
```

Schema 规则：

- `purpose` 是必填或强推荐字段，首批默认值域为 `writing | building | operating | learning | personal | research`。
- Purpose Lens 是可扩展受控词表，稳定定义放在 `schema/purpose-lenses.yml`；首批六类是产品默认，不阻止团队后续增加 organization-specific lens。
- 一个页面可属于多个 purpose，但 Inbox 审阅必须展示 maintainer 推荐理由。
- `purpose` 不只参与 UI 过滤，也参与 Ask source selection、Rules templates、patrol quality report。
- Maintainer 可以提出 purpose 变更，外部 AI 可以在受控写入范围内修改；所有非人工直接编辑都需要 Git diff、lineage 和可撤销记录。

## 6. 信息架构

### 6.1 顶层工作区

建议把 Buddy 当前路由收束为 6 个稳定工作区：

| 工作区 | 当前基础 | 用户问题 | 关键能力 |
|---|---|---|---|
| Home / Pulse | Dashboard、stats、patrol | 我的外脑今天是否健康？ | 健康体检、今日摄入、待审阅、质量风险、最近表达 |
| Ask | AskPage、source binding、SSE | 我知道什么？出处在哪里？ | 有出处问答、限定目的、引用、结晶为 raw/inbox |
| Inbox | InboxPage、candidate scoring、diff | AI 做完了哪些需要我拍板？ | 批量审阅、合并/新建/拒绝、冲突、Inbox Zero |
| Knowledge | KnowledgeHub、Raw、Graph | 我的长期记忆长什么样？ | 页面、邻域、反链、来源、关系图、原始素材 |
| Rules | SchemaEditor 迁移、templates、patrol | 我要如何教它整理？ | schema、templates、policies、guidance、校验报告 |
| Connections | WeChat、providers、MCP、Git | 哪些入口和 agent 已连接？ | 微信、模型、外部 AI、MCP、Buddy Vault/Git/remote |

Home/Pulse 是默认首页。`Raw / Graph / Schema` 不继续作为普通用户的一等主导航；它们成为 Knowledge 和 Rules 内的 lens/tab，高级用户仍可通过 Command Palette 打开。当前 `/schema` 应迁移为 `/rules`，旧路径保留 redirect 或兼容入口。

### 6.2 主工作台布局

Buddy 应引入 Tolaria 式可调整四区布局，但保留当前 56px rail 的优势：

```text
┌──────┬────────────────┬────────────────────────────┬──────────────────┐
│ Rail │ Purpose/Nav     │ Queue/List                  │ Inspector / Agent │
│ 56   │ 250 default     │ 300-360 or content list      │ 280-360 / 40      │
└──────┴────────────────┴────────────────────────────┴──────────────────┘
┌───────────────────────────────────────────────────────────────────────┐
│ Global StatusBar 30                                                    │
└───────────────────────────────────────────────────────────────────────┘
```

布局规则：

- Rail：保留当前 Buddy 56px icon rail，承载 6 个工作区。
- Secondary nav：Knowledge / Rules 默认展开 Tolaria-style 250px sidebar；Inbox / Ask 按上下文出现。sidebar 可折叠，但默认给用户稳定的空间地图，而不是每页重复大 header。
- Queue/List：Inbox、Knowledge、Raw、Search 共享 Tolaria NoteList 模式。
- Content：Ask conversation、Wiki reader、diff preview、Rules editor。
- Right pane：Inspector 与 Agent 二选一，宽度 280-360，可折叠到 40。
- StatusBar：全局常驻 30px，替代各页面零散状态。

参考 Tolaria 尺寸：

| 区域 | Tolaria | Buddy 建议 |
|---|---:|---:|
| macOS title/native chrome | 38 | 由 Tauri/native shell 决定 |
| rail | 无固定 rail | 56，沿用 Buddy |
| sidebar/nav | 250，min 180 | Knowledge/Rules 默认 250，min 180，其他工作区按需 220-280 |
| list | 300，min 220 | 300-360，min 220 |
| editor/content | min 800 | min 420，复杂页可独占 |
| inspector | 280，min 240 | 320 default，collapsed 40 |
| AI panel | 320 | 320 |
| status bar | 30 | 30 |
| command palette | 520-640 x 440 | 560 x 440 |

### 6.3 降低门槛的信息架构

Starchild 的公开站点把复杂产品压成一条低门槛叙事链：我是谁、第一问是什么、我如何陪你、成长地图是什么、下一步行动是什么、为什么可信、如何开始。Buddy 是工具型桌面产品，不能照搬沉浸式 landing page，但可以借这个“少数问题串起复杂能力”的 IA 方法。

Buddy 首页不应问用户“你要打开哪个模块”，而应先做“外脑健康体检”：用一屏告诉用户今天外脑是否健康、哪里有风险、下一步处理哪 3 件事。

| 用户心中问题 | Buddy 默认入口 | 暴露能力 |
|---|---|---|
| 今天外脑是否健康？ | Home/Pulse 健康体检 | capture、maintainer、Git、provider、risk、schema |
| 哪里需要我处理？ | Top 3 风险/待办 | failed capture、low confidence、schema violation、dirty Git |
| 我现在最该处理什么？ | 3 分钟 Inbox 卡片 | review queue、approve/reject、diff |
| 我能问它什么？ | Ask quick prompt | source binding、purpose mode |
| 我的知识在哪里？ | Buddy Vault 卡片 | raw/wiki/schema/.clawwiki、打开文件夹、Git |
| 它为什么可信？ | Trust strip | local-first、lineage、Git diff、受控写入 |
| 我如何教它？ | Rules Studio card | types、templates、policies、guidance |

健康体检首屏模块：

- Capture Health：微信/URL/文件摄入是否正常，最近失败是否需要重试。
- Maintenance Health：maintainer 是否有积压、低置信度、冲突、重复候选。
- Knowledge Health：孤立页、过期页、缺来源、缺 purpose、schema violation。
- Git/Vault Health：是否已初始化 Git、是否有未提交变更、是否需要 checkpoint。
- External Tool Health：provider、MCP、外部 AI 写入权限是否正常且未越权。
- Today Focus：把以上风险压成最多 3 个建议动作，主 CTA 永远指向第一项。

首次使用路径应压到 5 步内：

1. 选择或创建 Buddy Vault。
2. 连接微信或先导入一条 URL。
3. Home/Pulse 显示“已接收，正在维护”。
4. 进入 Inbox 审阅第一条建议。
5. Ask 基于刚批准的知识回答，并可结晶回 Inbox。

高级能力采用渐进披露：

- Git 出现在 StatusBar 和 Vault 卡片；新建 Buddy Vault 默认已初始化 Git，导入旧 Vault 或显式 opt-out 时才提示“启用版本历史”。
- Rules Studio 默认展示模板和说明，高级 YAML/CodeMirror 编辑折叠在 “Advanced”。
- 外部 AI/MCP 默认只读，用户显式开启后才允许受控写入。
- Graph/Raw/Lineage 默认作为上下文抽屉或 Inspector section，而不是强迫新手理解三套页面。

### 6.4 Rail 瘦身 + 单 CTA 硬约束

§6.1 / §6.3 已经把"6 工作区 + 渐进披露"作为方向。本节把它落到具体的
路由动作和评审硬约束。

#### 6.4.1 Rail 路由动作清单

当前 `apps/desktop-shell/src/shell/clawwiki-routes.tsx` 列了 14 条路由
（含 `dashboard / ask / inbox / wiki / wechat / raw / graph / cleanup /
breakdown / viewer / schema / settings / connect-wechat / ask.demo`）。
全部进入 sidebar 的 `primary | funnel | advanced | settings`，整体呈现
为一个长 rail。这是 v1 评审反馈中最直接的"门槛"来源。

v2 lock：rail 仅显示 7 项，分为三组，由分割线视觉降权：

```text
Daily（rail 上半）           Tune（rail 中段）        System（rail 底部）
  首页    /                     规则   /rules            设置 /settings
  问      /ask                  连接   /connections
  待整    /inbox  (badge •)
  知识    /wiki
```

退出 rail 的现有路由：

| 路由 | 去向 | 用户感知 |
|---|---|---|
| `/dashboard` | 重命名为 `/`，作为默认 Pulse | URL 变化，旧链接 redirect |
| `/schema` | 迁到 `/rules`，旧路径 redirect | Rules Studio 替换 |
| `/wechat`、`/connect-wechat` | 合并到 `/connections` 内 tab | 一处看完所有连接 |
| `/raw` | 进 `/wiki` 的 "原始素材" tab + 命令面板 | 不再是顶层 |
| `/graph` | 进 `/wiki` 的 "关系图" tab + 命令面板 | 不再是顶层 |
| `/cleanup` | 进 `/inbox` 的 "质量风险" 分组 + 命令面板 | 与 patrol 合流 |
| `/breakdown` | 仅命令面板 + Inbox row deep-link | 高级，不打扰新手 |
| `/viewer` | 仅命令面板 + raw row deep-link | 工具向 |
| `/ask.demo` | 仅命令面板，或下线 | 演示用，不应进 rail |

> 实现位点：`CLAWWIKI_ROUTES.section` 字段需要从 `primary | funnel |
> advanced | settings` 改为 `daily | tune | system | hidden`。`hidden`
> 表示路由仍存在、命令面板可达，但不渲染到 rail。

#### 6.4.2 单 CTA 硬约束（Starchild 直接借鉴）

每个 rail 顶层页面的可视 chrome（头部 80–96px）必须满足：

1. 一行工作区标题 + 一行 12–13px 说明（用普通中文，不是术语）。
2. **有且只有一个** 主按钮，颜色用 `--accent-primary`，圆角与
   `--radius-md` 一致。
3. 其它二级动作降级为：
   - 标题旁 16px icon 按钮（不超过 3 个）；
   - 工作区 secondary toolbar；
   - `⌘K` 命令面板。

每个页面的"唯一主 CTA"（v2 lock）：

| 工作区 | 主按钮文案 | 触发行为 |
|---|---|---|
| 首页 / Pulse | 健康风险 → "处理最重要 3 项" / Inbox Zero → "做一次体检" / 空 vault → "创建我的外脑" | 上下文敏感：先体检，再把用户推到当前最该做的一步 |
| 问问题 / Ask | "问外脑"（输入框默认聚焦） | `Enter` 提交 |
| 待整理 / Inbox | "审阅下一条"（队首高亮） | `Enter` |
| 知识库 / Knowledge | "打开页面…"（Quick-open，与 `⌘P` 一致） | 模糊匹配 |
| 整理规则 / Rules | "编辑当前类型"（默认聚焦最近改动的 Type） | 进入 CodeMirror |
| 连接 / Connections | "连接微信" / 已连接时变为 "查看健康" | 跳转或诊断 |
| 设置 / Settings | 不设主 CTA，纯陈列 | — |

PR 评审硬规则：任何工作区头部新增 primary 按钮的 PR，需要先在 spec 里
把这条 CTA 替代或解释清楚为什么破例。这是 §11 操作纪律的延伸。

#### 6.4.3 首屏文案锁定（避免冷启动空白）

Pulse `/` 头部第一行（按 vault 状态变化）：

| Vault 状态 | 第一行 | 第二行（说明） | CTA |
|---|---|---|---|
| 空 vault | 你的外脑还没诞生。 | 30 秒内创建本地外脑，数据全部留在你的电脑里。 | 创建我的外脑 |
| 新建后第一次 | 你的外脑刚刚醒来。 | 先问它一个问题，或者投喂一条素材。 | 投喂第一条 / 问外脑 |
| 有待审阅 | 外脑体检发现 K 项需要你确认。 | 其中 N 条来自今天的新素材。 | 处理最重要 3 项 |
| Inbox Zero | 外脑今天很干净。 | 没有待审阅风险，可以继续投喂或直接提问。 | 做一次体检 / 粘贴素材 |
| 离线 / 微信断开 | 外脑体检发现入口离线。 | 你仍可以在桌面粘贴素材。 | 修复连接 / 粘贴素材 |

这些文案进入 i18n 资源，用 `pulse.headline.<state>` / `pulse.cta.<state>`
命名，方便未来 a/b 与翻译。

#### 6.4.4 Narrative 与 Workbench 双密度

针对偶尔回来的用户与 power user 的张力，rail 之外引入两种密度切换
（`appSettingsStore.layoutDensity`，跟随机器）：

| 模式 | 默认场景 | 视觉 |
|---|---|---|
| Narrative | 首启、Pulse、Knowledge 阅读、单条 Inbox 详情 | 单列 max-width 720（与 Tolaria editor maxWidth 对齐），rail 折叠成 40，状态栏简化 |
| Workbench | Inbox 批量、Rules Studio、用户主动展开 | §6.2 完整四区 56 + 250 default + 300–360 + 320 |

切换通过工作区头部 toggle 或 `⌘.` 完成。命令面板里也提供 `view.density.toggle`
命令，支持回归测试。

> 落地说明：Narrative 模式不是新组件，而是给现有四区加 CSS class
> `data-density="narrative"`，配合 token 与媒体查询收敛。

## 7. 核心工作流设计

### 7.1 Capture：随手投喂

入口：

- 微信转发 URL/文字/文件/图片/语音。
- 桌面 Ask 粘贴 URL 或文本。
- 本地文件拖入。
- 未来浏览器剪藏。

规则：

- raw 是不可变证据，任何整理失败都不影响 raw。
- 捕获成功后先显示“已接收”，再显示“维护中/待审阅/无需整理”。
- 若 AI 无法归类，进入 Inbox，而不是静默丢弃。
- Capture 的状态进入全局 StatusBar，而不是藏在页面局部 toast。

### 7.2 Inbox：AI 维护，人类定夺

Inbox 应从“任务审批页”升级为“外脑维护队列”。

队列视图：

- 左：按风险和动作分组：Needs decision、Safe merge、Possible duplicate、Conflict、Stale、Low confidence。
- 中：选中项的 source summary、proposal、diff、candidate pages。
- 右：Inspector 展示目的、来源、lineage、schema violations、推荐动作。

行模型：

| 字段 | 说明 |
|---|---|
| recommended_action | merge / create / split / deprecate / reject |
| confidence | AI 提案可信度 |
| purpose | writing / building / operating / learning / personal / research |
| source_refs | raw/source 引用 |
| target | 推荐 wiki page |
| risk | destructive / duplicate / low-source / schema-violation |

关键交互：

- `J/K` 或上下键移动，Enter 打开详情。
- `A` approve，`R` reject，`M` merge，`N` create new，`Space` multi-select。
- 批量操作必须有 preview，不能直接写。
- Approve 后立即显示 lineage 写入和 git/local dirty 状态。
- Inbox Zero 不等于清空所有内容，而是所有需要人判断的维护都已处理。

### 7.3 Knowledge：从页面列表到知识地图

Knowledge 不只是 `pages / graph / raw` 三个 tab。建议重构为三栏：

```text
Purpose/Type/Source filters | Knowledge list | Reader + Inspector
```

默认阅读体验：

- 正文最大宽度 720，遵循 Tolaria `theme.json`。
- 右侧 Inspector 展示来源、反链、被引用、相关页、lineage、quality。
- 页面顶部不放营销式大 hero，只放紧凑 breadcrumb、title、status、purpose chips。
- 原始素材和关系图是 context lens，而非跳离当前心智的独立页。
- 用户可以直接编辑 `wiki/` 页面。默认使用 Markdown/YAML 编辑，不引入富文本复杂度。
- 用户可以修改 frontmatter 全量字段；常用字段在 Inspector 里表单化编辑，完整 YAML 通过 CodeMirror Advanced editor 暴露。
- 人工直接编辑写入磁盘；AI 和外部工具写入必须进入受控写入通道，保留 diff、lineage 和 Git checkpoint。

Knowledge 页面类型建议：

| 类型 | 用途 |
|---|---|
| evergreen | 小、原子、可复用的想法 |
| topic | 主题索引和学习/研究范围 |
| project | 有交付目标的工作上下文 |
| responsibility | 长期责任、KPI、流程 ownership |
| procedure | 重复工作的方法 |
| decision | 决策记录、取舍、依据 |
| learning-card | 学习卡片、概念掌握、复习点 |
| person | 人、组织、客户、作者 |
| source-note | 从 raw 派生的事实性摘记 |

### 7.4 Ask：问答可结晶

Ask 不是孤立聊天页，而是 Express 的主入口。

Ask 应支持：

- Source binding：当前已有 `wiki/raw/inbox` 绑定，应扩展到 purpose/project/responsibility。
- Purpose mode：用户可选择 “按写作/项目/管理/学习/个人/研究目的回答”。
- Citation first：回答中的关键判断必须能跳到 source/lineage。
- Crystallize：把一个好问题、好回答或回答片段保存为 raw/query，然后进入 Inbox 审阅。
- Open in Knowledge：回答引用的页面可直接在 Knowledge workbench 中打开，并保持右侧 Inspector。

建议命令：

- `Ask with current page`
- `Ask with selected raw`
- `Crystallize answer into Inbox`
- `Create writing outline from selected sources`
- `Create decision memo from this conversation`

Wiki 直接编辑规则：

- Reader 顶部提供 `Edit`，进入 CodeMirror Markdown mode。
- Frontmatter 与正文同屏编辑；用户允许修改 frontmatter 全量字段，purpose/type/status/source_refs 等关键字段需要 schema validation。
- 基础模式优先展示 schema-aware 属性表单；Advanced YAML/CodeMirror 可展开编辑完整 frontmatter，默认折叠。
- 必填字段、未知枚举、破坏性路径等硬错误阻止保存；非关键字段和推荐字段缺失以 warning 形式进入 Inspector。
- 保存前展示 dirty state；Git 已启用时建议生成 commit message，未启用时提示开启版本历史。
- 保存后写入 lineage：`human_edit_wiki_page`。
- 外部 AI 修改 wiki 页面时必须创建 proposed diff；用户可选择 approve，也可授予受控自动写入。

### 7.5 Rules Studio：用户教外脑

Tolaria 把 Type documents、frontmatter、root guidance 做成 AI 与用户共享的语义层。Buddy 应把当前 SchemaEditor 升级为 Rules Studio：

| Tab | 内容 | 技术基础 |
|---|---|---|
| Types | page types、fields、required/optional、icon/color | schema files |
| Templates | evergreen/project/decision/procedure/learning-card 模板 | markdown + YAML |
| Policies | merge/split/deprecate/confidence/source rules | schema/policy docs |
| Guidance | AGENTS.md / CLAUDE.md / external agent instructions | root shims |
| Validation | patrol、schema violations、orphan/stale/stub reports | wiki_patrol |

Rules Studio 替代当前独立 Schema Editor 页面。`/schema` 作为兼容路径跳转到 `/rules`，Rail 和 Command Palette 对普通用户只展示 Rules。

编辑器建议：

- 近期引入 CodeMirror 6，专用于 YAML/Markdown rules、templates、guidance。
- Advanced YAML / CodeMirror 默认折叠；首屏展示可读说明、模板预览、字段表单和验证结果，避免新手先看到规则源码。
- 保留 ReactMarkdown preview。
- 暂不引入 BlockNote，除非 Buddy 后续明确需要富文本笔记编辑。Tolaria 使用 BlockNote 是因为它是编辑器产品，Buddy 不是。

### 7.6 Connections：显式外部连接

Tolaria 的 Git/MCP/AI agent 设计强调显式连接和 least privilege。Buddy 应把所有外部能力放进 Connections。Git 成为一等能力，外部 AI 首期允许受控写入：

- WeChat：通道状态、最近摄入、失败重试。
- Model provider：当前 provider/model、健康、切换。
- External AI/MCP：安装状态、权限范围、可用工具、最后调用。
- Vault：本地路径、打开文件夹、备份、导出。
- Git/Remote：未启用/初始化、本地 changes、commit、remote connected、conflict、pull required。
- External AI Write Access：默认只读；用户可授权 scoped write，范围限定到 `wiki/`、`schema/templates`、root guidance 或当前选中页面。

这些状态同时在 StatusBar 提供一键入口。

受控写入策略：

- 外部 AI 读：默认允许读取用户授权的 Buddy Vault 范围。
- 外部 AI 写 `raw/`：允许作为新 capture 追加，不覆盖旧 raw。
- 外部 AI 写 `wiki/`：需要 diff preview；用户可选择单次批准或对明确 scope 开启自动写入。
- 外部 AI 写 `schema/templates`：允许进入受控写入范围，但默认需要 diff preview、schema validation 和 Git checkpoint。
- 外部 AI 写 `schema/` 其他规则或 root guidance：默认需要人工批准；开启自动写入必须显示高风险状态。
- 受控自动写入授权分两级：
  - 本次会话有效：只在当前 app session / agent task 内生效，结束后自动回到只读。
  - 永久规则：写入 Rules/Connections，可撤销、可审计，并在 StatusBar 长期显示授权 badge。
- 永久规则必须比会话授权更窄，例如限定到 `wiki/project-x/**` 或 `schema/templates/decision.md`，不能默认给整个 Vault。
- 每次写入记录 lineage，并在 Git 已启用时产生可提交 diff。

## 8. 设计 Token 方案

### 8.1 Token 策略

Buddy 应新增 Tolaria-style semantic token contract。现有 Claude/ClawWiki token 不应立刻删除，而应迁移为别名层：

```text
semantic role tokens
  --surface-*
  --text-*
  --border-*
  --state-*
  --accent-*
  --feedback-*
  --syntax-*
  --diff-*
    ↓
shadcn / Tailwind aliases
  --background / --foreground / --card / --primary / --border / --ring
    ↓
legacy Buddy aliases
  --claude-orange / --claude-blue / --color-* / --deeptutor-*
```

迁移原则：

- Active selection、links、focus 默认使用 Tolaria blue。
- Claude/Buddy orange 保留为 brand、maintainer/action、running、capture attention。
- Warm surfaces 保留，但减少当前 `ds-canvas` 径向光斑和装饰性渐变，让工作台更安静、更耐用。
- 设计 token 文档通过评审后更新 `docs/desktop-shell/tokens/design-tokens.md`。

### 8.2 核心颜色建议

Light：

| Token | 值 | 用途 |
|---|---|---|
| `--surface-app` | `#FFFFFF` 或 Buddy parchment variant | App 背景 |
| `--surface-sidebar` | `#F7F6F3` | 导航/状态栏 |
| `--surface-panel` | `#FFFFFF` | 列表/Inspector |
| `--surface-card` | `#FFFFFF` | 行、面板内卡片 |
| `--text-primary` | `#37352F` | 主文字 |
| `--text-secondary` | `#787774` | 辅助文字 |
| `--border-default` | `#E9E9E7` | 默认分割 |
| `--state-hover` | `#EBEBEA` | hover |
| `--state-selected` | `#E8F4FE` | 选中行 |
| `--accent-blue` | `#155DFF` | link/focus/selected/primary nav |
| `--accent-orange` | `#D9730D` 或 Buddy terracotta | maintainer/capture/running |

Dark：

| Token | 值 | 用途 |
|---|---|---|
| `--surface-app` | `#1F1E1B` | App 背景 |
| `--surface-sidebar` | `#191814` | 导航/状态栏 |
| `--surface-panel` | `#23221F` | 列表/Inspector |
| `--surface-popover` | `#292823` | 弹层 |
| `--text-primary` | `#E6E1D8` | 主文字 |
| `--text-secondary` | `#B8B1A6` | 辅助文字 |
| `--border-default` | `#34322D` | 默认分割 |
| `--state-hover` | `#2D2B27` | hover |
| `--state-selected` | `#1E344C` | 选中行 |
| `--accent-blue` | `#78A4FF` | link/focus/selected |
| `--accent-orange` | `#F3A15B` | maintainer/capture/running |

### 8.3 Typography 与密度

Tolaria 内容排版可直接成为 Buddy 阅读区标准：

| 场景 | 建议 |
|---|---|
| UI row title | 13px / 500-600 |
| UI metadata | 11-12px |
| StatusBar | 11px / 30px high |
| Reader body | 15px / line-height 1.5 |
| Reader max width | 720px |
| H1 | 32px / 700 / 1.2 |
| H2 | 27px / 600 / 1.4 |
| H3/H4 | 20px / 600 |
| Inline code | 14px mono / radius 3 |
| List indent | 24px |
| Blockquote | 3px left border, accent blue |

Buddy 当前 `globals.css` 的 micro/nano/caption/body/head scale 可以保留，但要把阅读区和工作台 chrome 分开：工作台密集，正文舒展。

### 8.4 Components Tokenization

必需新增或规范以下组件角色：

| Component | Token 角色 |
|---|---|
| `BuddyRail` | `surface-sidebar`, `border-default`, `state-hover`, `accent-blue/orange` |
| `PurposeSidebar` | `surface-sidebar`, section label 10-11px, row 32-36px |
| `KnowledgeList` | `surface-card`, selected `state-selected`, row 48-64px |
| `Reader` | `surface-editor`, `text-primary`, `syntax-*` |
| `Inspector` | `surface-panel`, `border-default`, row 36px |
| `AgentPanel` | `surface-panel`, active pulse `accent-blue` |
| `StatusBar` | `surface-sidebar`, 30px, badges 11px |
| `CommandPalette` | `surface-popover`, width 560, top 15vh, shadow dialog |

## 9. 交互设计方案

### 9.1 Command Palette

Buddy 现有 `cmdk` 全局面板已经具备基础搜索、分组、二级 action chips。建议升级为 Tolaria-style command registry：

- 命令来自统一 registry，而不是散在 palette item builder。
- 支持 group sort order，空 query 显示最近命令和当前上下文命令。
- 支持 AI/Purpose mode：输入前缀 `?` 或空格进入 Ask with context。
- 支持路径拖入、文件 source bind。
- 每个命令有 keyboard shortcut manifest，方便测试和菜单栏复用。

建议分组：

| Group | 示例 |
|---|---|
| Navigate | Open Home / Ask / Inbox / Knowledge / Rules / Connections |
| Capture | Add URL / Import file / Open WeChat / Retry failed capture |
| Review | Approve selected / Reject / Batch preview / Open Inbox Zero |
| Knowledge | Open page / Open lineage / Show backlinks / Open raw source |
| Express | Ask with page / Crystallize answer / Create outline / Create decision memo |
| Rules | Edit type / Validate schema / Open guidance / Run patrol |
| System | Open vault / Settings / Check provider / Toggle theme |

### 9.2 StatusBar

新增全局 `BuddyStatusBar`，替代页面级零散状态：

Left：

- Vault path / local-only / Git / remote
- Capture status：WeChat connected、last ingest、failed count
- Maintainer status：running、pending Inbox、patrol warnings

Right：

- Provider/model
- External AI/MCP status
- Theme/zoom
- Settings

状态颜色：

- idle：text-secondary
- running：accent-orange
- ok：accent-green
- warning：accent-orange/yellow
- error：accent-red
- selected/focus：accent-blue

### 9.3 Inspector / Agent Pane

右侧应成为全局上下文面板，而不是某个页面的局部 aside：

| Mode | 用途 |
|---|---|
| Inspector | properties、purpose、source refs、lineage、backlinks、quality |
| Agent | 对当前上下文发起维护、摘要、写作、决策、学习操作 |
| Activity | maintainer logs、SSE progress、recent writes |

Inspector 在不同工作区显示不同 sections：

- Inbox：proposal、risk、candidate targets、lineage、schema violations。
- Knowledge：properties、source refs、backlinks、referenced by、quality、git/history。
- Ask：used sources、citations、crystallization queue。
- Rules：validation results、affected pages、policy explain。

### 9.4 Keyboard-first

最低键盘协议：

| 快捷键 | 行为 |
|---|---|
| `Cmd/Ctrl+K` | Command Palette |
| `Cmd/Ctrl+Shift+F` | Global search |
| `Cmd/Ctrl+L` | Source/URL capture |
| `Cmd/Ctrl+Enter` | Ask/send/approve contextual primary action |
| `Esc` | close panel / clear selection / stop streaming |
| `J/K` 或上下键 | list navigation |
| `Enter` | open selected |
| `Space` | multi-select in Inbox/list |
| `A/R/M/N` | Inbox approve/reject/merge/new |
| `Cmd/Ctrl+[` / `]` | back/forward |
| `Cmd/Ctrl+Shift+I` | toggle Inspector |
| `Cmd/Ctrl+Shift+A` | toggle Agent |

所有快捷键必须进入 command registry 和测试 manifest。

## 10. 技术架构与选型

### 10.1 前端

Buddy 当前依赖已经足够支撑 Tolaria 化：

- Tauri v2
- React 19
- TypeScript
- Vite
- Tailwind v4
- Radix/shadcn
- lucide-react
- cmdk
- TanStack Query
- Zustand
- react-markdown / remark-gfm
- dnd-kit
- Playwright

确认技术栈：

- 保持 React/Tauri/Rust，不引入 Electron。
- 保持 shadcn/Radix，token 通过 CSS variables 注入。
- Command Palette 继续用 `cmdk`，但数据层升级为 command registry。
- Markdown reader 继续用 `react-markdown`。
- Wiki 直接编辑和 Rules Studio 使用 CodeMirror 6，而不是 Monaco。
- 知识文件格式继续采用 Markdown + YAML frontmatter。
- 外部 AI 连接采用 MCP / CLI agent，首期支持受控写入。
- BlockNote 暂不引入主线；Buddy 不是笔记编辑器，富文本编辑会放大复杂度。
- 大列表可评估 `@tanstack/react-virtual` 或 `react-virtuoso`，Buddy 已有 `@tanstack/react-virtual`。

### 10.2 Rust / 本地服务

保持现有 Rust workspace：

- `desktop-core`：session、provider、persistence integration。
- `desktop-server`：HTTP/SSE API surface。
- `wiki_store`：on-disk raw/wiki/inbox/schema storage。
- `wiki_ingest`：URL/file/HTML/Markdown ingest。
- `wiki_maintainer`：LLM-backed absorb/query maintainer。
- `wiki_patrol`：质量巡检。

Tolaria 的 local-first vault 原则映射到 Buddy：

- 文件系统仍是真相。
- HTTP/SSE 是 desktop shell 和 Rust 的边界，不让 React 直接写复杂文件。
- `.clawwiki/lineage.jsonl`、reports、cache 是可审计运行痕迹。
- Git 是一等能力：Home/Pulse、StatusBar、Inbox、Wiki edit、Rules edit、外部 AI 写入都需要展示 Git/diff 状态。新建 Buddy Vault 默认执行 `git init`；导入既有非 Git Vault 或用户显式 opt-out 时仍可打开，但 StatusBar 和 Vault 卡片应持续提示可开启版本历史。

### 10.3 Vault 结构建议

建议把 Buddy Vault 产品化为用户可理解结构：

```text
Buddy Vault/
  raw/
    2026/
      04/
        ...
  wiki/
    evergreen/
    topic/
    project/
    responsibility/
    procedure/
    decision/
    learning-card/
    person/
  schema/
    types/
    templates/
    policies/
    purpose-lenses.yml
  .clawwiki/
    inbox.jsonl
    lineage.jsonl
    reports/
    cache/
    providers.json
  AGENTS.md
  CLAUDE.md
```

Root guidance 文件是外部 AI 的入口 shim：

- `AGENTS.md`：说明 Buddy Vault 的结构、写入边界、必须走 Inbox 的规则。
- `CLAUDE.md`：如果需要兼容 Claude Code，可指向同一规则。
- 真实规则仍由 `schema/` 和 `.clawwiki/` 管理，避免重复正文。
- Root shims 进入近期范围，和 Rules Studio 的 Guidance tab 同步管理。

### 10.4 API/状态分层

前端状态：

- Router：工作区与 URL。
- TanStack Query：server state、wiki/inbox/raw/lineage/patrol/providers。
- Zustand：UI state、panel layout、command palette、settings、current context。
- SSE：maintainer progress、Ask streaming、capture events、patrol activity。

服务端 API 方向：

- `/api/wiki/*`：raw/wiki/inbox/schema/lineage/patrol。
- `/api/desktop/*`：session/provider/settings/permission。
- `/api/connections/*`：wechat/mcp/git/external AI health。
- `/api/git/*`：init/status/diff/commit/remote/conflict。
- `/api/events/*`：capture/maintainer/activity stream。

## 11. 研发方式借鉴

Tolaria 的研发方式应被 Buddy 明确采用：

### 11.1 Main-only + Spec -> Plan -> Implementation

Buddy 采用 Tolaria 式 main-only 工作流。含义不是降低代码审查标准，而是把质量前移到本地验证、自动化检查、文档同步和小步提交：

- 产品结构变化先写 spec。
- 评审通过后拆 plan。
- Durable choices 写 ADR 或更新 architecture/tokens/operations。
- 稳定落地后回填 `docs/desktop-shell/architecture/`、`tokens/`、`operations/`。
- 直接在 main 线推进时，每个切片必须小、可回滚、可验证。
- 合并前必须跑对应质量门禁；不能用“后续 PR 修”替代最低验证。
- 涉及 AI 写入、Git、Rules、Vault 结构的变更必须同时补测试和操作文档。

### 11.2 Token-first UI

- 新 UI 先定义 token roles，再写组件样式。
- 禁止直接堆硬编码颜色作为主方案。
- 迁移过程允许 legacy aliases，但新组件只读 semantic tokens。

### 11.3 Testable interaction

- Command registry 需要可测试 manifest。
- 快捷键、菜单、palette 路由使用同一份定义。
- Inbox 决策、candidate scoring、schema validation 优先单元测试。
- Ask streaming、Command Palette、Inbox approval 走 Playwright 回归。

### 11.4 Native QA

- Tauri shell 至少覆盖 build、cargo check、关键交互 smoke。
- 对 StatusBar、Inspector、Agent、Command Palette 进行桌面窗口尺寸 QA。
- 对 light/dark、中文长标题、窄宽度、空状态、错误状态做截图检查。

### 11.5 Docs sync

落地时必须同步：

- 结构变更：`docs/desktop-shell/architecture/`
- token 变更：`docs/desktop-shell/tokens/`
- 操作/验证变更：`docs/desktop-shell/operations/`
- 新功能设计与执行：`spec` + `plan`

## 12. 分阶段路线

### Phase 0：评审与边界确认

产物：

- 本 spec 按确认项修订。
- Operations 纳入 main-only、spec/plan、TDD、质量门禁、原生 QA、文档同步。
- 确认第一批实施 plan 的切片顺序。

### Phase 1：Token 与 Shell 骨架

目标：

- 新增 semantic token contract。
- 降低 `ds-canvas` 装饰性背景，建立 Tolaria-style quiet workbench。
- Home/Pulse 成为默认首页。
- Home/Pulse 首屏改为外脑健康体检。
- 新增全局 StatusBar。
- 为右侧 Inspector/Agent 预留 shell slot。
- Knowledge / Rules 默认展开 250px sidebar。
- Buddy Vault 卡片公开展示本地路径、Git 状态和目录解释；新建 Vault 默认初始化 Git。

验证：

- `npm run build`
- light/dark visual QA
- narrow/desktop widths

### Phase 2：Purpose Lens 数据与 UI

目标：

- frontmatter schema 增加 purpose 字段和值域，默认包含 `writing / building / operating / learning / personal / research`。
- Inbox proposal 支持 purpose 建议。
- Knowledge list 支持 purpose filter。
- Ask 支持 purpose mode。
- Root `AGENTS.md` / `CLAUDE.md` shim 由 Rules Studio 生成和同步。

验证：

- candidate scoring/purpose assignment 单元测试。
- Inbox proposal fixture。
- guidance shim 重复正文检测。

### Phase 3：Knowledge Workbench

目标：

- Knowledge 从 tabs 改为 Tolaria-style list + reader + inspector。
- Raw 和 Graph 变成 context lens。
- WikiArticle 右侧 Inspector 展示 source refs、backlinks、lineage、quality。
- Wiki 页面支持直接 Markdown/YAML 编辑，使用 CodeMirror 6。
- Wiki 编辑允许修改 frontmatter 全量字段，Advanced YAML 默认折叠。
- 保存时集成 schema validation、lineage、Git diff。

验证：

- 页面打开、搜索、反链、source 跳转。
- 关系图不破坏主工作台。
- Wiki edit 保存/取消/失败/dirty state 回归。

### Phase 4：Inbox Zero Workbench

目标：

- Inbox 三栏化。
- 批量预览、风险分组、键盘审阅。
- Approve/reject/merge/new 的 lineage 与 status bar 联动。
- Git diff/commit 状态成为 Inbox 审阅的一等信息。

验证：

- queue intelligence 单元测试。
- Playwright approval/rejection smoke。

### Phase 5：Rules Studio

目标：

- Rules Studio 替代 SchemaEditor，`/schema` 兼容跳转到 `/rules`。
- 提供 Types/Templates/Policies/Guidance/Validation。
- 引入 CodeMirror 6。
- Advanced YAML / CodeMirror 默认折叠。
- patrol report 与 schema validation 集成。

验证：

- schema 保存、校验、preview。
- guidance 文件生成不制造重复正文。

### Phase 6：Connections 与外部 Agent

目标：

- Connections 聚合 WeChat、provider、MCP、Git/remote、external AI。
- StatusBar badges 一键打开对应面板。
- 外部 AI least privilege setup，并支持 `wiki/`、`schema/templates`、root guidance 的受控写入范围。
- 受控自动写入支持“本次会话有效”和“永久规则”两种授权级别。
- Git init/status/diff/commit/remote/conflict 进入一等连接能力。

验证：

- offline/no remote/no provider/no WeChat 的明确状态。
- MCP/外部 AI 不可用时不阻塞本地使用。
- 外部 AI controlled write 的 scope、diff、lineage、Git checkpoint 测试。

## 13. 最终决议

以下评审问题已经收敛为产品决策，后续 plan 不再把它们作为开放问题处理：

| 议题 | 决议 |
|---|---|
| Purpose Lens 值域 | 首批包含 `writing / building / operating / learning / personal / research`，并通过 `schema/purpose-lenses.yml` 保持可扩展。 |
| Knowledge / Rules sidebar | 默认展开 Tolaria-style 250px sidebar；可折叠，但默认给用户稳定地图。 |
| Home/Pulse 首屏 | 偏“外脑健康体检”，先展示健康、风险、Top 3 建议动作，再进入具体审阅。 |
| Wiki frontmatter 编辑 | 允许修改 frontmatter 全量字段；基础属性表单 + Advanced YAML/CodeMirror，保存前强制 schema validation。 |
| 外部 AI 写入范围 | 受控写入允许 `wiki/`、`schema/templates`、root guidance 和当前选中页面；高风险范围需要显著状态。 |
| Git 默认策略 | 新建 Buddy Vault 默认初始化 Git；导入旧 Vault 或显式 opt-out 才走非 Git 路径。 |
| 自动写入授权级别 | 同时支持“本次会话有效”和“永久规则”；永久规则必须更窄、可撤销、可审计。 |
| Rules Studio Advanced | Advanced YAML / CodeMirror 默认折叠，新手首屏看到说明、模板、字段和校验。 |
| Tolaria 源码复用 | 可复制 Tolaria 源码，但必须按 AGPL 义务、来源记录、许可证保留、差异说明和质量门禁执行。 |

评审会后需要拆分的不是“是否要做”，而是各项进入哪个 milestone、由哪些测试和文档回填兜住。

## 14. 成功标准

产品评审通过后的第一阶段成功标准：

- 用户能清楚看到 Buddy Vault 在哪里、包含什么、如何被维护。
- 新摄入内容从微信到 raw、Inbox、Knowledge、Ask 的链路可解释。
- Inbox 不再只是审批列表，而是有风险、目的、来源、diff 的维护工作台。
- Knowledge detail 默认展示来源、反链、lineage、quality。
- 用户可以直接编辑 wiki 页面，并在保存前看见 schema/Git/lineage 影响。
- Ask 的回答能被结晶回 Inbox。
- Rules Studio 取代独立 Schema Editor，让用户知道“我可以教 Buddy 怎么整理”。
- StatusBar 让用户一眼知道 capture、maintainer、provider、vault、external tools 的状态。
- Home/Pulse 以外脑健康体检为首屏，让新手不用理解所有模块，也知道今天哪里健康、哪里需要处理。
- Git、外部 AI 写入、root guidance 都是可见、可控、可撤销的。
- Tolaria 源码复用有清晰 provenance、许可证声明、修改说明和评审记录。
- 新 UI 主要读 semantic tokens，而不是硬编码品牌色。

## 15. 一句话给评审会

Tolaria 教给 Buddy 的不是“做一个更漂亮的笔记应用”，而是：

> 用本地文件、稳定工作台、语义 token、键盘命令和 AI 协作，把知识从“保存”推到“产出”。

Buddy 要把这套系统接到自己的强入口：微信捕获。这样它才不是另一个知识库，而是一个真正能长期为用户买单的外脑工作台。

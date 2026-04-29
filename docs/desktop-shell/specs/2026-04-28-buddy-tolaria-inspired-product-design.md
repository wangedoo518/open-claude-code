---
title: Buddy Tolaria-Inspired Product Design Proposal
doc_type: spec
status: superseded
owner: desktop-shell
last_verified: 2026-04-28
source_of_truth: false
related:
  - docs/desktop-shell/README.md
  - docs/desktop-shell/architecture/overview.md
  - docs/desktop-shell/tokens/functional-tokens.md
  - docs/desktop-shell/specs/README.md
  - docs/design/technical-design.md
  - rust/README.md
superseded_by:
  - docs/desktop-shell/specs/2026-04-29-buddy-tolaria-deep-product-design.md
---

# Buddy Tolaria-Inspired Product Design Proposal

> Superseded by
> [Buddy Tolaria Deep Product Design](./2026-04-29-buddy-tolaria-deep-product-design.md).

本文用于团队评审：在仔细阅读
`/Users/champion/Documents/develop/tolaria` 后，提炼 Tolaria 的产品、交互、
架构思想，并映射到 Buddy 当前的 `desktop-shell` / `rust` 产品主线。

本文不是当前实现真相，不直接等同于实施计划。评审通过后，再拆成 plan
与具体任务。

## 1. 执行结论

Buddy 不应该复制 Tolaria 成为一个通用笔记编辑器。Buddy 的核心楔子仍然是：

> 微信里随手投喂，桌面端由 AI 维护成可追溯、可审阅、可问答的长期记忆。

Tolaria 最值得借鉴的不是某个组件，而是一套自洽系统：

- 知识必须服务于产出。保存不是目的，持续写作、构建、决策、学习表达才是目的。
- 本地文件是用户信任的地基。
- 约定优于配置，让人和 AI 都能读懂知识库。
- Capture 与 Organize 分离，Inbox 是派生状态，不是文件夹。
- 关系是一等公民，知识浏览不是文件列表，而是邻域、反链、来源和时间线。
- AI 不是“聊天功能”，而是能读写知识库的本地协作者，但写入必须受边界和授权控制。
- Onboarding 交付方法论，而不是只交付空壳应用。

Buddy 的产品方向应从“微信素材自动整理工具”升级为：

> WeChat-fed AI Memory Workbench：一个本地优先、来源可追溯、由 AI 维护、人类审阅的外脑工作台。

## 2. 分析输入

### Tolaria 已阅读材料

- `README.md`
- `docs/VISION.zh-CN.md`
- `docs/ABSTRACTIONS.zh-CN.md`
- `docs/ARCHITECTURE.zh-CN.md`
- `docs/GETTING-STARTED.md`
- 关键 ADR：
  - `0002-filesystem-source-of-truth.md`
  - `0011-mcp-server-for-ai-integration.md`
  - `0020-keyboard-first-design.md`
  - `0028-cli-agent-only-no-api-key.md`
  - `0051-shared-shortcut-manifest-for-testable-routing.md`
  - `0062-selectable-cli-ai-agents.md`
  - `0065-root-managed-ai-guidance-files.md`
  - `0070-starter-vaults-local-first-with-explicit-remote-connection.md`
  - `0074-explicit-external-ai-tool-setup-and-least-privilege-desktop-scope.md`
  - `0085-non-git-vault-support.md`

### Buddy 已对照材料

- `README.md`
- `docs/desktop-shell/README.md`
- `docs/desktop-shell/architecture/overview.md`
- `docs/desktop-shell/tokens/*`
- `docs/design/technical-design.md`
- `docs/design/modules/*`
- `apps/desktop-shell/src/shell/clawwiki-routes.tsx`
- `apps/desktop-shell/src/features/ask/*`
- `apps/desktop-shell/src/features/wiki/*`
- `apps/desktop-shell/src/features/inbox/*`
- `rust/crates/wiki_store`
- `rust/crates/wiki_maintainer`
- `rust/crates/wiki_patrol`
- `rust/README.md`

## 3. Tolaria 的核心产品发现

| Tolaria 机制 | 产品含义 | Buddy 借鉴方式 |
|---|---|---|
| Files-first vault | 用户真正拥有数据；App 只是读取和写入文件 | 把 `~/.clawwiki/` 产品化为 Buddy Vault，而不是隐藏实现细节 |
| Git-first / non-git supported | 历史、审计、同步可选；普通文件夹可先打开 | Buddy 可先 local-only，后续显式接入 Git/remote，不阻塞微信投喂 |
| 知识是有目的的 | 笔记存在是为了 do something，而不是收藏 | Buddy 必须把每条知识绑定到写作、项目、决策、学习等表达目标 |
| 约定优于配置 | `type/status/belongs_to` 等字段自动触发 UI 行为 | 把 Buddy 的 schema v1 字段升级为用户可理解的语义约定 |
| Capture -> Organize -> Express | 捕获快，组织慢，表达可复用 | 对应 Buddy 的 WeChat ingest -> Inbox review -> Ask/report/crystallize |
| Inbox 是派生状态 | 未组织内容自然出现，连接后自动消失 | Buddy Inbox 应从“待审批任务”升级成“外脑维护队列” |
| 关系一等公民 | 笔记不只是文档，而是知识图节点 | Buddy 页面详情必须展示来源、反链、相关页、冲突、可信度 |
| Type documents | 类型本身也是可编辑知识对象 | Buddy 的 schema/templates/policies 应进入 Rules Studio，而非隐藏文件 |
| Root AGENTS/CLAUDE guidance | 外部 agent 一进 vault 就懂规则 | Buddy Vault 应暴露 root guidance shim，指向 schema 的真实规则 |
| CLI agent + MCP | AI 可以用标准工具读写 vault | Buddy 可提供显式外部 AI 工具连接，但默认最小权限 |
| Keyboard/command-first | 所有能力可从命令面板触达 | Buddy 的 Ask、Absorb、Review、Open Source、Open Lineage 应统一命令化 |
| Starter vault | Onboarding 交付方法论 | Buddy 应提供“示例外脑”，让用户体验投喂、审阅、问答闭环 |
| 严格研发纪律 | TDD、ADR、文档同步、质量门禁、原生 QA | Buddy 应把设计评审、实现、验证、文档回填作为同一条流水线 |

## 4. Buddy 当前基线

Buddy 已经拥有 Tolaria 没有的强楔子：

- 微信是低摩擦捕获入口，用户不用打开桌面端也能投喂。
- `raw/ -> wiki/ -> schema/ -> inbox` 已经是明确的三层文件系统。
- `wiki_maintainer` 已经实现“AI 提案，人类批准”的维护范式。
- Ask 已经接入 session、provider、source binding、SSE 和 citations。
- Inbox 已经有 diff preview、candidate scoring、duplicate guard、combined merge。
- `wiki_patrol` 已经有 orphan/stale/schema/stub/oversized/confidence decay 等质量检测。
- provenance/lineage 已经有 `.clawwiki/lineage.jsonl` 事件基础。

当前主要差距不在后端能力，而在产品心智：

- 用户不容易感知自己拥有一个可携带的 Buddy Vault。
- `raw/wiki/schema/.clawwiki` 的层级对用户不可见，因此信任感弱。
- Inbox 更像审批中心，还没有形成 Tolaria 式 Inbox Zero 纪律。
- Knowledge Hub 目前是页面/关系图/素材库入口，但缺少“邻域浏览”和“来源审计”作为默认阅读体验。
- Schema/Rules 是开发者感强的规则文件，还没有成为“你教外脑如何整理”的产品入口。
- Ask 与 Wiki 的关系仍像“问答页引用知识库”，还没有形成“问答结果可结晶回外脑”的闭环心智。
- 外部 AI 工具/MCP/agent 的能力没有变成可评审的用户级产品承诺。

## 5. 产品定位

### 一句话

Buddy 是一个微信喂养的 AI 外脑：它把你主动转发的内容沉淀为本地长期记忆，并让你用有出处的回答、可审阅的维护队列和可修改的整理规则来控制它。

### 不做什么

- 不做通用 Obsidian/Tolaria 替代品。
- 不让用户从空白笔记开始构建系统。
- 不把所有 AI 写入都自动落盘。
- 不把私有云、微信中继或 LLM gateway 当成用户数据的唯一归宿。
- 不复制 Tolaria 的品牌、文案、代码或视觉资产；只借鉴产品模式和架构原则。

### 设计原则

1. **微信捕获优先**：捕获入口必须比任何手动笔记软件更轻。
2. **知识服务产出**：每条知识最终都应帮助用户写作、构建、决策或学习表达。
3. **本地拥有优先**：用户应知道 Buddy Vault 在哪里、里面有什么、如何离开。
4. **AI 维护，人类定夺**：不可逆的知识判断进入 Inbox，而不是静默写入。
5. **来源可追溯**：每个结论都能回到 raw/source、inbox decision、wiki diff、lineage。
6. **约定优于配置**：常见字段和目录有默认语义；高级用户可以编辑规则。
7. **问答可结晶**：好问题和好回答可以成为新 raw/query 来源，再走维护流程。
8. **规则可被人教**：Schema/Policies/Templates 是用户调教外脑的地方。
9. **权限显性**：AI、微信、外部工具、Git/remote 都以可见状态和显式动作连接。

## 6. 严格借鉴：知识是有目的的

Tolaria 的关键判断必须成为 Buddy 的产品底层原则：

> 知识不是为了存起来。知识存在，是为了帮助用户把事情做成。

Buddy 因此不能只问“这条微信素材该归到哪个页面”，还要问：

- 这条知识最终服务什么产出？
- 它应该帮助用户表达什么、决定什么、推进什么？
- 它是一条可复用的 evergreen，还是某个项目/责任/学习目标的上下文？

### 6.1 四类用户目的

| 用户类型 | 主要产出 | Capture 服务什么 | Buddy 应生成什么 | Express 形态 |
|---|---|---|---|---|
| 写作者 / 内容创作者 | 文章、随笔、帖子、报告 | 微信文章、高亮、语音观点、案例素材 | evergreen 知识卡、主题页、对比页、引用来源 | 大纲、论点、段落、长文草稿 |
| 构建者 / 项目驱动者 | 被交付的产品、代码、方案、项目决策 | 技术材料、竞品、会议判断、需求片段 | 项目知识图、决策记录、概念页、流程页 | PRD、技术方案、任务拆解、决策备忘 |
| 运营者 / 管理者 | 更好的系统、流程、KPI、组织决策 | 复盘、客户反馈、指标变化、团队流程 | Responsibility 页面、Procedure 页面、People/Topic 关系 | 周报、复盘、会议准备、流程优化建议 |
| 学习者 / Learner | 掌握与表达：考试、申请、研究、成长 | 学科资料、课程笔记、论文、兴趣内容 | 学习卡片、Topic、Project、申请/论文素材库 | 答题、讲解、研究提纲、申请材料 |

多数用户是四者的混合体。Buddy 的 IA 不应把用户锁死在某一类，而应允许一条 raw 在维护时被绑定到多个目的：

- `purpose: writing`
- `purpose: project`
- `purpose: decision`
- `purpose: learning`
- `purpose: responsibility`

这些 purpose 可以先作为产品语义和 UI 分组出现，后续再落成 schema 字段。

### 6.2 Buddy 的 capture -> organize -> express

Tolaria 的流动方向是 `capture -> organize -> express`。Buddy 的严格映射如下：

```text
capture
  微信转发 / URL / 文件 / 语音 / Ask 中粘贴
  ↓
raw/
  保留原始证据，不改写
  ↓
organize
  maintainer 提案 + Inbox 人类审阅 + schema 约束
  ↓
wiki/
  evergreen / topic / project / responsibility / procedure / people
  ↓
express
  Ask 回答 / 写作素材 / 项目决策 / 学习表达 / 周报复盘
```

### 6.3 新增产品对象：Purpose Lens

建议在 Knowledge 和 Inbox 中引入 Purpose Lens，而不是只按文件分类：

| Lens | 展示问题 | 主要页面 |
|---|---|---|
| 写作 | 我能用这些素材表达什么观点？ | evergreen、topic、compare |
| 项目 | 哪些知识正在帮助我推进项目？ | project、decision、procedure |
| 管理 | 哪些知识在改善责任、流程和 KPI？ | responsibility、procedure、people |
| 学习 | 我掌握了什么？还缺什么？ | learning-card、topic、project |

Buddy 当前 `concept/people/topic/compare` 可以继续保留，但下一步应评审是否扩展：

- `evergreen`
- `project`
- `responsibility`
- `procedure`
- `decision`
- `learning-card`

这不是为了做复杂分类，而是为了让知识有明确回报路径。

## 7. 目标信息架构

建议把当前路由收束成 6 个用户心智稳定的工作区：

| 工作区 | 当前基础 | 用户问题 | 关键视图 |
|---|---|---|---|
| Home / Pulse | Dashboard + stats + patrol | 我的外脑今天发生了什么？ | 今日摄入、维护日志、待处理、质量风险 |
| Ask | AskPage | 我知道什么？这些判断从哪里来？ | 对话、source bindings、citations、结晶动作 |
| Inbox | InboxPage | AI 做完了哪些需要我拍板？ | 待审阅队列、diff、冲突、批量 Inbox Zero |
| Knowledge | KnowledgeHub + Graph + Raw | 我的长期记忆长什么样？ | 页面、邻域、来源、反链、关系图、lineage |
| Rules | SchemaEditor + templates | 我要如何教它整理？ | Templates、Policies、AGENTS/CLAUDE、校验报告 |
| Connections | WeChat + providers + external tools | 哪些入口和 agent 已连接？ | 微信、LLM provider、MCP/外部 AI、Vault/Git |

左侧 sidebar 仍保持密集工具型，不做营销式入口。底部状态栏建议承载 5 类一眼可见的状态：

- Vault：当前 Buddy Vault 路径、local-only/Git/remote 状态。
- Capture：微信连接、最近摄入、失败重试入口。
- Maintainer：absorb 运行中、pending Inbox 数、patrol 风险。
- Model：当前 provider/model、运行健康。
- External Tools：外部 AI/MCP 是否已显式连接。

## 8. 核心用户闭环

### 8.1 Capture：随手投喂

入口：

- 微信 URL / 文字 / 文件 / 图片 / 语音。
- 桌面 Ask 输入框中的 URL 或粘贴文本。
- 未来可扩展浏览器剪藏。

产品规则：

- 写入 `raw/` 后即视为“已接收”，后续维护失败也不丢数据。
- raw 永远不可变；修正只能通过新 raw 或 wiki decision 表达。
- 摄入成功回复必须告诉用户：已保存、是否进入维护、是否需要稍后审阅。

### 8.2 Maintain：AI 维护

当前 `wiki_maintainer` 的 `/absorb` 是基础。建议产品上重命名为“维护外脑”，而不是暴露技术词：

- 创建新页面。
- 合并到已有页面。
- 标记冲突。
- 更新反链与 changelog。
- 记录 lineage。
- 触发 patrol/quality sample。

AI 可以自动完成低风险结构性写入，但下列动作必须进入 Inbox：

- 合并两个已有判断。
- 降低或替换高置信判断。
- 标记页面 deprecated。
- 修改 schema/templates/policies。
- 无法判断目标页面的内容。

### 8.3 Review：Inbox Zero

Inbox 不只是“审批列表”，而是 Buddy 的组织纪律：

- 队列按 create/update/conflict/patrol/schema proposal 分组。
- 每个任务显示 Evidence、Decision、Result 三段。
- 用户动作只有少数几个：接受、改后接受、合并、拒绝、稍后。
- 当一个 raw 的知识归属被解决，它从 Inbox 派生视图中消失。
- Inbox 页面显示预计处理时间和本周 Inbox Zero 进度。

### 8.4 Navigate：Knowledge Atlas

Knowledge 页面详情建议升级为 4 个固定区块：

1. **Answer Surface**：标题、摘要、正文、confidence、last_verified。
2. **Sources**：raw 来源、引用片段、摄入时间、原始链接/文件。
3. **Neighborhood**：出链、反链、同主题、同来源、冲突页面。
4. **History**：changelog、lineage、最近一次 Inbox decision、diff。

这借鉴 Tolaria 的 Neighborhood 模式，但不复制它的笔记编辑器。Buddy 的重点是“读懂这条知识为什么存在”。

### 8.5 Ask：问答结晶

Ask 的目标不是一次性回答，而是让外脑变聪明：

- 默认回答必须带 sources。
- 每个 source 可以打开到 Knowledge Atlas 的来源区块。
- 对高价值回答提供“结晶到外脑”动作：
  - 将 question + answer 写为 `raw/source=query`。
  - 触发 maintainer proposal。
  - 经 Inbox 审阅后进入 wiki。
- 对低置信回答提供“补充素材”动作，引导用户把更多微信/URL 投喂进来。

## 9. Buddy Vault 设计

保留当前 `wiki_store` 布局，但把它从实现细节提升为产品合约：

```text
Buddy Vault
├── raw/        # 用户投喂的原始素材，只读、不可变
├── wiki/       # AI 维护、人类审阅后的长期知识
├── schema/     # 用户教 AI 如何整理的规则
├── .clawwiki/  # 派生索引、lineage、inbox、sessions 等机器状态
├── AGENTS.md   # 可选 root shim：说明外部 agent 如何读这个 vault
└── CLAUDE.md   # 可选 compatibility shim：指向 AGENTS.md 或 schema/CLAUDE.md
```

### 9.1 目录边界

| 层 | 写入者 | 用户心智 | 规则 |
|---|---|---|---|
| `raw/` | ingest 管道 | 我喂过什么 | 永不改写，只追加 |
| `wiki/` | maintainer + 用户批准 | 我长期相信什么 | 可更新，但必须留 lineage |
| `schema/` | 用户 | 我如何教它整理 | AI 只能提案，不能直接写 |
| `.clawwiki/` | 系统 | 派生状态 | 可重建或可审计，不作为正文入口 |

### 9.2 语义 frontmatter

Buddy 已有 schema v1。建议在产品文档中明确这些字段的用户语义：

| 字段 | 语义 | UI 行为 |
|---|---|---|
| `type` | 页面类型：concept/people/topic/compare/raw/changelog，未来扩展 project/responsibility/procedure/decision/learning-card | 分类、图谱节点形状、模板校验 |
| `status` | ingested/draft/canonical/stale/deprecated | 状态 badge、patrol 过滤 |
| `owner` | user/maintainer | 人写与 AI 写的信任边界 |
| `purpose` | writing/project/decision/learning/responsibility | Purpose Lens、Ask 答案意图、Inbox 分组 |
| `source_raw_id` | 主要来源 | Sources 区块和 lineage |
| `confidence` | 当前可信度 | 颜色、排序、衰减检测 |
| `last_verified` | 最近验证时间 | stale/confidence decay |
| `summary` | 一句话结论 | 列表和 Ask source card |
| `related_to` / `supports` / `contradicts` | 关系字段 | Neighborhood 和 Graph |

### 9.3 Root guidance

Tolaria 的 root `AGENTS.md` 很适合外部 AI 工具。Buddy 可采用更保守的兼容设计：

- `schema/AGENTS.md` 和 `schema/CLAUDE.md` 继续作为规则真相。
- Vault root 生成轻量 `AGENTS.md`，说明 Buddy Vault 的目录边界，并指向 `schema/`。
- Vault root 生成轻量 `CLAUDE.md`，兼容 Claude Code 类工具。
- 如果用户自定义 root guidance，Buddy 不覆盖，只显示“custom”状态和恢复入口。

## 10. Tolaria 式技术架构与选型

Buddy 当前技术基础已经接近 Tolaria，但需要把“借鉴”变成明确选型原则。

### 10.1 架构原则

| Tolaria 原则 | Buddy 落地 |
|---|---|
| Filesystem source of truth | `raw/`、`wiki/`、`schema/` 是知识真相；缓存、索引、React state 都可重建 |
| Markdown + YAML frontmatter | 继续以 `.md` + schema v1 frontmatter 作为人和 AI 共享格式 |
| Derived cache | `.clawwiki/` 下的 index、lineage、inbox、sessions 都必须说明是否可重建、如何迁移 |
| Rust owns filesystem boundary | 所有读写都经 Rust 层校验路径，前端不直接绕过边界 |
| Feature modules own UI | Ask、Inbox、Knowledge、Rules、Connections 各自拥有 UI；跨域访问走 `src/api` / `src/domain` |
| Command-first | 路由、命令面板、快捷键、菜单共享同一个命令事实源 |
| Explicit external tools | 外部 AI/MCP 连接显式、可撤销、最小权限 |

### 10.2 推荐技术栈

| 层 | 借鉴 Tolaria | Buddy 推荐 |
|---|---|---|
| 桌面壳 | Tauri v2 | 保持 Tauri v2，承担窗口、文件权限、webview、原生能力 |
| 前端 | React + TypeScript + Vite | 保持 React/TS/Vite，继续使用 TanStack Query + Zustand |
| UI 原语 | Radix / shadcn / Tailwind CSS variables | 继续使用现有 shadcn/ui、lucide、CSS variables；避免一次性换设计系统 |
| Markdown 阅读 | ReactMarkdown + 自定义 wikilink | Knowledge 默认只读渲染，优先做来源/关系/lineage，不急着做富编辑器 |
| Raw/Rules 编辑 | CodeMirror 6 | Rules Studio 与 raw markdown 预览/编辑使用 CodeMirror |
| 富文本编辑 | BlockNote | 仅当 Buddy 决定支持用户直接写 wiki 页面时再引入；不作为近期默认 |
| 后端语言 | Rust | 保持 `rust/` workspace，继续按 crate 拆分领域 |
| HTTP/IPC | Tauri IPC + Rust backend | Buddy 当前 Axum `desktop-server` 可保留；Tauri 负责启动和本机集成，不强行迁回 IPC |
| 搜索 | 文件扫描 + 可重建索引 | 先保持本地扫描/backlinks index；需要规模化时加派生 cache，不上专有数据库 |
| Git | 一等但可选 | 评审后再引入 Git history/pulse；必须支持 non-git Buddy Vault |
| AI 工具 | CLI agent + MCP | Ask runtime 继续支持 provider gateway；外部 AI 采用 MCP/CLI-agent 显式连接模式 |

### 10.3 Rust crate 目标形态

Buddy 已有 `wiki_store`、`wiki_ingest`、`wiki_maintainer`、`wiki_patrol`。建议进一步按 Tolaria 的模块清晰度收敛：

| Crate / 模块 | 目标职责 |
|---|---|
| `wiki_store` | Vault 布局、raw/wiki/schema CRUD、frontmatter、lineage、backlinks、cache |
| `wiki_ingest` | URL/PDF/DOCX/PPTX/image/audio/video -> markdown raw |
| `wiki_maintainer` | absorb、merge、query crystallization、purpose classification |
| `wiki_patrol` | orphan/stale/schema/confidence/oversized/stub/uncrystallized |
| `desktop-core` | session、provider、permission、wechat bridge、task manager、external tool state |
| `desktop-server` | HTTP/SSE route assembly and handlers，只做编排，不堆领域逻辑 |
| 未来 `buddy_mcp` | 暴露最小权限 MCP 工具，连接外部 AI |
| 未来 `buddy_git` | 可选 Git status/history/remote/pulse，不影响 non-git vault |

### 10.4 状态存储规则

借鉴 Tolaria 的 vault/app setting 边界：

| 跟随 Vault | 跟随本机安装 |
|---|---|
| schema/templates/policies | window size / zoom |
| page purpose/type/status/confidence | active provider |
| root guidance shim | API keys / auth tokens |
| user-visible organization rules | WeChat local credential |
| changelog/lineage | external tool setup state |

规则：和知识组织、表达、信任有关的东西进入 vault；和机器、凭证、窗口、运行时有关的东西留在本机设置。

## 11. 外部 AI / MCP 设计

Tolaria 的关键经验是：AI 工具不是普通集成，而是 vault 的第二操作者。Buddy 应保持最小权限默认：

### 11.1 显式连接

- 默认不改写 `~/.claude`、Cursor 或其他外部工具配置。
- 用户在 Connections 或命令面板中点击“连接外部 AI 工具”。
- 明确说明本次暴露的是当前 Buddy Vault，不是整个文件系统。
- 支持断开连接并移除配置。

### 11.2 工具表面

建议 MCP/外部工具只提供这些安全工具：

- `search_wiki`
- `read_wiki_page`
- `read_raw_source`
- `list_inbox`
- `create_inbox_proposal`
- `open_buddy_page`
- `highlight_source`

高风险工具不默认开放：

- 直接写 `raw/`
- 直接写 `schema/`
- 绕过 Inbox 写 `wiki/`
- 删除页面

如果未来开放写入，必须映射到 Buddy 的权限等级和 Inbox proposal。

## 12. Onboarding 设计

Tolaria 的 starter vault 说明一个事实：用户需要方法论，不只是空应用。

Buddy 的首次启动建议变成 5 步：

1. **创建或打开 Buddy Vault**
   - 默认创建本地 vault。
   - 显示路径和“你的数据在这里”。
   - Git/remote 是可选后续动作，不阻塞进入。

2. **选择知识整理目的**
   - 写作、项目、管理、学习四类 Purpose Lens。
   - 可多选，默认推荐“微信外脑 + 写作/决策”。

3. **选择知识整理方法**
   - 默认模板：微信外脑。
   - 可选模板：研究、写作、项目知识、团队知识。
   - 实际上是 seed `schema/templates` 和 `schema/policies`。

4. **连接入口**
   - 微信连接是主入口。
   - 可跳过，允许桌面粘贴 URL 或文本先体验。

5. **完成第一轮闭环**
   - 导入示例素材。
   - 触发维护。
   - 审阅一条 Inbox。
   - 在 Ask 里问一个带来源的问题。

这样用户第一次使用就理解：Buddy 不是聊天框，而是一个会生长的外脑。

## 13. 界面草图

### 13.1 Home / Pulse

```text
┌─────────────┬────────────────────────────────────────────────────┐
│ Sidebar     │ 外脑今日                                          │
│             ├──────────────┬──────────────┬──────────────┬──────┤
│ Home        │ 新素材 14     │ 待审阅 3      │ 已维护 23     │ 风险 2│
│ Ask         ├────────────────────────────────────────────────────┤
│ Inbox       │ Timeline                                            │
│ Knowledge   │ 09:12 微信文章入库 -> 创建 topic/ai-memory          │
│ Rules       │ 09:30 发现冲突 -> Inbox #42                         │
│ Connections │ 10:05 Ask 回答已结晶 -> raw/query-17                │
└─────────────┴────────────────────────────────────────────────────┘
```

### 13.2 Knowledge Atlas

```text
┌─────────────┬──────────────┬──────────────────────┬──────────────┐
│ Sidebar     │ Page List    │ Wiki Page             │ Context      │
│ Knowledge   │ AI Memory    │ # AI Memory           │ Sources      │
│             │ RAG          │ Summary / Body        │ Backlinks    │
│             │ Agent Loop   │ Confidence / Verified │ Lineage      │
│             │              │                       │ Related      │
└─────────────┴──────────────┴──────────────────────┴──────────────┘
```

### 13.3 Inbox Workbench

```text
┌─────────────┬───────────────────┬───────────────────────────────┐
│ Queue       │ Evidence          │ Decision                      │
│ Create 5    │ raw #71           │ 建议合并到 ai-memory          │
│ Update 8    │ source excerpt    │ diff preview                  │
│ Conflict 2  │ related pages     │ Accept / Edit / Reject        │
│ Patrol 3    │ lineage           │                               │
└─────────────┴───────────────────┴───────────────────────────────┘
```

## 14. Tolaria 式研发方式

Tolaria 的研发方式值得 Buddy 借鉴的是“质量闭环”，不是机械照搬它的 main-only 工作流。Buddy 应按自己的 Git/PR 规则执行，但吸收以下纪律。

### 14.1 设计先行

- 重大产品变化先写 `spec`，再写 `plan`，再实现。
- 评审稿必须说明用户目的、信息架构、数据边界、权限边界、验证方式。
- 落地后，稳定事实回填到 `architecture/`、`tokens/`、`operations/`。
- 旧 spec/plan 不作为当前真相；当前真相只在 `docs/desktop-shell/*` 对应入口。

### 14.2 TDD 与回归优先

- Bug：先复现，写失败测试，再修复。
- 功能：高风险逻辑先单元测试，用户主流程补 E2E/smoke。
- 不为 UI 细节滥加脆弱测试，但 Inbox/Ask/Vault/WeChat/Rules 等核心流程必须有回归入口。
- 每个新增数据模型、权限路径、写盘路径都需要至少一条 Rust 或 TS 测试。

### 14.3 质量门禁

- 借鉴 Tolaria 的 CodeScene/Code Health 思路：任何触碰文件不应降低可维护性。
- Buddy 可以先采用可执行门禁：
  - `cd apps/desktop-shell && npm run build`
  - `cd apps/desktop-shell/src-tauri && cargo check`
  - `cd rust && cargo check --workspace`
  - 关键 crate 的 targeted tests
  - `git diff --check`
- 若后续接入 CodeScene 或同类工具，应把“ touched files 不变差”作为规则。

### 14.4 键盘优先与命令统一

- 每个用户可见能力必须能从命令面板触达。
- 路由、sidebar、命令面板、快捷键共享同一事实源。
- 高价值动作需要稳定 command id，方便 Playwright/native QA。
- UI 不只靠 hover 或鼠标右键完成关键流程。

### 14.5 原生 QA

- Web mock 只能证明渲染和交互，不等于桌面功能可用。
- 涉及 Tauri、文件系统、webview、WeChat、provider、external tools 的功能需要原生 smoke。
- 对微信/外部服务不可稳定自动化的部分，应区分“代码 smoke 已过”和“真实设备/账号 E2E 待验”。

### 14.6 文档同步

任何实现触及以下内容，必须同改文档：

- Vault 布局、frontmatter、schema 字段。
- 新 route、command、settings、provider、WeChat 通道。
- 新 Rust crate/module 或 handler 边界。
- 新权限模式、AI 工具、MCP 能力。
- 新验证入口或运维流程。

## 15. 实施阶段建议

### Phase A：产品心智和文档

- 把“知识服务产出”写入产品说明和 onboarding。
- 将 `~/.clawwiki` 命名为 Buddy Vault。
- 在 Home/Settings 显示 Vault 路径、目录解释、数据所有权说明。
- 更新 architecture/tokens/operations 文档中对 raw/wiki/schema 的用户语义。

### Phase B：Purpose Lens

- 为 raw/wiki 页面引入 purpose 语义。
- 在 Inbox 中显示这条知识建议服务写作、项目、管理还是学习。
- 在 Ask 中允许用户按目的提问，如“帮我用这些素材写文章/做决策/复习”。

### Phase C：Knowledge Atlas

- 页面详情增加 Sources、Neighborhood、Lineage、History。
- 把 confidence、last_verified、owner、source_raw_id 作为一等 UI。
- Graph 与页面详情互相跳转。

### Phase D：Inbox Zero

- Inbox 任务按决策类型和风险分组。
- 增加预计处理时间、批量接受、稍后处理、已清空状态。
- 将 patrol/schema/query crystallization 全部纳入同一维护队列。

### Phase E：Rules Studio

- `schema/CLAUDE.md`、`schema/AGENTS.md`、templates、policies 统一浏览。
- AI 对规则的修改只能进入 Inbox proposal。
- Schema 校验错误能直接跳到相关规则。

### Phase F：外部 AI 工具

- 显式 MCP/外部 AI setup。
- Root guidance shim。
- 最小权限工具集。
- 与现有 permission mode 对齐。

### Phase G：Local-first / Git optional

- 保持当前本地默认。
- 评审是否引入 Git pulse/remote 作为可选能力。
- 若引入，必须支持 non-git vault，不能阻塞微信外脑主流程。

## 16. 评审问题

1. 是否接受“Buddy Vault”作为公开产品概念，而不是隐藏 `~/.clawwiki`？
2. Root `AGENTS.md` / `CLAUDE.md` shim 是否应该进入近期范围？
3. Buddy 是否允许用户直接编辑 `wiki/` 页面，还是所有知识变更都必须经 Inbox？
4. Git 是否成为一等能力，还是只保留 changelog/lineage，不引入 remote 心智？
5. 外部 AI 工具首期是否只读 + proposal，还是允许受控写入？
6. Rules Studio 是否应替代当前独立 Schema Editor 页面？
7. Ask 结果“结晶到外脑”是否默认进入 Inbox，而不是直接写 wiki？
8. Home/Pulse 是否应成为默认首页，强化“外脑今天发生了什么”？
9. Purpose Lens 首期是否只作为 UI/提示词语义，还是直接进入 frontmatter schema？
10. 是否确认技术栈借鉴 Tolaria：Tauri + React/TS + Rust + Markdown/YAML + CodeMirror + MCP/CLI agent？
11. 是否把 Tolaria 式研发纪律纳入 Buddy operations：spec/plan、TDD、质量门禁、原生 QA、文档同步？

## 17. 成功标准

评审通过后的产品改造应满足：

- 新用户 10 分钟内完成：投喂一条素材 -> 审阅一条维护建议 -> 问一个有来源的问题。
- 用户能说清楚自己的知识主要服务写作、项目、管理、学习中的哪些目的。
- 用户能说清楚 Buddy Vault 的四层目录分别代表什么。
- 任意 wiki 结论都能追到 raw/source 和 Inbox decision。
- Inbox pending 数能下降到 0，且用户知道“清空”意味着什么。
- Schema/Rules 的修改路径对用户可见，对 AI 受限。
- 外部 AI 工具连接是显式、可撤销、最小权限的。
- 技术实现继续保持本地文件、人/AI 可读约定、Rust 边界和可重建索引。
- 每个落地 slice 都有测试、QA 记录和 current-truth 文档回填。
- 没有为了借鉴 Tolaria 而削弱 Buddy 的微信投喂楔子。

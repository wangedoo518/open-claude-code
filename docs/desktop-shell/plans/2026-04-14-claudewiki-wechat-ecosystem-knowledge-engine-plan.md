# ClawWiki × 微信生态 · 知识引擎产品设计方案

> **文档类型**: 产品设计方案（团队评审用）
> **日期**: 2026-04-14
> **版本**: v1.0
> **评审状态**: 待评审

---

## 一、产品定位与核心洞察

### 1.1 Karpathy "LLM Wiki" 内核

Karpathy 开源的个人 LLM Wiki 方法论提出了一个范式转变：

> AI 正从"回答问题的工具"，变成"维护认知系统的基础设施"。

三层架构：
- **Raw 层**：原始资料（只读，不可变事实源）
- **Wiki 层**：知识页面（由 LLM 持续维护的摘要/概念/人物/比较页）
- **Schema 层**：规则系统（目录结构、命名规范、页面模板、更新流程）

核心差异：传统 RAG 是"问了再找"（查找效率），LLM Wiki 是"平时就在持续建系统"（认知复利）。

**我们要做的**：将这套"认知操作系统"与微信生态打通，让用户在微信中即可喂养、查询、维护个人知识系统。

### 1.2 产品一句话

**微信是入口，SKILL prompt + agentic_loop 是引擎，Markdown Wiki 是资产 —— 让每个人在微信里拥有一个会自己生长的知识系统。**

### 1.3 与现有 ClawWiki 的关系

ClawWiki 已具备：
- ✅ Raw 素材库（URL/PDF/DOCX 摄入）
- ✅ Ask 对话工作台（Claude 流式对话）
- ✅ Wiki/Graph/Schema 骨架（S4-S6 规划中）
- ✅ Maintainer AI（5 步自动维护流程）
- ✅ 微信 iLink 个人桥接（QR 登录 + 消息接收）
- ✅ 微信客服 Kefu 设计（官方 API + 自托管 Relay）

本方案在此基础上：**编写 SKILL.md 驱动 /absorb /query /patrol，补齐 Wiki 层 + Schema 层 + 微信闭环。**

---

## 二、产品架构总览

> **设计原则**：参照 Karpathy llm-wiki 验证的极简架构——全部智能收敛到一个
> SKILL prompt + agentic_loop，不做伪 Agent 编排。摄入是机械脚本，维护是 prompt 驱动，
> 查询是索引导航。没有 Agent 编排层，没有微服务，没有向量数据库，没有第二套前端。
> Desktop App（已有）是唯一前端，未来 Web 访问只加轻量 wiki viewer（~500 行）。

```
┌─────────────────────────────────────────────────────────────────────┐
│                      用户触达层 (Reach Layer)                        │
│                                                                     │
│  ┌─────────────────────────────┐  ┌───────────────────────────────┐ │
│  │ 微信入口（客服 API）          │  │ Desktop App（已有 Electron 壳）│ │
│  │ "ClaudeWiki助手"             │  │                               │ │
│  │                             │  │  Ask · Raw · Wiki · Inbox     │ │
│  │ · 发 URL/文件/文本 → 入库    │  │  Graph · WeChat Hub · 设置    │ │
│  │ · ?提问 → 查询回答           │  │                               │ │
│  │ · 收审核通知 → 通过/拒绝     │  │  ┌───────────────────────┐    │ │
│  │                             │  │  │ 历史数据导入            │    │ │
│  │ 唯一微信触点 · 扫码即用      │  │  │ (wechat-cli 驱动)     │    │ │
│  │                             │  │  │ 选群聊 → 批量导入 raw  │    │ │
│  └──────────────┬──────────────┘  │  └───────────────────────┘    │ │
│                 │                 └──────────────┬────────────────┘ │
└─────────────────┼────────────────────────────────┼──────────────────┘
                  │                                │
                  ▼                                ▼
┌───────────────────────────────────────────────────────────────────┐
│                  摄入适配层 (Ingest Adapters)                      │
│           机械脚本 · 无 LLM · 把任何来源转成 raw/entries/*.md       │
│                                                                   │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐             │
│  │ URL 抓取  │ │ 微信消息  │ │ 文件提取  │ │ wechat-  │             │
│  │ (reqwest) │ │ (Kefu/   │ │ (PDF/    │ │  cli 导入│             │
│  │          │ │  iLink)  │ │  DOCX)   │ │ (批量)   │             │
│  └──────────┘ └──────────┘ └──────────┘ └──────────┘             │
│       全部输出：raw/entries/{date}_{slug}.md（YAML frontmatter）    │
└───────────────────────────┬───────────────────────────────────────┘
                            │
                            ▼
┌───────────────────────────────────────────────────────────────────┐
│              Rust agentic_loop + SKILL prompt                     │
│                                                                   │
│  ┌─────────────────────────────────────────────────────────────┐  │
│  │  SKILL.md（等价于 llm-wiki 的 .claude/skills/wiki/SKILL.md） │  │
│  │                                                             │  │
│  │  /absorb  → 逐条读 raw → 匹配 _index → 更新/创建 wiki 页    │  │
│  │  /query   → 读 _index → 跟反链 → 合成回答（只读）            │  │
│  │  /cleanup → 审计所有 wiki 页 · 修结构 · 修反链                │  │
│  │  /patrol  → 定期巡检 · 孤立页 · 过期内容 · 模板合规           │  │
│  └─────────────────────────────────────────────────────────────┘  │
│                                                                   │
│  agentic_loop（已有）执行 SKILL prompt · permission_gate 审批     │
└───────────────────────────┬───────────────────────────────────────┘
                            │
                            ▼
┌───────────────────────────────────────────────────────────────────┐
│              知识存储层 (Knowledge Layer)                          │
│                                                                   │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌──────────────┐        │
│  │  Raw    │  │  Wiki   │  │ Schema  │  │  Changelog   │        │
│  │ ~/.cw/  │  │ ~/.cw/  │  │ ~/.cw/  │  │  ~/.cw/      │        │
│  │ raw/    │  │ wiki/   │  │ schema/ │  │  changelog/  │        │
│  └─────────┘  └─────────┘  └─────────┘  └──────────────┘        │
│         全部 Markdown · Obsidian 兼容 · Git 版本化                 │
└───────────────────────────────────────────────────────────────────┘
```

---

## 三、微信生态接入设计

### 3.1 设计原则：一个微信触点，不暴露技术细节

用户不关心消息走的是"客服 API"还是"iLink 长轮询"。他们只需要：
**微信里有一个 ClaudeWiki 助手，发什么都能收、问什么都能答。**

因此：
- **微信客服（官方 API）** = 唯一微信入口，面向所有用户
- **iLink** = 降级为内部开发工具，不面向用户（单向、有封号风险、与客服功能重叠）
- **wechat-cli** = 不是"微信渠道"，而是 Desktop App 里的"历史数据导入"功能按钮

### 3.2 微信入口 —— ClaudeWiki 助手（客服 API）

```
用户手机                     Cloudflare Worker              ClawWiki Desktop
┌──────────┐                ┌──────────────┐              ┌──────────────────┐
│ 微信扫码  │  ─消息──▶     │  Relay Server │  ─转发──▶   │  desktop-server  │
│ 发消息    │  ◀─回复──     │  (自托管)     │  ◀─回复──   │  wechat_kefu mod │
└──────────┘                └──────────────┘              └──────────────────┘
                                                                  │
                                                    ┌─────────────▼─────────────┐
                                                    │ 微信消息 → raw/entries/*.md │
                                                    │ (机械适配，无 LLM)          │
                                                    │ 然后 agentic_loop 执行      │
                                                    │ SKILL /absorb 或 /query     │
                                                    └───────────────────────────┘
```

**消息处理流程**：

1. **投喂知识**：用户发送 URL/文本/文件 → 机械适配函数写入 `raw/entries/*.md` → agentic_loop 执行 SKILL `/absorb` 自动更新 Wiki 层
2. **查询知识**：用户提问（`?` 前缀）→ agentic_loop 执行 SKILL `/query`，读 `_index.md` → 跟反链 → 合成回答 → 回复到微信
3. **审核通知**：`/absorb` 过程中遇到冲突 → 标记到 Inbox → 推送审核卡片到微信 → 用户点击"通过"/"拒绝"
4. **知识巡检**：定时触发 agentic_loop 执行 SKILL `/patrol` → 摘要推送到微信

**交互设计**：

| 用户动作 | 触发方式 | 系统响应 |
|---------|---------|---------|
| 直接发送 URL | 自动识别 | "✓ 已入库，正在维护 N 个相关页面" |
| 直接发送文本 | 自动识别 | "✓ 已记录" |
| 发送文件/图片 | 自动识别 | "✓ 已入库 (PDF/3页)" |
| `?` + 问题 | 前缀 `?` | 基于 Wiki 结构化回答 |
| `/recent` | 前缀命令 | 最近 24h 摄入概览 |
| `/stats` | 前缀命令 | 知识库统计 |

### 3.3 历史数据导入 —— Desktop App 功能（wechat-cli 驱动）

这不是"微信渠道"，而是 Desktop App WeChat Hub 页面里的一个功能。
用户偶尔用一次，批量导入过去的微信聊天精华。

**Desktop 端操作流程**：

1. 打开 WeChat Hub → 点击"导入历史聊天"
2. 选择群聊/联系人/收藏夹 + 时间范围
3. ClawWiki 调用 wechat-cli 提取 JSON → 智能聚合 + 噪音过滤 → 写入 `raw/entries/`
4. agentic_loop 执行 SKILL `/absorb`：按话题聚合、生成 Wiki 页

**导入类型**：
- **群聊精华**：导入技术群/行业群的高价值讨论
- **对话笔记**：导入与特定联系人的重要对话
- **收藏文章**：批量导入微信收藏的文章为 Raw 素材

---

## 四、知识引擎核心设计（Karpathy 范式实现）

### 4.1 三层知识结构

```
~/.clawwiki/
├── raw/                          # 第一层：不可变事实源
│   ├── 2026-04-14-url-xxx.md     # URL 摘入
│   ├── 2026-04-14-wechat-xxx.md  # 微信消息摘入
│   ├── 2026-04-14-file-xxx.md    # 文件摘入
│   └── 2026-04-14-voice-xxx.md   # 语音转录
│
├── wiki/                         # 第二层：LLM 持续维护的知识页
│   ├── _index.md                 # 全局索引页（自动更新）
│   ├── concepts/                 # 概念页
│   │   ├── rust-ownership.md
│   │   └── transformer-arch.md
│   ├── people/                   # 人物页
│   │   ├── karpathy.md
│   │   └── ...
│   ├── topics/                   # 专题页
│   │   ├── llm-wiki.md
│   │   └── wechat-ecosystem.md
│   ├── comparisons/              # 比较分析页
│   │   └── rag-vs-wiki.md
│   └── decisions/                # 决策记录页
│       └── 2026-04-tech-stack.md
│
├── schema/                       # 第三层：规则系统
│   ├── SKILL.md                  # /absorb /query /patrol 规则定义
│   ├── TEMPLATES.md              # 页面模板定义
│   ├── TAXONOMY.md               # 分类法规则
│   └── PATROL.md                 # 巡检规则
│
└── changelog/                    # 变更日志
    └── 2026-04-14.md
```

### 4.2 SKILL `/absorb` —— 知识维护流程

这不是一个独立 Agent，而是 SKILL.md 中 `/absorb` 段的执行逻辑，由 agentic_loop 驱动。
等价于 llm-wiki 的 SKILL.md absorption loop。

```
agentic_loop 执行 SKILL /absorb
     │
     ▼
逐条读 raw/entries/（按时间顺序，跳过已处理）
     │
     ▼
匹配 _index.md → 找到相关 wiki 页
     │
     ├── 已有页面 → 更新内容（主题驱动，非日记化）
     └── 无匹配 → 创建新页（concepts/ people/ topics/）
     │
     ▼
添加双向 [[wikilinks]]
     │
     ▼
新信息与已有判断矛盾？
     ├── 是 → 标记冲突 → 推送 Inbox 待审
     └── 否 → 自动合并
     │
     ▼
每 15 条 checkpoint：
  · 重建 _index.md（含 also: 别名）
  · 重建 _backlinks.json
  · 质量审计（防日记化、堆砌、稀薄）
  · 记录变更日志
```

### 4.3 Confidence 自动计算（借鉴 LLM Wiki v2）

> 来源：rohitg00/llm-wiki-v2 — Memory Lifecycle 中的 confidence scoring 思路。
> 但不引入其 forgetting curve / hybrid search / multi-agent 等过度工程。

每个 Wiki 页的 frontmatter `confidence` 字段由 `/absorb` 自动计算，不再手动标注：

```
confidence 规则（写入 SKILL.md /absorb 段）：
  - source_count ≥ 3 且 newest_source < 30天 且 无冲突 → high
  - source_count ≥ 2 且 newest_source < 90天              → medium
  - 其他                                                   → low
  - 有未解决冲突（Inbox 中）                                → contested
```

### 4.4 Supersession（判断变迁记录）

当 `/absorb` 检测到新信息与已有判断矛盾，且用户在 Inbox 中选择"采纳新观点"时：
- 旧观点**不直接覆盖**，而是在 changelog 中显式记录判断变迁
- Wiki 页更新后 frontmatter 添加 `superseded` 字段

```yaml
---
title: RAG 与知识库
confidence: high
superseded:
  - claim: "RAG 适合企业知识库"
    replaced_by: "结构化 Wiki 优于 RAG（Karpathy 范式）"
    date: 2026-04-14
    source: raw/2026-04-14-url-042.md
---
```

这样知识不只是"最新状态"，而是带有**判断演化历史**的活文档。

### 4.5 Crystallization（对话结晶）

`/query` 的回答如果产生了高价值内容，应沉淀回 Wiki 而非一次性丢弃。

流程：
1. 用户通过 Chat 或微信提问 → `/query` 生成回答
2. 回答写入 `raw/entries/{date}_query-{slug}.md`（source_type: query）
3. 下一次 `/absorb` 自动将高质量 query 结果吸收进 Wiki 页
4. 形成闭环：**问得越多 → Wiki 越强 → 回答越准**

```
用户提问 → /query 回答 → 写入 raw/ → /absorb 吸收 → Wiki 更强
     ▲                                                    │
     └────────────── 认知复利闭环 ◄─────────────────────────┘
```

### 4.6 Schema 驱动的知识质量保障

Schema 层是区分"聊天机器人"和"认知系统"的关键。定义：

```markdown
# TEMPLATES.md 示例

## concept_page
- title: 必填，≤30 字
- aliases: 可选，别名列表
- definition: 必填，一句话定义
- key_points: 必填，3-7 个要点
- related_concepts: 必填，≥2 个反链
- sources: 必填，引用 Raw 条目 ID
- last_updated: 自动填充
- confidence: 自动计算（见 4.3）
- superseded: 可选，判断变迁记录（见 4.4）

## person_page
- name: 必填
- role: 必填，一句话描述
- key_contributions: 3-5 个要点
- related_topics: ≥1 个反链
- sources: 引用 Raw 条目
```

### 4.7 知识巡检（Patrol）

Schema 中定义巡检规则，agentic_loop 定期执行 SKILL `/patrol`：

| 巡检项 | 频率 | 动作 |
|--------|------|------|
| 孤立页面检测 | 每日 | 无反链的 Wiki 页 → 建议添加关联 |
| 过期内容检测 | 每周 | 超过 30 天未更新 → 提醒复查 |
| 冲突积压检测 | 每日 | Inbox 中超过 3 天未处理 → 微信推送提醒 |
| 索引完整性 | 每次摄入后 | _index.md 是否包含所有页面 |
| 模板合规性 | 每次维护后 | Wiki 页是否符合 Schema 模板 |
| **Confidence 衰减** | 每周 | source 超过 90 天未更新的 high → medium |
| **Crystallization 检查** | 每次 /query 后 | 回答是否已写入 raw/entries/ |

---

## 五、前端信息架构（借鉴 Rowboat IA）

### 5.1 设计原则

**借鉴 Rowboat 的信息架构模式，不 fork 代码。**

Rowboat IA 的精髓：
- **Chat | Knowledge 双 Tab** —— 用户要么在对话，要么在浏览知识，不会同时做七件事
- **Knowledge 下是文件树** —— 熟悉的目录结构，不是抽象的"页面管理"
- **顶部浏览器式多 Tab** —— 打开的页面并排，随时切换
- **右侧 Chat 面板始终可用** —— 浏览知识时也能随时提问
- **Settings 收敛为 modal** —— 不占主导航

ClaudeWiki 当前的问题：7 个独立页面（Ask/Raw/Wiki/Graph/Schema/Inbox/WeChat/Settings）
平铺在侧边栏，用户认知负担重，不知道先去哪。

### 5.2 新信息架构

```
┌─ 顶栏 ─────────────────────────────────────────────────────────────────┐
│  ◀ ▶  │ _index │ transformer │ karpathy │ Graph View │  New chat  │ ⬜ │
├────────┴────────────────────────────────────────────────────────────────┤
│                                                                        │
│ ┌─ 左侧边栏 ──┐  ┌─ 主内容区 ─────────────────┐  ┌─ 右侧 Chat ─────┐ │
│ │              │  │                             │  │                  │ │
│ │ [Chat][Wiki] │  │   （当前打开的 Tab 内容）     │  │  Ask anything... │ │
│ │              │  │                             │  │                  │ │
│ │ ─────────── │  │                             │  │                  │ │
│ │              │  │                             │  │                  │ │
│ │ Chat 模式:   │  │                             │  │                  │ │
│ │  会话列表    │  │                             │  │  （始终可用，     │ │
│ │              │  │                             │  │   浏览知识时      │ │
│ │ Wiki 模式:   │  │                             │  │   也能提问）      │ │
│ │  文件树      │  │                             │  │                  │ │
│ │  + Graph     │  │                             │  │                  │ │
│ │              │  │                             │  │                  │ │
│ │ ─────────── │  │                             │  │                  │ │
│ │ 微信助手 ●   │  │                             │  │                  │ │
│ │ Settings     │  │                             │  │                  │ │
│ │ Help         │  │                             │  │                  │ │
│ └──────────────┘  └─────────────────────────────┘  └──────────────────┘ │
└────────────────────────────────────────────────────────────────────────┘
```

### 5.3 Chat Tab（对话模式）— 全部对话分类显示

> **现存问题**（截图实测 2026-04-17 发现）：
>
> `SessionSidebar` 显示所有 session，但存在三个问题：
> 1. 所有 session 用同一个 `MessageSquare` 图标，**无法区分类型**
> 2. 客服 session 标题是 openid（`wmbeQYRgAAxm`），**不是人话**
> 3. **没有过滤器**，各类 session 混在一起找不到
>
> 所有 session 都是对话，都应该显示。问题不是"显示了太多"，
> 而是"视觉上区分不出来"。

#### 设计原则

**全部显示，分类区分，可过滤。**

所有 session 类型都有用户价值：
- `ask` — 用户自己的问答，核心场景
- `kefu` — 微信用户通过客服问了什么、系统怎么回的
- `wechat` — iLink 投喂进来的内容
- `patrol` — 自动巡检发现了什么
- `system` — 调试/测试（可隐藏）

#### 后端改动

`DesktopSessionSummary` 增加 `source` 字段：

```rust
pub struct DesktopSessionSummary {
    // ... 现有字段 ...
    /// 对话来源，用于前端分类显示
    pub source: SessionSource,  // "ask" | "kefu" | "wechat" | "patrol" | "system"
}
```

#### 前端：分类图标 + 颜色 + 过滤器

每种 source 使用不同图标和颜色，一眼区分：

```
source    图标          颜色       标题处理
────────────────────────────────────────────────────────
ask       💬 (对话)     默认色     从第一条消息提取 ≤15 字
kefu      📱 (手机)     绿色       "微信用户: " + 第一条消息摘要
wechat    🔗 (链接)     蓝色       "收到: " + 内容摘要
patrol    🔄 (循环)     橙色       "巡检: " + 发现摘要
system    ⚙ (齿轮)     灰色       保持原 title
```

#### 过滤器栏

对话历史顶部增加过滤 pills，点击切换显示/隐藏：

```
┌─ 对话历史 ─────── + ✦ ✕ ─┐
│ [全部] [💬Ask] [📱客服]    │  ← 过滤 pills，可多选
│ [🔗微信] [🔄巡检]          │
├──────────────────────────┤
│ 今天                      │
│  💬 Transformer vs RNN    │  ← Ask: 自动提取标题
│  💬 认知复利分析            │
│  📱 "RAG 还是 Wiki？"     │  ← Kefu: 微信用户的提问
│  📱 "帮我总结这篇文章"     │
│  🔗 收到: Karpathy 文章   │  ← WeChat: 投喂内容
│  🔄 巡检: 3 页过期         │  ← Patrol: 巡检结果
│                           │
│ 昨天                      │
│  💬 Karpathy 方法论总结    │
│  📱 "Rust 所有权怎么理解"  │
│  🔄 巡检: 发现 2 个冲突    │
└──────────────────────────┘
```

#### 线框图

```
┌──────────────────────────────────────────────────────────────────────┐
│  ◀ ▶  │                    New chat                          │ ⬜   │
├────────┴─────────────────────────────────────────────────────────────┤
│ ┌────────────────┐                                                   │
│ │ [Chat] [Wiki]  │                                                   │
│ ├────────────────┤                                                   │
│ │ 对话历史  + ✦ ✕ │                                                   │
│ │[全部][💬][📱][🔄]│                                                   │
│ ├────────────────┤  ┌─────────────────────────────────────────────┐  │
│ │ 今天            │  │                                             │  │
│ │ 💬 Transformer  │  │                                             │  │
│ │    vs RNN       │  │          Ask anything...                    │  │
│ │ 💬 认知复利分析  │  │                                             │  │
│ │ 📱 "RAG还是Wiki"│  └─────────────────────────────────────────────┘  │
│ │ 🔗 收到:Karpathy│                                                   │
│ │ 🔄 巡检:3页过期 │  ┌──────────┐ ┌──────────┐ ┌──────────────────┐  │
│ │                 │  │ 投喂 URL  │ │ 查询知识  │ │ 查看最近摄入     │  │
│ │ 昨天            │  └──────────┘ └──────────┘ └──────────────────┘  │
│ │ 💬 Karpathy方法 │                                                   │
│ │ 📱 "Rust所有权" │  ┌─────────────────────────────────────────────┐  │
│ │ 🔄 巡检:2个冲突 │  │ ＋  🌐                       Claude 4.6 ▼  │  │
│ │                 │  └─────────────────────────────────────────────┘  │
│ ├────────────────┤                                                   │
│ │ 微信助手 ●      │                                                   │
│ │ Settings        │                                                   │
│ └────────────────┘                                                   │
└──────────────────────────────────────────────────────────────────────┘
```

#### 改造前后对比

| 改造前（当前） | 改造后 |
|--------------|--------|
| 💬 Ask · new conversation | 💬 Transformer vs RNN |
| 💬 Ask · new conversation | 💬 认知复利分析 |
| 💬 客服 · wmbeQYRgAAxm | 📱 "RAG 还是 Wiki？" |
| 💬 客服 · wmbeQYRgAAxm | 📱 "帮我总结这篇文章" |
| 💬 WeChat · vrv8rXkg | 🔗 收到: Karpathy 文章 |
| 💬 Morning sweep | 🔄 巡检: 3 页过期 |
| 💬 debug-test | ⚙ debug-test（可过滤隐藏） |
| 同一图标、openid 标题、无法区分 | 分类图标 + 人话标题 + 可过滤 |

**Quick Actions**（替换 Rowboat 的 Draft an email / Prep for meeting）：
- **投喂 URL** → 粘贴链接，机械摄入 + /absorb
- **查询知识** → 打开 /query 对话
- **查看最近摄入** → 展示最近 24h 的 raw entries
- **知识统计** → Raw/Wiki/反链 数量概览

### 5.4 Wiki Tab（知识模式）

对应 Rowboat 的 Knowledge Tab。三层知识结构映射为文件树。

```
┌──────────────────────────────────────────────────────────────────────┐
│  ◀ ▶  │ _index │ transformer-arch │ Graph View │  New chat  │  ⬜  │
├────────┴─────────────────────────────────────────────────────────────┤
│ ┌────────────┐  ┌────────────────────────────────┐ ┌──────────────┐ │
│ │[Chat][Wiki]│  │ concepts/transformer-arch.md   │ │Ask anything..│ │
│ ├────────────┤  │                                │ │              │ │
│ │ 📄 🗂 🔗 📊 ✕│  │ # Transformer 架构             │ │              │ │
│ ├────────────┤  │                                │ │ 你可以在这里  │ │
│ │            │  │ **定义**: 基于自注意力机制的      │ │ 随时提问，    │ │
│ │ Inbox (3)  │  │ 序列转换模型架构。               │ │ /query 会    │ │
│ │            │  │                                │ │ 基于 Wiki    │ │
│ │ ▾ Raw 素材  │  │ ## 要点                        │ │ 回答。       │ │
│ │   2026-04..│  │ - Self-Attention 是核心创新     │ │              │ │
│ │   2026-04..│  │ - 位置编码解决序列顺序           │ │              │ │
│ │   2026-04..│  │ - 多头注意力捕获不同层次关系     │ │              │ │
│ │            │  │                                │ │              │ │
│ │ ▾ Wiki 知识 │  │ ## 反链                        │ │              │ │
│ │   ▸concepts│  │ → [[people/vaswani]]           │ │              │ │
│ │   ▸people  │  │ → [[topics/llm-wiki]]          │ │              │ │
│ │   ▸topics  │  │ → [[comparisons/rnn-vs-trans]] │ │              │ │
│ │   ▸compari.│  │                                │ │              │ │
│ │   ▸decisio.│  │ ## 来源                        │ │              │ │
│ │   _index.md│  │ raw/2026-04-01-url-001.md      │ │              │ │
│ │            │  │ raw/2026-04-10-wechat-003.md   │ │              │ │
│ │ ▸ Schema   │  │                                │ │              │ │
│ │   SKILL.md │  │ ---                            │ │              │ │
│ │   TEMPLATE.│  │ confidence: high               │ │              │ │
│ │            │  │ 更新于 2h 前                    │ │              │ │
│ ├────────────┤  └────────────────────────────────┘ │              │ │
│ │ 微信助手 ●  │                                     │              │ │
│ │ Settings   │                                     └──────────────┘ │
│ └────────────┘                                                       │
└──────────────────────────────────────────────────────────────────────┘
```

**文件树结构**（对应 Rowboat 的 Agent Notes / My Notes）：

| 文件树节点 | 对应 Rowboat | 内容 |
|-----------|-------------|------|
| **Inbox (3)** | — | /absorb 产生的冲突待审，badge 显示数量 |
| **Raw 素材** | Agent Notes | `raw/entries/*.md`，只读，按时间排列 |
| **Wiki 知识** | My Notes | `wiki/` 目录树，concepts/people/topics/... |
| **Schema** | — | SKILL.md + TEMPLATES.md，可编辑 |

**顶部工具栏图标**（对应 Rowboat Knowledge 下的 📄🗂🔗📊✕）：

| 图标 | 功能 |
|------|------|
| 📄 新建 | 手动创建 Wiki 页 |
| 🗂 导入 | 打开历史数据导入面板（wechat-cli） |
| 🔗 反链 | 切换到反链视图 |
| 📊 统计 | 知识统计概览面板 |
| ✕ 折叠 | 折叠侧边栏 |

### 5.5 Graph View（图谱视图）

对应 Rowboat 的 Graph View Tab。基于 `_backlinks.json` 渲染。

```
┌──────────────────────────────────────────────────────────────────────┐
│  ◀ ▶  │ _index │ transformer │ Graph View │  New chat  │      ⬜   │
├────────┴─────────────────────────────────────────────────────────────┤
│ ┌────────────┐  ┌──────────────────────────────────────────────────┐ │
│ │[Chat][Wiki]│  │                                                  │ │
│ │            │  │         FOLDERS                                  │ │
│ │  (文件树)   │  │         ● concepts  ● people                    │ │
│ │            │  │         ● topics    ● comparisons                │ │
│ │            │  │                                                  │ │
│ │            │  │              ●transformer                        │ │
│ │            │  │             ╱  ╲                                 │ │
│ │            │  │        ●vaswani  ●attention                      │ │
│ │            │  │            ╲      ╱                              │ │
│ │            │  │         ●llm-wiki                                │ │
│ │            │  │            │                                     │ │
│ │            │  │         ●karpathy                                │ │
│ │            │  │                                                  │ │
│ │            │  │  ┌──────────────────────────────────────┐        │ │
│ │            │  │  │ Search nodes...                      │        │ │
│ │            │  │  └──────────────────────────────────────┘        │ │
│ │            │  └──────────────────────────────────────────────────┘ │
│ └────────────┘                                                       │
└──────────────────────────────────────────────────────────────────────┘
```

点击节点 → 在顶部新开 Tab 显示对应 Wiki 页。

### 5.6 Settings（收敛为 Modal）

对应 Rowboat 的 Settings Modal，不占主导航。

| Rowboat Settings | ClaudeWiki 映射 |
|-----------------|----------------|
| Account | 账号信息 |
| Connected Accounts | **微信助手**（客服 API 配置 + 历史导入） |
| MCP Servers | LLM Provider（Claude API Key） |
| Security | 安全（数据加密、权限） |
| Appearance | 外观（主题） |
| Tools Library | **SKILL 编辑器**（SKILL.md 查看/编辑） |
| Note Tagging | **Schema 模板**（TEMPLATES.md 编辑） |

### 5.7 侧边栏内容随 Tab 切换 — 不共享 PRIMARY 导航

> **关键设计决策**：Chat Tab 和 Wiki Tab 的侧边栏内容**完全不同**。
> 不是同一个 PRIMARY 列表在两种模式下都显示。
> 当前代码 `clawwiki-routes.ts` 定义了 7 个 PRIMARY 路由全部平铺，
> 需要拆散分配到对应 Tab。

#### 旧的 7 个 PRIMARY 路由如何分配

| 旧路由 | 归属 | 如何呈现 |
|-------|------|---------|
| **Dashboard** | Wiki Tab | 📊 工具栏按钮（统计面板），不是独立页 |
| **Ask** | ~~删除~~ | **Chat Tab 本身就是 Ask**，不需要单独入口 |
| **Inbox** | Wiki Tab | 文件树顶部 `Inbox (3)` badge |
| **Raw Library** | Wiki Tab | 文件树 `▾ Raw 素材` 折叠节点 |
| **Wiki Pages** | Wiki Tab | 文件树 `▾ Wiki 知识` 折叠节点 |
| **Graph** | Wiki Tab | 文件树底部 `Graph View` 或顶部 Tab |
| **Schema** | Wiki Tab | 文件树 `▸ Schema` 折叠节点 |
| **WeChat Bridge** | Settings Modal | `微信助手` 配置页 |
| **Settings** | 侧边栏底部 | 固定，两种模式都显示 |

#### 两种模式的侧边栏对比

```
  Chat Tab 侧边栏                    Wiki Tab 侧边栏
┌────────────────────┐            ┌────────────────────┐
│ [Chat]  [Wiki]     │            │ [Chat]  [Wiki]     │
├────────────────────┤            ├────────────────────┤
│ 对话历史   + ✦ ✕   │            │ 📄 🗂 🔗 📊 ✕      │
│ [全部][💬][📱][🔄]  │            ├────────────────────┤
├────────────────────┤            │ Inbox          (3) │
│ 今天               │            │                    │
│  💬 Transformer    │            │ ▾ Raw 素材     247 │
│  📱 "RAG还是Wiki"  │            │   Karpathy 文章    │
│  🔗 收到:文章       │            │   Q1 财务报告      │
│  🔄 巡检:3页过期    │            │                    │
│ 昨天               │            │ ▾ Wiki 知识     89 │
│  💬 Karpathy方法   │            │   ▸ concepts   34  │
│  📱 "Rust所有权"   │            │   ▸ people     12  │
│                    │            │   ▸ topics     28  │
│  没有 Dashboard    │            │   _index.md        │
│  没有 Inbox        │            │                    │
│  没有 Raw Library  │            │ ▸ Schema           │
│  没有 Wiki Pages   │            │   SKILL.md         │
│  没有 Graph        │            │                    │
│  没有 Schema       │            │ 没有对话历史列表     │
├────────────────────┤            ├────────────────────┤
│ 微信助手 ●          │            │ 微信助手 ●          │
│ Settings           │            │ Settings           │
└────────────────────┘            └────────────────────┘

  只有对话列表                       只有文件树
  没有页面导航                       没有对话列表
```

#### `clawwiki-routes.ts` 改造方案

```typescript
// 旧：7 个 PRIMARY 平铺
// 新：按 Tab 分组，侧边栏根据当前 Tab 渲染不同内容

// Chat Tab 不需要路由列表 — 侧边栏由 SessionSidebar 渲染
// Wiki Tab 不需要路由列表 — 侧边栏由 WikiFileTree 渲染

// 只保留固定项：
export const FIXED_FOOTER = [
  { key: "wechat-status", icon: "🔗", label: "微信助手" },
  { key: "settings", path: "/settings", icon: "⚙️", label: "Settings" },
];

// 原来的 7 个 PRIMARY 路由 → 全部删除
// Dashboard → Wiki Tab 📊 工具栏
// Ask → 删除（Chat Tab 本身）
// Inbox → Wiki Tab 文件树节点
// Raw → Wiki Tab 文件树节点
// Wiki → Wiki Tab 文件树节点
// Graph → Wiki Tab 文件树节点 / 顶部 Tab
// Schema → Wiki Tab 文件树节点
// WeChat → Settings Modal
```

### 5.8 信息架构映射总结

| Rowboat | ClaudeWiki | 改动 |
|---------|-----------|------|
| Chat Tab 侧边栏 | **对话列表**（分类图标 + 过滤器） | 无页面导航 |
| Knowledge Tab 侧边栏 | **文件树**（Inbox/Raw/Wiki/Schema） | 无对话列表 |
| Graph View | 顶部 Tab 或文件树入口 | 不变 |
| 顶部多 Tab | 顶部多 Tab | 打开的 Wiki 页 + Graph + 对话 |
| Settings Modal | Settings Modal | 含微信助手、SKILL 编辑器 |
| 7 个 PRIMARY 路由 | **全部删除** | 分散到文件树和工具栏 |

**用户心智模型**：
- **Chat Tab** = 我和 Wiki 的所有对话（Ask + 客服 + 巡检）
- **Wiki Tab** = 我的知识资产（Raw + Wiki + Schema + Inbox）
- 两件事，不是七件事，**侧边栏内容随 Tab 完全切换**

---

## 六、技术架构

### 6.1 技术选型

| 层次 | 技术 | 说明 |
|------|------|------|
| **前端** | React 18 + Vite + TypeScript (已有) | Desktop App 继续迭代 |
| **UI 库** | Tailwind 3.4 + Lucide (已有) | 参考 Rowboat 设计语言 |
| **桌面壳** | Electron 40 (已有) | 唯一客户端 |
| **Agent Runtime** | Rust (desktop-core) | 已有 agentic_loop，扩展 SKILL |
| **LLM Provider** | Claude API (Anthropic) | 主力模型 |
| **知识存储** | Markdown on disk (~/.clawwiki/) | Obsidian 兼容，Git 版本化 |
| **知识检索** | _index.md + _backlinks.json | Karpathy 式结构化检索，非 RAG |
| **微信入口** | 官方客服 API + CF Worker Relay | 唯一微信触点，扫码即用 |
| **历史导入** | wechat-cli (Python) | Desktop 功能按钮，非微信渠道 |
| **HTTP Server** | Axum (已有 desktop-server) | 扩展 API |
| **Web viewer** | 轻量 Next.js 静态站（未来可选） | ~500 行，仅渲染 wiki/ 目录 |

### 6.2 Rust 核心：agentic_loop + SKILL prompt（非多 Agent）

参照 llm-wiki 的验证：**不需要 4 个 Agent，只需要 1 个 agentic_loop + 1 个 SKILL prompt。**

```rust
// 现有 agentic_loop 已经够用，只需扩展 tool_registry

// 摄入适配器：机械函数，无 LLM
pub fn ingest_url(url: &str) -> Result<RawEntry>;     // reqwest + scraper
pub fn ingest_file(path: &Path) -> Result<RawEntry>;   // PDF/DOCX 提取
pub fn ingest_wechat_msg(msg: &WeChatMsg) -> Result<RawEntry>; // 微信消息转 md

// SKILL prompt 定义（等价于 llm-wiki 的 SKILL.md）
// 存储在 ~/.clawwiki/schema/SKILL.md
// agentic_loop 读取此 prompt 执行：
//   /absorb  → 逐条读 raw/entries → 更新 wiki 页
//   /query   → 读 _index.md → 跟反链 → 合成回答
//   /cleanup → 审计 wiki 页质量
//   /patrol  → 定期巡检
```

**为什么不做多 Agent 编排？**

llm-wiki 用 ~1500 行代码 + 一个 SKILL.md 实现了完整的知识引擎。
它证明了：
- "Ingestion Agent" = 一个 Python/Rust 函数（机械，无需 LLM）
- "Maintainer Agent" = SKILL.md 中的 `/absorb` 段落
- "Ask Agent" = SKILL.md 中的 `/query` 段落
- "WeChat Agent" = 一个 ingest adapter（把微信消息转成 raw entry）

做 4 个 Agent + 编排层 = 过度工程。agentic_loop 本身就是编排器。

### 6.3 Rowboat 前端对接 Rust 后端

```
Rowboat Next.js Frontend (port 3000)
        │
        │  HTTP/SSE API
        ▼
Rust desktop-server (Axum, port 7878)
        │
        ├── /api/v1/chat          → agentic_loop 执行 SKILL /query
        ├── /api/v1/ingest        → 机械适配函数（无 LLM）
        ├── /api/v1/wiki          → Wiki 文件 CRUD
        ├── /api/v1/raw           → Raw 文件 CRUD
        ├── /api/v1/absorb        → agentic_loop 执行 SKILL /absorb
        ├── /api/v1/wechat/kefu   → 微信消息 → ingest adapter → raw entry
        ├── /api/v1/wechat/import → wechat-cli JSON → 批量 raw entries
        └── /api/v1/patrol        → agentic_loop 执行 SKILL /patrol
```

**关键原则**：
- 摄入（ingest）是机械的 → Rust 函数直接处理，不调用 LLM
- 维护（absorb）是智能的 → agentic_loop + SKILL prompt
- 查询（query）是智能的 → agentic_loop + SKILL prompt
- 微信消息只是另一个摄入来源 → 转成 raw entry，走同一条路

---

## 七、数据流设计

### 7.1 微信客服 → 知识沉淀完整链路

```
用户在微信发送: "https://arxiv.org/abs/2401.xxxxx 这篇论文讲的是什么"

  Step 1: 微信 → Relay → desktop-server
  ┌─────────────────────────────────────────┐
  │ POST /api/v1/wechat/kefu/message       │
  │ { "content": "https://...", "user": "x"}│
  └────────────────────┬────────────────────┘
                       │
  Step 2: 机械适配 — 检测到 URL → ingest_url()
  ┌────────────────────▼────────────────────┐
  │ reqwest 抓取 → 提取正文（无 LLM）        │
  │ → 写入 raw/entries/2026-04-14-url-xxx.md │
  └────────────────────┬────────────────────┘
                       │
  Step 3: agentic_loop 执行 SKILL /absorb
  ┌────────────────────▼────────────────────┐
  │ 读取新 raw entry → 匹配 _index.md        │
  │ → 更新/创建 wiki 页 → 添加反链            │
  │ → 冲突检测 → 写入 changelog               │
  │ （与 llm-wiki 完全同构）                   │
  └────────────────────┬────────────────────┘
                       │
  Step 4: 检测到用户同时在提问 → SKILL /query
  ┌────────────────────▼────────────────────┐
  │ 读 _index.md → 跟反链 → 合成回答         │
  │ → 回复到微信客服                          │
  └─────────────────────────────────────────┘
```

### 7.2 历史数据导入链路（Desktop App 功能）

```
用户在 Desktop WeChat Hub 点击"导入历史聊天" → 选择"AI 技术群"

  Step 1: 前端调用
  ┌─────────────────────────────────────┐
  │ POST /api/v1/wechat/import         │
  │ { "chat": "AI技术群", "limit": 200 }│
  └──────────────┬──────────────────────┘
                 │
  Step 2: Rust 调用 wechat-cli
  ┌──────────────▼──────────────────────┐
  │ Command::new("wechat-cli")          │
  │   .args(["history", "AI技术群",     │
  │          "--limit", "200",          │
  │          "--format", "json"])        │
  │   .output()                         │
  └──────────────┬──────────────────────┘
                 │
  Step 3: 逐条写入 Raw
  ┌──────────────▼──────────────────────┐
  │ for msg in messages:                │
  │   write_raw_entry(msg) → raw/       │
  └──────────────┬──────────────────────┘
                 │
  Step 4: 批量 Maintainer 任务
  ┌──────────────▼──────────────────────┐
  │ 智能合并（不是每条消息一个 Wiki 页）  │
  │ 按话题聚合 → 生成话题摘要页           │
  │ 提取关键人物 → 更新人物页             │
  │ 提取关键概念 → 更新概念页             │
  └─────────────────────────────────────┘
```

---

## 八、实施路线图

### Phase 1: SKILL + 摄入适配（2 周）

| 任务 | 详情 | 产出 |
|------|------|------|
| 编写 SKILL.md | 参照 llm-wiki SKILL.md，定义 /absorb /query /cleanup /patrol | 知识引擎核心 |
| Wiki CRUD API | Rust 端实现 wiki/ 目录读写 + _index.md + _backlinks.json | `/api/v1/wiki/*` |
| 摄入适配函数 | ingest_url / ingest_file / ingest_wechat_msg（机械，无 LLM） | raw/entries/ 可写入 |
| Desktop 前端完善 | Wiki Explorer 页面（三级目录 + Markdown 渲染 + 反链） | 可浏览 Wiki |

### Phase 2: 微信闭环（2 周）

| 任务 | 详情 | 产出 |
|------|------|------|
| Kefu 消息适配 | 微信消息 → ingest adapter → raw entry → /absorb | 微信可投喂 |
| Kefu 查询适配 | `?` 前缀消息 → /query → 回复到微信 | 微信可查询 |
| wechat-cli 集成 | Rust 调用 wechat-cli + 批量导入 UI | 历史数据导入 |
| 微信推送 | /absorb 冲突 → Inbox → 推送到微信客服 | 移动端审核通知 |

### Phase 3: Schema + 巡检（2 周）

| 任务 | 详情 | 产出 |
|------|------|------|
| Schema 模板系统 | SKILL.md 中定义页面模板 + 合规检查规则 | 知识质量保障 |
| /patrol 实现 | 孤立页 · 过期内容 · 模板合规 · 索引完整性 | 知识自动巡检 |
| Dashboard 页 | 知识增长趋势 + /absorb 活动流 + 巡检报告 | 认知复利可感知 |
| WeChat Hub 页 | 客服状态 + wechat-cli 导入面板 | 统一微信管理 |

### Phase 4: 图谱 + 消费（2 周）

| 任务 | 详情 | 产出 |
|------|------|------|
| Knowledge Graph | 基于 _backlinks.json 渲染反链关系图 | 知识网络可见 |
| Wiki 消费层 | /query 结果 → 生成 Slide / 公众号稿 / PPT（未来） | 消费闭环 |
| 轻量 Web viewer | ~500 行 Next.js 静态站渲染 wiki/ 目录（可选） | Web 访问 Wiki |

---

## 九、Rowboat 参考清单（学习，不搬运）

**原则**：不 fork Rowboat 代码，但参考其设计模式。

| 参考内容 | Rowboat 源路径 | 学什么 |
|----------|---------------|--------|
| 双 Tab 布局 | `app/projects/[projectId]/` | Chat + Knowledge 的 Tab 切换交互 |
| Markdown 渲染 | `app/lib/components/markdown-content.tsx` | react-markdown + remark-gfm 配置 |
| 对话消息 UI | `app/lib/components/message-display.tsx` | 流式消息、tool call 可视化 |
| Conversations 列表 | `app/projects/[projectId]/conversations/` | 会话历史列表交互模式 |
| 定时任务 UI | `app/projects/[projectId]/jobs/` | 巡检调度的前端交互 |

### Desktop App 需要新增/完善的模块

| 模块 | 说明 | 已有基础 |
|------|------|---------|
| Wiki Explorer | 三级目录 + Markdown 渲染 + 反链导航 | 骨架已有 (S4) |
| Dashboard | 摄入/维护/增长统计 | 骨架已有 (stub) |
| WeChat Hub | 客服状态 + wechat-cli 导入面板 | 骨架已有 (WeChatBridgePage) |
| SKILL Editor | SKILL.md 可视化预览 + 编辑 | 全新 |

---

## 十、风险与决策点（评审重点）

### 需要团队决策的问题

| # | 问题 | 决策 | 理由 |
|---|------|------|------|
| 1 | **前端** | Desktop App 是唯一前端 | 已有完整页面骨架，不维护第二套 |
| 2 | **Rowboat** | 参考设计，不 fork 代码 | fork = 维护两套前端 + 适配成本 > 收益 |
| 3 | **Agent 架构** | agentic_loop + SKILL.md，不做多 Agent | llm-wiki 验证了极简架构足够 |
| 4 | **知识检索** | _index.md + 反链，不用向量 DB | Karpathy 范式：维护好 = 不需要 RAG |
| 5 | **微信优先级** | 先客服再 wechat-cli | 客服是用户主入口，wechat-cli 是补充 |
| 6 | **多用户** | 单用户本地优先 | 先跑通单人认知复利闭环 |

### 技术风险

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| Rowboat 前端与 Rust 后端接口不匹配 | 适配工作量大 | Phase 1 先做最小可用 API 子集 |
| /absorb 输出质量不稳定 | 生成的 Wiki 页面质量差 | Schema 模板约束 + checkpoint 质量审计 + Inbox 人工审核兜底 |
| 微信客服 API 限制 | 消息频率/格式受限 | 消息队列缓冲 + 降级为纯文本 |
| wechat-cli 依赖本地微信 | 非所有用户都用桌面微信 | 定位为 Desktop 可选功能，非主流程 |

---

## 十一、成功指标

| 指标 | Phase 1-2 目标 | Phase 3-4 目标 |
|------|---------------|---------------|
| Raw 素材数 | 100+ 条 | 500+ 条 |
| Wiki 页面数 | 30+ 页（自动生成） | 100+ 页 |
| 微信日活消息 | - | 20+ 条/天 |
| 知识检索准确率 | 70%+ | 85%+ |
| /absorb 自动通过率 | 60%+ | 80%+ |
| 巡检发现问题数 | - | 5+/周 |

---

## 附录 A：术语表

| 术语 | 定义 |
|------|------|
| **Raw** | 不可变原始素材，只读事实源 |
| **Wiki** | LLM 持续维护的知识页面集合 |
| **Schema** | 定义知识结构的规则系统 |
| **Maintainer (/absorb)** | SKILL.md 中驱动 Wiki 自动维护的 prompt 段 |
| **Patrol** | 定期巡检知识质量的机制 |
| **微信入口** | 微信官方客服 API，唯一微信触点 |
| **历史导入** | Desktop App 中 wechat-cli 驱动的批量导入功能 |
| **反链 (Backlink)** | 双向页面引用关系 |
| **认知复利** | 知识越积累越产生新价值的系统效应 |

## 附录 B：参考资料

- Karpathy LLM Wiki Gist: https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f
- Rowboat 源码: `/Users/champion/Documents/develop/Warwolf/rowboat`
- wechat-cli 源码: `/Users/champion/Documents/develop/Warwolf/wechat-cli`
- ClawWiki 现有设计: `/docs/desktop-shell/specs/`

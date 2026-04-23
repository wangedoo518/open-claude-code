# 01 - SKILL Engine 模块规格书

> **模块**: SKILL Engine (wiki_maintainer 扩展 + desktop-core SkillRouter)
> **版本**: v2.0-draft
> **最后更新**: 2026-04-14
> **状态**: 设计完成, 待实现
> **前置依赖**: `technical-design.md` 第一至五章

SKILL Engine 是 ClawWiki 的核心智能引擎。它驱动 `/absorb` `/query` `/cleanup` `/patrol` 四个一级 SKILL 命令, 将用户的原始素材 (raw/) 转化为结构化的个人知识 Wiki (wiki/), 并持续维护其质量。

**设计哲学** (继承自 llm-wiki SKILL.md):

- **你是作者, 不是档案员**: 每篇 wiki 页面应是经过提炼的知识产物, 不是原始素材的搬运
- **反填鸭 (Anti-cramming)**: 新概念创建新页面, 而非无限膨胀已有页面
- **反稀薄 (Anti-thinning)**: 每次触碰一个页面都必须让它更丰富, 不允许出现残桩
- **概念文章优先**: 从数据中涌现的模式和主题才是 wiki 的骨架
- **百科全书语气**: 平实、事实、中立, 归因优于断言
- **按主题组织, 不按时间排列**: 避免日记体结构

**中文适配**:

素材优先级层级:
1. 微信文章 (最高信号: 用户主动转发的长文)
2. URL 抓取 (用户指定的网页内容)
3. 微信消息 (对话片段, 需要高度筛选)
4. 文件 (PDF/DOCX/PPTX, 结构化但可能冗长)
5. 粘贴文本 (手动输入, 信号中等)
6. 语音转文字 (噪声最大, 需要最强的摘要能力)

---

## 1. 职责边界

### 1.1 SKILL Engine 拥有的职责

| 职责 | 说明 | 代码位置 |
|------|------|----------|
| 批量吸收循环 | 将一批 raw entries 转化为 wiki pages | `wiki_maintainer::absorb_batch` |
| Wiki 知识问答 | 基于 wiki 内容的 grounded Q&A | `wiki_maintainer::query_wiki` |
| 质量审计 | 检测重叠、残桩、膨胀等质量问题 | `wiki_maintainer` (cleanup 子流程) |
| 结构巡检 | 检测孤儿页、过期页、Schema 违规 | `wiki_patrol::run_patrol` |
| 进度追踪 | 管理异步 SKILL 任务的生命周期和进度 | `desktop-core::AbsorbTaskManager` |
| SKILL 路由 | 将 Chat 输入框的 `/absorb` 等命令分发到后端 | `desktop-core::SkillRouter` |
| 检查点机制 | 每 15 条 entry 重建索引 + 质量审计 | `absorb_batch` 内部逻辑 |
| 冲突检测 | 新信息与已有页面矛盾时生成 Inbox 条目 | `absorb_batch` 步骤 3i |
| 反向链接维护 | 吸收后自动更新双向链接索引 | `absorb_batch` 步骤 3h |

### 1.2 SKILL Engine 不拥有的职责

| 职责 | 归属模块 | 说明 |
|------|----------|------|
| 原始素材写入 raw/ | `wiki_store` | SKILL Engine 只读 raw/, 永远不写 |
| 格式转换 (PDF/DOCX/HTML -> markdown) | `wiki_ingest` | 适配器层负责格式统一 |
| LLM API 调用 | `codex_broker` / `BrokerAdapter` | SKILL Engine 通过 `BrokerSender` trait 间接调用 |
| HTTP 路由注册 | `desktop-server` | SKILL Engine 提供业务函数, 不绑定 HTTP 框架 |
| 前端渲染 | React 组件层 | SKILL Engine 通过 SSE 事件推送数据, 不控制 UI |
| Schema 直接写入 | 人工操作 | SKILL Engine 可通过 Inbox 提议 Schema 变更, 不直接修改 schema/ |
| 微信消息接收 | `wechat_kefu` / `wechat_ilink` | 微信通道负责接收, SKILL Engine 在 raw/ 已有内容后介入 |

### 1.3 层级合约 (不可违反)

```
raw/     SKILL Engine 只读。每个文件有唯一 sha256。永远不可变。
wiki/    SKILL Engine 写入。必须通过 Schema v1 frontmatter 验证。
schema/  人工专属。SKILL Engine 可通过 Inbox 提议变更, 永远不直接写入。
```

---

## 2. 依赖关系

### 2.1 Crate 依赖图

```
                 ┌─────────────────┐
                 │ desktop-server  │  (HTTP handlers, SSE broadcast)
                 └────────┬────────┘
                          │ 调用
                          ▼
                 ┌─────────────────┐
                 │  desktop-core   │
                 │  SkillRouter    │  (SKILL 命令分发)
                 │  AbsorbTask     │  (任务生命周期管理)
                 │  Manager        │
                 └───┬─────────┬───┘
                     │         │
        ┌────────────┘         └────────────┐
        ▼                                   ▼
┌──────────────┐                   ┌──────────────┐
│wiki_maintainer│  (核心算法)      │ wiki_patrol  │  (巡检引擎)
│ absorb_batch │                   │ detect_*     │
│ query_wiki   │                   └──────┬───────┘
└──────┬───────┘                          │
       │                                  │
       └──────────┬───────────────────────┘
                  ▼
          ┌──────────────┐
          │  wiki_store   │  (磁盘 CRUD, absorb_log, backlinks)
          └──────────────┘
```

### 2.2 上游依赖 (SKILL Engine 读取/调用)

| 依赖 | Crate | 接口 | 用途 |
|------|-------|------|------|
| 磁盘存储 | `wiki_store` | `WikiPaths`, `read_raw_entry`, `list_raw_entries`, `write_wiki_page_in_category`, `rebuild_wiki_index`, `append_wiki_log`, `append_absorb_log`, `is_entry_absorbed`, `build_backlinks_index`, `save_backlinks_index`, `extract_internal_links`, `read_wiki_page`, `list_all_wiki_pages` | 所有磁盘操作 |
| 素材适配 | `wiki_ingest` | (间接) 素材已由 ingest 写入 raw/, SKILL Engine 读取结果 | 格式转换完成后的 markdown |
| LLM 调用 | `codex_broker` (通过 `BrokerSender` trait) | `chat_completion(MessageRequest) -> MessageResponse` | 摘要生成、知识问答、质量审计 |

### 2.3 下游消费者 (读取 SKILL Engine 的输出)

| 消费者 | Crate/层 | 接口 | 用途 |
|--------|----------|------|------|
| HTTP 层 | `desktop-server` | `POST /api/wiki/absorb` 等 9 个端点 | 接收前端请求, 调用 SKILL Engine 函数 |
| 前端 | React (SSE EventSource) | `absorb_progress`, `absorb_complete` 等 9 种 SSE 事件 | 实时显示吸收进度、查询结果、巡检报告 |
| 微信客服 | `wechat_kefu` (间接) | 通过 `inbox_notify` broadcast channel | 吸收完成后通知微信侧更新 |
| Dashboard | 前端 DashboardPage | `GET /api/wiki/stats` | 展示统计面板 |

---

## 3. API 接口

### 3.1 POST /api/wiki/absorb

触发批量吸收, 将 pending 状态的 raw entries 转化为 wiki pages。

**请求**:

```json
{
  "entry_ids": [1, 2, 3],       // 可选, null 时吸收所有未吸收 entries
  "date_range": {                // 可选, 与 entry_ids 互斥
    "from": "2026-04-01",
    "to": "2026-04-14"
  }
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `entry_ids` | `u32[] | null` | 否 | 指定 raw entry ID 列表; null 时处理所有未吸收 |
| `date_range.from` | `String` | 条件 | ISO 日期, 含当日 |
| `date_range.to` | `String` | 条件 | ISO 日期, 含当日 |

**约束**: `entry_ids` 和 `date_range` 同时提供时 `entry_ids` 优先。

**响应 (202 Accepted)**:

```json
{
  "task_id": "absorb-1713072000-a3f2",
  "status": "started",
  "total_entries": 5
}
```

**SSE 进度事件** (`absorb_progress`):

```json
{
  "type": "absorb_progress",
  "task_id": "absorb-1713072000-a3f2",
  "processed": 2,
  "total": 5,
  "current_entry_id": 3,
  "action": "create",
  "page_slug": "transformer-architecture",
  "page_title": "Transformer 架构",
  "error": null
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `action` | `String` | `"create"` 新建 / `"update"` 更新 / `"skip"` 跳过 |
| `error` | `String | null` | 单条处理失败时的错误信息 |

**SSE 完成事件** (`absorb_complete`):

```json
{
  "type": "absorb_complete",
  "task_id": "absorb-1713072000-a3f2",
  "created": 3,
  "updated": 1,
  "skipped": 1,
  "failed": 0,
  "duration_ms": 12500
}
```

**错误码**:

| HTTP | 错误 | 条件 |
|------|------|------|
| 400 | `INVALID_DATE_RANGE` | from > to 或日期格式错误 |
| 404 | `ENTRIES_NOT_FOUND` | entry_ids 含不存在的 ID |
| 409 | `ABSORB_IN_PROGRESS` | 已有 absorb 任务执行中 |
| 503 | `BROKER_UNAVAILABLE` | 无可用 LLM provider |

### 3.2 POST /api/wiki/query

基于 Wiki 知识库的 grounded Q&A。返回 SSE 流式应答。

**请求**:

```json
{
  "question": "Transformer 和 RNN 有什么区别?",
  "session_id": "ask-session-abc123",
  "max_sources": 5
}
```

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| `question` | `String` | 是 | - | 用户提问, 最大 2000 字符 |
| `session_id` | `String | null` | 否 | null | 关联的 Ask 会话 ID |
| `max_sources` | `usize` | 否 | 5 | 最多引用 wiki 页面数, [1, 20] |

**SSE 事件序列**:

1. `query_sources` (一次): 检索到的相关 wiki 页面列表

```json
{
  "type": "query_sources",
  "sources": [
    {
      "slug": "transformer-architecture",
      "title": "Transformer 架构",
      "relevance_score": 0.95,
      "snippet": "Transformer 是一种基于自注意力机制的..."
    }
  ]
}
```

2. `query_chunk` (多次): 流式回答增量文本

```json
{
  "type": "query_chunk",
  "delta": "根据你的知识库记录, ",
  "source_refs": ["transformer-architecture"]
}
```

3. `query_done` (一次): 完成信号

```json
{
  "type": "query_done",
  "total_tokens": 850,
  "sources_used": ["transformer-architecture", "rnn-basics"]
}
```

**错误码**:

| HTTP | 错误 | 条件 |
|------|------|------|
| 400 | `EMPTY_QUESTION` | question 为空 |
| 400 | `QUESTION_TOO_LONG` | question > 2000 字符 |
| 404 | `WIKI_EMPTY` | Wiki 无页面 |
| 503 | `BROKER_UNAVAILABLE` | 无可用 LLM provider |

### 3.3 POST /api/wiki/cleanup

质量审计, 检测合并/扩展/删除机会。

**请求**: 空对象 `{}`

**响应 (202 Accepted)**:

```json
{
  "task_id": "cleanup-1713072000-b4e1",
  "status": "started"
}
```

**SSE 进度** (`cleanup_progress`):

```json
{
  "type": "cleanup_progress",
  "task_id": "cleanup-1713072000-b4e1",
  "phase": "analyzing",
  "checked": 15,
  "total": 30
}
```

| `phase` 值 | 说明 |
|------------|------|
| `"analyzing"` | 逐页分析中 |
| `"generating_suggestions"` | LLM 生成优化建议中 |
| `"complete"` | 完成 |

**SSE 完成** (`cleanup_report`):

```json
{
  "type": "cleanup_report",
  "task_id": "cleanup-1713072000-b4e1",
  "suggestions": [
    {
      "kind": "merge",
      "pages": ["transformer-architecture", "attention-mechanism"],
      "reason": "内容高度重叠, 建议合并",
      "confidence": 0.85
    },
    {
      "kind": "expand",
      "pages": ["rag-overview"],
      "reason": "页面内容不足 50 词, 建议扩展",
      "confidence": 0.92
    }
  ],
  "inbox_entries_created": 2,
  "duration_ms": 8000
}
```

**错误码**:

| HTTP | 错误 | 条件 |
|------|------|------|
| 409 | `CLEANUP_IN_PROGRESS` | 已有 cleanup 任务执行中 |
| 503 | `BROKER_UNAVAILABLE` | 无可用 LLM provider |

### 3.4 POST /api/wiki/patrol

结构巡检, 运行 5 种检测器。

**请求**: 空对象 `{}`

**响应 (202 Accepted)**:

```json
{
  "task_id": "patrol-1713072000-c5d2",
  "status": "started"
}
```

**SSE 进度** (`patrol_progress`):

```json
{
  "type": "patrol_progress",
  "task_id": "patrol-1713072000-c5d2",
  "check": "orphans",
  "status": "running"
}
```

| `check` 值 | 检测内容 |
|------------|----------|
| `"orphans"` | 孤儿页面 (无入链) |
| `"stale"` | 过期页面 (last_verified > 90 天) |
| `"schema_violations"` | frontmatter Schema 违规 |
| `"oversized"` | 超长页面 (> 500 行) |
| `"stubs"` | 残桩页面 (< 10 行) |

**SSE 完成** (`patrol_report`):

```json
{
  "type": "patrol_report",
  "task_id": "patrol-1713072000-c5d2",
  "issues": [
    {
      "kind": "orphan",
      "page_slug": "abandoned-concept",
      "description": "该页面无任何入链, 且不被 index.md 引用",
      "suggested_action": "添加至相关 topic 页面的引用, 或标记为 deprecated"
    }
  ],
  "summary": {
    "orphans": 2,
    "stale": 3,
    "schema_violations": 1,
    "oversized": 1,
    "stubs": 2
  },
  "inbox_entries_created": 9,
  "duration_ms": 350
}
```

**错误码**:

| HTTP | 错误 | 条件 |
|------|------|------|
| 409 | `PATROL_IN_PROGRESS` | 已有 patrol 任务执行中 |

---

## 4. 数据模型

### 4.1 AbsorbLogEntry

记录单次吸收操作的结果。持久化到 `{wiki_root}/.clawwiki/_absorb_log.json`。

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AbsorbLogEntry {
    /// 被吸收的 raw entry ID
    pub entry_id: u32,
    /// ISO-8601 精确时间戳
    pub timestamp: String,
    /// 执行动作: "create" | "update" | "skip"
    pub action: String,
    /// 目标 wiki page slug; action="skip" 时为 None
    pub page_slug: Option<String>,
    /// 目标 wiki page 标题; action="skip" 时为 None
    pub page_title: Option<String>,
    /// 目标 wiki page 分类; "concept"|"people"|"topic"|"compare"
    pub page_category: Option<String>,
}
```

**验证规则**:
- `entry_id`: >= 1, 对应已存在的 raw entry
- `timestamp`: ISO-8601 格式, 精确到秒
- `action`: 严格枚举 `"create"` / `"update"` / `"skip"`
- `page_slug`: `action != "skip"` 时必须为有效 kebab-case slug
- `page_title`: `action != "skip"` 时应提供标题
- `page_category`: `action != "skip"` 时应提供分类, 枚举 `"concept"` / `"people"` / `"topic"` / `"compare"`

### 4.2 AbsorbProgressEvent

通过 mpsc channel 从 `absorb_batch` 发送到 HTTP handler 的进度事件。

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbsorbProgressEvent {
    pub processed: usize,
    pub total: usize,
    pub current_entry_id: u32,
    /// "create" | "update" | "skip"
    pub action: String,
    pub page_slug: Option<String>,
    pub page_title: Option<String>,
    /// 单条处理失败时的错误信息; 成功时为 None
    pub error: Option<String>,
}
```

### 4.3 AbsorbResult

`absorb_batch` 的最终返回值。

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbsorbResult {
    pub created: usize,
    pub updated: usize,
    pub skipped: usize,
    pub failed: usize,
    pub duration_ms: u64,
    /// 用户取消时为 true, 此时 created+updated+skipped+failed < total
    pub cancelled: bool,
}
```

### 4.4 QueryResult 及关联类型

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryChunkEvent {
    pub delta: String,
    pub source_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuerySource {
    pub slug: String,
    pub title: String,
    pub relevance_score: f32,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub sources: Vec<QuerySource>,
    pub total_tokens: usize,
}
```

**验证规则**:
- `relevance_score`: [0.0, 1.0] 范围浮点数
- `snippet`: 最长 200 字符, 从页面正文提取
- `source_refs`: 每项必须是当前 wiki 中存在的有效 slug

### 4.5 CleanupSuggestion

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupSuggestion {
    /// "merge" | "expand" | "split" | "deprecate"
    pub kind: String,
    /// 涉及的 wiki page slugs
    pub pages: Vec<String>,
    /// 中文理由描述
    pub reason: String,
    /// LLM 置信度 [0.0, 1.0]
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupResult {
    pub suggestions: Vec<CleanupSuggestion>,
    pub inbox_entries_created: usize,
    pub duration_ms: u64,
}
```

### 4.6 PatrolReport

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatrolIssue {
    pub kind: PatrolIssueKind,
    pub page_slug: String,
    pub description: String,
    pub suggested_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PatrolIssueKind {
    Orphan,           // "orphan"
    Stale,            // "stale"
    SchemaViolation,  // "schema-violation"
    Oversized,        // "oversized"
    Stub,             // "stub"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatrolSummary {
    pub orphans: usize,
    pub stale: usize,
    pub schema_violations: usize,
    pub oversized: usize,
    pub stubs: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatrolReport {
    pub issues: Vec<PatrolIssue>,
    pub summary: PatrolSummary,
    pub checked_at: String,
}
```

---

## 5. Rust 实现

本节是整个 SKILL Engine 的算法核心。开发者应直接按照本节伪代码实现。

### 5.1 absorb_batch -- 批量吸收主循环

这是最重要的函数。它精确复制 llm-wiki 的 absorption loop, 适配到 Rust 异步 + `BrokerSender` trait 架构。

#### 函数签名

```rust
/// 批量吸收: 对一批 raw entries 依次执行 propose + write 循环。
///
/// 遵循 llm-wiki SKILL.md 的吸收策略:
/// - 按 ingested_at 升序处理 (最早的先吸收)
/// - 每 15 条执行 checkpoint (重建索引 + 质量审计)
/// - 新概念创建新页面, 而非膨胀已有页面 (anti-cramming)
/// - 每次触碰页面都使其更丰富 (anti-thinning)
/// - 按主题组织, 不按时间排列
pub async fn absorb_batch(
    paths: &wiki_store::WikiPaths,
    entry_ids: Vec<u32>,
    broker: &(impl BrokerSender + ?Sized),
    progress_tx: tokio::sync::mpsc::Sender<AbsorbProgressEvent>,
    cancel_token: tokio_util::sync::CancellationToken,
) -> Result<AbsorbResult>
```

#### 完整算法 (逐步)

```rust
pub async fn absorb_batch(
    paths: &wiki_store::WikiPaths,
    entry_ids: Vec<u32>,
    broker: &(impl BrokerSender + ?Sized),
    progress_tx: tokio::sync::mpsc::Sender<AbsorbProgressEvent>,
    cancel_token: tokio_util::sync::CancellationToken,
) -> Result<AbsorbResult> {
    let start = std::time::Instant::now();
    let mut result = AbsorbResult {
        created: 0, updated: 0, skipped: 0, failed: 0,
        duration_ms: 0, cancelled: false,
    };

    // ── 步骤 1: 过滤已吸收的 entries ──────────────────────
    //
    // 读取 _absorb_log.json, 排除已经处理过的 entry_id。
    // 这防止重复吸收: 用户可以安全地多次调用 /absorb 而不会
    // 产生重复的 wiki 页面。
    let mut pending: Vec<u32> = Vec::new();
    for &id in &entry_ids {
        if wiki_store::is_entry_absorbed(paths, id) {
            result.skipped += 1;
            // 发送 skip 进度事件
            let _ = progress_tx.send(AbsorbProgressEvent {
                processed: result.skipped,
                total: entry_ids.len(),
                current_entry_id: id,
                action: "skip".to_string(),
                page_slug: None,
                page_title: None,
                error: None,
            }).await;
        } else {
            pending.push(id);
        }
    }

    // ── 步骤 2: 按 ingested_at 升序排列 ──────────────────
    //
    // llm-wiki 策略: "写作优先, 推文其次, 书签第三, 消息最后"。
    // ClawWiki 适配: 微信文章 > URL > 微信消息 > 文件 > 粘贴 > 语音。
    // 同一优先级内按时间升序 (最早的先处理, 建立知识骨架)。
    //
    // 实现: 读取每个 entry 的 RawEntry metadata, 按 ingested_at 排序。
    // source 优先级作为次要排序键。
    let mut entries_with_meta: Vec<(u32, wiki_store::RawEntry)> = Vec::new();
    for &id in &pending {
        match wiki_store::read_raw_entry(paths, id) {
            Ok((entry, _body)) => entries_with_meta.push((id, entry)),
            Err(e) => {
                // raw entry 不可读 -> 记录错误, 跳过
                result.failed += 1;
                let _ = progress_tx.send(AbsorbProgressEvent {
                    processed: result.created + result.updated
                        + result.skipped + result.failed,
                    total: entry_ids.len(),
                    current_entry_id: id,
                    action: "skip".to_string(),
                    page_slug: None,
                    page_title: None,
                    error: Some(format!("无法读取 raw entry: {e}")),
                }).await;
            }
        }
    }
    entries_with_meta.sort_by(|a, b| {
        let priority_a = source_priority(&a.1.source);
        let priority_b = source_priority(&b.1.source);
        priority_a.cmp(&priority_b)
            .then_with(|| a.1.ingested_at.cmp(&b.1.ingested_at))
    });

    let total = entry_ids.len();
    let mut processed_in_batch = 0usize;

    // ── 步骤 3: 主吸收循环 ────────────────────────────────
    for (id, _entry_meta) in &entries_with_meta {
        // 取消检查: 每轮迭代前检查 cancel_token
        if cancel_token.is_cancelled() {
            result.cancelled = true;
            break;
        }

        processed_in_batch += 1;
        let current_processed = result.created + result.updated
            + result.skipped + result.failed;

        // ── 3a: 读取 raw entry 内容 ──
        let (entry, body) = match wiki_store::read_raw_entry(paths, *id) {
            Ok(pair) => pair,
            Err(e) => {
                result.failed += 1;
                let _ = progress_tx.send(AbsorbProgressEvent {
                    processed: current_processed + 1,
                    total,
                    current_entry_id: *id,
                    action: "skip".to_string(),
                    page_slug: None,
                    page_title: None,
                    error: Some(format!("读取失败: {e}")),
                }).await;
                continue;
            }
        };

        // ── 3b: 读取 _index.md 提供上下文 ──
        //
        // llm-wiki 做法: 每次吸收前读取 index 作为 LLM 的上下文,
        // 让 LLM 知道 wiki 中已有哪些页面, 避免创建重复概念。
        let index_content = std::fs::read_to_string(
            paths.wiki.join(wiki_store::WIKI_INDEX_FILENAME)
        ).unwrap_or_default();

        // ── 3c: 构建 SKILL prompt ──
        //
        // System prompt = CLAUDE.md 规则 + 当前 wiki 目录概览
        // User prompt = raw entry 内容 + 指令
        //
        // 关键: prompt 中包含 anti-cramming 和 anti-thinning 规则,
        // 要求 LLM 判断是创建新页面还是更新已有页面。
        let system_prompt = build_absorb_system_prompt(paths, &index_content);
        let user_prompt = build_absorb_user_prompt(&entry, &body);
        let request = build_absorb_request(&system_prompt, &user_prompt);

        // ── 3d: 调用 LLM ──
        let response = match broker.chat_completion(request).await {
            Ok(resp) => resp,
            Err(e) => {
                // LLM 调用失败 -> 重试一次
                match broker.chat_completion(
                    build_absorb_request(&system_prompt, &user_prompt)
                ).await {
                    Ok(resp) => resp,
                    Err(retry_err) => {
                        // 二次失败 -> 跳过, 不中断批次
                        result.failed += 1;
                        let _ = progress_tx.send(AbsorbProgressEvent {
                            processed: current_processed + 1,
                            total,
                            current_entry_id: *id,
                            action: "skip".to_string(),
                            page_slug: None,
                            page_title: None,
                            error: Some(format!(
                                "LLM 调用失败 (已重试): {e} / {retry_err}"
                            )),
                        }).await;
                        continue;
                    }
                }
            }
        };

        // ── 3e: 解析 LLM 响应为 WikiPageProposal ──
        //
        // 复用现有 propose_for_raw_entry 的 JSON 解析逻辑。
        // 容忍 ```json 围栏、缺失的 source_raw_id 等。
        let proposal = match parse_absorb_response(&response, *id) {
            Ok(p) => p,
            Err(e) => {
                // JSON 解析失败 -> 记录预览, 跳过, 继续
                result.failed += 1;
                let _ = progress_tx.send(AbsorbProgressEvent {
                    processed: current_processed + 1,
                    total,
                    current_entry_id: *id,
                    action: "skip".to_string(),
                    page_slug: None,
                    page_title: None,
                    error: Some(format!("LLM 响应解析失败: {e}")),
                }).await;
                continue;
            }
        };

        // ── 3f: 判断 create vs update ──
        //
        // 检查目标 slug 对应的 wiki 页面是否已存在。
        // 存在 -> 读取已有内容, 合并 (主题驱动, 非日记驱动)
        // 不存在 -> 创建新页面
        let page_exists = wiki_store::read_wiki_page(paths, &proposal.slug).is_ok();
        let action;
        let final_body;
        let category = determine_category(&proposal);

        if page_exists {
```

> **Phase 1 MVP note** (as of 2026-04-23) · item 1 in `backlog/phase1-deferred.md`:
> the `update` branch below ships as a string concat (`format!("{}\n\n---\n\n{}",
> existing_body, proposal.body)`) in Phase 1 — the LLM-driven merge round-trip
> described in this pseudocode is deferred to Phase 2. Anti-thinning still holds
> because the new proposal body is appended (never replaces), but the topic
> re-organisation the merge prompt would perform is not run at absorb time.

```rust
            // ── 3f-update: 合并已有页面 ──
            //
            // anti-thinning: 合并后页面必须比合并前更丰富。
            // 实现: 读取旧 body, 构建 merge prompt, 让 LLM 生成合并后的 body。
            // 合并 prompt 强调 "按主题组织, 不按时间排列"。
            let (existing_summary, existing_body) =
                wiki_store::read_wiki_page(paths, &proposal.slug)
                    .map_err(|e| MaintainerError::Store(e.to_string()))?;

            let merge_request = build_merge_request(
                &existing_body,
                &proposal.body,
                &existing_summary.title,
            );
            match broker.chat_completion(merge_request).await {
                Ok(merge_resp) => {
                    final_body = extract_merged_body(&merge_resp)
                        .unwrap_or(proposal.body.clone());
                }
                Err(_) => {
                    // 合并失败 -> 使用新 proposal body 追加到末尾
                    final_body = format!(
                        "{}\n\n---\n\n{}", existing_body, proposal.body
                    );
                }
            }
            action = "update";
        } else {
            // ── 3f-create: 新建页面 ──
            final_body = proposal.body.clone();
            action = "create";
        }

        // ── 3g: 写入磁盘 ──
        match wiki_store::write_wiki_page_in_category(
            paths,
            &category,
            &proposal.slug,
            &proposal.title,
            &proposal.summary,
            &final_body,
            Some(*id),
        ) {
            Ok(_path) => {}
            Err(e) => {
                result.failed += 1;
                let _ = progress_tx.send(AbsorbProgressEvent {
                    processed: current_processed + 1,
                    total,
                    current_entry_id: *id,
                    action: "skip".to_string(),
                    page_slug: Some(proposal.slug.clone()),
                    page_title: Some(proposal.title.clone()),
                    error: Some(format!("磁盘写入失败: {e}")),
                }).await;
                continue;
            }
        }

```

> **Phase 1 MVP note** (as of 2026-04-23) · item 2 in `backlog/phase1-deferred.md`:
> step 3h (bidirectional wikilink maintenance) is not implemented in Phase 1.
> Reverse discoverability is instead served at query time by the persisted
> `_backlinks.json` index (built by `build_backlinks_index` + refreshed at the
> 15-entry checkpoint + final checkpoint in §5.1 step 4 / 5). The body-mutation
> path that would call `ensure_bidirectional_link(paths, &proposal.slug,
> target_slug)` is postponed to Phase 2 until maintainer quality telemetry
> shows it's needed.

```rust
        // ── 3h: 添加双向 wikilinks ──
        //
        // 扫描新写入页面的 body, 提取 [[slug]] 或 [](concepts/slug.md) 引用。
        // 对每个引用的目标页面, 反向添加对当前页面的链接 (如果尚未存在)。
        // 这保证 A -> B 暗含 B -> A 的可发现性。
        let outgoing_links = wiki_store::extract_internal_links(&final_body);
        for target_slug in &outgoing_links {
            ensure_bidirectional_link(paths, &proposal.slug, target_slug);
        }

```

> **Phase 1 MVP note** (as of 2026-04-23) · item 3 in `backlog/phase1-deferred.md`:
> step 3i (LLM-based conflict detection → Inbox) is explicitly skipped in Phase 1.
> The actual `absorb_batch` (`wiki_maintainer/src/lib.rs:1858`) carries a comment
> `// 3i: Conflict detection (simplified: skip LLM-based detection for MVP).
> Full LLM-based conflict detection deferred to later sprint.` No `Conflict`
> Inbox entries are produced on update paths until Phase 2 ships a calibrated
> conflict-detect prompt.

```rust
        // ── 3i: 冲突检测 ──
        //
        // 如果 action == "update", 检查新信息是否与已有判断矛盾。
        // 实现: 构建 conflict detection prompt, 让 LLM 比较
        // 旧内容和新 raw entry, 判断是否存在矛盾。
        // 矛盾 -> mark_conflict -> 创建 Inbox 条目。
        if action == "update" {
            if let Some(conflict_reason) = detect_conflict(
                paths, broker, &proposal.slug, &body
            ).await {
                wiki_store::append_inbox_pending(
                    paths,
                    "conflict",  // InboxKind::Conflict, serde kebab-case
                    &format!("冲突: {} - {}", proposal.title, conflict_reason),
                    &conflict_reason,
                    Some(*id),
                ).ok();
            }
        }

```

> **Phase 1 MVP note** (as of 2026-04-23) · item 5 in `backlog/phase1-deferred.md`:
> step 3g-extra computes `confidence` in Phase 1 but the three-dimensional
> evaluation described here is simplified. Actual impl
> (`wiki_maintainer/src/lib.rs:1866-1877`) uses `source_count = absorb_log
> entries targeting this slug + 1`, fixes `newest_source_age_days = 0`, and
> fixes `has_conflict = false`. The real `count_sources_for_page` +
> `newest_source_age_days` lookups wait on a raw→page provenance index that
> doesn't exist yet; `has_pending_conflict` waits on item 3. Phase 3 target.

```rust
        // ── 3g-extra: 计算 confidence 分数 ──
        //
        // confidence 由 absorb 自动计算, 不可手动设置。
        // 规则: 基于来源数量 + 时效性 + 冲突状态三维评估。
        // 见 technical-design.md §3.3 WikiFrontmatter 的完整计算规则。
        {
            let source_count = count_sources_for_page(paths, &proposal.slug);
            let newest_age = newest_source_age_days(paths, &proposal.slug);
            let has_conflict = wiki_store::has_pending_conflict(paths, &proposal.slug);
            let confidence = compute_confidence(source_count, newest_age, has_conflict);
            let _ = wiki_store::update_page_confidence(paths, &proposal.slug, confidence);
        }

        // ── 3i-extra: Supersession 判断变迁 ──
        //
        // 当用户在 Inbox 中审批冲突条目并选择 "采纳新观点" 时:
        // 1. 将旧判断封装为 SupersessionRecord
        // 2. 追加到页面 frontmatter 的 superseded 数组
        // 3. 更新页面内容为新判断
        // 4. 写入 changelog
        //
        // 注: 此步骤的实际执行在 Inbox conflict resolution 流程中触发,
        // 而非在 absorb 主循环中。absorb 仅负责检测冲突 (步骤 3i)。
        // conflict resolution 伪代码:
        //
        // fn resolve_conflict_adopt_new(paths, slug, old_claim, new_claim, source) {
        //     let record = SupersessionRecord {
        //         claim: old_claim,
        //         replaced_by: new_claim,
        //         date: now_iso8601(),
        //         source: source,
        //     };
        //     wiki_store::append_supersession(paths, slug, record)?;
        //     wiki_store::update_wiki_page_content(paths, slug, new_content)?;
        //     wiki_store::append_wiki_log(paths, "supersede", &format!("{} → {}", old_claim, new_claim));
        //     // 重新计算 confidence (冲突解决后不再是 contested)
        //     recompute_confidence(paths, slug);
        // }

        // ── 3j: 记录 absorb_log ──
        let log_entry = wiki_store::AbsorbLogEntry {
            entry_id: *id,
            timestamp: wiki_store::now_iso8601(),
            action: action.to_string(),
            page_slug: Some(proposal.slug.clone()),
            page_title: Some(proposal.title.clone()),
            page_category: Some(category.clone()),  // category 由 determine_category() 在步骤 3f 计算
        };
        let _ = wiki_store::append_absorb_log(paths, log_entry);

```

> **Phase 1 MVP note** (as of 2026-04-23) · item 6 in `backlog/phase1-deferred.md`:
> Phase 1 appends only to the global `wiki/log.md` (via
> `append_wiki_log(paths, verb, title)` at `wiki_maintainer/src/lib.rs:1864`).
> The per-day `wiki/changelog/YYYY-MM-DD.md` file mentioned in the step header
> is **not** written. Day files are a UX convenience for the Dashboard "Today"
> view; Phase 4 will add `append_changelog_entry` once the Dashboard consumer
> lands.

```rust
        // ── 3j-extra: 追加 wiki/log.md 和 changelog ──
        let verb = if action == "create" {
            "absorb-create"
        } else {
            "absorb-update"
        };
        let _ = wiki_store::append_wiki_log(paths, verb, &proposal.title);

        // ── 3k: 发送进度事件 ──
        if action == "create" { result.created += 1; }
        else { result.updated += 1; }

        let _ = progress_tx.send(AbsorbProgressEvent {
            processed: result.created + result.updated
                + result.skipped + result.failed,
            total,
            current_entry_id: *id,
            action: action.to_string(),
            page_slug: Some(proposal.slug.clone()),
            page_title: Some(proposal.title.clone()),
            error: None,
        }).await;

```

> **Phase 1 MVP note** (as of 2026-04-23) · item 4 in `backlog/phase1-deferred.md`:
> the 15-entry checkpoint in Phase 1 runs `rebuild_wiki_index` +
> `build_backlinks_index` + `save_backlinks_index` (items 4a+4b) but **skips
> step 4c `quality_spot_check`**. The anti-thinning + topic-organisation prompt
> rules (§5.1 system prompt items 3-4) already bias the LLM against diary
> bodies at write time; the spot-check is belt-and-suspenders. Phase 3 target.

```rust
        // ── 步骤 4: 每 15 条执行 checkpoint ──────────────
        //
        // llm-wiki 规则: "every 15 entries: checkpoint"
        //   - rebuild_wiki_index(): 刷新 wiki/index.md 目录
        //   - build_backlinks_index() + save: 重建反向链接
        //   - 质量审计: 随机抽取 3 个最近更新的页面, 检查是否存在
        //     日记体结构 (violation of "按主题组织" 原则)
        if processed_in_batch % 15 == 0 && processed_in_batch > 0 {
            // 4a: 重建 wiki/index.md
            let _ = wiki_store::rebuild_wiki_index(paths);

            // 4b: 重建反向链接索引
            if let Ok(bl_index) = wiki_store::build_backlinks_index(paths) {
                let _ = wiki_store::save_backlinks_index(paths, &bl_index);
            }

            // 4c: 质量抽查 (pick 3 most-updated pages)
            //
            // 读取最近 3 个被 absorb 更新的 page, 检查其 body
            // 是否呈现日记体结构 (连续的日期标题 ## 2026-04-xx)。
            // 如果发现日记体, 生成 cleanup-suggestion 类型的 Inbox 条目。
            quality_spot_check(paths, broker, 3).await;
        }
    }

    // ── 步骤 5: 最终 checkpoint ──────────────────────────
    //
    // 无论批次大小, 结束时都执行一次完整的 index 重建。
    let _ = wiki_store::rebuild_wiki_index(paths);
    if let Ok(bl_index) = wiki_store::build_backlinks_index(paths) {
        let _ = wiki_store::save_backlinks_index(paths, &bl_index);
    }

    result.duration_ms = start.elapsed().as_millis() as u64;
    Ok(result)
}
```

#### 辅助函数说明

```rust
/// 素材优先级排序: 数字越小越优先
///
/// "query" 类型来自对话结晶 (Crystallization): /query 生成的高质量回答
/// 自动写入 raw/, 优先级低于真实来源素材但高于粘贴文本,
/// 确保结晶内容被吸收但不会压制原始素材。
fn source_priority(source: &str) -> u8 {
    match source {
        "wechat-article" => 1,  // 微信文章 (最高)
        "url" => 2,             // URL 抓取
        "wechat-text" => 3,     // 微信消息
        "pdf" | "docx" | "pptx" => 4,  // 文件
        "query" => 5,           // 对话结晶 (Crystallization): /query 回答回写
        "paste" => 6,           // 粘贴文本
        "voice" => 7,           // 语音 (最低)
        _ => 8,
    }
}

/// Confidence 自动计算函数
///
/// 由 absorb_batch 步骤 3g-extra 调用, 每次写入/更新页面后重新计算。
/// 规则见 technical-design.md §3.3 WikiFrontmatter。
fn compute_confidence(
    source_count: usize,
    newest_source_age_days: u64,
    has_pending_conflict: bool,
) -> f32 {
    if has_pending_conflict {
        return 0.3; // contested: 有未解决冲突, 最高优先判定
    }
    if source_count >= 3 && newest_source_age_days < 30 {
        return 0.9; // high: 多源佐证 + 时效性
    }
    if source_count >= 2 && newest_source_age_days < 90 {
        return 0.6; // medium: 有交叉验证
    }
    0.2 // low: 单源或陈旧
}

/// 判断 proposal 应写入哪个 wiki 分类目录。
///
/// 逻辑:
///   1. LLM 在 proposal 中可能指定 category (未来扩展)
///   2. 默认: 大多数内容归 "concept"
///   3. 如果 title 含有人名特征 -> "people"
///   4. 如果 title 含有 "vs" / "对比" -> "compare"
///   5. 如果 body 涉及多个子概念聚合 -> "topic"
fn determine_category(proposal: &WikiPageProposal) -> String {
    // MVP: 默认全部归 "concept", 后续由 LLM 在 proposal 中指定
    "concept".to_string()
}

/// 构建 absorb 阶段的 system prompt。
///
/// 包含:
/// - CLAUDE.md 中的 wiki-maintainer 规则
/// - 当前 wiki/index.md 内容 (让 LLM 知道已有哪些页面)
/// - anti-cramming + anti-thinning 指令
/// - 输出格式要求 (strict JSON WikiPageProposal)
fn build_absorb_system_prompt(
    paths: &wiki_store::WikiPaths,
    index_content: &str,
) -> String {
    let claude_md = std::fs::read_to_string(&paths.schema_claude_md)
        .unwrap_or_default();
    format!(
        "{claude_md}\n\n\
         ## 当前 Wiki 目录\n\n{index_content}\n\n\
         ## 吸收规则\n\n\
         1. 如果 raw entry 的核心概念在 Wiki 中已有对应页面, \
            返回该页面的 slug 和合并后的 body。\n\
         2. 如果是全新概念, 创建新页面。宁可创建新页面也不要把不相关\
            的内容塞进已有页面 (anti-cramming)。\n\
         3. 如果更新已有页面, 合并后的内容必须比更新前更丰富 \
            (anti-thinning)。严禁产生残桩。\n\
         4. 按主题组织内容, 不要按时间排列。不要写成日记体。\n\
         5. 百科全书语气: 平实、事实、中立。归因优于断言。\n\
         6. 每篇 body 不超过 200 词。引用不超过 15 个连续词。\n\
         7. 返回 STRICT JSON, 格式同 WikiPageProposal。"
    )
}

/// 构建 absorb 阶段的 user prompt。
fn build_absorb_user_prompt(entry: &wiki_store::RawEntry, body: &str) -> String {
    // 截断超大 body: > 50KB 时截取前 10K 字符
    let truncated = if body.len() > 50_000 {
        &body[..10_000]
    } else {
        body
    };
    format!(
        "Raw entry:\n\
         - id: {id}\n\
         - filename: {filename}\n\
         - source: {source}\n\
         - ingested_at: {ingested_at}\n\
         \n\
         Body:\n\
         {body}\n\
         \n\
         产出 wiki 页面 JSON proposal。JSON only, source_raw_id = {id}。",
        id = entry.id,
        filename = entry.filename,
        source = entry.source,
        ingested_at = entry.ingested_at,
        body = truncated,
    )
}

/// 构建合并请求: 让 LLM 将新内容合并到已有页面。
fn build_merge_request(
    existing_body: &str,
    new_body: &str,
    title: &str,
) -> MessageRequest {
    let system = format!(
        "你是 wiki 页面合并助手。将新内容合并到已有页面 \"{title}\" 中。\n\n\
         规则:\n\
         1. 按主题组织, 不按时间排列\n\
         2. 合并后必须比合并前更丰富 (anti-thinning)\n\
         3. 不超过 200 词\n\
         4. 返回纯 markdown body, 不要 frontmatter, 不要 JSON 围栏"
    );
    let user = format!(
        "已有内容:\n{existing_body}\n\n新增内容:\n{new_body}\n\n\
         请输出合并后的完整 body。"
    );
    // ... 构建 MessageRequest
    todo!("assemble MessageRequest with system + user")
}

/// 双向链接维护: 确保 A -> B 时, B 的 body 中也包含对 A 的引用。
fn ensure_bidirectional_link(
    paths: &wiki_store::WikiPaths,
    from_slug: &str,
    to_slug: &str,
) {
    // 读取 to_slug 页面 body
    // 检查其中是否已有 from_slug 的链接
    // 如果没有, 在 "相关页面" section 末尾追加
    //   [FromTitle](concepts/from_slug.md)
    // 原子写入更新后的文件
}

/// 冲突检测: 让 LLM 判断新内容是否与已有页面内容矛盾。
async fn detect_conflict(
    paths: &wiki_store::WikiPaths,
    broker: &(impl BrokerSender + ?Sized),
    slug: &str,
    new_raw_body: &str,
) -> Option<String> {
    // 读取已有页面 body
    // 构建 conflict detection prompt
    // LLM 返回: "no_conflict" 或 "conflict: {reason}"
    // 返回 Some(reason) 或 None
    None // placeholder
}

/// 质量抽查: 检查最近更新的 N 个页面是否有日记体结构。
async fn quality_spot_check(
    paths: &wiki_store::WikiPaths,
    broker: &(impl BrokerSender + ?Sized),
    count: usize,
) {
    // 从 _absorb_log.json 最后 N 条 "update" 记录中取 page_slug
    // 读取对应页面 body
    // 检测: body 中是否出现 3+ 个连续日期标题 (## YYYY-MM-DD)
    // 如果发现 -> 创建 cleanup-suggestion 类型 Inbox 条目
}
```

### 5.2 query_wiki -- Wiki 知识问答

```rust
/// Wiki-grounded Q&A: 检索 -> 构建上下文 -> 流式回答。
///
/// 算法:
///   1. 读取 wiki/index.md 获取所有页面列表
///   2. 使用关键词匹配 + 反向链接拓扑找到相关页面
///   3. 读取 top-K 页面全文作为 LLM context
///   4. 构建 RAG prompt (wiki context + user question)
///   5. 调用 broker 获取回答, 通过 response_tx 逐 chunk 发送
pub async fn query_wiki(
    paths: &wiki_store::WikiPaths,
    question: &str,
    max_sources: usize,
    broker: &(impl BrokerSender + ?Sized),
    response_tx: tokio::sync::mpsc::Sender<QueryChunkEvent>,
) -> Result<QueryResult> {
    // ── 步骤 1: 加载 wiki 索引 ──
    let all_pages = wiki_store::list_all_wiki_pages(paths)
        .map_err(|e| MaintainerError::Store(e.to_string()))?;
    if all_pages.is_empty() {
        return Err(MaintainerError::RawNotAvailable(
            "wiki 为空, 无法回答问题".to_string()
        ));
    }

    // ── 步骤 2: 相关性排序 ──
    //
    // MVP 方案: 关键词匹配 (question 中的词 vs page title + summary)。
    // 后续可替换为 embedding-based 检索。
    //
    // 额外信号: 使用 backlinks index, 被引用最多的页面 relevance +0.1
    let backlinks = wiki_store::load_backlinks_index(paths).unwrap_or_default();
    let mut scored: Vec<(f32, &WikiPageSummary)> = Vec::new();
    for page in &all_pages {
        let score = compute_relevance(question, page, &backlinks);
        scored.push((score, page));
    }
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let top_k: Vec<_> = scored.into_iter().take(max_sources).collect();

    // ── 步骤 3: 读取 top-K 页面全文 ──
    let mut context_parts: Vec<String> = Vec::new();
    let mut sources: Vec<QuerySource> = Vec::new();
    for (score, page) in &top_k {
        if let Ok((_summary, body)) = wiki_store::read_wiki_page(paths, &page.slug) {
            // 截取 snippet (前 200 字符)
            let snippet: String = body.chars().take(200).collect();
            sources.push(QuerySource {
                slug: page.slug.clone(),
                title: page.title.clone(),
                relevance_score: *score,
                snippet,
            });
            context_parts.push(format!(
                "## {title} (slug: {slug})\n\n{body}",
                title = page.title, slug = page.slug, body = body
            ));
        }
    }

    // ── 发送 query_sources 事件 ──
    // (由 HTTP handler 负责, 此处 sources 在返回值中)

    // ── 步骤 4: 构建 RAG prompt ──
    let wiki_context = context_parts.join("\n\n---\n\n");
    let system = format!(
        "你是 ClawWiki 知识问答助手。基于以下 wiki 页面回答用户问题。\n\
         引用时使用 [页面标题](concepts/slug.md) 格式。\n\
         如果 wiki 中没有相关信息, 明确说明。\n\n\
         --- Wiki 上下文 ---\n\n{wiki_context}"
    );
    let request = MessageRequest {
        model: prompt::MAINTAINER_MODEL.to_string(),
        max_tokens: 2000,
        system: Some(system),
        messages: vec![InputMessage {
            role: "user".to_string(),
            content: vec![InputContentBlock::Text {
                text: question.to_string()
            }],
        }],
        tools: None,
        tool_choice: None,
        stream: false, // MVP 非流式, 后续改为流式
    };

    // ── 步骤 5: 调用 LLM 获取回答 ──
    let response = broker.chat_completion(request).await?;
    let answer_text = extract_first_text(&response).unwrap_or_default();

    // 发送回答 chunk (MVP: 一次性发送全文)
    let source_refs: Vec<String> = sources.iter().map(|s| s.slug.clone()).collect();
    let _ = response_tx.send(QueryChunkEvent {
        delta: answer_text.clone(),
        source_refs,
    }).await;

    // ── 步骤 6: Crystallization — 对话结晶 ──
    //
    // 将高质量回答写入 raw/ 目录, 供下次 /absorb 吸收进 Wiki。
    // 这构成 "问得越多 → Wiki 越强 → 回答越准" 的正反馈闭环。
    //
    // 条件: 仅结晶长度 > 200 字符的实质性回答, 过滤掉 "未找到" 等简短回复。
    // source_type = "query", 优先级低于 wechat-article 但高于 paste (见 source_priority)。
    if answer_text.chars().count() > 200 {
        let slug = slugify(question);
        let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let crystal_frontmatter = wiki_store::RawFrontmatter {
            source: "query".to_string(),
            slug: Some(slug.clone()),
            title: Some(format!("Query: {}", truncate(question, 100))),
            ingested_at: wiki_store::now_iso8601(),
        };
        // 写入 raw/entries/{date}_query-{slug}.md
        let _ = wiki_store::write_raw_entry(
            paths,
            "query",
            &format!("{date}_query-{slug}"),
            &answer_text,
            crystal_frontmatter,
        );
        // 注: 不在此处触发 absorb, 等待用户或定时任务的下次 /absorb
    }

    Ok(QueryResult {
        sources,
        total_tokens: response.usage.input_tokens
            + response.usage.output_tokens,
    })
}

/// 计算问题与页面的相关性分数。
///
/// 算法:
/// - title 完全匹配: +1.0
/// - title 包含问题中的关键词: +0.3/词
/// - summary 包含关键词: +0.15/词
/// - 页面被其他页面引用的次数 (从 backlinks index): +0.05/次, 上限 0.3
/// - 最终分数 clamp 到 [0.0, 1.0]
fn compute_relevance(
    question: &str,
    page: &WikiPageSummary,
    backlinks: &BacklinksIndex,
) -> f32 {
    let mut score: f32 = 0.0;
    let q_lower = question.to_lowercase();
    let title_lower = page.title.to_lowercase();

    // 精确匹配
    if q_lower.contains(&title_lower) || title_lower.contains(&q_lower) {
        score += 1.0;
    }

    // 关键词匹配 (简单分词: 按空格和标点切分)
    let keywords: Vec<&str> = q_lower
        .split(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
        .filter(|w| w.len() >= 2)
        .collect();
    for kw in &keywords {
        if title_lower.contains(kw) { score += 0.3; }
        if page.summary.to_lowercase().contains(kw) { score += 0.15; }
    }

    // 反向链接加成
    let inbound = backlinks.get(&page.slug).map(|v| v.len()).unwrap_or(0);
    score += (inbound as f32 * 0.05).min(0.3);

    score.min(1.0)
}
```

### 5.3 cleanup -- 质量审计

```rust
/// 质量审计: 让 LLM 批量审查 wiki 页面, 生成优化建议。
///
/// 算法:
///   1. 列出所有 wiki 页面
///   2. 按 5 页一批分组 (避免单次 LLM 调用上下文过长)
///   3. 对每批: 读取全部 body, 构建 cleanup prompt, 让 LLM 判断
///      哪些页面应该合并/扩展/拆分/废弃
///   4. 对每条建议创建 Inbox 条目 (kind = "cleanup-suggestion")
pub async fn run_cleanup(
    paths: &wiki_store::WikiPaths,
    broker: &(impl BrokerSender + ?Sized),
    progress_tx: tokio::sync::mpsc::Sender<CleanupProgressEvent>,
) -> Result<CleanupResult> {
    let all_pages = wiki_store::list_all_wiki_pages(paths)
        .map_err(|e| MaintainerError::Store(e.to_string()))?;
    let total = all_pages.len();
    let mut suggestions: Vec<CleanupSuggestion> = Vec::new();
    let mut inbox_created = 0usize;
    let start = std::time::Instant::now();

    // 分批: 每 5 页一组
    for (batch_idx, chunk) in all_pages.chunks(5).enumerate() {
        let _ = progress_tx.send(CleanupProgressEvent {
            phase: "analyzing".to_string(),
            checked: batch_idx * 5 + chunk.len(),
            total,
        }).await;

        // 读取本批全部页面 body
        let mut pages_text = String::new();
        for page in chunk {
            if let Ok((_s, body)) = wiki_store::read_wiki_page(paths, &page.slug) {
                pages_text.push_str(&format!(
                    "### {} (slug: {}, {} words)\n{}\n\n",
                    page.title, page.slug,
                    body.split_whitespace().count(),
                    body
                ));
            }
        }

        // 构建 cleanup prompt
        let system = "你是 wiki 质量审计员。审查以下页面, 找出:\n\
            1. 合并机会 (merge): 两个页面内容高度重叠\n\
            2. 扩展需求 (expand): 页面不足 50 词, 是残桩\n\
            3. 拆分建议 (split): 页面超过 300 词, 涵盖多个独立概念\n\
            4. 废弃建议 (deprecate): 内容过时或已被其他页面完全覆盖\n\n\
            返回 JSON 数组, 每项: {kind, pages: [slugs], reason, confidence}。\
            如果没有问题, 返回空数组 []。";

        let request = MessageRequest {
            model: prompt::MAINTAINER_MODEL.to_string(),
            max_tokens: 1000,
            system: Some(system.to_string()),
            messages: vec![InputMessage {
                role: "user".to_string(),
                content: vec![InputContentBlock::Text { text: pages_text }],
            }],
            tools: None, tool_choice: None, stream: false,
        };

        if let Ok(resp) = broker.chat_completion(request).await {
            if let Some(text) = extract_first_text(&resp) {
                if let Ok(batch_suggestions) =
                    serde_json::from_str::<Vec<CleanupSuggestion>>(&text)
                {
                    for s in batch_suggestions {
                        // 置信度 >= 0.7 时创建 Inbox 条目
                        if s.confidence >= 0.7 {
                            let _ = wiki_store::append_inbox_pending(
                                paths,
                                "cleanup-suggestion",
                                &format!("清理建议: {} {:?}", s.kind, s.pages),
                                &s.reason,
                                None,
                            );
                            inbox_created += 1;
                        }
                        suggestions.push(s);
                    }
                }
            }
        }
    }

    Ok(CleanupResult {
        suggestions,
        inbox_entries_created: inbox_created,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}
```

### 5.4 patrol -- 结构巡检

Patrol 不依赖 LLM, 纯本地检测。由 `wiki_patrol` crate 提供。

```rust
/// 运行全部 5 种巡检检测器, 生成 PatrolReport。
///
/// 5 种检测器按顺序执行:
///   1. orphans   - 无入链的孤儿页面
///   2. stale     - last_verified > 90 天的过期页面
///   3. schema_violations - frontmatter 缺少必填字段
///   4. oversized - 正文 > 500 行
///   5. stubs     - 正文 < 10 行
///
/// 每种检测器发现的问题都生成对应类型的 Inbox 条目。
pub fn run_patrol(
    paths: &wiki_store::WikiPaths,
    progress_tx: &std::sync::mpsc::Sender<PatrolProgressEvent>,
) -> PatrolReport {
    let mut all_issues: Vec<PatrolIssue> = Vec::new();

    // ── 检测器 1: 孤儿页面 ──
    let _ = progress_tx.send(PatrolProgressEvent {
        check: "orphans".into(), status: "running".into(),
    });
    let orphans = detect_orphans(paths);
    let _ = progress_tx.send(PatrolProgressEvent {
        check: "orphans".into(), status: "done".into(),
    });

    // ── 检测器 2: 过期页面 ──
    let _ = progress_tx.send(PatrolProgressEvent {
        check: "stale".into(), status: "running".into(),
    });
    let stale = detect_stale(paths, 90); // 默认 90 天阈值
    let _ = progress_tx.send(PatrolProgressEvent {
        check: "stale".into(), status: "done".into(),
    });

    // ── 检测器 3: Schema 违规 ──
    let _ = progress_tx.send(PatrolProgressEvent {
        check: "schema_violations".into(), status: "running".into(),
    });
    let violations = detect_schema_violations(paths);
    let _ = progress_tx.send(PatrolProgressEvent {
        check: "schema_violations".into(), status: "done".into(),
    });

    // ── 检测器 4: 超长页面 ──
    let _ = progress_tx.send(PatrolProgressEvent {
        check: "oversized".into(), status: "running".into(),
    });
    let oversized = detect_oversized(paths, 500); // 默认 500 行阈值
    let _ = progress_tx.send(PatrolProgressEvent {
        check: "oversized".into(), status: "done".into(),
    });

    // ── 检测器 5: 残桩页面 ──
    let _ = progress_tx.send(PatrolProgressEvent {
        check: "stubs".into(), status: "running".into(),
    });
    let stubs = detect_stubs(paths, 10); // 默认 10 行阈值
    let _ = progress_tx.send(PatrolProgressEvent {
        check: "stubs".into(), status: "done".into(),
    });

    // 汇总
    let summary = PatrolSummary {
        orphans: orphans.len(),
        stale: stale.len(),
        schema_violations: violations.len(),
        oversized: oversized.len(),
        stubs: stubs.len(),
    };

    all_issues.extend(orphans);
    all_issues.extend(stale);
    all_issues.extend(violations);
    all_issues.extend(oversized);
    all_issues.extend(stubs);

    // 为每个 issue 创建 Inbox 条目
    for issue in &all_issues {
        let inbox_kind = match issue.kind {
            PatrolIssueKind::Stale => "stale",  // InboxKind::Stale, serde kebab-case
            PatrolIssueKind::SchemaViolation => "schema-violation",
            _ => continue, // orphan/oversized/stub 仅报告, 不自动创建 inbox
        };
        let _ = wiki_store::append_inbox_pending(
            paths,
            inbox_kind,
            &format!("{:?}: {}", issue.kind, issue.page_slug),
            &issue.description,
            None,
        );
    }

    PatrolReport {
        issues: all_issues,
        summary,
        checked_at: wiki_store::now_iso8601(),
    }
}

/// 检测孤儿页面: 无入链 + 不在 index.md 引用中。
pub fn detect_orphans(paths: &wiki_store::WikiPaths) -> Vec<PatrolIssue> {
    let backlinks = wiki_store::load_backlinks_index(paths).unwrap_or_default();
    let index_content = std::fs::read_to_string(
        paths.wiki.join(wiki_store::WIKI_INDEX_FILENAME)
    ).unwrap_or_default();

    let all_pages = wiki_store::list_all_wiki_pages(paths).unwrap_or_default();
    let mut orphans = Vec::new();

    for page in &all_pages {
        let has_inbound = backlinks.get(&page.slug)
            .map(|v| !v.is_empty()).unwrap_or(false);
        let in_index = index_content.contains(&format!("{}.md", page.slug));
        if !has_inbound && !in_index {
            orphans.push(PatrolIssue {
                kind: PatrolIssueKind::Orphan,
                page_slug: page.slug.clone(),
                description: format!(
                    "该页面无任何入链, 且不被 index.md 引用"
                ),
                suggested_action:
                    "添加至相关 topic 页面的引用, 或标记为 deprecated".into(),
            });
        }
    }
    orphans
}

/// 检测过期页面: last_verified 为 None 或距今超过 max_age_days 天。
pub fn detect_stale(
    paths: &wiki_store::WikiPaths,
    max_age_days: u32,
) -> Vec<PatrolIssue> {
    let all_pages = wiki_store::list_all_wiki_pages(paths).unwrap_or_default();
    let mut stale = Vec::new();
    let now = chrono::Utc::now();

    for page in &all_pages {
        let is_stale = match &page.last_verified {
            None => true, // 从未验证过
            Some(date_str) => {
                // 解析日期, 判断是否超过阈值
                // 解析失败视为 stale
                chrono::DateTime::parse_from_rfc3339(date_str)
                    .map(|dt| (now - dt).num_days() > max_age_days as i64)
                    .unwrap_or(true)
            }
        };
        if is_stale {
            stale.push(PatrolIssue {
                kind: PatrolIssueKind::Stale,
                page_slug: page.slug.clone(),
                description: format!(
                    "最后验证时间超过 {} 天 ({:?})",
                    max_age_days, page.last_verified
                ),
                suggested_action:
                    "重新验证页面内容时效性".into(),
            });
        }
    }
    stale
}
```

---

## 6. 前端实现

### 6.1 组件清单

| 组件 | 位置 | 用途 |
|------|------|------|
| `SkillProgressCard` | `src/components/skill/SkillProgressCard.tsx` | 显示 /absorb 进度条 + 实时日志 |
| `AbsorbTriggerButton` | `src/components/skill/AbsorbTriggerButton.tsx` | "开始维护" 按钮, 出现在 Raw Library 和 Inbox |
| `PatrolReportPanel` | `src/components/skill/PatrolReportPanel.tsx` | 巡检报告面板, 嵌入 Dashboard |
| `QueryPanel` | `src/components/chat/QueryPanel.tsx` | 增强的 Chat Tab, 支持 wiki-grounded 引用 |
| `CleanupReportDialog` | `src/components/skill/CleanupReportDialog.tsx` | cleanup 报告弹窗 |

### 6.2 SkillProgressCard

```
┌──────────────────────────────────────────┐
│  吸收进度                         [取消]  │
│                                          │
│  ██████████████░░░░░░░░░░  3/5 (60%)     │
│                                          │
│  ✓ #1 → transformer-architecture [新建]   │
│  ✓ #2 → attention-mechanism [更新]        │
│  ⟳ #3 → processing...                    │
│  ○ #4 待处理                              │
│  ○ #5 待处理                              │
│                                          │
│  已创建: 1  已更新: 1  跳过: 0  失败: 0    │
└──────────────────────────────────────────┘
```

**数据流**:
- 订阅 SSE `absorb_progress` 事件
- 每收到一个事件, 更新进度条和日志列表
- 收到 `absorb_complete` 时显示汇总并触发 wiki 页面列表 refetch

**状态管理** (Zustand store):

```typescript
interface SkillStore {
  activeTask: {
    taskId: string;
    type: 'absorb' | 'cleanup' | 'patrol';
    status: 'running' | 'complete' | 'cancelled' | 'error';
    progress: AbsorbProgressEvent[];
    result: AbsorbCompleteEvent | null;
  } | null;

  startAbsorb: (entryIds?: number[]) => Promise<void>;
  cancelAbsorb: () => void;
  onProgress: (event: AbsorbProgressEvent) => void;
  onComplete: (event: AbsorbCompleteEvent) => void;
}
```

### 6.3 AbsorbTriggerButton

两个放置位置:

1. **Raw Library 页面**: 批量选择 raw entries 后, "吸收选中" 按钮
2. **Inbox 页面**: 对 kind="new-raw" 的条目, "自动维护" 按钮

```typescript
interface AbsorbTriggerButtonProps {
  entryIds?: number[];     // 指定 IDs; 省略时吸收全部
  variant: 'primary' | 'ghost';
  size: 'sm' | 'md';
}
```

### 6.4 PatrolReportPanel

嵌入 Dashboard 的巡检健康面板:

```
┌────────────────────────────────────┐
│  Wiki 健康度                        │
│                                    │
│  孤儿页面    ██  2                  │
│  过期页面    ███ 3                  │
│  Schema违规  █  1                  │
│  超长页面    █  1                  │
│  残桩页面    ██  2                  │
│                                    │
│  上次巡检: 2 小时前   [立即巡检]     │
└────────────────────────────────────┘
```

### 6.5 QueryPanel

Chat Tab 中的增强问答面板。当 `/query` 回答返回时, 在消息气泡下方显示引用来源:

```
┌──────────────────────────────────────┐
│ [Assistant]                          │
│                                      │
│ 根据你的知识库, Transformer 与 RNN    │
│ 的主要区别在于...                     │
│                                      │
│ ┌─ 引用来源 ─────────────────────┐   │
│ │ ● Transformer 架构 (95%)       │   │
│ │ ● RNN 基础 (82%)              │   │
│ │ ● 注意力机制 (71%)            │   │
│ └────────────────────────────────┘   │
└──────────────────────────────────────┘
```

---

## 7. 交互流程图

### 7.1 /absorb 完整流程

```
用户                    前端                   desktop-server          wiki_maintainer
 │                       │                        │                       │
 │ 点击 "开始维护"       │                        │                       │
 │─────────────────────>│                        │                       │
 │                       │ POST /api/wiki/absorb  │                       │
 │                       │──────────────────────>│                       │
 │                       │                        │ 验证请求               │
 │                       │                        │ 解析 entry_ids        │
 │                       │                        │ 检查 409 (已有任务)    │
 │                       │                        │                       │
 │                       │ 202 {task_id, total}   │                       │
 │                       │<──────────────────────│                       │
 │                       │                        │                       │
 │                       │ 订阅 SSE /events       │ tokio::spawn          │
 │                       │──────────────────────>│──────────────────────>│
 │                       │                        │                       │
 │                       │                        │          absorb_batch │
 │                       │                        │                       │
 │                       │                        │   ┌─── 循环 per entry │
 │                       │                        │   │                   │
 │                       │                        │   │ read_raw_entry    │
 │                       │                        │   │ build prompt      │
 │                       │                        │   │ chat_completion   │
 │                       │                        │   │ parse proposal    │
 │                       │                        │   │ write_wiki_page   │
 │                       │                        │   │ append_absorb_log │
 │                       │                        │   │                   │
 │                       │  SSE: absorb_progress  │   │ progress_tx.send  │
 │                       │<──────────────────────│<──│                   │
 │ 更新进度条            │                        │   │                   │
 │<─────────────────────│                        │   │ (每 15 条)        │
 │                       │                        │   │ rebuild_index     │
 │                       │                        │   │ build_backlinks   │
 │                       │                        │   │ quality_check     │
 │                       │                        │   │                   │
 │                       │                        │   └───────────────── │
 │                       │                        │                       │
 │                       │  SSE: absorb_complete  │   return AbsorbResult │
 │                       │<──────────────────────│<──────────────────────│
 │ 显示完成汇总          │                        │                       │
 │ refetch wiki pages    │                        │                       │
 │<─────────────────────│                        │                       │
```

### 7.2 /query 流程

```
用户                    前端                   desktop-server          wiki_maintainer
 │                       │                        │                       │
 │ 输入 "/query 问题"    │                        │                       │
 │─────────────────────>│                        │                       │
 │                       │ POST /api/wiki/query   │                       │
 │                       │──────────────────────>│                       │
 │                       │                        │ query_wiki()          │
 │                       │                        │──────────────────────>│
 │                       │                        │                       │
 │                       │  SSE: query_sources    │                       │
 │                       │<──────────────────────│   search + rank       │
 │ 显示引用来源          │                        │                       │
 │<─────────────────────│                        │                       │
 │                       │                        │   build RAG prompt    │
 │                       │                        │   chat_completion     │
 │                       │  SSE: query_chunk (N)  │                       │
 │                       │<──────────────────────│   stream answer       │
 │ 流式显示回答          │                        │                       │
 │<─────────────────────│                        │                       │
 │                       │  SSE: query_done       │                       │
 │                       │<──────────────────────│   return QueryResult  │
 │ 显示完成 + token统计  │                        │                       │
 │<─────────────────────│                        │                       │
```

### 7.3 /patrol 流程

```
用户                    前端                   desktop-server          wiki_patrol
 │                       │                        │                       │
 │ 点击 "立即巡检"       │                        │                       │
 │─────────────────────>│                        │                       │
 │                       │ POST /api/wiki/patrol  │                       │
 │                       │──────────────────────>│                       │
 │                       │ 202 {task_id}          │                       │
 │                       │<──────────────────────│                       │
 │                       │                        │ run_patrol()          │
 │                       │                        │──────────────────────>│
 │                       │                        │                       │
 │                       │ SSE: patrol_progress   │  detect_orphans()     │
 │                       │<──────────────────────│<──────────────────────│
 │                       │ SSE: patrol_progress   │  detect_stale()       │
 │                       │<──────────────────────│<──────────────────────│
 │                       │ SSE: patrol_progress   │  detect_schema_*()    │
 │                       │ SSE: patrol_progress   │  detect_oversized()   │
 │                       │ SSE: patrol_progress   │  detect_stubs()       │
 │                       │                        │                       │
 │                       │ SSE: patrol_report     │  return PatrolReport  │
 │                       │<──────────────────────│<──────────────────────│
 │ Dashboard 更新        │                        │                       │
 │<─────────────────────│                        │                       │
```

---

## 8. 测试用例

### 8.1 absorb_batch 基础流程

```rust
#[tokio::test]
async fn absorb_batch_creates_and_updates() {
    // Setup:
    //   - 初始化空 wiki
    //   - 写入 3 个 raw entries:
    //     #1: "Transformer 架构" (wechat-article)
    //     #2: "RNN 基础" (paste)
    //     #3: "Transformer 的注意力机制" (wechat-article, 与 #1 相关)
    //   - MockBrokerSender:
    //     #1 -> proposal: slug="transformer-architecture", title="Transformer 架构"
    //     #2 -> proposal: slug="rnn-basics", title="RNN 基础"
    //     #3 -> proposal: slug="transformer-architecture", title="Transformer 架构" (同 slug -> update)

    // Execute: absorb_batch(paths, [1, 2, 3], mock_broker, tx, token)

    // Assert:
    //   result.created == 2 (transformer-architecture + rnn-basics)
    //   result.updated == 1 (transformer-architecture 被 #3 更新)
    //   result.skipped == 0
    //   result.failed == 0
    //   wiki/concepts/transformer-architecture.md 存在
    //   wiki/concepts/rnn-basics.md 存在
    //   _absorb_log.json 有 3 条记录
    //   progress_tx 收到 3 个 AbsorbProgressEvent
}
```

### 8.2 absorb_batch 冲突检测

```rust
#[tokio::test]
async fn absorb_batch_detects_conflict_creates_inbox() {
    // Setup:
    //   - 初始化 wiki, 预先写入 wiki page: "RNN 优于 Transformer"
    //   - 写入 raw entry: "Transformer 全面超越 RNN" (矛盾内容)
    //   - MockBrokerSender: proposal 更新 rnn-basics, conflict detection 返回 "矛盾"

    // Execute: absorb_batch

    // Assert:
    //   inbox 中新增一条 kind="conflict" 的条目
    //   inbox entry.description 包含冲突原因
    //   result.updated == 1 (仍然写入了更新, 冲突仅标记不阻断)
}
```

### 8.3 absorb_batch 跳过已吸收

```rust
#[tokio::test]
async fn absorb_batch_skips_already_absorbed() {
    // Setup:
    //   - 初始化 wiki
    //   - 写入 raw entry #1
    //   - 在 _absorb_log.json 中预写入 entry_id=1 的记录

    // Execute: absorb_batch(paths, [1], ...)

    // Assert:
    //   result.skipped == 1
    //   result.created == 0
    //   MockBrokerSender 未被调用 (chat_completion 调用次数 == 0)
}
```

### 8.4 query_wiki 基本问答

```rust
#[tokio::test]
async fn query_wiki_returns_answer_with_citations() {
    // Setup:
    //   - 初始化 wiki, 写入 2 个概念页面:
    //     "transformer-architecture" (body: "Transformer 使用自注意力...")
    //     "rnn-basics" (body: "RNN 是一种序列模型...")
    //   - MockBrokerSender: 回答 "Transformer 与 RNN 的区别在于..."

    // Execute: query_wiki(paths, "Transformer 和 RNN 有什么区别?", 5, ...)

    // Assert:
    //   result.sources.len() >= 2
    //   result.sources 包含 "transformer-architecture" 和 "rnn-basics"
    //   response_tx 收到至少 1 个 QueryChunkEvent
    //   QueryChunkEvent.source_refs 非空
}
```

### 8.5 checkpoint 索引重建

```rust
#[tokio::test]
async fn absorb_batch_checkpoint_rebuilds_index_after_15() {
    // Setup:
    //   - 初始化空 wiki
    //   - 写入 16 个 raw entries
    //   - MockBrokerSender: 每个都返回唯一的 proposal

    // Execute: absorb_batch(paths, [1..=16], ...)

    // Assert:
    //   wiki/index.md 在第 15 条处理后被重建 (检查 modified time 或内容)
    //   _backlinks.json 存在且非空
    //   wiki/index.md 包含前 15 个创建的页面
    //   最终 (第 16 条后) index 包含全部 16 个页面
}
```

### 8.6 patrol 检测器

```rust
#[test]
fn patrol_detects_orphan_pages() {
    // Setup: wiki 中有 page A (被 B 引用) 和 page C (无入链)
    // Assert: detect_orphans 返回 [C]
}

#[test]
fn patrol_detects_stale_pages() {
    // Setup: page 的 last_verified = "2025-01-01" (超过 90 天)
    // Assert: detect_stale(paths, 90) 包含该 page
}

#[test]
fn patrol_detects_stubs() {
    // Setup: page body 仅 3 行
    // Assert: detect_stubs(paths, 10) 包含该 page
}

#[test]
fn patrol_detects_oversized() {
    // Setup: page body 超过 500 行
    // Assert: detect_oversized(paths, 500) 包含该 page
}
```

---

## 9. 边界条件 & 错误处理

### 9.1 LLM 相关

| 条件 | 处理策略 | 影响范围 |
|------|----------|----------|
| LLM 返回格式错误的 JSON | 记录 preview (前 512 字符) 到日志, 跳过该 entry, 继续下一条 | 单条 entry, 不中断批次 |
| LLM 调用超时 | 重试 1 次 (相同请求); 二次失败则跳过 | 单条 entry |
| LLM 返回空响应 | 视为格式错误, 走 BadJson 处理路径 | 单条 entry |
| LLM 返回过长内容 (> 800 tokens) | 截断到 MAX_OUTPUT_TOKENS, 仍尝试解析 | 无 (LLM 端 max_tokens 已限制) |
| BrokerSender 无可用 provider | 返回 503 BROKER_UNAVAILABLE, 不启动任务 | 整个批次 |

### 9.2 并发控制

| 条件 | 处理策略 |
|------|----------|
| 并发 absorb 调用 | 第二个调用返回 `409 ABSORB_IN_PROGRESS`; 通过 `AbsorbTaskManager` 的 `AtomicBool` 标志位控制 |
| 并发 cleanup 调用 | 同上, `409 CLEANUP_IN_PROGRESS` |
| 并发 patrol 调用 | 同上, `409 PATROL_IN_PROGRESS` |
| absorb 期间 query | 允许并行; query 读取的是磁盘快照, absorb 的原子写入保证不会读到半写文件 |
| absorb 期间手动 approve-with-write | 允许; wiki_store 的 `WIKI_WRITE_GUARD` mutex 保证串行写入 |

**实现**: `desktop-core::AbsorbTaskManager`

```rust
pub struct AbsorbTaskManager {
    absorb_running: AtomicBool,
    cleanup_running: AtomicBool,
    patrol_running: AtomicBool,
    current_task_id: Mutex<Option<String>>,
    cancel_token: Mutex<Option<CancellationToken>>,
}

impl AbsorbTaskManager {
    pub fn try_start_absorb(&self, task_id: &str) -> Result<(), SkillError> {
        if self.absorb_running.compare_exchange(
            false, true, Ordering::SeqCst, Ordering::SeqCst
        ).is_err() {
            return Err(SkillError::AlreadyRunning("absorb"));
        }
        *self.current_task_id.lock().unwrap() = Some(task_id.to_string());
        *self.cancel_token.lock().unwrap() = Some(CancellationToken::new());
        Ok(())
    }

    pub fn finish_absorb(&self) {
        self.absorb_running.store(false, Ordering::SeqCst);
        *self.current_task_id.lock().unwrap() = None;
        *self.cancel_token.lock().unwrap() = None;
    }

    pub fn cancel_absorb(&self) {
        if let Some(token) = self.cancel_token.lock().unwrap().as_ref() {
            token.cancel();
        }
    }
}
```

### 9.3 数据边界

| 条件 | 处理策略 |
|------|----------|
| raw/ 为空 (无 entry) | `absorb_batch` 立即返回 `AbsorbResult { processed: 0, ... }`, HTTP 返回 202 但 total=0 |
| entry body > 50KB | 截断到前 10,000 字符后发送给 LLM; 在 progress event 中标注 "(已截断)" |
| entry body 为空 | 跳过该 entry, action="skip", error="entry body 为空" |
| slug 冲突 (两个不同 raw entry 生成相同 slug) | 第二个走 "update" 路径, 合并内容 |
| wiki/index.md 不存在 | `absorb_batch` 步骤 3b 使用空字符串; checkpoint 会创建它 |
| _absorb_log.json 损坏 | `list_absorb_log` 返回 `Err(AbsorbLogCorrupted)`; `absorb_batch` 降级为不检查已吸收状态, 全量处理 |
| _backlinks.json 损坏 | `load_backlinks_index` 返回空 HashMap; 下次 checkpoint 时重建 |

### 9.4 用户取消

用户点击 "取消" 按钮:
1. 前端调用 `DELETE /api/wiki/absorb/{task_id}` (或通过 WebSocket 信令)
2. HTTP handler 调用 `AbsorbTaskManager::cancel_absorb()`
3. `CancellationToken` 被 cancel
4. `absorb_batch` 在下一次循环迭代前检测到取消
5. 设置 `result.cancelled = true`, 返回已完成部分的结果
6. HTTP handler 发送 `absorb_complete` 事件 (带 cancelled=true 标志)
7. 前端显示 "已取消, 已处理 N/M 条"

---

## 10. 已有代码复用清单

| 函数/结构体 | 文件路径 | 复用方式 |
|------------|----------|----------|
| `wiki_store::rebuild_wiki_index()` | `rust/crates/wiki_store/src/lib.rs` L1871 | 在 checkpoint (每 15 条) 和最终步骤中直接调用 |
| `wiki_store::append_wiki_log()` | `rust/crates/wiki_store/src/lib.rs` L1804 | 每次写入 wiki page 后调用, 记录 "absorb-create" 或 "absorb-update" |
| `wiki_store::write_wiki_page_in_category()` | `rust/crates/wiki_store/src/lib.rs` L1137 | `absorb_batch` 步骤 3g 的核心写入函数 |
| `wiki_store::extract_internal_links()` | `rust/crates/wiki_store/src/lib.rs` L1383 | 步骤 3h 提取 wikilinks, 驱动双向链接维护 |
| `wiki_store::read_raw_entry()` | `rust/crates/wiki_store/src/lib.rs` | 步骤 3a 读取 raw entry 内容 |
| `wiki_store::read_wiki_page()` | `rust/crates/wiki_store/src/lib.rs` L1214 | 步骤 3f 判断页面是否存在 + 读取已有内容 |
| `wiki_store::list_all_wiki_pages()` | `rust/crates/wiki_store/src/lib.rs` L1190 | query_wiki 和 patrol 遍历全部页面 |
| `wiki_store::list_raw_entries()` | `rust/crates/wiki_store/src/lib.rs` | HTTP handler 过滤未吸收的 entry |
| `wiki_store::WikiPaths` | `rust/crates/wiki_store/src/lib.rs` L236 | 所有 SKILL Engine 函数的第一个参数 |
| `wiki_maintainer::propose_for_raw_entry()` | `rust/crates/wiki_maintainer/src/lib.rs` L126 | `absorb_batch` 内部复用其 prompt 构建和 JSON 解析逻辑 |
| `wiki_maintainer::prompt::build_concept_request()` | `rust/crates/wiki_maintainer/src/prompt.rs` L74 | absorb prompt 的基础模板 (扩展了 index 上下文) |
| `wiki_maintainer::prompt::SYSTEM_PROMPT` | `rust/crates/wiki_maintainer/src/prompt.rs` L43 | absorb system prompt 的基础 (追加 anti-cramming 等规则) |
| `wiki_maintainer::BrokerSender` trait | `rust/crates/wiki_maintainer/src/lib.rs` L107 | absorb_batch 和 query_wiki 的 LLM 调用接口 |
| `wiki_maintainer::WikiPageProposal` | `rust/crates/wiki_maintainer/src/lib.rs` L46 | LLM 返回的标准 proposal 结构 |
| `desktop-core::BrokerAdapter` | `rust/crates/desktop-core/src/wiki_maintainer_adapter.rs` L44 | HTTP handler 构建 `BrokerAdapter::from_global()` 传给 SKILL 函数 |
| `desktop-core::agentic_loop::PermissionGate` | `rust/crates/desktop-core/src/agentic_loop.rs` L74 | 取消协议参照 (CancellationToken 使用方式) |
| `desktop-core::agentic_loop::MAX_LOOP_ITERATIONS` | `rust/crates/desktop-core/src/agentic_loop.rs` L27 | 值 50; absorb_batch 不使用 agentic_loop, 但遵循相同的循环上限约定 |
| `.clawwiki/schema/CLAUDE.md` | `.clawwiki/schema/CLAUDE.md` | absorb system prompt 读取此文件作为规则来源 |
| `wiki_store::append_inbox_pending()` | `rust/crates/wiki_store/src/lib.rs` | 冲突检测和 patrol 用于创建 Inbox 条目 |
| `wiki_store::list_backlinks()` | `rust/crates/wiki_store/src/lib.rs` L1427 | patrol orphan 检测的辅助 (v1 per-page 版本, v2 用 backlinks index 替代) |

### 新增函数 (需实现)

| 函数 | Crate | 说明 |
|------|-------|------|
| `append_absorb_log()` | `wiki_store` | 追加吸收日志, 受 ABSORB_LOG_GUARD 保护 |
| `list_absorb_log()` | `wiki_store` | 读取吸收日志, 按 timestamp 倒序 |
| `is_entry_absorbed()` | `wiki_store` | 判断 entry 是否已吸收 |
| `build_backlinks_index()` | `wiki_store` | 重建完整反向链接索引 |
| `save_backlinks_index()` | `wiki_store` | 持久化反向链接索引 |
| `load_backlinks_index()` | `wiki_store` | 加载已持久化的索引 |
| `validate_frontmatter()` | `wiki_store` | 校验 frontmatter 合规性 |
| `wiki_stats()` | `wiki_store` | 计算聚合统计 |
| `absorb_batch()` | `wiki_maintainer` | 批量吸收主循环 |
| `query_wiki()` | `wiki_maintainer` | Wiki-grounded Q&A |
| `run_cleanup()` | `wiki_maintainer` | 质量审计 |
| `run_patrol()` | `wiki_patrol` (新 crate) | 结构巡检 |
| `detect_orphans()` | `wiki_patrol` | 孤儿页面检测 |
| `detect_stale()` | `wiki_patrol` | 过期页面检测 |
| `detect_schema_violations()` | `wiki_patrol` | Schema 违规检测 |
| `detect_oversized()` | `wiki_patrol` | 超长页面检测 |
| `detect_stubs()` | `wiki_patrol` | 残桩页面检测 |

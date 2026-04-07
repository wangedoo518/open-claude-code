# Audit Lessons — Agent Runtime Failures & Fixes

## 背景

2026-04 对 open-claude-code(8) 的破坏性审计暴露了 Phase 1-18 实施过程中累积的 15 个未修复问题。本文档记录每个问题的**根因、错误表现、修复方法和未来防护措施**，防止同类问题再次出现。

---

## 失败模式分类

### 1. 时序/竞态类 (Race Conditions)
异步代码中两个路径在无锁或锁粒度不足时互相覆盖状态。典型症状：间歇性故障、"偶尔"用户看到错误决定。

### 2. 真相源分裂 (Split Source of Truth)
同一个概念在多个地方存储（前端 Zustand + 后端内存 + 磁盘文件），无同步机制。典型症状：用户改了 UI 但后端不生效、刷新页面后设置"丢失"。

### 3. 欺骗性完成 (Deceptive Completion)
代码编译通过、测试通过，但实际功能未完成。典型症状：commit message 声称"完成 Phase X"但代码里只有 log、stub、或未注册的孤儿对象。

### 4. 边界条件遗漏 (Edge Case Omissions)
未考虑 UTF-8 边界、null/空/极值输入、并发修改。典型症状：生产环境偶发 panic、非 ASCII 用户看到乱码、大数据量溢出。

---

## 防护 Checklist（所有 PR 必过）

每次审查代码时，对照此 checklist 检查是否存在同类模式：

- [ ] **时序**: 所有 `tokio::time::timeout + Mutex/HashMap` 组合是否考虑了"成功响应与超时同时发生"？
- [ ] **时序**: 所有 `tokio::spawn` 是否处理了 runtime shutdown 场景？
- [ ] **时序**: 所有 fire-and-forget 的 `tokio::spawn` 是否可能与下一次 spawn 乱序？
- [ ] **真相源**: 新增任何"设置"字段时，问自己：如果用户在 UI 改它，后端怎么知道？
- [ ] **真相源**: 前端 state 和后端 state 是否都 persist？持久化路径一致吗？
- [ ] **诚实性**: commit message 里的 "完成 Phase X" 是否对应了实际功能工作？不只是编译通过？
- [ ] **诚实性**: log 里的 "discovered X items" 是否意味着 X 真的可用？
- [ ] **边界**: 字符串切片是否用 `is_char_boundary` 保护？
- [ ] **边界**: JSON 解析后是否校验了类型（`is_object()`, `is_array()`）？
- [ ] **边界**: SSE/byte stream 解析是否处理了跨 chunk 的 UTF-8 字符？

---

## 漏洞档案格式

每个漏洞按以下格式记录：

```
### L-XX: <简短标题>
- **严重度**: Critical / High / Medium / Low
- **类别**: <从 4 个失败模式选一个>
- **发现日期**: YYYY-MM-DD
- **修复 commit**: <hash>
- **涉及文件**: <file:line>

#### 症状
用户/开发者实际看到的现象。

#### 根因
代码层面的真实原因（不只是"有 bug"，而是为什么代码会这样写）。

#### 修复
具体改了什么，link 到代码。

#### 防护
如何在未来的 PR review / 测试 / 静态分析里检测同类问题。
```

---

## 漏洞档案汇总表

| ID | 类别 | 严重度 | 症状 | 修复 commit |
|----|------|--------|------|------------|
| L-01 | 时序/竞态 | Critical | 用户点 Allow 被静默判为 Deny | 6249672 |
| L-02 | 时序/竞态 | High | 长会话后磁盘丢消息 | 631307b |
| L-03 | 时序/竞态 | High | 崩溃后会话卡死 Running | 42cd302 |
| L-04 | 时序/竞态 | Medium | 取消按钮等 5 分钟才生效 | pending |
| L-05 | 边界条件 | Critical | 中文/emoji 流式输出乱码 | 157dc64 |
| L-06 | 真相源分裂 | High | UI 改权限后端不生效 | 6249672 |
| L-07 | 真相源分裂 | High | StreamingIndicator 闪烁 | 06e8734 |
| L-08 | 真相源分裂 | High | 两个独立 CWD 锁等于没锁 | 631307b |
| L-09 | 欺骗性完成 | Critical | MCP "discovered X tools" 但全部不可用 | 13c038b (降级) → pending (真修) |
| L-10 | 欺骗性完成 | High | fork 后会话重复压缩 | 56d377f |
| L-11 | 真相源分裂 | High | /compact 失败后 UI 已清空 | 157dc64 |
| L-12 | 欺骗性完成 | Medium | hooks 系统 config 源未接入 | pending |
| L-13 | 边界条件 | High | 工具循环第 2 轮 API 400 | 157dc64 |
| L-15 | 边界条件/安全 | Medium | CLAUDE.md prompt 注入无警告 | 56d377f |

**未修复（deferred）**：
（无）

**审计修复总计**：15 个问题全部处理完毕 ✅

---

## 漏洞档案详情

### L-01: PermissionGate 超时 vs resolve race

- **严重度**: Critical
- **类别**: 时序/竞态类
- **发现日期**: 2026-04-07
- **修复 commit**: 6249672 (Phase 2.1)
- **涉及文件**: `rust/crates/desktop-core/src/agentic_loop.rs:139-167`

#### 症状
用户在权限对话框上点 "Allow"，但 agentic loop 收到 "permission request timed out"。用户怀疑自己手抖点错了按钮。间歇性、难以复现。

#### 根因
之前修复（P1-2）把 `pending` 从 `Option<>` 改成 `HashMap<String, PendingPermission>` 修了一个 race，但引入了另一个：

```
1. agentic loop: check_permission 插入 entry, 等待 oneshot, 触发 timeout
2. 几乎同时: 前端发来 Allow → resolve() 调用
3. timeout path 抢先拿到锁 → 主动 pending.remove() → drop sender
4. resolve() 拿到锁时 entry 已消失 → 返回 false
5. agentic loop 的 receiver.await 返回 Err → 被判为 Deny
```

核心错误：check_permission 在**成功路径**也 remove entry。但 `Ok(Ok(decision))` 意味着 sender 已经发送过——这只能发生在 resolve() 已经 remove 了 entry 的情况下。**成功路径不需要二次清理**。

#### 修复
只在 failure 路径（timeout / channel closed）清理 pending entry。`Ok(Ok(decision))` 路径跳过清理——信任 resolve() 已经做完。

新增 5 个 async 测试：
- `resolve_wins_when_user_responds_before_timeout` — 核心 race scenario
- `allow_always_remembers_tool`
- `bypass_all_short_circuits`
- `read_only_tools_auto_allowed`
- `resolve_with_unknown_id_returns_false`

#### 防护
- [ ] **审查 checklist**：`tokio::time::timeout + oneshot` 组合必须明确"谁在哪条路径清理资源"
- [ ] 不要在成功路径和失败路径都 remove HashMap entry——只在一条路径做
- [ ] 成功路径的 `Ok(Ok(_))` 意味着 sender 已经发送 → entry 已被 resolve() 移除

---

### L-06: permissionMode 前端 Zustand vs 磁盘双源头

- **严重度**: High
- **类别**: 真相源分裂
- **发现日期**: 2026-04-07
- **修复 commit**: 6249672 (Phase 2.2)
- **涉及文件**: `apps/desktop-shell/src/state/settings-store.ts`, `rust/crates/desktop-core/src/lib.rs`

#### 症状
用户在 UI 切换 permission mode 到 "DangerFullAccess"，但 agentic loop 仍然弹权限对话框。用户困惑："我不是关了吗？"

#### 根因
前端 Zustand `settings-store.setPermissionMode` 只更新内存 + localStorage。后端 agentic loop 读取 `.claw/settings.json` 磁盘文件。两个存储**没有任何同步机制**。用户的 UI 操作不会 propagate 到后端。

之前的 P0-3 修复让后端读磁盘是对的，但只解决了一半——前端还是在更新另一个源头。

#### 修复
确立**磁盘为单一真相源**：
1. 后端新增 `DesktopState::set_permission_mode` / `get_permission_mode`，读写 `.claw/settings.json`
2. 新增 HTTP 路由 `POST/GET /api/desktop/settings/permission-mode`
3. 前端 API 层新增 `writePermissionModeToDisk` / `readPermissionModeFromDisk`
4. Zustand `setPermissionMode` 改为**optimistic update + rollback**：先 setState（UI 响应），后台调 API，失败则回滚
5. 新增 `hydratePermissionModeFromDisk` 用于启动时同步
6. Mode label 规范化：`bypassPermissions` ↔ `danger-full-access`

#### 防护
- [ ] **审查 checklist**：任何"设置"字段新增时，问：如果用户在 UI 改它，后端怎么知道？
- [ ] 前后端必须有明确的**真相源所有权**——不能两边都是权威
- [ ] 前端 state 变更后如果涉及后端行为，必须有 write-through 或 event-driven sync

---

### L-09: MCP init 只 discover 不 register (欺骗性完成 → 真修复)

- **严重度**: Critical
- **类别**: 欺骗性完成
- **发现日期**: 2026-04-07
- **初次降级 commit**: 13c038b (Phase 3.1)
- **真修复 commit**: pending (medium-term MCP fix)
- **涉及文件**: `rust/crates/desktop-core/src/agentic_loop.rs`, `rust/crates/desktop-core/src/lib.rs`

#### 真修复方案（绕过 crate-private API）
1. `DesktopState` 新增持久化字段：
   - `mcp_manager: Arc<Mutex<Option<McpServerManager>>>`
   - `mcp_tools: Arc<RwLock<Vec<ManagedMcpTool>>>`
2. 新增 `ensure_mcp_initialized(project_path)` 方法：
   - 从 `.claw/settings.json` 加载 MCP 配置
   - 创建 `McpServerManager`，调用 `discover_tools()` 连接子进程
   - **manager 持久化保存**（不再临时创建后 drop）
3. 新增 `mcp_call_tool(qualified_name, args)` 方法直接调用 manager
4. Agentic loop 在工具执行时：
   - 检测 `mcp__*` 前缀 → 路由到 `call_mcp_tool` 辅助函数
   - 其他工具 → 走 `execute_tool_in_workspace` 正常路径
5. `build_api_request` 的 tools 列表**追加** discovered MCP tools 的 qualified_name，LLM 就能看到并调用
6. `append_user_message` 首次调用时 `ensure_mcp_initialized`（懒初始化）

**效果**：完全绕过 `tools::global_mcp_registry()`，MCP 工具现在是可用的，且 subprocess 连接跨调用复用（性能好）。

#### 防护
- [ ] **审查 checklist**：commit message 里的"完成"必须对应"功能可用"，不是"编译通过"
- [ ] log 里的 "discovered X items" 必须意味着 X 真的可用——否则前缀 `[PROBE]` 或 `[VALIDATION]`
- [ ] 依赖第三方 crate 的 private 全局状态是**反模式**——必须识别并绕过
- [ ] 任何标记为"已完成"的 Phase 必须有端到端手测验证

#### 症状
Commit message 声称 "Phase 16 MCP Client Runtime 完成"。日志显示 "MCP: discovered 3 tools from 1 servers"。但 LLM 调用 MCP 工具时返回 `{"server": "...", "status": "disconnected", "message": "Server not registered. Use MCP tool to connect first."}`。

#### 根因
`init_mcp_servers` 创建了一个本地 `McpServerManager`，调用 `discover_tools()` 发现工具，**打印 log**，然后函数返回——**manager 立即被 drop，子进程被杀**。

关键问题：vendored `tools` crate 使用一个 crate-private 的 `global_mcp_registry()` 来存储 MCP server 状态。`execute_tool("MCP", ...)` 访问的是这个私有 registry。我们在外部创建的 manager 永远不会被注册到那个 registry。

Commit 里的"完成"基于"编译通过 + log 有输出"，而不是"工具真的可调用"。这是**欺骗性完成**的教科书案例。

#### 修复（诚实降级，不是真修复）
1. 函数重命名 `init_mcp_servers` → `probe_mcp_servers`
2. 顶部 doc comment 明确写出 LIMITATION：不能真正注册工具
3. 在 `system_prompt::build_system_prompt` 过滤掉 MCP 工具（ListMcpResources/ReadMcpResource/McpAuth/MCP），避免 LLM 调用永远失败的工具
4. 日志前缀改为 `[MCP probe]`，消息改为 "WARNING: These tools are NOT callable from the agentic loop"
5. `tasks/todo.md` 里 Phase 16 从 ✅ 降为 ⚠️
6. 顺便修 Phase 7.2 的一部分：用 `catch_unwind` 捕获 panic + 捕获 JoinHandle

真正的修复需要：
- Fork `claw-code-parity` 并把 `global_mcp_registry()` 暴露为 pub
- 或在 desktop-core 实现独立 MCP client 绕过 vendored dispatcher
- 或使用 legacy `execute_live_turn` 路径（它通过 runtime 内部初始化触发 MCP 连接）

#### 防护
- [ ] **审查 checklist**：commit message 里的"完成"必须对应"功能可用"，不是"编译通过"
- [ ] log 里的 "discovered X items" 必须意味着 X 真的可用——否则前缀 `[PROBE]` 或 `[VALIDATION]`
- [ ] 依赖第三方 crate 的 private 全局状态是**反模式**，必须在代码设计阶段识别并声明
- [ ] 任何标记为"已完成"的 Phase 必须有端到端手测验证——不能只是 `cargo check`

---

### L-04: cancel_token 不中断 HTTP/permission 等待

- **严重度**: Medium
- **类别**: 时序/竞态类
- **发现日期**: 2026-04-07
- **修复 commit**: pending (Short-term improvements)
- **涉及文件**: `rust/crates/desktop-core/src/agentic_loop.rs:139-200, 660-720`

#### 症状
用户点 "取消" 按钮，`cancel_session` fire 了 `cancel_token`。但 agentic loop 仍然卡住，要等**最多 300 秒**（reqwest 请求超时）或 **5 分钟**（权限等待超时）才真正停止。用户体验极差——"取消"看起来不起作用。

#### 根因
Cancel 只在 loop 的**边界**（iteration 开始时、tool 执行前）检查。`call_llm_api_streaming` 内部的 `reqwest` HTTP 调用 **不监听 cancel_token**，bytes_stream 读取也不监听。`PermissionGate::check_permission` 的 `tokio::time::timeout` 也不监听。

在等待上游响应、读 SSE chunk、或等待用户决定的时候，cancel 信号被无视。

#### 修复
把三个阻塞点都包进 `tokio::select!`：
1. `call_llm_api_streaming` 的 `client.post(...).send()` → `select! { cancel, send }`
2. `parse_sse_stream` 的 `stream.next().await` → `loop { select! { cancel, next } }`（`while let` 改 `loop`+`break`）
3. `PermissionGate::check_permission` 的 `timeout(receiver)` → `select! { cancel, timeout }`

所有三处在 cancel 时返回 `Err("cancelled by user")` 或 `Deny { reason: "cancelled by user" }`。

新增测试 `cancel_aborts_pending_permission_wait` 验证 permission 等待被 cancel 立即中断（应在 2 秒内完成，而不是 5 分钟）。

#### 防护
- [ ] **审查 checklist**：任何 `.await` 的**长时间阻塞点**必须考虑是否需要配合 cancel_token 用 `tokio::select!`
- [ ] 所有 SSE / byte-stream 读取必须 race against cancellation
- [ ] 取消语义要一致：顶层取消 → 立即停止，而不是等到下一个 loop iteration

---

### L-12: hooks 系统 config 源未接入

- **严重度**: Medium
- **类别**: 欺骗性完成
- **发现日期**: 2026-04-07
- **修复 commit**: pending (Short-term improvements)
- **涉及文件**: `rust/crates/desktop-core/src/lib.rs:2733`

#### 症状
Phase 14 声称"Hooks 系统完成"并且 `HookRunner::new(config)` 被集成到 agentic loop 的 PreToolUse / PostToolUse 调用中。但用户在 `.claw/settings.json` 里配置的 hooks **从未被加载**——原因是 `config.hooks` 硬编码为 `None`，注释写着 `// TODO: load from .claude/settings.json`。

#### 根因
Phase 14 只完成了 hooks 的**执行通路**（hook_runner.run_pre_tool_use 等），但没有完成**数据通路**（从磁盘读到 HookRunner）。这是经典的"代码骨架完成但数据源缺失"的欺骗性完成。

`RuntimeConfig` 已经有 `hooks()` 方法返回 `RuntimeHookConfig`，Phase 2.2 的 permission mode 加载已经在同一个位置调用了 `ConfigLoader::default_for(&project_path).load()`。只需要在同一次 load 中多取一个字段。

#### 修复
重构 permission mode 加载代码为一次 `load()` + 返回 tuple:
```rust
let (bypass_permissions, hooks_config) = {
    let loader = ConfigLoader::default_for(&project_path_buf);
    match loader.load() {
        Ok(rc) => {
            let bypass = ...;
            let hooks = rc.hooks().clone();  // ← 新增
            (bypass, Some(hooks))
        }
        Err(_) => (false, None),
    }
};
```

然后 `config.hooks: hooks_config` 替代 `hooks: None`。

现在 `.claw/settings.json` 里的 `hooks` 节会被正确加载，PreToolUse/PostToolUse 会真实触发用户配置的 command。

#### 防护
- [ ] **审查 checklist**：任何"X 系统已集成"的 commit 必须验证数据源（配置文件、API、状态）是否真的接通
- [ ] `// TODO: load from ...` 的注释必须被 code review 发现并要求在同一次 PR 中完成
- [ ] "执行通路" 和 "数据通路" 是两个独立的完成度维度

---

### L-05: SSE multi-byte UTF-8 跨 chunk 损坏

- **严重度**: Critical
- **类别**: 边界条件遗漏
- **发现日期**: 2026-04-07
- **修复 commit**: pending (Phase 1.1)
- **涉及文件**: `rust/crates/desktop-core/src/agentic_loop.rs:705-707`

#### 症状
所有非 ASCII 语言的用户在流式响应中看到偶发乱码（U+FFFD 替换字符）。尤其明显于中日韩文字和 emoji。用户以为是 LLM 输出了垃圾字符。

#### 根因
SSE 解析器使用 `buffer: String` + `String::from_utf8_lossy(&chunk)`。HTTP `bytes_stream()` 返回的 chunk 边界是任意的，一个 3 字节的 UTF-8 字符（e.g., "中" = `E4 B8 AD`）可能被拆成两个 chunk：
- Chunk N 末尾：`E4 B8`
- Chunk N+1 开头：`AD`

`from_utf8_lossy` 对部分字节插入 U+FFFD 替换字符，导致字符永久损坏。

此外 `buffer = buffer[newline_pos+1..].to_string()` 每行重分配整个 buffer，是 O(n²)。

#### 修复
重写 buffer 为 `Vec<u8>`，只在 complete line（遇到 `\n`）时用 `String::from_utf8_lossy` decode（此时不会截断）。抽取 `drain_next_line` 纯函数独立测试。引入 6 个单元测试覆盖跨 chunk 字符、CRLF、空行、多行、大 buffer 性能。

#### 防护
- [ ] **审查 checklist**：任何处理 byte stream 的代码必须检查 UTF-8 边界安全
- [ ] 不要对 chunk 直接调用 `from_utf8_lossy` 或 `from_utf8`
- [ ] 测试必须包含"多字节字符跨 chunk 分割"场景
- [ ] 字节 buffer 消费必须用 `drain(..)` 而非 reslice

---

### L-13: tool_use input 非 object 类型

- **严重度**: High
- **类别**: 边界条件遗漏
- **发现日期**: 2026-04-07
- **修复 commit**: pending (Phase 1.2)
- **涉及文件**: `rust/crates/desktop-core/src/agentic_loop.rs:562-573`

#### 症状
工具循环的**第 2 轮**必然失败，Anthropic API 返回 400 Bad Request。用户看到 "API error: 400" 并以为是网络问题。

#### 根因
`build_api_request` 对 `ContentBlock::ToolUse.input` 使用 `serde_json::from_str(input).unwrap_or(Value::Object(empty))`。只处理了"解析失败"但没有处理"解析成功但类型不是 object"。如果 LLM 返回 `"input": "null"` 或 `"input": "[1,2,3]"`：
- `from_str` 成功返回 `Value::Null` 或 `Value::Array`
- 直接发给 Anthropic API
- API 规定 `tool_use.input` 必须是 object → 400

#### 修复
抽取 `coerce_tool_input_to_object` 辅助函数：`from_str(raw).ok().filter(|v| v.is_object()).unwrap_or_else(empty)`。引入 9 个单元测试覆盖 null/array/number/string/bool/malformed/empty/nested 场景。

#### 防护
- [ ] **审查 checklist**：任何 JSON 解析后要发送给严格 schema 的 API 时，必须 type-filter
- [ ] `.ok().filter()` 模式应用于所有"解析成功但类型不对"的场景
- [ ] 不要只用 `unwrap_or` 处理 `Result::Err`，要考虑 `Ok(wrong_type)`

---

### L-02: on_iteration_complete 持久化乱序写

- **严重度**: High
- **类别**: 时序/竞态类
- **发现日期**: 2026-04-07
- **修复 commit**: 631307b (Phase 6.1)
- **涉及文件**: `rust/crates/desktop-core/src/lib.rs:2684-2720`

#### 症状
长 agentic 循环（10+ 轮）结束后磁盘状态偶发丢失中间轮的 tool_result。重启应用后发现某些消息不见了。间歇性、难复现。

#### 根因
`on_iteration_complete` 回调每轮 spawn 一个新 tokio task 来持久化：
```rust
Arc::new(move |session| {
    tokio::spawn(async move {
        let mut store = s.store.write().await;
        // ...
        s.persist().await;
    });
});
```
轮 N 的 task 和轮 N+1 的 task 在 `store.write().await` 上竞争。tokio 的 RwLock 没有 FIFO 保证（除非用 Mutex）。任务 N+1 可能先拿到锁写入新状态，任务 N 后拿到锁用旧状态覆盖。**后写者赢**——但是旧状态。

#### 修复
在 callback 内增加一个**per-session tokio::sync::Mutex**，spawned task 必须先获取这个 mutex 才能访问 store。`tokio::Mutex::lock()` 文档保证 **FIFO 顺序**。这把并发 spawn 序列化成 FIFO 队列：
```rust
let persist_serial = Arc::new(Mutex::new(()));
// ...
let _persist_guard = serial.lock().await;  // FIFO
let mut store = s.store.write().await;
```

#### 防护
- [ ] **审查 checklist**：任何 fire-and-forget spawn 后访问共享状态时，问：两个 task 的顺序谁保证？
- [ ] `tokio::sync::Mutex` 是 FIFO，`tokio::sync::RwLock` 不是——需要顺序就用 Mutex
- [ ] 考虑用 mpsc channel 做持久化队列，比 mutex 更清晰表达"序列化"意图

---

### L-03: Drop guard async spawn 在 shutdown 时失败

- **严重度**: High
- **类别**: 时序/竞态类
- **发现日期**: 2026-04-07
- **修复 commit**: 42cd302 (Phase 4.2)
- **涉及文件**: `rust/crates/desktop-core/src/lib.rs:2707-2735`

#### 症状
应用 kill -9 或 panic 后重启，之前 Running 的会话卡住，用户无法发消息（SessionBusy 错误）。

#### 根因
`SessionCleanupGuard::drop` 在 panic 路径尝试 `tokio::spawn(async move { cleanup })`。但 runtime 正在 shutdown 时，`tokio::spawn` 会失败（或 task 被立即 drop）。结果：`permission_gates` / `cancel_tokens` / `turn_state` 都不会被清理。`turn_state` 保持 `Running` 永远卡住。

#### 修复
1. **Phase 4.1**: 启动时 reconcile stuck sessions（with_executor 中遍历 persisted sessions，Running → Idle）——这是主要防护
2. **Phase 4.2**: Drop guard 改为同步 `try_write`——非阻塞、无需 spawn；如果 lock 被占用就放弃（交给 startup reconcile 兜底）

#### 防护
- [ ] **审查 checklist**：`Drop` impl 里不应该依赖异步 runtime
- [ ] 关键状态（如 turn_state）必须有启动时的 reconciliation pass
- [ ] `tokio::spawn` inside `Drop` 不是 reliable 的 cleanup 机制

---

### L-08: 两个独立的 CWD process lock

- **严重度**: High
- **类别**: 真相源分裂
- **发现日期**: 2026-04-07
- **修复 commit**: 631307b (Phase 6.2)
- **涉及文件**: `rust/crates/desktop-core/src/lib.rs:3915`, `rust/crates/desktop-core/src/agentic_loop.rs:1058`

#### 症状
Legacy `execute_live_turn` 和 agentic_loop 同时运行时，两个任务互相修改进程 CWD（`std::env::set_current_dir`），互相看不到对方的 lock，导致工具在错的目录运行。间歇性——取决于调度顺序。

#### 根因
`lib.rs::process_workspace_lock` 和 `agentic_loop.rs::execute_tool_in_workspace` 的 local static OnceLock **是两个独立的 Mutex 实例**。

```rust
// lib.rs
fn process_workspace_lock() -> &'static StdMutex<()> {
    static LOCK: OnceLock<StdMutex<()>> = OnceLock::new();  // ← 局部 static
    // ...
}

// agentic_loop.rs
fn execute_tool_in_workspace(...) {
    static LOCK: OnceLock<StdMutex<()>> = OnceLock::new();  // ← 另一个局部 static
    // ...
}
```

两个"全局"锁，等于没有锁。P1-1 修复 CWD 并发时把锁加错了地方。

#### 修复
1. `process_workspace_lock` 公开为 `pub(crate)`
2. `agentic_loop` 的 `execute_tool_in_workspace` 删除自己的 local LOCK
3. 改用 `crate::process_workspace_lock()`

现在 legacy + agentic 共享同一个锁。

#### 防护
- [ ] **审查 checklist**：任何"global"的 `static` 变量必须问"真的只有一个吗？"
- [ ] 如果多个文件需要共用资源，定义一个中心化的 accessor 函数（pub 或 pub(crate)）
- [ ] 搜索代码库里所有 `OnceLock<Mutex<>>` 的位置，确保没有"多个全局"

---

### L-10: fork_session 用 default() 丢失状态

- **严重度**: High
- **类别**: 欺骗性完成
- **发现日期**: 2026-04-07
- **修复 commit**: 56d377f (Phase 7.1)
- **涉及文件**: `rust/crates/desktop-core/src/lib.rs:2532`

#### 症状
fork 一个跑过多次压缩的长会话后，fork 出来的会话 `compaction` 字段是 `None`。下一次 agentic loop 触发 `should_compact` 时，重新从零压缩——用户看到"📦 Context compacted"重复出现在同一份历史上。

#### 根因
```rust
let mut forked_session = RuntimeSession::default();  // ← 重置全部
for msg in fork_messages {
    let _ = forked_session.push_message(msg);  // 只复制 messages
}
```
`RuntimeSession::default()` 重置所有非 messages 字段：`compaction`, `usage`, `version`, `session_id`, `fork`（会被后续覆盖）。仅手动复制了 messages。任何运行时状态都丢失。

`let _ = push_message(msg)` 还吞掉了错误——如果 runtime 校验失败，那条消息静默丢失。

#### 修复
```rust
let mut forked_session = parent_session;  // Clone 完整状态
if let Some(idx) = message_index {
    forked_session.messages.truncate(idx + 1);
}
forked_session.fork = Some(...);
```

#### 防护
- [ ] **审查 checklist**："复制 + 修改少量字段" 场景用 clone + mutate，而不是 default + rebuild
- [ ] 数据结构有很多字段时，clone() 是最安全的，手动复制是最容易漏的

---

### L-15: CLAUDE.md 路径注入未警告

- **严重度**: Medium
- **类别**: 边界条件遗漏 / 安全
- **发现日期**: 2026-04-07
- **修复 commit**: 56d377f (Phase 7.3)
- **涉及文件**: `rust/crates/desktop-core/src/system_prompt.rs:76-99`

#### 症状
用户打开任意目录作为 project，上级目录（或 `~/.claude/`）中的 CLAUDE.md 会被自动加载为系统 prompt——**没有任何警告**，用户不知道。恶意或误放的 CLAUDE.md 可以覆盖用户意图、指示模型 exfiltrate 数据等。

#### 根因
`find_claude_md` 向上遍历项目路径查找 CLAUDE.md，找到就用。**没有追踪来源路径**，也**没有区分"项目内"和"祖先目录"**。所有找到的文件被同等信任。

#### 修复
1. 新结构 `ClaudeMdDiscovery { content, source, is_ancestor }`
2. 新函数 `find_claude_md_with_source` 返回 discovery，记录 `is_ancestor = true` 当文件来自非项目本身的目录
3. `build_system_prompt_with_source` 当 `is_ancestor == true` 时在 CLAUDE.md 块前插入 "Context Source Warning" 警告块
4. 所有发现都打 stderr 日志 `[CLAUDE.md] loaded from {path} (ancestor={bool})`

保留原 `find_claude_md` / `build_system_prompt` 作为向后兼容的 thin wrapper。

#### 防护
- [ ] **审查 checklist**：任何"向上遍历找配置"的功能必须追踪来源，并区分可信/不可信来源
- [ ] 文件系统路径是攻击面——不能默认信任任意目录下的文件
- [ ] 任何注入系统 prompt 的内容必须有明确的来源标记

---

### L-11: /compact 乐观 UI 无回滚

- **严重度**: High
- **类别**: 真相源分裂 / 欺骗性完成
- **发现日期**: 2026-04-07
- **修复 commit**: pending (Phase 1.3)
- **涉及文件**: `apps/desktop-shell/src/features/session-workbench/commandExecutor.ts:55-76`

#### 症状
用户执行 `/compact` 看到：
1. UI 瞬间清空
2. 显示 "Compacting conversation..."
3. 几秒后显示 "Failed to compact on backend. Local display cleared"
4. 用户困惑：我的消息呢？
5. 刷新页面 → 消息回来了

#### 根因
命令执行是 fire-and-forget：先 `onClearMessages()` 清空 UI，再 `void import(...).then(compactSession)`。即使 backend 返回错误（e.g., SessionBusy，因为刚修的 P1-3 Running guard），UI 已经清空。**乐观更新没有回滚机制**。

#### 修复
1. 扩展 `CommandDefinition.execute` 支持 `Promise<CommandResult>` 返回类型
2. `/compact` 改为 async：先 `await compactSession(sessionId)`，成功才 `onClearMessages()`
3. 失败时**不清 UI**，返回错误消息
4. `executeCommand` 调用方 (`SessionWorkbenchTerminal.handleSlashCommand`) 用 `instanceof Promise` 判断同步/异步路径

#### 防护
- [ ] **审查 checklist**：任何依赖 backend state 的 UI 操作必须是 "wait for ACK then apply"
- [ ] 乐观更新必须配套回滚逻辑
- [ ] 命令系统设计时就要考虑同步/异步两种返回类型

---

<!--
实施时在此处追加真实条目：
L-01 PermissionGate 超时 vs resolve race
L-02 on_iteration_complete 乱序写
L-03 Drop guard async spawn 失败
L-04 cancel_token 不中断 HTTP 请求
L-05 SSE multi-byte UTF-8 跨 chunk
L-06 permissionMode 前端 Zustand vs 磁盘
L-07 isStreaming vs turn_state 双源头
L-08 两个独立的 CWD process lock
L-09 MCP init 只 discover 不 register
L-10 fork_session 用 default() 丢失状态
L-11 /compact 乐观 UI 无回滚
L-12 hooks 系统 config 源未接入
L-13 tool_use input 非 object 类型
L-14 truncate 字节切片 UTF-8 panic（已修于 P0-2）
L-15 CLAUDE.md 路径注入未警告
-->

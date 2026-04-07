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

## 漏洞档案

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

### L-09: MCP init 只 discover 不 register (欺骗性完成)

- **严重度**: Critical
- **类别**: 欺骗性完成
- **发现日期**: 2026-04-07
- **修复 commit**: pending (Phase 3.1 — honest downgrade, not fixed)
- **涉及文件**: `rust/crates/desktop-core/src/agentic_loop.rs:977-1022`

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

# Audit Fix Plan (v3) — 修复 15 个未修漏洞

## Status: Awaiting Execution
**Last updated**: 2026-04-07
**Commit baseline**: `314c1c2` (Phase 1-18 + P0/P1 表面修复)
**Trigger**: 破坏性审计发现 15 个遗留问题

---

## 审计发现总结

| 分类 | 数量 | 代表性问题 |
|------|------|-----------|
| 高危逻辑漏洞 | 4 | PermissionGate race、Drop guard shutdown 无效、持久化乱序、MCP 空壳 |
| 崩溃场景 | 3 | SSE UTF-8 跨 chunk、fork_session 丢状态、build_api_request null 输入 |
| 语义不一致 | 3 | permissionMode 双源头、isStreaming 双源头、/compact 假成功 |
| 静默失败 | 5 | 删会话竞态、CLAUDE.md 注入、Client 不复用、MCP 线程 panic、CWD 锁不共享 |

---

## 依赖图

```
Phase 0: Lessons Learned 框架
   ↓
┌──────────┬──────────┬──────────┐
│Phase 1   │Phase 2   │Phase 3   │
│独立修复  │权限统一  │MCP披露   │
│(并行)    │          │          │
└──────────┴──────────┴──────────┘
   ↓
┌──────────┬──────────┐
│Phase 4   │Phase 5   │
│Crash恢复 │isStream  │
│          │统一      │
└──────────┴──────────┘
   ↓
Phase 6: 持久化 + CWD锁 + Client复用
   ↓
Phase 7: 清理 (fork, MCP panic, CLAUDE.md警告)
```

---

## Phase 0: Lessons Learned 框架

**Task 0.1** — 创建 `docs/audit-lessons.md` 骨架，为后续 15 个条目预留格式

**验收**：文件存在、结构清晰、至少 1 个示例条目

---

## Phase 1: 独立 bug 修复

### Task 1.1: SSE UTF-8 边界保护
**文件**: `rust/crates/desktop-core/src/agentic_loop.rs:687-730`
**修复**: `buffer: String` → `buffer: Vec<u8>`，用 `drain(..=newline_pos)` 消除 O(n²)
**测试**: 拆分中文字符 "中" 跨 2 个 chunk，验证 decode 正确

### Task 1.2: build_api_request 严格类型验证
**文件**: `rust/crates/desktop-core/src/agentic_loop.rs:562-573`
**修复**: `from_str(input).ok().filter(|v| v.is_object()).unwrap_or_else(...)`
**测试**: null/array/object 三种输入 coerce 到 object

### Task 1.3: /compact 命令等待 backend ACK
**文件**: `apps/desktop-shell/src/features/session-workbench/commandExecutor.ts:56-76`
**修复**: 改为 async，先 await backend 再 clear UI
**测试**: Running 状态下不清空 UI + 显示错误

**Checkpoint 1**: cargo test + tsc 零 regression

---

## Phase 2: 权限系统真相源统一

### Task 2.1: PermissionGate race 修复
**文件**: `rust/crates/desktop-core/src/agentic_loop.rs:132-178`
**修复**: 成功路径不二次清理 HashMap（resolve 已移除），只在 timeout 路径清理
**测试**:
- `resolve_wins_race_against_timeout` - 95ms resolve vs 100ms timeout → Allow
- `timeout_wins_race_when_no_resolve` - 200ms 后 Deny(timeout)，pending 为空

### Task 2.2: permissionMode 单一真相源
**后端**: 新增 `POST /api/desktop/settings/permission-mode` + `DesktopState::set_permission_mode` 写入 `.claude/settings.json`
**前端**: `settings-store.setPermissionMode` 改 async，先 API 后 setState；启动从 customize 接口 hydrate
**验收**: UI 切换 mode → 发消息 → backend 日志显示新 mode

**Checkpoint 2**: 手测权限对话框 + permissionMode 同步

---

## Phase 3: MCP 诚实披露

### Task 3.1: 移除假 init + 降级 Phase 16
**文件**: `rust/crates/desktop-core/src/agentic_loop.rs:935-1022`
**修复**:
1. 重命名 `init_mcp_servers` → `probe_mcp_servers`（只检查连通性）
2. 顶部警告注释说明 `tools::global_mcp_registry()` 是 crate-private
3. System prompt 不包含 MCP 工具描述
4. `tasks/todo.md` Phase 16 从 ✅ 降级为 ⚠️

**验收**: Phase 16 标记降级，lessons L-09 记录

---

## Phase 4: Shutdown/Crash 恢复

### Task 4.1: 启动时 reconcile stuck sessions
**文件**: `rust/crates/desktop-core/src/lib.rs:1196-1199`
**修复**: 加载后遍历 sessions，`turn_state == Running` → 重置为 `Idle` + log
**测试**: 持久化文件含 Running session → load → 全变 Idle

### Task 4.2: Drop guard 同步化
**文件**: `rust/crates/desktop-core/src/lib.rs:2592-2640`
**修复**: `tokio::spawn` → `try_write` 同步尝试。失败时依赖 Task 4.1 启动 reconcile
**验收**: kill -9 backend → 重启 → Running 全部变 Idle

---

## Phase 5: isStreaming 单一真相源

### Task 5.1: 拆分概念 + 单向数据流
**洞察**: `session.turn_state` = "后端在处理" vs `streamingContent` = "正在积累文本片段"——两个正交维度

**文件**:
- `state/streaming-store.ts` — 删除 isStreaming/setStreaming
- `SessionWorkbenchPage.tsx` — 只 appendStreamingContent
- `SessionWorkbenchTerminal.tsx` — StreamingIndicator 根据 `turn_state + streamingContent` 显示
- onMessage → `clearStreamingContent`

**验收**: Streaming 不闪烁，Cancel 立即消失

**Checkpoint 3**: 手测 crash recovery + streaming 无闪烁

---

## Phase 6: 持久化乱序 + CWD 锁 + Client 复用

### Task 6.1: 持久化通道化
**文件**: `rust/crates/desktop-core/src/lib.rs`
**修复**: `DesktopState` 新增 `persist_tx: mpsc::UnboundedSender<PersistJob>` + 长期消费任务
**测试**: 10 个乱序 spawn 的 job 后，最终磁盘状态 = 最后一个 job

### Task 6.2: 统一 CWD workspace lock
**文件**:
- `rust/crates/desktop-core/src/lib.rs:3791-3794` (公开为 `pub(crate)`)
- `rust/crates/desktop-core/src/agentic_loop.rs:1049-1073` (改用 `crate::process_workspace_lock()`)
**修复**: 合并两个独立的 `OnceLock<StdMutex<()>>`

### Task 6.3: reqwest Client 复用
**文件**: `rust/crates/desktop-core/src/agentic_loop.rs:276`
**修复**: `DesktopState` 新增 `http_client: reqwest::Client`，`with_executor` 构造一次
**验收**: netstat 观察连续 10 条消息只建 1 个 TCP 连接

---

## Phase 7: 清理收尾

### Task 7.1: fork_session 保留完整状态
**文件**: `rust/crates/desktop-core/src/lib.rs:2395-2410`
**修复**: `parent_session.clone()` + `truncate`，而非 `RuntimeSession::default()`
**测试**: fork 后 compaction/usage 字段保留

### Task 7.2: MCP init 线程错误传播
**文件**: `rust/crates/desktop-core/src/agentic_loop.rs:1008-1041`
**修复**: 保留 JoinHandle + log + `catch_unwind`
**验收**: 注入 panic config → stderr 有清晰错误

### Task 7.3: CLAUDE.md 注入警告
**文件**: `rust/crates/desktop-core/src/system_prompt.rs`
**修复**: 记录来源路径；parent directory 来源添加警告块
**验收**: `~/.claude/CLAUDE.md` 被加载时显示来源警告

**Checkpoint 4**: 全量 cargo test + 手测回归 + lessons-learned 更新

---

## 验证流程

```bash
# 后端
cd rust && cargo test -p desktop-core
cd rust && cargo check -p desktop-server

# 前端
cd apps/desktop-shell && npx tsc --noEmit

# 手测
# Phase 2: 切换权限 → 发消息 → 确认生效
# Phase 4: kill -9 + 重启 → 确认 turn_state 恢复
# Phase 1.1: 发送中文 → 前端无乱码
```

---

## 关键文件修改清单

| 文件 | Tasks |
|------|-------|
| `rust/crates/desktop-core/src/agentic_loop.rs` | 1.1, 1.2, 2.1, 3.1, 6.2, 6.3, 7.2 |
| `rust/crates/desktop-core/src/lib.rs` | 2.2, 4.1, 4.2, 6.1, 6.2, 6.3, 7.1 |
| `rust/crates/desktop-core/src/system_prompt.rs` | 7.3 |
| `rust/crates/desktop-server/src/lib.rs` | 2.2（新路由） |
| `apps/desktop-shell/src/state/settings-store.ts` | 2.2 |
| `apps/desktop-shell/src/state/streaming-store.ts` | 5.1 |
| `apps/desktop-shell/src/features/session-workbench/SessionWorkbenchPage.tsx` | 5.1 |
| `apps/desktop-shell/src/features/session-workbench/SessionWorkbenchTerminal.tsx` | 5.1, 7.3 |
| `apps/desktop-shell/src/features/session-workbench/commandExecutor.ts` | 1.3 |
| `apps/desktop-shell/src/features/session-workbench/api/client.ts` | 2.2（新 API） |
| `docs/audit-lessons.md` | 每个 task 后追加 |
| `tasks/todo.md` | 3.1（Phase 16 降级） |

# Agent Runtime Implementation Plan (v2)

## Status: In Progress
**Last updated**: 2026-04-06
**Commit baseline**: `06e71b7` (open-claude-code(8) — post Zustand migration)

---

## Architecture Change: Redux → Zustand

(8) 版本完成了完整的 Redux → Zustand 迁移：
- 5 个 Zustand store: `permissions-store`, `settings-store`, `code-tools-store`, `tabs-store`, `minapps-store`
- `subscribeToSessionEvents` 移至 `features/session-workbench/api/client.ts`
- Session 列表完全由 React Query 管理（无 Redux 双源头问题）
- `store/slices/` 目录已删除

**影响评估：**
- ✅ Phase 1-5 (Rust 后端) — 完好无损，cargo check 通过
- ❌ Phase 6 前端改动 — 被 Zustand 迁移覆盖，需重新接入
- ✅ Session 双源头问题 — 已被 Zustand 迁移自然解决

---

## Completed Phases

### Phase 1-5: Rust Agent Runtime ✅
- `agentic_loop.rs`: 异步 agentic loop + PermissionGate + 流式 SSE
- `system_prompt.rs`: CLAUDE.md 查找 + 工具描述 system prompt
- `lib.rs`: agentic loop 接入 session flow + cancel + finalize
- `Cargo.toml`: tokio-util, futures-util, reqwest stream 依赖

---

## Remaining Phases

### Phase 7: SSE 事件接入 Zustand

**Goal**: 将后端的 `PermissionRequest` 和 `TextDelta` SSE 事件接入 Zustand 前端。

#### Task 7.1: 扩展 SSE 客户端支持新事件类型
**File**: `apps/desktop-shell/src/features/session-workbench/api/client.ts`
- 在 `subscribeToSessionEvents` 中添加 `permission_request` 和 `text_delta` 事件监听
- 添加 `onTextDelta` 和 `onPermissionRequest` 到 handlers 接口
- **不改动**已有的 `onSnapshot`、`onMessage`、`onError` 逻辑
- **验收**: tsc --noEmit 通过；SSE 事件类型齐全

#### Task 7.2: 添加 streaming state 到 Zustand
**方案A**: 在 `permissions-store.ts` 中添加 `streamingContent` 字段
**方案B**: 新建 `streaming-store.ts`（更干净）
- 字段: `isStreaming: boolean`, `streamingContent: string`
- Actions: `appendStreamingContent`, `setStreaming`, `clearStreamingContent`
- **不改动**已有的 permissions store 结构
- **验收**: Zustand store 正确持有 streaming 状态

#### Task 7.3: 在 SessionWorkbenchPage 中连接 SSE handlers
**File**: `apps/desktop-shell/src/features/session-workbench/SessionWorkbenchPage.tsx`
- `onTextDelta` → 调用 streaming store 的 `appendStreamingContent`
- `onPermissionRequest` → 调用 permissions store 的 `setPendingPermission`
- `onSnapshot` → 根据 `turn_state` 更新 `isStreaming`
- **不改动**已有的 session/query 逻辑
- **验收**: 权限对话框在 SSE 事件到达时弹出；streaming 文本实时更新

#### Checkpoint 7:
```bash
npx tsc --noEmit  # 零错误
# 手动测试: 发消息 → TextDelta 到达 → streaming 状态更新
```

---

### Phase 8: InputBar stub 命令清理

**Goal**: 移除 InputBar 中无 executor 实现的命令，添加有实现但未列出的命令。

#### Task 8.1: 对齐 InputBar 和 commandExecutor
**File**: `apps/desktop-shell/src/features/session-workbench/InputBar.tsx`
- 移除: `doctor`, `login`, `logout`, `memory`, `terminal-setup`, `vim`（无 executor）
- 添加: `commit`, `diff`, `session`, `theme`（有 executor 但未列出）
- **不改动** commandExecutor.ts 本身
- **验收**: InputBar 列表与 executor 1:1 对应

#### Checkpoint 8:
```bash
npx tsc --noEmit  # 零错误
```

---

### Phase 9: 端到端测试 + 健壮性

#### Task 9.1: 增量持久化
**File**: `rust/crates/desktop-core/src/agentic_loop.rs`
- 每轮 loop 迭代后持久化 session 状态
- 防止崩溃丢失中间结果

#### Task 9.2: 错误恢复
**File**: `rust/crates/desktop-core/src/agentic_loop.rs`
- API 错误 → 创建 error message, 广播, 设 Idle
- 工具 panic → 捕获, 创建 error tool_result, 继续循环
- 超过 50 轮 → 创建 system message 说明限制

#### Task 9.3: 清理 gates/tokens
**File**: `rust/crates/desktop-core/src/lib.rs`
- `finalize_agentic_turn` 中清理 permission_gate 和 cancel_token（已实现）
- 验证无内存泄漏

#### Checkpoint 9:
```bash
cargo test -p desktop-core   # 全部通过
cargo test -p desktop-server  # 全部通过
```

---

## Future Phases (Not Scoped)

| Phase | Description |
|-------|-------------|
| 10 | MCP Client (连接 MCP servers, 暴露工具) |
| 11 | Sub-agent spawning (Agent tool 实现) |
| 12 | Plan Mode (EnterPlanMode/ExitPlanMode) |
| 13 | Session compaction/fork |
| 14 | Hooks system (PreToolUse/PostToolUse) |
| 15 | Windows terminal support |

---

## 关键原则

1. **不影响 (8) 已有代码** — 只做增量添加
2. **Zustand store 只增不改** — 新建 streaming-store 而非修改现有 store
3. **SSE 客户端扩展** — 在 handlers 接口上增加字段，不改动已有字段
4. **后端代码不动** — Phase 1-5 已完成且编译通过

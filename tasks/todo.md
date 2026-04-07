# Audit Fix Checklist (v3)

**Status**: Awaiting execution
**Baseline**: `314c1c2`
**Source**: 破坏性审计报告 (15 问题)

---

## Phase 0: 文档框架
- [ ] **0.1** 创建 `docs/audit-lessons.md` 骨架

## Phase 1: 独立 bug 修复
- [ ] **1.1** SSE UTF-8 边界保护（buffer: Vec<u8> + drain）
- [ ] **1.2** build_api_request 严格类型验证（is_object filter）
- [ ] **1.3** /compact 命令等待 backend ACK（await before clear）
- [ ] **Checkpoint 1**: cargo test + tsc 零 regression

## Phase 2: 权限系统真相源统一
- [ ] **2.1** PermissionGate race 修复（timeout 路径单独清理）
- [ ] **2.2** permissionMode 单一真相源（backend API + 前端 hydrate）
- [ ] **Checkpoint 2**: 手测权限对话框 + permissionMode 同步

## Phase 3: MCP 诚实披露
- [ ] **3.1** 移除假 init + 降级 Phase 16 标记
- [ ] lessons-learned L-09 填充

## Phase 4: Shutdown/Crash 恢复
- [ ] **4.1** 启动时 reconcile stuck sessions
- [ ] **4.2** Drop guard 同步化（try_write）

## Phase 5: isStreaming 统一真相源
- [ ] **5.1** 拆分概念 + 删除冗余 isStreaming 字段
- [ ] **Checkpoint 3**: 手测 crash recovery + streaming 无闪烁

## Phase 6: 持久化 + CWD + Client
- [ ] **6.1** 持久化通道化（mpsc + FIFO）
- [ ] **6.2** 统一 CWD workspace lock
- [ ] **6.3** reqwest Client 全局复用

## Phase 7: 清理收尾
- [ ] **7.1** fork_session 保留完整状态（clone + truncate）
- [ ] **7.2** MCP init 线程错误传播（catch_unwind）
- [ ] **7.3** CLAUDE.md 注入警告
- [ ] **Checkpoint 4**: 全量 cargo test + lessons-learned 更新

---

## Lessons Learned 预填充条目（15 条）

### 时序/竞态类 (5)
- [x] L-01 PermissionGate 超时 vs resolve race (Phase 2.1)
- [x] L-02 on_iteration_complete 乱序写 (Phase 6.1)
- [x] L-03 Drop guard async spawn 失败 (Phase 4.2)
- [ ] L-04 cancel_token 不中断 HTTP 请求 (deferred)
- [x] L-05 SSE multi-byte UTF-8 跨 chunk (Phase 1.1)

### 真相源分裂 (3)
- [x] L-06 permissionMode 前端 Zustand vs 磁盘 (Phase 2.2)
- [x] L-07 isStreaming vs turn_state 双源头 (Phase 5.1)
- [x] L-08 两个独立的 CWD process lock (Phase 6.2)

### 欺骗性完成 (4)
- [x] L-09 MCP init 只 discover 不 register (Phase 3.1 — marked honest, not fixed)
- [x] L-10 fork_session 用 default() 丢失状态 (Phase 7.1)
- [x] L-11 /compact 乐观 UI 无回滚 (Phase 1.3)
- [ ] L-12 hooks 系统 config 源未接入 (deferred)

### 边界条件 (3)
- [x] L-13 tool_use input 非 object 类型 (Phase 1.2)
- [x] L-14 truncate 字节切片 UTF-8 panic (P0-2)
- [x] L-15 CLAUDE.md 路径注入未警告 (Phase 7.3)
- [ ] L-14 truncate 字节切片 UTF-8 panic（已修）
- [ ] L-15 CLAUDE.md 路径注入未警告

---

## 执行原则
1. **TDD 优先** — 每个 critical bug 先写失败测试
2. **不连续实施** — 每个 checkpoint 停下来手测
3. **诚实降级** — 不能修的标记为 ⚠️，不造假完成
4. **即时记录** — 修完一个就写一个 lessons-learned 条目

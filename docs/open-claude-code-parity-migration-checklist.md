# open-claude-code -> claw-code-parity 实施清单

> 实施状态（2026-04-04）：Phase 1 已完成，workspace 已收缩到 `desktop-core`、`desktop-server`、`server`。当前仓库不再使用 sibling `path dependency`，而是固定依赖 `https://github.com/wangedoo518/claw-code-parity.git@736069f1ab45a4e90703130188732b7e5ac13620`；旧的 vendored `api/runtime/tools/commands/plugins/compat-harness/claw-cli/lsp` 目录已从本仓库移除。

本文是 [open-claude-code-parity-dependency-design.md](./open-claude-code-parity-dependency-design.md) 的落地版，目标是把评审结论压成可以直接执行的改造清单。

## 1. 改造目标

把 `open-claude-code` 当前 vendored 的 Rust 核心移出本仓库，只保留产品扩展层：

- 保留：
  - `rust/crates/desktop-core`
  - `rust/crates/desktop-server`
  - `rust/crates/server`
- 依赖 `claw-code-parity`：
  - `api`
  - `runtime`
  - `tools`
  - `plugins`
- 暂不纳入第一阶段：
  - `claw-cli`
  - `commands`
  - `compat-harness`
  - `telemetry`
  - `mock-anthropic-service`

---

## 2. 第一阶段交付物

第一阶段的完成标准建议定为：

1. `open-claude-code/rust` 只包含本地扩展 crate。
2. `desktop-core`、`server` 已切换到依赖 parity crates。
3. `desktop-server` 仍可被 `apps/desktop-shell/src-tauri/src/main.rs` 正常拉起。
4. `cargo check --workspace` 在 `open-claude-code/rust` 通过。
5. 不再在 `open-claude-code` 中维护 vendored `api/runtime/tools/plugins`。

---

## 3. 文件级改造清单

## 3.1 workspace 根配置

### 文件

- [rust/Cargo.toml](/Users/champion/Documents/develop/Warwolf/open-claude-code/rust/Cargo.toml)

### 现状

当前是：

```toml
[workspace]
members = ["crates/*"]
```

这会把所有 vendored core crate 一起纳入 workspace。

### 建议改造

改为显式列出本地扩展 crate，并在 `[workspace.dependencies]` 中声明 parity 依赖：

```toml
[workspace]
members = [
  "crates/desktop-core",
  "crates/desktop-server",
  "crates/server",
]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"
publish = false

[workspace.dependencies]
api = { git = "https://github.com/wangedoo518/claw-code-parity.git", rev = "736069f1ab45a4e90703130188732b7e5ac13620" }
runtime = { git = "https://github.com/wangedoo518/claw-code-parity.git", rev = "736069f1ab45a4e90703130188732b7e5ac13620" }
tools = { git = "https://github.com/wangedoo518/claw-code-parity.git", rev = "736069f1ab45a4e90703130188732b7e5ac13620" }
plugins = { git = "https://github.com/wangedoo518/claw-code-parity.git", rev = "736069f1ab45a4e90703130188732b7e5ac13620" }
serde_json = "1"
```

### 评审点

- Phase 1 bring-up 可接受 sibling path dependency
- 当前收口版本建议统一使用固定 git revision，避免本地目录结构耦合

---

## 3.2 `desktop-core` 依赖切换

### 文件

- [rust/crates/desktop-core/Cargo.toml](/Users/champion/Documents/develop/Warwolf/open-claude-code/rust/crates/desktop-core/Cargo.toml)

### 现状

当前直接依赖本仓库 vendored crate：

- `api = { path = "../api" }`
- `plugins = { path = "../plugins" }`
- `runtime = { path = "../runtime" }`
- `tools = { path = "../tools" }`

### 建议改造

改成 workspace 依赖：

```toml
[dependencies]
api.workspace = true
plugins.workspace = true
runtime.workspace = true
tools.workspace = true
```

### 影响

- `desktop-core` 将成为 parity 核心的真正下游
- `desktop-core` 是本次迁移的核心适配点

---

## 3.3 `server` 依赖切换

### 文件

- [rust/crates/server/Cargo.toml](/Users/champion/Documents/develop/Warwolf/open-claude-code/rust/crates/server/Cargo.toml)

### 建议改造

当前：

```toml
runtime = { path = "../runtime" }
```

改成：

```toml
runtime.workspace = true
```

### 说明

`server` 依赖面很小，只消费 `runtime::Session` / `ConversationMessage` 之类公开接口，迁移难度低。

---

## 3.4 `desktop-server` 维持本地边界

### 文件

- [rust/crates/desktop-server/Cargo.toml](/Users/champion/Documents/develop/Warwolf/open-claude-code/rust/crates/desktop-server/Cargo.toml)

### 建议

`desktop-server` 继续只依赖本地 `desktop-core`，不直接依赖 parity。

这是正确的产品分层：

- parity：核心运行时
- desktop-core：产品 domain 层
- desktop-server：对桌面前端暴露 API

---

## 4. 最小兼容补丁点

## 4.1 `desktop-core` 的 provider kind 命名适配

### 文件

- [rust/crates/desktop-core/src/lib.rs](/Users/champion/Documents/develop/Warwolf/open-claude-code/rust/crates/desktop-core/src/lib.rs)

### 当前代码点

在 `default_auth_source()` 中：

```rust
if detect_provider_kind(model) != ProviderKind::ClawApi {
    return Ok(None);
}
```

### parity 目标

应改成：

```rust
if detect_provider_kind(model) != ProviderKind::Anthropic {
    return Ok(None);
}
```

### 原因

parity 已从 `ClawApi` 语义切到 `Anthropic` 语义。

---

## 4.2 `desktop-core` 的 ProviderClient 构造函数适配

### 文件

- [rust/crates/desktop-core/src/lib.rs](/Users/champion/Documents/develop/Warwolf/open-claude-code/rust/crates/desktop-core/src/lib.rs)

### 当前代码点

在 `DesktopRuntimeClient::new()` 中：

```rust
client: ProviderClient::from_model_with_default_auth(&model, default_auth)
```

### parity 目标

改成：

```rust
client: ProviderClient::from_model_with_anthropic_auth(&model, default_auth)
```

### 原因

parity 的 `api/src/client.rs` 已使用新的 API 命名。

---

## 4.3 `desktop-core` 顶部 import 的同步调整

### 文件

- [rust/crates/desktop-core/src/lib.rs](/Users/champion/Documents/develop/Warwolf/open-claude-code/rust/crates/desktop-core/src/lib.rs)

### 当前 import 风格

顶部从 `api` 导入的内容仍带旧语义：

- `read_base_url as read_claw_base_url`
- `AuthSource`
- `ProviderKind`
- `resolve_startup_auth_source`

### 建议

这里不一定要大改结构，但建议做两件事：

1. 保留 `read_base_url as read_claw_base_url` 这种 alias，避免下游产品文案一次性大面积变化。
2. 只修正真正编译会断的命名：
   - `ProviderKind::ClawApi` -> `ProviderKind::Anthropic`
   - `ProviderClient::from_model_with_default_auth` -> `from_model_with_anthropic_auth`

### 结论

第一阶段不追求把 desktop 层全部命名重写成 Anthropic 风格，先保证依赖切换成功。

---

## 4.4 `claw-cli` 不纳入第一阶段

### 文件

- 历史本地 crate：`rust/crates/claw-cli/Cargo.toml`（该目录现已删除）
- 对应上游 crate：`claw-code-parity/rust/crates/rusty-claude-cli/Cargo.toml`

### 建议

第一阶段直接把它从 `open-claude-code/rust` workspace 成员里移除，不做 parity 对接。

### 原因

- `claw-cli` 与 parity 的 `rusty-claude-cli` 差异大
- desktop 壳层不依赖它
- 把 CLI 也绑进第一阶段，会显著扩大改造范围

### 后续选择

后续单独决策：

1. 完全不在 `open-claude-code` 保留 CLI
2. CLI 只作为 parity 的产物使用
3. 如果产品需要 Warwolf 包装 CLI，再单独做 wrapper crate

---

## 5. 目录与 crate 处理顺序

## Step 1

先改 workspace 根：

- [rust/Cargo.toml](/Users/champion/Documents/develop/Warwolf/open-claude-code/rust/Cargo.toml)

目标：

- 只保留 `desktop-core` / `desktop-server` / `server`
- 加入 parity path dependencies

## Step 2

改本地 crate 的依赖声明：

- [rust/crates/desktop-core/Cargo.toml](/Users/champion/Documents/develop/Warwolf/open-claude-code/rust/crates/desktop-core/Cargo.toml)
- [rust/crates/server/Cargo.toml](/Users/champion/Documents/develop/Warwolf/open-claude-code/rust/crates/server/Cargo.toml)

## Step 3

修 `desktop-core` 的最小兼容代码：

- [rust/crates/desktop-core/src/lib.rs](/Users/champion/Documents/develop/Warwolf/open-claude-code/rust/crates/desktop-core/src/lib.rs)

只改：

- `ProviderKind::ClawApi`
- `from_model_with_default_auth`

## Step 4

跑编译：

```bash
cd /Users/champion/Documents/develop/Warwolf/open-claude-code/rust
cargo check --workspace
```

## Step 5

若通过，再处理文件清理：

- 从 workspace 中移除但不立即物理删除 vendored core crate
- 先保留目录，避免一次性大删影响回退

建议先“退出 workspace”，后“物理删除目录”。

---

## 6. 建议的提交拆分

为了便于 review，建议拆成 4 个提交：

### Commit 1

`rust workspace: shrink members and add parity path dependencies`

内容：

- 改 [rust/Cargo.toml](/Users/champion/Documents/develop/Warwolf/open-claude-code/rust/Cargo.toml)

### Commit 2

`desktop-core/server: switch cargo deps to parity workspace deps`

内容：

- 改 [rust/crates/desktop-core/Cargo.toml](/Users/champion/Documents/develop/Warwolf/open-claude-code/rust/crates/desktop-core/Cargo.toml)
- 改 [rust/crates/server/Cargo.toml](/Users/champion/Documents/develop/Warwolf/open-claude-code/rust/crates/server/Cargo.toml)

### Commit 3

`desktop-core: adapt to parity api naming`

内容：

- 改 [rust/crates/desktop-core/src/lib.rs](/Users/champion/Documents/develop/Warwolf/open-claude-code/rust/crates/desktop-core/src/lib.rs)

### Commit 4

`docs: document parity dependency model for open-claude-code`

内容：

- 更新 README / CLAW / docs

---

## 7. 验证清单

## 编译验证

1. `open-claude-code/rust` 执行：

```bash
cargo check --workspace
```

2. `apps/desktop-shell/src-tauri` 仍能通过本地开发流程拉起 `desktop-server`

### 关键验证点

- [apps/desktop-shell/src-tauri/src/main.rs](/Users/champion/Documents/develop/Warwolf/open-claude-code/apps/desktop-shell/src-tauri/src/main.rs)

它当前仍假设：

- workspace 位于 `../../../rust`
- debug 时可执行 `cargo run -p desktop-server`

所以只要 `desktop-server` 继续留在 open repo 本地 workspace，桌面壳层就不会因 parity 迁移而失效。

## 运行验证

建议至少手测：

1. Session 列表获取
2. 新建 session
3. 发送一轮消息
4. provider runtime 状态读取
5. provider hub 相关页面是否还能加载

---

## 8. 暂不做的事情

为了控制第一阶段风险，建议明确暂不处理：

1. 不把 `claw-cli` 对齐到 parity `rusty-claude-cli`
2. 不把 `commands` / `compat-harness` 继续留在 open repo
3. 不在第一阶段引入新的 adapter 大层
4. 不做大规模命名重写
5. 不同步把 sibling path dependency 改成 git dependency

---

## 9. 第二阶段待办

当前已完成：

1. sibling path dependency -> git pinned dependency
2. 清理 vendored core 目录

后续可继续评估：

1. 决定 `claw-cli` 的最终归属
2. 如果 parity 接口变化频繁，再评估是否引入薄 `parity-adapter`

---

## 10. 建议的评审结论模板

如果团队认可，可以直接采用下面这段作为评审结论：

> 评审同意将 `claw-code-parity` 作为 Rust 核心唯一上游，`open-claude-code` 只保留产品扩展层。当前 `open-claude-code/rust` 仅保留 `desktop-core`、`desktop-server`、`server` 三个本地 crate，并通过固定 git revision 依赖 parity 的 `api/runtime/tools/plugins`。兼容改动主要集中在 `desktop-core` / `server` 的最小 API 与序列化边界适配；原 vendored core 已从仓库移除，本地 CLI fork 不纳入本阶段。

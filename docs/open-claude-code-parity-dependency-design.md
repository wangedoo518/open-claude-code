# open-claude-code 依赖 claw-code-parity 的拆分设计方案

> 实施状态（2026-04-04）：第一阶段收缩已经完成，`open-claude-code/rust` 只保留 `desktop-core`、`desktop-server`、`server` 三个本地 crate；当前实现已进一步从 sibling `path dependency` 收敛到固定 `git rev` 依赖，pin 到 `https://github.com/wangedoo518/claw-code-parity.git@736069f1ab45a4e90703130188732b7e5ac13620`。旧的 vendored `api/runtime/tools/commands/plugins/compat-harness/claw-cli/lsp` 目录已从本仓库移除。

## 1. 背景与目标

当前 `open-claude-code` 仓库下的 `rust/` 不是“依赖” `claw-code-parity`，而是**直接内嵌了一份已经分叉的 parity Rust workspace**，再在其上叠加 Warwolf/OpenClaw 的 desktop、provider hub、desktop server 等能力。

本方案的目标是把现状改造成：

1. `claw-code-parity` 作为 **Rust CLI/runtime 核心仓库**。
2. `open-claude-code` 只保留 **Warwolf/OpenClaw 的产品扩展层**。
3. `open-claude-code` 通过依赖 `claw-code-parity` 的 crate 来构建，而不再长期维护一份 vendored copy。
4. 为后续团队协作建立清晰边界：**核心能力 upstream 到 parity，产品能力 downstream 到 open-claude-code**。

---

## 2. 现状分析

### 2.1 两个 Rust workspace 的职责已经天然分层

`claw-code-parity/rust/crates` 当前包含：

- `api`
- `runtime`
- `tools`
- `commands`
- `plugins`
- `compat-harness`
- `telemetry`
- `mock-anthropic-service`
- `rusty-claude-cli`

`open-claude-code/rust/crates` 当前包含：

- `api`
- `runtime`
- `tools`
- `commands`
- `plugins`
- `compat-harness`
- `claw-cli`
- `lsp`
- `server`
- `desktop-core`
- `desktop-server`

可以看出，`open-claude-code` 中有两类 crate：

1. **与 parity 同名的核心 crate**
   - `api`
   - `runtime`
   - `tools`
   - `commands`
   - `plugins`
   - `compat-harness`
   - `claw-cli`（对应 parity 的 `rusty-claude-cli`）

2. **Warwolf 自己的扩展 crate**
   - `desktop-core`
   - `desktop-server`
   - `server`
   - `lsp`

这说明 repo 边界本身已经很清楚，只是目前实现方式还是“复制一份核心进去”。

### 2.2 当前不是轻量分叉，而是明显漂移

按关键 crate 统计，`open-claude-code` 与 `claw-code-parity` 的差异非常大：

- `runtime`: 38 个文件变更，约 `+14888/-584`
- `tools`: 1 个核心文件变更，约 `+2503/-146`
- `commands`: 1 个核心文件变更，约 `+2692/-1102`
- `claw-cli` 对 `rusty-claude-cli`: 11 个文件变更，约 `+5097/-1938`
- `api`: 11 个文件变更，约 `+1604/-161`
- `plugins`: 4 个文件变更，约 `+607/-85`

关键结论：

- `open-claude-code` 里的 Rust 核心不是“几处 patch”，而是**一份已经与 parity 明显分叉的 fork**。
- 因此不能直接把 path 改掉就结束，必须先做**职责收缩**。

### 2.3 parity 已经是更完整的核心实现

从 crate 表面看，`claw-code-parity` 的核心能力覆盖更完整：

- 额外包含 `telemetry`
- 额外包含 `mock-anthropic-service`
- `runtime` 暴露更多 hardened/worker/policy/plugin lifecycle 能力
- `rusty-claude-cli` 有独立测试与 mock parity harness

而 `open-claude-code` 的 desktop 侧并不依赖全部核心 crate：

- `desktop-core` 仅依赖 `api`、`runtime`、`tools`、`plugins`
- `desktop-server` 仅依赖 `desktop-core`
- `server` 仅依赖 `runtime`

这意味着：

- 对 `open-claude-code` 而言，真正需要外部依赖的核心面，其实只有：
  - `api`
  - `runtime`
  - `tools`
  - `plugins`
- `commands` / `compat-harness` / `rusty-claude-cli` 主要是 parity 自己的 CLI 面，不是 desktop 必需依赖。

### 2.4 当前扩展层与核心层的主要耦合点

`desktop-core` 当前直接使用这些核心接口：

- `api`
  - `detect_provider_kind`
  - `resolve_model_alias`
  - `resolve_startup_auth_source`
  - `ProviderClient`
  - `AuthSource`
  - 消息/流式类型
- `runtime`
  - `ConversationRuntime`
  - `PermissionPolicy`
  - `ConfigLoader`
  - `RuntimeConfig`
  - `Session`
- `tools`
  - `GlobalToolRegistry`
- `plugins`
  - `PluginManager`
  - `PluginManagerConfig`

`apps/desktop-shell/src-tauri/src/main.rs` 还会直接假定本仓库 `rust/` 下能启动 `desktop-server`：

- debug 下执行 `cargo run -p desktop-server`
- release 下从 `rust/target/{debug,release}/desktop-server` 找二进制

这意味着桌面壳层只需要本地 `desktop-server` 继续存在，不要求 parity 的 CLI binary 被放在 `open-claude-code` 仓库里。

---

## 3. 当前直接替换会遇到的兼容性问题

### 3.1 `api` 命名语义已经发生变化

`open-claude-code` 当前仍使用旧命名：

- `ClawApiClient`
- `ProviderKind::ClawApi`
- `ProviderClient::from_model_with_default_auth`

而 `claw-code-parity` 已经改为：

- `AnthropicClient`
- `ProviderKind::Anthropic`
- `ProviderClient::from_model_with_anthropic_auth`

这意味着 `desktop-core` 不能“零改动切换”，至少需要一层兼容改造。

### 3.2 `claw-cli` 与 parity 的 `rusty-claude-cli` 不适合作为第一阶段复用目标

虽然两者都产出 `claw` 二进制，但当前：

- package 名称不同：`claw-cli` vs `rusty-claude-cli`
- CLI 主文件差异很大
- `open-claude-code` 本仓库的 desktop 功能并不直接依赖 `claw-cli`

因此如果目标是“先完成 repo 解耦”，**不建议第一阶段继续在 `open-claude-code` 内保留本地 CLI fork**。

### 3.3 `lsp` crate 在 parity 已不存在同名本地 crate

`open-claude-code` 的 `runtime` 依赖本地 `lsp` crate，但 parity 的 runtime 已改为自己的 `lsp_client` 模块。

因此如果 `open-claude-code` 改为直接依赖 parity `runtime`，则：

- 本地 `lsp` crate 不应继续作为核心依赖链的一部分
- 最好与 vendored `runtime` 一起退出 `open-claude-code` 本地 workspace

---

## 4. 目标架构

### 4.1 目标 repo 职责分工

#### A. `claw-code-parity`

作为上游核心仓库，拥有：

- `api`
- `runtime`
- `tools`
- `plugins`
- `commands`
- `compat-harness`
- `telemetry`
- `mock-anthropic-service`
- `rusty-claude-cli`

职责：

- 核心 agent/runtime/tool/plugin/CLI 能力演进
- parity / harness / upstream-compatible 行为验证
- 核心 Rust 测试与兼容基线

#### B. `open-claude-code`

作为下游产品仓库，保留：

- TypeScript / Electron / Tauri / Desktop UI
- `desktop-core`
- `desktop-server`
- `server`（如仍需要）
- provider hub / OpenClaw / Codex / Warwolf 特有产品逻辑

职责：

- 产品组合层
- 工作台 / 控制台 / provider 配置 / 桌面服务
- 对 parity 核心的接入与二次封装

### 4.2 推荐目录边界

#### `open-claude-code/rust/crates` 最终保留

- `desktop-core`
- `desktop-server`
- `server`
- 可选：`parity-adapter`（如果决定做兼容层）

#### `open-claude-code/rust/crates` 最终移除

- `api`
- `runtime`
- `tools`
- `plugins`
- `commands`
- `compat-harness`
- `claw-cli`
- `lsp`

说明：

- `lsp` 之所以也建议移除，不是因为功能一定废弃，而是它现在属于“旧核心 runtime 的陪跑 crate”。
- 若后续确实有 Warwolf 特有 LSP 需求，应以产品扩展 crate 形式重新定义，而不是继续绑在旧 runtime fork 上。

### 4.3 目标依赖图

```text
claw-code-parity
  ├─ api
  ├─ runtime
  ├─ tools
  ├─ plugins
  ├─ commands
  ├─ compat-harness
  ├─ telemetry
  ├─ mock-anthropic-service
  └─ rusty-claude-cli

open-claude-code
  ├─ desktop-core ------> api/runtime/tools/plugins
  ├─ desktop-server ----> desktop-core
  ├─ server -----------> runtime
  └─ apps/desktop-shell -> launch local desktop-server
```

---

## 5. 推荐实现策略

## 5.1 总体策略：先“收口职责”，再“切换依赖”

不建议直接做“一步到位大替换”。推荐三阶段：

1. **Phase A：核心 ownership 切换**
   - 明确 parity 是唯一核心来源
   - open repo 停止继续演化 vendored core

2. **Phase B：open repo 只保留扩展 crate**
   - `desktop-core` / `desktop-server` / `server` 继续留在 open repo
   - 其余核心 crate 从 open repo workspace 中摘除

3. **Phase C：对接 parity 依赖**
   - `desktop-core` / `server` 改为依赖 parity crates
   - 通过少量适配修复 API 命名差异

---

## 5.2 推荐依赖模式

### 方案一：Sibling path dependency（推荐作为第一阶段）

适合你们当前本地目录布局：

- `/Users/champion/Documents/develop/Warwolf/open-claude-code`
- `/Users/champion/Documents/develop/Warwolf/claw-code-parity`

`open-claude-code/rust/Cargo.toml` 推荐改为只管理本地扩展 crate，并在 `[workspace.dependencies]` 中声明 parity 依赖：

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
api = { path = "../../claw-code-parity/rust/crates/api" }
runtime = { path = "../../claw-code-parity/rust/crates/runtime" }
tools = { path = "../../claw-code-parity/rust/crates/tools" }
plugins = { path = "../../claw-code-parity/rust/crates/plugins" }
serde_json = "1"
```

对应本地 crate 改成：

```toml
[dependencies]
api.workspace = true
runtime.workspace = true
tools.workspace = true
plugins.workspace = true
```

优点：

- 改造量最小
- 最符合当前“依赖本地 sibling repo”的诉求
- 方便两仓库并行联调

缺点：

- CI/构建机必须保证 sibling repo 同时存在
- repo 不再自包含
- 对目录结构有强耦合

### 方案二：Git pinned dependency（推荐作为第二阶段稳定化）

当第一阶段稳定后，建议把 canonical 依赖改为 git + rev：

```toml
[workspace.dependencies]
api = { git = "git@github.com:wangedoo518/claw-code-parity.git", package = "api", rev = "COMMIT" }
runtime = { git = "git@github.com:wangedoo518/claw-code-parity.git", package = "runtime", rev = "COMMIT" }
tools = { git = "git@github.com:wangedoo518/claw-code-parity.git", package = "tools", rev = "COMMIT" }
plugins = { git = "git@github.com:wangedoo518/claw-code-parity.git", package = "plugins", rev = "COMMIT" }
```

优点：

- CI 可复现
- 不依赖本地 sibling 目录
- 能明确 pin 上游版本

缺点：

- 本地联调上游修改不如 path dependency 直接

### 结论

建议路线：

- **评审通过后先上方案一**
- **稳定后收敛到方案二**

---

## 6. 兼容层设计建议

### 6.1 不建议继续在 open repo 维护一整套核心 fork

因为当前漂移已经足够大，如果再保留本地 fork：

- 未来 parity 每次升级都需要双向手工搬运
- `desktop-core` 对核心接口的依赖会越来越隐式
- 团队很难判断 bug 应该修在 parity 还是 open repo

### 6.2 推荐只保留“很薄”的兼容层

兼容层可以有两种做法：

#### 做法 A：直接修改 `desktop-core`

把 `desktop-core` 内旧命名直接替换为 parity 新接口：

- `ProviderKind::ClawApi` -> `ProviderKind::Anthropic`
- `ProviderClient::from_model_with_default_auth` -> `from_model_with_anthropic_auth`
- `ClawApiClient` 相关命名同步为 `AnthropicClient`（如果扩展层仍直接引用）

优点：

- 最终结构最干净
- 不增加额外中间层

缺点：

- 第一次改动会集中落到 `desktop-core`

#### 做法 B：新增一个本地 `parity-adapter` crate

例如：

- `open-claude-code/rust/crates/parity-adapter`

职责：

- re-export parity 的 `api/runtime/tools/plugins`
- 提供少量命名桥接函数
- 屏蔽 parity 后续少量 API 调整

适合场景：

- 如果评审认为 downstream 不能直接依赖 parity 的“原生接口命名”
- 或者你们预计 parity 接口还会继续快速变化

#### 我的建议

**优先做法 A，必要时局部加做法 B。**

理由：

- 当前 `desktop-core` 真正不兼容的点并不多
- 用一个过厚 adapter 容易重新长成第二套 fork

---

## 7. 分阶段迁移方案

## Phase 0：冻结边界与基线

目标：

- 确认 parity 为核心唯一来源
- 确认 open repo 只保留产品扩展

动作：

1. 在评审会上确认 crate ownership
2. 选定 parity commit 作为切换基线
3. 记录当前 open repo 必须保留的 crate：
   - `desktop-core`
   - `desktop-server`
   - `server`
4. 记录当前将退出 open repo 的 crate：
   - `api`
   - `runtime`
   - `tools`
   - `plugins`
   - `commands`
   - `compat-harness`
   - `claw-cli`
   - `lsp`

验收：

- 团队对 repo 边界无争议

## Phase 1：workspace 拆分

目标：

- open repo 的 `rust/` 只留下本地扩展 crate

动作：

1. 精简 `open-claude-code/rust/Cargo.toml`
2. 删除或迁出 vendored 核心 crate
3. `desktop-core`、`server` 改为引用 parity path dependency
4. `desktop-server` 保持只依赖 `desktop-core`

验收：

- `cargo check --workspace` 在 open repo 的新本地 workspace 下通过
- `apps/desktop-shell/src-tauri/src/main.rs` 仍可 `cargo run -p desktop-server`

## Phase 2：兼容修正

目标：

- 修掉 open 扩展层与 parity 核心之间的 API 名称差异

最小改动清单：

1. `desktop-core`
   - `ProviderKind::ClawApi` 改为 `ProviderKind::Anthropic`
   - `ProviderClient::from_model_with_default_auth` 改为 `from_model_with_anthropic_auth`

2. 如仍保留本地 CLI wrapper，再决定：
   - 是彻底移除 `claw-cli`
   - 还是把 parity CLI 另行抽成 library 之后做 wrapper

验收：

- `desktop-core`、`desktop-server`、`server` 能基于 parity crates 编译

## Phase 3：CI / 文档 / 发布链路收敛

目标：

- 新依赖模型可持续维护

动作：

1. 更新 `rust/README.md`
2. 更新本地启动脚本、构建说明、发布说明
3. 为 CI 增加 parity 依赖准备步骤
4. 若切 git dependency，则 pin rev 并校验 lockfile

验收：

- 新成员按文档可从零拉起
- CI 能稳定构建

---

## 8. 风险评估

### 风险 A：直接 path 依赖导致 CI/构建不可复现

等级：高

原因：

- sibling path 只适合本地联调
- 团队机器和 CI 不一定具备完全相同目录布局

应对：

- Phase 1 用 path
- Phase 3 收敛到 git pinned dependency

### 风险 B：`desktop-core` 继续吸收太多 parity 内部细节

等级：高

原因：

- 一旦 downstream 依赖了 parity 的内部实现细节，而非稳定公开接口，后续升级仍然会痛

应对：

- 只使用 parity 已经稳定 export 的公共接口
- 不在 `desktop-core` 中直接依赖 parity 的深层内部模块

### 风险 C：保留本地 `claw-cli` 会重新形成核心 fork

等级：高

原因：

- CLI 是核心行为最密集的部分
- 目前它与 parity CLI 差异已很大

应对：

- 第一阶段不要把“保留本地 CLI fork”作为目标
- 若必须保留，则单独立项评估 CLI core 抽库

### 风险 D：`lsp` crate 去留不清，导致残余旧依赖链

等级：中

应对：

- 与 vendored `runtime` 一并清退
- 若确实需要，后续以产品扩展能力重新引入，不再作为 parity fork 残留

---

## 9. 评审建议重点

建议团队评审时重点看下面四个问题：

1. **repo ownership 是否接受**
   - parity 是不是唯一核心 Rust 来源
   - open repo 是否只保留产品扩展

2. **依赖模式是否接受**
   - 第一阶段 path dependency
   - 第二阶段 git pinned dependency

3. **CLI 是否纳入第一阶段**
   - 我的建议是不纳入
   - 若要纳入，成本会明显升高

4. **是否需要本地 adapter crate**
   - 我的建议是先不做厚 adapter
   - 只在 `desktop-core` 做最小兼容修正

---

## 10. 最终建议

### 推荐决策

建议采用下面这条路线：

1. **把 `claw-code-parity` 定义为 Rust 核心唯一上游**
2. **把 `open-claude-code` 的 Rust workspace 缩到 `desktop-core` / `desktop-server` / `server`**
3. **第一阶段用 sibling path dependency 接 parity**
4. **第二阶段改为 git pinned dependency 固化构建**
5. **第一阶段不保留本地 `claw-cli` fork**
6. **兼容改动优先直接落在 `desktop-core`，不先引入厚 adapter**

### 这条路线的核心价值

- 解决长期双维护问题
- 让 parity 成为真正的上游核心
- 让 open repo 回到“产品组合层”的角色
- 为后续 Warwolf/OpenClaw 功能继续演进保留清晰边界

---

## 附：本次分析基线

本次方案基于以下事实确认：

1. `open-claude-code/rust` 与 `claw-code-parity/rust` 两边都能 `cargo check --workspace`
2. `open-claude-code` 的 desktop 侧当前只真实依赖 `api/runtime/tools/plugins`
3. `open-claude-code` 的 Tauri 壳层只要求本地 `desktop-server` 能继续从 `rust/` workspace 启动
4. 目前最大技术风险不在 Cargo path 改写，而在于：
   - 核心 crate ownership 重新收口
   - `desktop-core` 对 parity 新 API 的小范围适配

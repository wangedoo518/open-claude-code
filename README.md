# Open Claude Code

![OpenClaudeCode](open-claude-code.png)

> **0.2.0** — 15 个审计漏洞全部修复 + 5 项 craft-agents-oss 借鉴功能
> （会话工作流、Workspace Skills、加密凭据、`ocl` CLI、拖拽附件）。
> 详见 [`CHANGELOG.md`](CHANGELOG.md) 与 [`docs/getting-started.md`](docs/getting-started.md)。

`open-claude-code` 当前是一个围绕桌面壳层与 Rust 本地服务构建的工作区，而不再是单纯的”还原后的 Claude Code CLI 源码快照”。

目前仓库的主线实现主要分成两部分：

- `apps/desktop-shell`：Tauri + React 桌面应用壳层
- `rust/`：桌面壳层依赖的本地 Rust 服务与集成层

其中 Rust 核心能力已经不再 vendored 在本仓库中，而是固定依赖 [`claw-code-parity`](https://github.com/wangedoo518/claw-code-parity)。
此前仓库里保留的本地 Python 镜像/恢复代码和浏览器态 `desktop-web` 镜像前端都已经清理完成，当前只保留 `desktop-shell` 这一条产品前端主线。

## 当前状态

- 桌面壳层主路由已包含：
  - `/home`：Workbench 首页
  - `/apps`：应用画廊
  - `/apps/:id`：MinApp 详情页
  - `/code`：独立的 Code Tools 页面
- `Code` 已从旧的 MinApp 模式提升为一级页面，并引入独立的 `code-tools` 状态与页面结构
- 模型服务主链路现已收敛为本机托管的 `Codex OAuth` 与 `Qwen OAuth`
- `Code Tools` 当前只消费这两条 OAuth 模型目录，不再支持第三方/API Key provider hub
- `rust/` 只保留 `desktop-core`、`desktop-server`、`server` 三个本地 crate
- `desktop-server` 会为桌面前端提供本地 HTTP API，默认地址为 `http://127.0.0.1:4357`
- 旧的 vendored Rust core 已从仓库移除，当前实现以 parity 作为唯一上游

## 仓库结构

```text
open-claude-code/
├── apps/
│   └── desktop-shell/        # 当前主桌面应用（Tauri + React）
├── rust/                     # Rust 集成层 workspace
│   └── crates/
│       ├── desktop-core/     # 会话、OAuth 模型服务、本地持久化、调度等桌面 domain
│       ├── desktop-server/   # 提供 /api/desktop/* 的本地 HTTP 服务
│       └── server/           # 更轻量的服务层
├── docs/                     # 设计评审与迁移文档
└── assets/                   # 仓库说明与界面资源
```

## 关键模块

### 1. `apps/desktop-shell`

当前主产品入口。

技术栈：

- React
- React Router
- Redux Toolkit + redux-persist
- TanStack Query
- Tauri 2
- Tailwind
- Ant Design
- styled-components

主要页面：

- `HomePage`：工作台总览
- `AppsGalleryPage`：应用画廊
- `MinAppDetailPage`：内置/自定义应用承载页
- `CodeToolsPage`：Cherry Studio 风格的 Code 工具入口
- `session-workbench/*`：原 `code/*` 会话工作台组件迁移后的承载区

### 2. `rust/`

当前 Rust workspace 是下游集成层，不是完整 CLI 仓库。

本地 crate：

- `desktop-core`
- `desktop-server`
- `server`

上游依赖：

- `api`
- `runtime`
- `tools`
- `plugins`

这些 crate 通过固定 git revision 依赖 `claw-code-parity`，具体见 [rust/Cargo.toml](rust/Cargo.toml) 和 [rust/README.md](rust/README.md)。

### 3. `docs/`

包含当前仓库的重要设计和迁移结论，建议优先阅读：

- [open-claude-code-parity-dependency-design.md](docs/open-claude-code-parity-dependency-design.md)
- [open-claude-code-parity-migration-checklist.md](docs/open-claude-code-parity-migration-checklist.md)

## 开发环境

建议环境：

- Node.js 22+
- npm
- Rust stable
- Cargo
- Tauri 2 所需本机依赖

## 快速开始

### 桌面壳层

安装前端依赖：

```bash
cd apps/desktop-shell
npm install
```

前端构建：

```bash
npm run build
```

本地开发：

```bash
npm run tauri:dev
```

说明：

- `desktop-shell` 启动时会尝试自动拉起本仓库 `rust/` 下的 `desktop-server`
- debug 环境下会执行 `cargo run -p desktop-server`
- 如果已有编译产物，也会直接复用 `rust/target/{debug,release}/desktop-server`

### Rust 服务层

进入 Rust workspace：

```bash
cd rust
```

常用命令：

```bash
cargo check --workspace
cargo test --workspace
```

单独启动桌面服务：

```bash
cargo run -p desktop-server
```

健康检查：

```bash
curl http://127.0.0.1:4357/healthz
```

## 验证命令

当前仓库常用验证命令：

### Rust

```bash
cd rust
cargo fmt --all
cargo check --workspace
cargo test --workspace
```

### Desktop Shell

```bash
cd apps/desktop-shell
npm run build
cd src-tauri
cargo check
```

## 设计边界

当前建议遵循以下边界：

- `claw-code-parity` 负责 Rust 核心能力
- `open-claude-code/rust` 只负责桌面集成层和产品边界适配
- `apps/desktop-shell` 负责桌面交互、页面结构和宿主行为

如果要改 Rust 核心行为，优先考虑 upstream 到 parity，而不是在本仓库重新形成 fork。

## 需要注意的历史内容

仓库里仍保留一些历史或迁移文档，例如：

- 根目录 [package.json](package.json) 仍反映旧的 Electron/Claude 桌面依赖树
- [PARITY.md](PARITY.md) 是针对旧 vendored Rust port 的历史分析文档

这些内容依然有参考价值，但不应再被当成当前主产品入口或当前实现来源。

## 相关文档

- [CLAW.md](CLAW.md)
- [rust/README.md](rust/README.md)
- [docs/desktop-shell/README.md](docs/desktop-shell/README.md)

## License

请结合仓库根目录现有文件与上游依赖的许可证约束一起使用本项目。

# Warwolf `Code` 应用严格复刻 Cherry Studio 设计方案

## 1. 目标

本方案的目标不是“参考 Cherry 做一个类似的 Code 页面”，而是：

1. 严格复刻 `/Users/champion/Documents/develop/Warwolf/cherry-studio` 中现有 `Code` 应用的产品形态、交互顺序、页面结构、字段条件、启动逻辑、平台差异和文案语义。
2. Warwolf 只允许做宿主层适配，不允许擅自改变产品设计。
3. 复刻对象包含：
   - 启动台 / 应用入口中的 `Code` 卡片
   - 侧边栏 / 顶部页签里的 `Code` 一级页面
   - `/code` 独立页面
   - CLI 工具选择、模型选择、工作目录、环境变量、终端、更新选项、启动按钮
   - Bun 检查与安装
   - 终端检测与 Windows 自定义终端路径
   - CLI 启动服务
   - 各 CLI 工具的环境变量生成规则
   - 对应测试

结论先行：

- 这件事不是“改一个 UI 页面”，而是一次跨前端路由、状态层、Provider/Model 数据层、桌面宿主启动层的完整迁移。
- 如果要求“严格复刻”，Warwolf 当前的 `Code` 原生页必须让位，不能继续拿现在的 `CodePage` 做延伸。

## 1.1 当前已确认决议

截至本轮评审，以下五项已经确认：

1. `Code` 要从当前 MinApp 模式中剥离，升级为一级 `/code` 路由页面。
2. 接受为 `Code` 单独恢复多 provider / 多模型底座，不继续受限于当前 OpenAI-only Provider 页面。
3. 接受在 `apps/desktop-shell` 引入 `antd + styled-components`，以直接迁移 Cherry 的页面与交互细节。
4. 当前 Warwolf `CodePage / CodeTerminal` 不再下沉复用，直接从产品层删除，不再继续代表 `Code` 应用或内部会话工作台。
5. 接受 `CodeToolsService` 在 Warwolf 中按 Rust/Tauri 1:1 重写，对齐 Cherry 当前启动服务能力。

这五项决议会直接锁定后续实施边界：

1. `warwolf://code` 不再作为最终产品入口。
2. Warwolf 现有 `CodePage / CodeTerminal` 不再能代表 Cherry `Code` 应用。
3. `Code` 页面可用的数据来源必须独立于当前简化版 Provider 页面。
4. `Code` 页面前端允许直接采用 Cherry 当前的 `antd + styled-components` 组合，不再受 Warwolf 现有 UI 体系约束。
5. 现有 `CodePage / CodeTerminal` 不做保留型重命名或内部下沉，而是按产品替换路径清理。
6. `Code` 的启动、终端、Bun、CLI 安装与执行链路，按 Cherry `CodeToolsService` 能力由 Rust/Tauri 重写承接。

---

## 2. Cherry Studio 源码基线

本方案以下列 Cherry Studio 源码作为唯一产品基线：

### 2.1 入口与路由

- `src/renderer/src/pages/launchpad/LaunchpadPage.tsx`
- `src/renderer/src/components/app/Sidebar.tsx`
- `src/renderer/src/Router.tsx`

### 2.2 Code 页面

- `src/renderer/src/pages/code/CodeToolsPage.tsx`
- `src/renderer/src/pages/code/index.ts`
- `src/renderer/src/i18n/locales/zh-cn.json`

### 2.3 前端状态与组件依赖

- `src/renderer/src/store/codeTools.ts`
- `src/renderer/src/hooks/useCodeTools.ts`
- `src/renderer/src/components/ModelSelector.tsx`
- `src/renderer/src/components/AnthropicProviderListPopover.tsx`
- `src/renderer/src/components/app/Navbar.tsx`

### 2.4 主进程能力

- `src/main/services/CodeToolsService.ts`
- `src/main/ipc.ts`
- `src/preload/index.ts`

### 2.5 基础常量与测试

- `packages/shared/config/constant.ts`
- `packages/shared/config/providers.ts`
- `src/renderer/src/pages/code/__tests__/index.test.ts`
- `src/main/services/__tests__/CodeToolsService.test.ts`

---

## 3. “严格复刻”在本项目中的定义

本次评审采用以下定义：

1. 页面结构、字段顺序、显隐条件、按钮位置、默认值、文案语义，均以 Cherry 当前实现为准。
2. CLI 工具列表和顺序不得改动：
   - Claude Code
   - Qwen Code
   - Gemini CLI
   - OpenAI Codex
   - iFlow CLI
   - GitHub Copilot CLI
   - Kimi CLI
   - OpenCode
3. `Code` 必须是一级独立页面 `/code`，不是套在 Warwolf 现有 MinApp 原生容器里的一个特例页。
4. Warwolf 当前 `CodePage / CodeTerminal` 不是目标产品，不能继续作为 `Code` 应用承载。
5. 允许的改动只有宿主适配：
   - Electron IPC -> Tauri / desktop-server
   - `window.api.xxx` -> Warwolf `tauri.ts` 包装
   - Cherry store 注入 -> Warwolf store 注入

不允许的改动：

1. 不允许把页面重写成另一套信息架构。
2. 不允许把 Antd 表单随意改成 shadcn 风格后再称为“复刻”。
3. 不允许删掉多 CLI 工具，仅保留 OpenAI Codex。
4. 不允许把 `/code` 改成 `/apps/code` 的二级 MinApp 页面。

---

## 4. Cherry Studio 当前 `Code` 应用真实形态

## 4.1 入口形态

Cherry 中 `Code` 有两个一级入口：

1. 启动台卡片，点击跳转 `/code`
2. 侧边栏 `code_tools` 图标，点击跳转 `/code`

这说明 `Code` 在 Cherry 中是“顶级页面”，不是嵌入式小程序详情页。

## 4.2 页面布局

`CodeToolsPage.tsx` 的页面结构非常明确：

1. 顶部 `Navbar + NavbarCenter`，标题为“代码工具”
2. 页面主体居中，主内容宽度固定 `600px`
3. 标题区：
   - 标题 `代码工具`
   - 描述 `快速启动多个代码 CLI 工具，提高开发效率`
4. Bun 提示区：
   - 如果未安装 Bun，显示一条黄色 `Alert`
   - 右侧有“安装 Bun”按钮
5. 表单区字段顺序固定：
   - CLI 工具
   - 模型
   - 工作目录
   - 环境变量
   - 终端
   - 更新选项
6. 底部主按钮：
   - 图标 + “启动”
   - 满宽
   - `!canLaunch || !isBunInstalled` 时禁用

## 4.3 字段显隐规则

严格复刻必须保留以下条件逻辑：

1. `GitHub Copilot CLI` 不显示“模型”字段。
2. `claude-code` 时，“模型”标题右侧额外显示 `AnthropicProviderListPopover`。
3. “终端”字段仅在 macOS / Windows 且确实检测到可用终端时显示。
4. Windows 下选择特定终端时，右侧显示“设置自定义路径”按钮与路径提示。

## 4.4 状态模型

Cherry 的 `codeTools` 状态不是临时表单，而是持久化状态，结构固定：

1. `selectedCliTool`
2. `selectedModels`：按 CLI 工具分别存模型
3. `environmentVariables`：按 CLI 工具分别存环境变量
4. `directories`：目录历史，最多 10 条，MRU 顺序
5. `currentDirectory`
6. `selectedTerminal`

这意味着用户切换 CLI 工具时，各工具模型与环境变量会分别记忆，不能共用一套草稿。

## 4.5 Provider / Model 依赖关系

Cherry 的 `Code` 应用强依赖全局 Provider/Model 数据层：

1. `useProviders()` 提供全部 provider
2. `ModelSelector` 根据 provider + predicate 过滤模型
3. 每个 CLI 工具有独立 provider 过滤规则
4. 每个 CLI 工具有独立环境变量映射规则

关键事实：

- `Code` 应用不是独立于 provider 系统存在的，它本质上是 Cherry “多 provider 模型系统”的一个 CLI 启动前端。

## 4.6 启动逻辑

Cherry 的 `CodeToolsService` 覆盖了完整桌面启动链路：

1. 检查工作目录是否存在
2. 根据 CLI 工具决定包名、可执行名
3. 检查是否已安装
4. 检查版本，必要时可自动更新
5. 按工具生成环境变量
6. 按平台选择终端打开命令
7. detached spawn 新终端窗口

并且存在大量工具特化逻辑：

1. `qwen-code >= 0.12.3` 时追加 `--auth-type openai`
2. `openai-codex` 通过 `--config` 注入 `model_provider`、`wire_api="responses"`、`model`
3. `opencode` 会生成并回收 `opencode.json`
4. Windows 会生成临时 `.bat` 文件，并做 cmd 元字符转义
5. npm registry 会根据用户是否在中国切换镜像

---

## 5. Warwolf 当前状态与差距

基于当前 Warwolf 源码，存在以下关键差距：

## 5.1 入口层差距

Warwolf 当前 `Code` 是 built-in minapp：

- `apps/desktop-shell/src/config/minapps.ts`

它当前是：

1. `id: "code"`
2. `url: "warwolf://code"`
3. 进入的是原生 `CodePage`

这和 Cherry 的 `/code` 一级路由模式不一致。

## 5.2 页面产品形态差距

Warwolf 当前 `apps/desktop-shell/src/features/code/CodePage.tsx` 是：

1. 会话列表 + 会话消息流
2. Claude Code 风格工作台
3. 本质是 AI session terminal

而 Cherry 的 `Code` 页面是：

1. 单页表单
2. 目标是启动本地 CLI 工具
3. 不承载消息流

这两个产品不是同一件事。

## 5.3 应用入口差距

Warwolf 当前 `AppsGalleryPage.tsx` 是通用搜索网格；
`features/workbench/AppsPage.tsx` 里的 `Code` 卡片点进去还是走现有工作台逻辑。

这与 Cherry 的 Launchpad -> `/code` 行为不一致。

## 5.4 状态层差距

Warwolf 当前没有 Cherry 等价的：

1. `codeTools` redux slice
2. `useCodeTools` hook
3. 按 CLI 工具记忆模型/环境变量的表单状态

## 5.5 Provider 数据层差距

这是最大 blocker。

Warwolf 当前刚刚收敛成 OpenAI-only Provider 页面，但 Cherry 的 `Code` 依赖：

1. Anthropic provider
2. Gemini provider
3. OpenAI / OpenAI-compatible provider
4. 多 provider 模型枚举与过滤

如果 Warwolf 不补这一层，就无法“完整复刻” Cherry 的 `Code`。

结论：

- 只复刻 UI 不够。
- 必须恢复一套专门给 `Code` 应用使用的 provider/model catalog 能力。

## 5.6 宿主能力差距

Warwolf 当前没有 Cherry 等价的以下桌面能力：

1. `codeTools.run`
2. `codeTools.getAvailableTerminals`
3. `codeTools.setCustomTerminalPath`
4. `codeTools.getCustomTerminalPath`
5. `codeTools.removeCustomTerminalPath`
6. `isBinaryExist('bun')`
7. `installBunBinary()`

---

## 6. Warwolf 目标方案

## 6.1 总体原则

推荐方案只有一个：

### 方案 A：直接迁移 Cherry Code 应用，Warwolf 只做宿主适配

1. 页面结构直接复刻
2. 状态结构直接复刻
3. 启动服务逻辑直接复刻
4. 测试直接迁移
5. 仅在以下位置做适配：
   - Electron IPC -> Warwolf API
   - Cherry 全局 provider hooks -> Warwolf provider/model bridge
   - Cherry 顶部壳层 -> Warwolf 路由与 Tab 宿主

不推荐方案：

### 方案 B：用 Warwolf 现有 `CodePage` 改造成 Code Tools

不推荐原因：

1. 这不是复刻，是重做
2. 会把消息工作台和 CLI 启动页混在一起
3. 最终外观和交互一定偏离 Cherry

## 6.2 路由与顶层导航

严格复刻要求 Warwolf 新增一级路由：

1. `/code`

并调整入口：

1. `AppsGalleryPage` 中 `Code` 卡片点击直达 `/code`
2. Home / Apps 里的 `Code` 卡片也直达 `/code`
3. 顶部 Tab 打开后标题为 `Code`
4. 不再通过 `/apps/code` 进入 `MinAppDetailPage`
5. 不再给 `Code` 页面套 `MinimalToolbar`

设计结论：

- `Code` 不再是 built-in minapp
- `Code` 是一级 route page
- `OpenClaw` 仍可继续保留 MinApp/NativePage 模式

## 6.3 UI 技术栈策略

为了满足“严格复刻”，建议：

1. 在 `apps/desktop-shell` 引入 `antd`
2. 引入 `styled-components`
3. 页面层尽量直接迁移 Cherry 组件结构

原因：

1. Cherry 的 `CodeToolsPage` 直接依赖 Antd 交互细节
2. `ModelSelector`、`AnthropicProviderListPopover` 也依赖 Antd
3. 如果改写成 shadcn/tailwind，像素级和交互细节会漂移

结论：

- 本页不应用“Warwolf 当前 UI 体系优先”原则
- 本页应用“Cherry 直接复刻优先”原则

## 6.4 前端文件迁移映射

建议映射如下：

### Cherry -> Warwolf

1. `src/renderer/src/pages/code/CodeToolsPage.tsx`
   -> `apps/desktop-shell/src/features/code-tools/CodeToolsPage.tsx`

2. `src/renderer/src/pages/code/index.ts`
   -> `apps/desktop-shell/src/features/code-tools/index.ts`

3. `src/renderer/src/store/codeTools.ts`
   -> `apps/desktop-shell/src/store/slices/codeTools.ts`

4. `src/renderer/src/hooks/useCodeTools.ts`
   -> `apps/desktop-shell/src/hooks/useCodeTools.ts`

5. `src/renderer/src/components/ModelSelector.tsx`
   -> `apps/desktop-shell/src/features/code-tools/components/ModelSelector.tsx`

6. `src/renderer/src/components/AnthropicProviderListPopover.tsx`
   -> `apps/desktop-shell/src/features/code-tools/components/AnthropicProviderListPopover.tsx`

7. `src/renderer/src/components/app/Navbar.tsx`
   -> Warwolf 需要新增一个 Cherry-compatible navbar 组件，仅供 `/code` 页面使用

## 6.5 Warwolf 壳层改造

涉及文件：

1. `apps/desktop-shell/src/shell/AppShell.tsx`
   - 新增 `/code` route

2. `apps/desktop-shell/src/config/minapps.ts`
   - 移除或降级 `code` built-in minapp

3. `apps/desktop-shell/src/features/apps/AppsGalleryPage.tsx`
   - `Code` 卡片改为直接打开 `/code`

4. `apps/desktop-shell/src/features/workbench/AppsPage.tsx`
   - `Code` 卡片改为直接打开 `/code`

5. `apps/desktop-shell/src/shell/TabBar.tsx`
   - 支持 `Code` route 打开顶部 `Code` 页签

## 6.6 Provider / Model 数据层设计

这是严格复刻的核心前提。

Warwolf 必须新增一个“给 Code Tools 使用”的 provider/model catalog 读取层，满足 Cherry 的这几个依赖：

1. 返回全部 provider
2. 返回每个 provider 下的 models
3. Provider 需要暴露：
   - `id`
   - `type`
   - `name`
   - `apiHost`
   - `anthropicApiHost`
   - `models`
4. Model 需要暴露：
   - `id`
   - `name`
   - `provider`
   - `supported_endpoint_types`
   - `group`

设计要求：

1. 不复用当前 OpenAI-only Provider 页面状态作为唯一来源
2. 单独给 Code Tools 暴露一个完整 provider catalog
3. 这层数据能力按 Cherry 的 provider 过滤规则服务于 `Code` 页面

如果不做这层，下面这些 Cherry 行为都没法保真：

1. Claude Code 只显示 Anthropic-compatible providers
2. Gemini CLI 只显示 Gemini-compatible providers
3. OpenAI Codex 只显示 OpenAI Responses / OpenAI-compatible providers
4. OpenCode 同时支持 openai/openai-response/anthropic

## 6.7 桌面后端服务设计

建议在 Warwolf 后端新增等价 `CodeToolsService`，但由 Tauri/Rust 实现 1:1 行为。

### 推荐落点

1. `rust/crates/desktop-core/src/code_tools.rs`
2. `rust/crates/desktop-core/src/lib.rs` 接入 `DesktopState`
3. `rust/crates/desktop-server/src/lib.rs` 暴露 `/api/desktop/code-tools/*`
4. `apps/desktop-shell/src/lib/tauri.ts` 暴露前端调用方法

### API 契约

Warwolf 应提供与 Cherry preload 等价的方法：

1. `run(cliTool, model, directory, env, options)`
2. `getAvailableTerminals()`
3. `setCustomTerminalPath(terminalId, path)`
4. `getCustomTerminalPath(terminalId)`
5. `removeCustomTerminalPath(terminalId)`
6. `isBinaryExist(binaryName)`
7. `installBunBinary()`

### 行为要求

`CodeToolsService` 必须 1:1 复刻 Cherry 逻辑：

1. CLI 包名映射
2. CLI 可执行名映射
3. Bun 全局安装路径
4. npm registry 中国区镜像切换
5. 版本检查与自动更新
6. `qwen-code` 版本判断追加 `--auth-type openai`
7. `openai-codex` 通过 `--config` 注入 provider/model/responses
8. `opencode` 生成与回收 `opencode.json`
9. macOS / Windows / Linux 终端启动分支
10. Windows `.bat` 临时文件与转义策略

## 6.8 文案策略

文案必须直接对齐 Cherry `zh-cn`：

1. 页面标题：`代码工具`
2. 描述：`快速启动多个代码 CLI 工具，提高开发效率`
3. 字段标题与帮助文案全部按 Cherry 对齐
4. 成功/失败提示按 Cherry 对齐

不允许自行重写为 Warwolf 口吻。

## 6.9 测试策略

测试也要直接复刻 Cherry 基线：

1. 迁移 `pages/code/__tests__/index.test.ts`
   - 验证 `generateToolEnvironment`
   - 验证 `/v1` 补全逻辑

2. 迁移 `main/services/__tests__/CodeToolsService.test.ts`
   - 验证 Windows batch 转义逻辑

3. Warwolf 额外补充集成测试：
   - `/code` 路由打开
   - `Code` 卡片从应用页跳转 `/code`
   - `Code` 顶部 tab 正常打开

---

## 7. 分阶段实施建议

## Milestone A：壳层与空页面对齐

1. 新增 `/code` route
2. `Code` 从 MinApp 中剥离
3. Apps 入口改为直达 `/code`
4. 顶部 Tab 支持 `Code`
5. 渲染 Cherry 风格空页面骨架

验收标准：

1. `Code` 不再进入现有 `CodePage`
2. 不再出现 `MinimalToolbar`
3. 顶部 tab / 路由 / 应用入口行为与 Cherry 一致

## Milestone B：前端页面与状态严格复刻

1. 迁移 `CodeToolsPage`
2. 迁移 `codeTools` slice
3. 迁移 `useCodeTools`
4. 迁移 `ModelSelector`
5. 迁移 `AnthropicProviderListPopover`
6. 接入 Cherry 原始中文文案

验收标准：

1. 页面结构与 Cherry 截图逐项一致
2. 字段顺序和条件显隐一致
3. 状态持久化行为一致

## Milestone C：Provider / Model 数据层

1. 增加 Code Tools 专用 provider catalog
2. 打通 model predicate
3. 打通 provider 过滤规则

验收标准：

1. 8 个 CLI 工具都能出现正确的 provider / model 选项
2. 切换不同 CLI 工具时模型列表符合 Cherry 行为

## Milestone D：后端启动服务

1. 复刻 `CodeToolsService`
2. 打通 Bun 检查/安装
3. 打通终端检测
4. 打通 CLI 启动

验收标准：

1. Bun 未安装时出现警告并可安装
2. 启动成功能打开新终端窗口
3. Windows/macOS 终端逻辑与 Cherry 一致

## Milestone E：测试与视觉回归

1. 迁移单测
2. 增加 UI 回归检查
3. 按 Cherry 截图逐项比对

---

## 8. 评审需要拍板的关键问题

1. 是否接受 `Code` 从当前 `warwolf://code` built-in minapp 中剥离，升级为一级 `/code` route？
   - 评审结论：已接受。

2. 是否接受当前 Warwolf `CodePage / CodeTerminal` 不再代表产品上的 `Code` 应用？
   - 评审结论：已接受，且进一步结论为直接删除，不做内部下沉复用。

3. 是否接受在 `apps/desktop-shell` 引入 `antd + styled-components`，以直接迁移 Cherry 页面？
   - 评审结论：已接受。

4. 是否接受为 `Code` 应用恢复一套多 provider / 多模型 catalog，而不是继续受限于当前 OpenAI-only Provider 页面？
   - 评审结论：已接受。

5. 是否接受 `CodeToolsService` 在 Warwolf 里以 Rust/Tauri 1:1 重写，而不是继续沿用当前工作台式 `CodePage`？
   - 评审结论：已接受。

---

## 9. 最终建议

如果目标真的是“严格复刻 Cherry Studio 的 Code 应用”，推荐结论是：

1. 直接迁移 Cherry 的 `Code` 页面与状态模型
2. 让 Warwolf 只做壳层适配
3. 将当前 Warwolf 的会话式 `CodePage` 从产品层面让位
4. 为 Code Tools 单独恢复多 provider / 多模型底座
5. 先做 `/code` 一级路由和壳层剥离，再接前端页面，再补后端服务

一句话总结：

这次不应该“在 Warwolf 现有 Code 上继续改”，而应该“把 Cherry 的 Code 作为一个完整产品模块迁进 Warwolf”。

## 9.1 基于已确认决议的下一步

在其余评审项未最终拍板前，已经可以先启动且不会返工的工作有：

1. `/code` 一级路由与顶层 tab 接入
2. `Code` 从 MinApp 入口中剥离
3. `Code Tools` 专用 provider/model 数据契约设计
4. `CodeToolsService` 的 Tauri/Rust 接口骨架

基于当前已确认的五项决议，实施边界已经明确为：

1. `Code` 的产品入口必须直接对齐 Cherry `/code`，不再允许以 `warwolf://code` 或 MinApp 容器继续承载最终产品。
2. `Code` 的 provider/model 底座必须按 Cherry 真实依赖恢复，不再允许以当前 OpenAI-only Provider 页面作为唯一数据来源。
3. `Code` 页面前端可以直接按 Cherry 技术栈迁移，包括 `antd + styled-components`，不再需要额外做 Warwolf 样式体系兼容。
4. 当前 `CodePage / CodeTerminal` 按产品替换路径删除，不再保留为内部工作台或兼容层。
5. `CodeToolsService` 的实现路径已经锁定为 Rust/Tauri 1:1 重写，不再讨论继续沿用旧工作台逻辑。
6. 后续所有实现均以“严格复刻 Cherry `Code` 产品”为约束，不再接受“复用 Warwolf 现有 Code 工作台并做近似改造”的方案。

在当前评审结论下，已经可以继续推进且不会返工的工作还新增包括：

1. `CodeToolsPage` 页面骨架与表单结构直接迁移
2. `codeTools` slice、`useCodeTools`、`ModelSelector`、`AnthropicProviderListPopover` 的前端迁移
3. Cherry 中文文案与页面交互细节的直接对齐

评审层面的关键实施边界已经补齐，可以直接进入编码实施。

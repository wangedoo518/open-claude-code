# Warwolf Provider 页面改版方案

## 1. 结论摘要

本次改版建议将 Warwolf 的 `Settings > Provider` 收敛为一个**仅支持 OpenAI 官方渠道**的模型服务页，并且认证方式**仅保留 Codex auth / ChatGPT 登录态**，不再提供任何 API Key 录入、第三方 OpenAI 兼容渠道、OpenClaw 渠道或自定义 provider 能力。

页面交互与视觉语言参考 `cherry-studio` 的模型服务页，但不机械复刻其“多 provider 平台市场”，而是采用：

- 保留 Cherry 的三栏结构与卡片节奏
- 将中栏收敛为“单一 OpenAI 服务商”
- 将右栏的“API 密钥 + API 地址 + 检测”改为“Codex 登录态 + Codex 同步 + 模型选择”
- 全部文案统一为中文

推荐实现路径：

- **前端产品层改成单 provider 页面**
- **底层暂时保留 Warwolf 现有 provider/codex 数据结构**
- **页面只暴露 `codex-openai` 这一条官方能力**

这样可以最快落地，同时保留后端已有的 Codex auth 基础设施，减少返工。

## 2. 背景与问题

当前 Warwolf 的 Provider 页已经具备类似 Cherry 的工作台骨架，但产品形态仍偏“通用 provider 管理器”：

- 支持多 runtime target
- 支持 OpenClaw / Codex 双轨
- 支持 preset catalog、导入、草稿、保存、同步
- 支持 API Key、Base URL、协议、模型列表编辑

这与当前产品目标不一致。你提出的新目标是：

- Provider 页面只接入 OpenAI
- 样式与 Cherry Studio 模型服务页一致
- 文案仅支持中文
- OpenAI 仅支持 Codex auth
- 其他 API Key 方式全部删除

所以这次不是“在现有 Provider 页面里再加一个限制”，而是要把页面从“通用 provider hub”改成“OpenAI for Codex 专页”。

## 3. 参考实现分析

### 3.1 Cherry Studio 可直接借鉴的部分

参考：

- [SettingsPage.tsx](/Users/champion/Documents/develop/Warwolf/cherry-studio/src/renderer/src/pages/settings/SettingsPage.tsx)
- [ProviderList.tsx](/Users/champion/Documents/develop/Warwolf/cherry-studio/src/renderer/src/pages/settings/ProviderSettings/ProviderList.tsx)
- [ProviderSetting.tsx](/Users/champion/Documents/develop/Warwolf/cherry-studio/src/renderer/src/pages/settings/ProviderSettings/ProviderSetting.tsx)

可借鉴点：

- 设置页左栏导航的密度、留白、分组方式
- Provider 页“中栏列表 + 右栏详情”的工作台结构
- 右栏标题区的“名称 + 官网跳转 + 启用开关”
- 警告条、表单区块、模型列表折叠区的组织方式

不建议照搬的点：

- Cherry 面向多 provider、多协议、多登录方式
- 大量字段围绕 API Key、API Host、协议分支展开
- 多 provider 搜索/筛选/新增在单 OpenAI 场景下会变成噪音

### 3.2 Warwolf 当前可复用的部分

参考：

- [ProviderSettings.tsx](/Users/champion/Documents/develop/Warwolf/open-claude-code/apps/desktop-shell/src/features/settings/sections/ProviderSettings.tsx)
- [main.rs](/Users/champion/Documents/develop/Warwolf/open-claude-code/apps/desktop-shell/src-tauri/src/main.rs)
- [provider_hub.rs](/Users/champion/Documents/develop/Warwolf/open-claude-code/rust/crates/desktop-core/src/provider_hub.rs)
- [codex_auth.rs](/Users/champion/Documents/develop/Warwolf/open-claude-code/rust/crates/desktop-core/src/codex_auth.rs)

可直接复用：

- `codex-openai` 预设已存在
- Codex runtime 状态读取已存在
- Codex ChatGPT 登录态导入 / 浏览器登录 / profile 激活已存在
- 同步到 `~/.codex/auth.json` 和 `~/.codex/config.toml` 已存在
- Tauri 已能自动拉起 `desktop-server`

不应继续暴露给用户的能力：

- `Import OpenClaw`
- `Import Codex`
- 添加 provider
- 自定义 provider 草稿
- API Key 输入
- Base URL 编辑
- 协议切换
- Runtime Target 切换
- Provider Type / Billing Category / Category 手工编辑
- OpenClaw env/tools 编辑

## 4. 产品范围

## 4.1 范围内

- Provider 页面只展示 OpenAI
- 只支持官方 OpenAI Responses 通道
- 只支持 Codex auth / ChatGPT 登录态
- 支持查看当前登录账号、套餐、同步状态
- 支持将 OpenAI provider 同步到 Codex
- 支持查看并选择默认模型
- 支持中文文案与 Cherry 风格布局

## 4.2 范围外

- Azure OpenAI
- OpenRouter / AiHubMix / DMXAPI 等第三方渠道
- API Key 登录
- OpenClaw provider 管理
- 自定义兼容接口
- 多 provider 排序、增删、导入

## 5. 信息架构

推荐保留设置页现有左栏结构，但 `Provider` 页内部改成“Cherry 风格的单 provider 工作台”。

### 5.1 页面结构

```text
+--------------------------------------------------------------------------------------------------+
| 设置                                                                                             |
+----------------------+------------------------------+--------------------------------------------+
| 左侧设置导航         | 中栏：模型服务列表           | 右侧：OpenAI 详情                           |
|                      |                              |                                            |
| 常规设置             | [ 搜索框移除 ]               | OpenAI                          [启用开关]  |
| 模型服务             |                              | 官网  登录状态  同步到 Codex               |
| 显示设置             |  OpenAI                      |--------------------------------------------|
| 数据设置             |  已连接 / 未连接             | 账号连接                                    |
| MCP 服务             |  官方                        | - 使用 ChatGPT 登录                         |
| ...                  |                              | - 导入当前 Codex 登录态                     |
|                      |                              | - 当前账号 / 套餐 / 最近同步时间            |
|                      |                              |                                            |
|                      |                              | 服务配置                                    |
|                      |                              | - 鉴权方式：Codex 登录                      |
|                      |                              | - API 地址：只读                            |
|                      |                              | - 协议：只读 Responses                      |
|                      |                              |                                            |
|                      |                              | 模型配置                                    |
|                      |                              | - 默认模型                                  |
|                      |                              | - 模型分组列表                              |
|                      |                              | - 写入 Codex                                |
|                      |                              |                                            |
|                      |                              | 诊断信息                                    |
|                      |                              | - Codex 已登录 / 未登录                     |
|                      |                              | - 已同步 / 未同步                           |
+----------------------+------------------------------+--------------------------------------------+
```

### 5.2 中栏策略

中栏不再保留“搜索、筛选、添加、导入、Catalog”。

原因：

- 只有一个 provider，搜索和筛选没有价值
- 没有新增 provider 能力，`Add` 没有产品意义
- 没有多渠道，`Catalog` 不成立

中栏改为一个固定列表区：

- 标题：`模型服务`
- 列表项：`OpenAI`
- 状态标签：`已启用` / `未启用`
- 连接标签：`已登录` / `未登录`
- 同步标签：`已写入 Codex` / `未写入 Codex`

这样视觉上仍然与 Cherry 保持“选中左项，右侧编辑”的节奏，但信息结构更贴合单 provider 产品。

## 6. 右栏交互设计

## 6.1 头部

对齐 Cherry：

- 左侧展示 `OpenAI`
- 紧跟一个小标签：`官方`
- 右上角是启用开关
- 标题右侧保留官网外链按钮

推荐头部操作：

- `同步到 Codex`
- `刷新状态`

不再保留：

- 保存
- 删除
- 检测 API
- 设为默认 Provider

因为页面不存在多个 provider，也不存在 API Key 测试流。

## 6.2 警告条

沿用 Cherry 的黄色提示条视觉。

状态文案建议：

- 未登录：`当前尚未连接 OpenAI 账号，请先使用 ChatGPT 登录。`
- 已登录未同步：`当前 OpenAI 账号已连接，但尚未写入 Codex 配置。`
- 已同步：不显示警告条

## 6.3 区块一：账号连接

这是对 Cherry 里“API 密钥”区块的替代。

展示内容：

- 当前状态：`未登录` / `已登录`
- 登录方式：`Codex 授权登录`
- 当前账号：邮箱或账号昵称
- 套餐：`Free` / `Plus` / `Pro` / `Team` 等
- 最近刷新时间

按钮：

- `使用 ChatGPT 登录`
- `导入当前 Codex 登录态`
- `设为当前账号`
- `刷新登录状态`
- `移除账号`

说明文案：

- `Warwolf 仅支持使用 Codex 登录态连接 OpenAI，不支持手动填写 API 密钥。`

## 6.4 区块二：服务配置

这一区块只保留只读信息，不允许编辑。

字段：

- 鉴权方式：`Codex 登录`
- API 地址：`https://api.openai.com`
- 协议：`OpenAI Responses`
- Codex 配置文件：`~/.codex/config.toml`
- 授权文件：`~/.codex/auth.json`

设计原则：

- 让用户“知道系统在做什么”
- 不让用户误以为可以切换 endpoint 或改协议

## 6.5 区块三：模型配置

模型区仍然需要保留，因为这是 Provider 页最核心的业务价值。

建议参考 Cherry 的模型展示方式：

- 顶部显示 `模型`
- 提供 `刷新模型列表`
- 使用折叠分组展示，例如：
  - `GPT 5`
  - `GPT 5.1`
  - `图像`

每个模型项建议展示：

- 模型名称
- 能力标签：`对话`、`推理`、`代码`、`图像`
- 当前默认标识

操作建议仅保留：

- `设为默认模型`

不保留：

- 自定义添加模型
- 删除模型
- 编辑 context window / max output tokens

原因：

- 官方 OpenAI 模型列表不应由用户手工维护
- 单 provider 页面里，“模型是系统能力，不是用户自定义资产”

## 6.6 区块四：Codex 同步与诊断

建议单独保留一个底部区块。

展示内容：

- 当前是否已写入 Codex
- 当前写入的 provider key
- 当前默认模型
- 上次同步时间
- 风险提示

文案示例：

- `已将 OpenAI 官方配置写入 Codex。`
- `当前尚未同步，Codex 将继续使用已有配置。`
- `检测到本地 Codex 已登录，但尚未设置默认模型。`

## 7. 中文文案规范

本页所有用户可见文案统一改成中文，不保留英文标签。

### 7.1 一级标题

- `Provider` -> `模型服务`

### 7.2 中栏

- `Provider Library` -> `模型服务`
- `Configured` -> `当前服务`
- `Draft` -> 删除
- `Catalog` -> 删除

### 7.3 右栏区块

- `Authentication & Connection` -> `账号连接`
- `Basic Information` -> `服务信息`
- `Models` -> `模型`
- `Diagnostics` -> `诊断`

### 7.4 按钮

- `Sync to Codex` -> `同步到 Codex`
- `Login with ChatGPT` -> `使用 ChatGPT 登录`
- `Import Current Auth` -> `导入当前 Codex 登录态`
- `Refresh` -> `刷新状态`
- `Apply` -> `设为当前账号`
- `Current` -> `当前使用中`

## 8. 交互流程

## 8.1 首次使用

```text
打开 模型服务
-> 页面只显示 OpenAI
-> 右侧显示“当前尚未连接 OpenAI 账号”
-> 用户点击“使用 ChatGPT 登录”
-> 浏览器完成授权
-> Warwolf 读取并保存 Codex 登录态
-> 用户点击“同步到 Codex”
-> 完成
```

## 8.2 已有本地 Codex 登录态

```text
打开 模型服务
-> 页面检测到 ~/.codex/auth.json
-> 提示“检测到现有 Codex 登录态”
-> 用户点击“导入当前 Codex 登录态”
-> 页面展示当前账号与套餐
-> 用户点击“同步到 Codex”
```

## 8.3 切换默认模型

```text
打开 模型服务
-> 进入 模型 区块
-> 点击某个模型的“设为默认模型”
-> 页面提示“已设为默认模型”
-> 再点击“同步到 Codex”
```

## 9. 前端改造建议

## 9.1 页面层

保留 [ProviderSettings.tsx](/Users/champion/Documents/develop/Warwolf/open-claude-code/apps/desktop-shell/src/features/settings/sections/ProviderSettings.tsx) 路由入口，但建议拆成新的单用途组件：

- `OpenAIProviderPage`
- `OpenAIProviderSidebarCard`
- `OpenAIAccountSection`
- `OpenAIServiceInfoSection`
- `OpenAIModelSection`
- `OpenAIDiagnosticsSection`

不要继续在现有 `ProviderSettings.tsx` 上叠更多 `if runtime_target === ...`。

## 9.2 需要删除或隐藏的前端能力

- `Add` 按钮
- `Import OpenClaw`
- `Import Codex`
- 搜索框
- 分类筛选
- `Custom provider`
- `OpenClaw Catalog`
- `Codex Catalog`
- API Key 输入框
- Base URL 输入框
- Protocol 下拉框
- Runtime Target 下拉框
- 连接测试按钮
- env/tools 编辑器

## 9.3 保留并改造的前端能力

- `codexAuthOverviewQuery`
- `codexRuntimeQuery`
- `beginCodexLogin`
- `importCodexAuthProfile`
- `activateCodexAuthProfile`
- `refreshCodexAuthProfile`
- `syncManagedProvider`

但这些能力不再以“通用 provider 管理器”方式出现，而是服务于单一 OpenAI 页面。

## 10. 后端改造建议

## 10.1 推荐策略

**保留底层 generic provider hub，收敛上层 API 与页面。**

理由：

- `codex-openai` 预设和同步逻辑已经存在
- Codex auth 相关数据结构已可用
- 这样交付速度最快

推荐做法：

- 启动时确保存在一个内置 managed provider：`codex-openai`
- 前端只读取这个 provider
- 屏蔽其他 provider preset 和编辑入口

## 10.2 页面专用接口

建议新增一组更语义化的接口，而不是让前端继续拼通用 provider API：

- `GET /api/desktop/openai-provider`
- `POST /api/desktop/openai-provider/sync`
- `POST /api/desktop/openai-provider/default-model`
- `GET /api/desktop/openai-provider/models`

这样可以把页面从 generic provider hub 解耦出来。

## 10.3 API Key 相关处理

设计要求是“其他 API key 方式删除”，因此建议：

- 页面不再暴露 API Key 输入
- 页面专用接口不再接受 API Key 字段
- `sync_provider_to_codex` 的 UI 入口只走 ChatGPT/Codex auth

兼容策略：

- 底层保留 `OPENAI_API_KEY` 兼容代码可以接受，但不对页面暴露
- 评审通过后可在第二阶段清理代码路径

## 11. 风险与注意事项

## 11.1 最大风险

如果只改 UI，不改中栏策略，用户会继续误以为“Codex 还在其他地方”。

因此必须同步完成：

- 单 provider 信息架构
- 中文文案
- API Key 删除
- Codex auth 作为唯一入口

## 11.2 兼容风险

Warwolf 现有 provider hub 已支持多 provider，如果直接硬删后端通用能力，后续再扩展会有返工。

所以推荐：

- **产品层单 provider**
- **技术底层暂保留 generic provider**

## 11.3 视觉风险

如果完全照抄 Cherry 的“多 provider 市场”布局，单 OpenAI 场景会显得空和假。

因此建议：

- 复制 Cherry 的视觉节奏
- 不复制其多 provider 控件密度

## 12. 评审建议

这次评审建议聚焦 5 个问题：

1. `Provider` 是否正式收敛为“OpenAI for Codex 专页”，而不是通用 provider hub
2. 是否接受“只保留 Codex auth，彻底删除 API Key UI”
3. 是否接受“页面只保留一个固定 OpenAI 列表项，中栏不再提供搜索/添加/导入”
4. 是否接受“模型列表只允许设默认，不允许手工增删改”
5. 是否接受“底层保留 generic provider 结构，但前端新增页面专用 API”

## 13. 推荐结论

推荐团队通过以下方案：

- `Settings > 模型服务` 页面只支持 OpenAI
- 只支持 Codex auth / ChatGPT 登录
- 页面样式参考 Cherry，但收敛为单 provider 结构
- 全中文文案
- 不再暴露 API Key 与第三方渠道
- 第一阶段保留底层 generic provider 实现，只改前端产品层和页面专用接口

这条路径交付最快、风险最低，也最符合你现在的产品目标。

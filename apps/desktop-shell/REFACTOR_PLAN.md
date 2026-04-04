# Warwolf 桌面端 UI 改造计划

## 目标

将当前 Warwolf 桌面端 UI 从 cherry-studio 风格改造为 Claude Code 桌面端风格。

---

## 一、当前状态 vs 目标状态逐区域对比

### 1. 顶栏（Top Bar）

**当前**：单行
```
[🔴🟡🟢] [🏠 首页] [📱 应用] [+] .................. [💬] [⚙️]
```
- 只有两个系统 Tab（首页/应用），带图标
- 右侧有主题切换和设置按钮
- 背景：`bg-muted/30`，单行 `h-10`

**目标**：双行
```
Row 1: [🔴🟡🟢] [Home] [Search] [Scheduled] [Dispatch] [Customize] [OpenClaw] [Settings] ...... [Code]
Row 2:          [New session ×] [+]
```
- Row 1 是功能导航栏，7 个文字按钮（无图标），右侧有 "Code" 按钮
- Row 2 是会话标签栏，显示打开的会话，可关闭，有 "+" 新建按钮
- 背景：暖米色/奶油色

**需要改动的文件**：
- `shell/TabBar.tsx` — 重写为双行布局
- `shell/TabItem.tsx` — 可能需要调整或新建 `SessionTab` 组件
- `store/slices/tabs.ts` — 移除 "首页"/"应用" 系统 Tab，改为导航由 homeSection 驱动
- `shell/AppShell.tsx` — 可能需要微调

**具体改动**：
1. TabBar 改为 flex-col 两行：
   - Row 1: 导航项直接用 button 渲染（不用 TabItem），NAV_ITEMS 数组对应 homeSection
   - Row 1 右侧: "Code" 按钮（打开/切换到 session 视图）
   - Row 2: 遍历可关闭的会话 Tab，加 "+" 按钮
2. SYSTEM_TABS 不再需要 "首页"/"应用" 两个固定 Tab，导航通过 dispatch(setHomeSection()) 实现
3. 移除右上角的主题切换和设置按钮（设置已在 Row 1 导航中）

---

### 2. 左侧边栏（Sidebar）

**当前**：
```
[+ New session]          ← 黑色按钮
Search                   ← 导航项
Scheduled
Dispatch
Customize
---
OpenClaw
Settings
---
All projects  [图标]
---
[更新横幅: Updated to latest]
[Relaunch 按钮]
[Warwolf / Desktop / LOCAL]
```

**目标**：
```
[+ New session]          ← 带图标的按钮
Search                   ← 导航项（带图标）
Scheduled
Dispatch
Customize
---
All projects  [图标]
---
TODAY                    ← 时间分组标题
  New session            ← 会话卡片（标题 + 预览文字）
  1. 仔细分析
  Analyze cross-platform...
---
[更新横幅: Updated to 1.569.0 / Relaunch to apply]
  [Relaunch 按钮]
[pumbaa / Max plan / Local 徽章]  ← 用户信息
```

**主要差异**：
- 目标中侧边栏没有 OpenClaw 和 Settings 导航项（它们在顶栏 Row 1 中）
- 会话列表有时间分组（TODAY）和卡片样式预览
- 底部账户信息显示用户名、计划类型、Local 徽章
- 更新横幅样式不同（带版本号、描述文字）

**需要改动的文件**：
- `features/workbench/HomePage.tsx` — 调整 PRIMARY_ITEMS/SECONDARY_ITEMS，改善会话列表样式

**具体改动**：
1. PRIMARY_ITEMS 保留 Search/Scheduled/Dispatch/Customize
2. 移除 SECONDARY_ITEMS（OpenClaw/Settings 已在顶栏中）
3. 会话列表改为卡片样式，每个卡片显示标题和预览文字
4. 底部账户区改为显示头像字母圆圈 + 用户名 + 计划 + Local 徽章

---

### 3. 内容区头部（Content Header）

**当前**：无独立头部。HomeOverview 有 "HOME" 标签 + "Claude Code style home workspace" 标题。

**目标**：
```
Warwolf                                    [Opus 4.1] [Local]
/Users/champion/Documents/develop/...
```
- 左侧：产品名 "Warwolf" + 项目路径
- 右侧：模型标签徽章 + 环境标签徽章
- 只在会话/对话视图中显示

**需要新建的文件**：
- `features/code/ContentHeader.tsx` — 新组件

**具体改动**：
1. 创建 ContentHeader 组件，接收 session 和 workbench 数据
2. 显示 "Warwolf" 标题、项目路径、模型徽章、环境徽章
3. 在 CodeTerminal 或 CodePage 中引入此组件

---

### 4. 消息气泡（Message Bubbles）

**当前**：
```
[👤 圆形头像] You
                消息文字（monospace）

[🤖 圆形头像] Claude
                消息文字（prose）
```
- 左侧小圆形头像 + 角色名称 + 文字内容
- 用户消息：紫色系头像
- 助手消息：主色系头像

**目标**：
```
┌─────────────────────────────────────────┐
│ USER                                     │
│ 1. 仔细分析                              │
└─────────────────────────────────────────┘

┌─────────────────────────────────────────┐
│ ASSISTANT                                │
│ We have enough context to start shaping  │
│ the desktop shell around the Rust...     │
└─────────────────────────────────────────┘
```
- 圆角卡片样式，有边框和浅色背景
- 卡片内部顶端有 "USER" / "ASSISTANT" 标签（大写、小字号、灰色）
- 卡片背景：用户消息略深的米色，助手消息略浅的白色
- 无头像图标，无 "You" / "Claude" 名称

**需要改动的文件**：
- `features/code/MessageItem.tsx` — 重写消息组件样式

**具体改动**：
1. UserMessage: 改为卡片布局，移除头像，添加 "USER" 标签
2. AssistantMessage: 改为卡片布局，移除头像，添加 "ASSISTANT" 标签
3. ToolUseMessage / ToolResultMessage: 保持折叠卡片风格但调整配色
4. 增加 CSS 变量控制消息卡片背景色

---

### 5. 输入栏（Input Bar）

**当前**：
```
┌──────────────────────────────────────┬──┐
│ Type your message...                  │➤│
└──────────────────────────────────────┴──┘
  Shift+Enter for new line       Enter to send
```
- 简单文本框 + 发送图标按钮
- 底部提示文字

**目标**：
```
┌──────────────────────────────────────────┐
│                                          │
│ Describe the next step for this desktop  │
│ implementation...                        │
│                                          │
└──────────────────────────────────────────┘
[Ask permissions]  [Local]              [Send ●]
```
- 更大的文本输入框（带圆角边框）
- 底部一行按钮：
  - 左侧: "Ask permissions" 按钮（普通样式）+ "Local" 按钮（普通样式）
  - 右侧: 橙色圆角 "Send" 按钮
- 无 "Shift+Enter" 提示文字

**需要改动的文件**：
- `features/code/InputBar.tsx` — 重写输入栏布局

**具体改动**：
1. 输入框改为独立的圆角大文本框，不用外层 border 包装
2. 下方按钮行：左侧 Ask permissions + Local，右侧 Send
3. Send 按钮使用 warwolf-orange 配色，圆角样式
4. 移除底部 "Shift+Enter" / "Enter to send" 提示

---

### 6. 状态栏（Status Bar）

**当前**：底部有 StatusBar 显示模型、权限、环境、token 计数。

**目标**：截图中没有单独的底部状态栏，模型/环境信息已在 ContentHeader 中。输入栏底部的 "Ask permissions" 和 "Local" 部分承担了原状态栏的功能。

**需要改动的文件**：
- `features/code/StatusBar.tsx` — 移除或隐藏
- `features/code/CodePage.tsx` — 不再渲染 StatusBar

---

### 7. 主题配色（Theme / Colors）

**当前**：shadcn 默认 zinc 灰色系
- 背景：纯白 oklch(1 0 0)
- 边框：灰色 oklch(0.922 0 0)
- 所有颜色无色相（hue=0），纯灰度

**目标**：暖奶油/米色调
- 背景：暖白/米色（有轻微黄/橙色相）
- 侧边栏：略深的奶油色
- 消息卡片：米色系
- 主强调色：橙色（Send 按钮、品牌色）
- 文字：暖灰/深棕色

**需要改动的文件**：
- `globals.css` — 修改所有 CSS 变量为暖色调

**具体改动**：
1. background / card / muted 等加入暖色相（hue ≈ 60-80）
2. 新增 warwolf-orange 系列变量用于强调色
3. 新增 msg-user-bg / msg-assistant-bg 变量用于消息卡片
4. dark 模式对应调整

---

## 二、改动文件清单

| 文件 | 操作 | 改动级别 |
|------|------|---------|
| `globals.css` | 修改 | 大 — 全部色值替换为暖色调 |
| `shell/TabBar.tsx` | 重写 | 大 — 单行改双行，导航逻辑变更 |
| `shell/TabItem.tsx` | 可选保留 | 小 — Row 2 会话Tab可复用或新建 |
| `store/slices/tabs.ts` | 修改 | 中 — SYSTEM_TABS 简化 |
| `features/code/ContentHeader.tsx` | 新建 | 中 — 新组件 |
| `features/code/MessageItem.tsx` | 重写 | 大 — 卡片样式替换 |
| `features/code/InputBar.tsx` | 重写 | 大 — 新布局新按钮 |
| `features/code/CodeTerminal.tsx` | 修改 | 小 — 引入 ContentHeader |
| `features/code/CodePage.tsx` | 修改 | 小 — 移除 StatusBar |
| `features/code/StatusBar.tsx` | 可选删除 | 小 — 不再使用 |
| `features/workbench/HomePage.tsx` | 修改 | 中 — 侧边栏调整 |
| `shell/AppShell.tsx` | 修改 | 小 — 可能需微调路由 |

---

## 三、实施顺序

建议从底层到外层，避免中间状态的冲突：

### Phase 1: 主题配色（基础层）
1. 修改 `globals.css`，替换为暖色调变量

### Phase 2: 顶栏重构（骨架层）
2. 修改 `store/slices/tabs.ts`，简化 SYSTEM_TABS
3. 重写 `shell/TabBar.tsx`，实现双行布局
4. 微调 `shell/AppShell.tsx` 如有需要

### Phase 3: 内容区组件（核心层）
5. 新建 `features/code/ContentHeader.tsx`
6. 重写 `features/code/MessageItem.tsx`，卡片样式
7. 重写 `features/code/InputBar.tsx`，新按钮布局
8. 修改 `features/code/CodeTerminal.tsx`，引入 ContentHeader
9. 修改 `features/code/CodePage.tsx`，移除 StatusBar

### Phase 4: 侧边栏优化（完善层）
10. 修改 `features/workbench/HomePage.tsx`，调整导航项和会话列表

### Phase 5: 验证和微调
11. 构建测试，检查所有页面
12. 微调间距、字号、颜色细节

---

## 四、关键设计决策

1. **导航机制**：顶栏 Row 1 的导航项不再是 Redux Tab，而是直接 dispatch `setHomeSection()`。只有会话 Tab（Row 2）走 Redux tabs 管理。

2. **"Code" 按钮**：点击后切换 homeSection 到 "session"，相当于进入代码会话视图。

3. **StatusBar 废弃**：模型/环境信息上移到 ContentHeader，权限信息下移到 InputBar 的 "Ask permissions" 按钮。

4. **消息样式**：从聊天应用风格（头像+气泡）改为文档/终端风格（标签+卡片），更符合 Claude Code 的专业定位。

5. **暖色主题**：所有 oklch 颜色加入 hue ≈ 60-80 的暖色相，保持足够的对比度。

---

## 五、风险分析与对策（Review Feedback 回应）

### 风险 1: 导航状态冲突 — HomeSection vs ActiveSession 互斥问题

**问题**：Row 1（homeSection 驱动）和 Row 2（session tabs 驱动）是两套独立状态。
当用户在 Row 1 点 "Search" 后又点击 Row 2 的会话标签，或反过来，两个状态会冲突：
顶栏显示 "Search" 高亮，但内容区显示的是某个 Session。

**解决方案：统一视图模式（ViewMode）**

在 `ui.ts` 中引入一个顶层 `viewMode` 状态，替代当前的隐式判断：

```typescript
type ViewMode =
  | { kind: "nav"; section: HomeSection }   // 功能页面（Search/Settings/...）
  | { kind: "session"; sessionId: string | null }  // 会话视图

interface UiState {
  viewMode: ViewMode;
  // ...其余保留
}
```

**状态切换规则**：
- 点击 Row 1 任意导航项 → `viewMode = { kind: "nav", section: "search" }`
  - Row 2 会话标签取消高亮（无 active），但保持显示
- 点击 Row 2 任意会话标签 → `viewMode = { kind: "session", sessionId: "xxx" }`
  - Row 1 高亮 "Home"（或不高亮任何项）
- 点击 Row 1 "Home" → `viewMode = { kind: "nav", section: "overview" }`
- 点击右上 "Code" 按钮 → `viewMode = { kind: "session", sessionId: null }`（新建或打开最近会话）

**互斥逻辑**：viewMode.kind 同一时刻只能是 "nav" 或 "session"，不会同时存在。
内容区根据 viewMode.kind 选择渲染哪个组件：

```typescript
// HomePage.tsx main content area
{viewMode.kind === "nav" ? (
  <NavPageRouter section={viewMode.section} />
) : (
  <CodePage sessionId={viewMode.sessionId} />
)}
```

**Row 1 高亮规则**：
- `viewMode.kind === "nav"` → 对应 section 的导航项高亮
- `viewMode.kind === "session"` → 无导航项高亮（或 "Home" 弱高亮）

**Row 2 高亮规则**：
- `viewMode.kind === "session"` → 对应 sessionId 的标签高亮
- `viewMode.kind === "nav"` → 无标签高亮

这确保了**在任意时刻，用户看到的顶栏状态和内容区始终一致**。

---

### 风险 2: 垂直空间侵占

**问题**：双行顶栏 + ContentHeader 在小屏上压缩对话空间。

**解决方案：Row 2 条件折叠**

```
Row 2 显示规则：
- viewMode.kind === "session" → 始终显示 Row 2
- viewMode.kind === "nav" → 隐藏 Row 2（因为用户在看功能页，不需要会话标签）
```

实际效果：
- 在 Search/Settings/Customize 等功能页 → 顶栏只有 1 行（40px），和当前一样
- 在会话视图 → 顶栏 2 行（40px + 36px = 76px），但同时没有底部 StatusBar（28px），净增仅 8px

ContentHeader 高度约 60px，但它替代了原 HomeOverview 中的 HOME 标题区域（约 180px），
所以实际上会话视图的可用空间反而更大。

**补充：极端小屏**
如果将来需要进一步优化，可以在 Row 1 上加 `overflow-x-auto` + 隐藏滚动条，
让导航项在极窄窗口下可水平滚动，避免换行。

---

### 风险 3: 消息可读性 — 无头像时长对话辨识度

**问题**：纯文字标签 USER/ASSISTANT 在长对话中视觉辨识度不如图标。

**解决方案：左侧 Accent Bar + 明度差强化**

每个消息卡片左侧加一条 3px 宽的竖线（Accent Bar）：
- USER 消息：暖灰色竖线 + 略深米色背景
- ASSISTANT 消息：橙色竖线 + 略浅白色背景

```
┌──┬─────────────────────────────────────┐
│▊ │ USER                                 │
│▊ │ 1. 仔细分析                          │
└──┴─────────────────────────────────────┘

┌──┬─────────────────────────────────────┐
│▊ │ ASSISTANT                            │
│▊ │ We have enough context to start...   │
└──┴─────────────────────────────────────┘
```

Accent Bar 提供持续的 **边缘锚定**，即使在快速滚动中也能立刻区分发言者。

**明度差保证**：
- msg-user-bg 和 msg-assistant-bg 在 oklch lightness 维度上至少保持 0.03（约 3%）的差距
- 同时在 chroma 维度上也略有差异（user 偏暖灰，assistant 偏纯白），提供双维度区分

**代码块处理**：
- 消息卡片内部如果包含代码块（```），代码块自带 terminal-bg 背景和 border，
  与卡片背景形成嵌套层次，不会和卡片边界混淆

---

### 风险 4: 主题系统深度 — shadcn 组件硬编码色值

**问题**：只改 CSS 变量不够，shadcn 组件（Popover/Command/Dialog）可能有硬编码白色。

**解决方案：双轨并行策略**

**不直接修改 globals.css 的默认值**。改为：

1. 在 globals.css 中新增 `.theme-warwolf` 类下的变量覆盖：

```css
.theme-warwolf {
  --color-background: oklch(0.97 0.01 80);
  --color-popover: oklch(0.96 0.012 75);
  /* ... 其余所有覆盖 ... */
}
```

2. 在 App.tsx 最外层容器加上 `className="theme-warwolf"`：

```tsx
<div className="theme-warwolf">
  <AppShell />
</div>
```

3. 这样做的好处：
   - **可切换**：移除 className 即回到默认 zinc 主题，方便 A/B 对比
   - **无侵入**：shadcn 组件的 Popover 等使用 `--color-popover` 变量，
     `.theme-warwolf` 覆盖了这个变量，所以弹窗也会变成暖色
   - **全覆盖**：只要组件用的是 CSS 变量（shadcn 规范），就一定被覆盖到

4. **Color Audit 清单**：在实施时逐一检查以下组件的背景是否走变量：
   - `components/ui/tooltip.tsx` → 用 `bg-popover` ✓
   - `components/ui/scroll-area.tsx` → 无背景 ✓
   - `components/ui/button.tsx` → 用 `bg-primary` / `bg-secondary` ✓
   - `components/ui/badge.tsx` → 用 `bg-secondary` ✓
   - `components/ui/input.tsx` → 用 `bg-transparent` / `border-input` ✓
   - 如果发现任何硬编码 `bg-white` / `bg-zinc-*`，在该组件内替换为变量

---

### 风险 5: 侧边栏数据依赖

**问题**：时间分组 + 会话预览需要 `lastMessagePreview` 和 `timestamp` 数据。

**现状确认**：
现有 `DesktopSessionSummary` 接口（`lib/tauri.ts`）已包含：
- `preview: string` — 会话预览文字 ✓
- `bucket: "today" | "yesterday" | "older"` — 时间分组 ✓
- `created_at: number` / `updated_at: number` — 时间戳 ✓

现有 `DesktopSessionSection` 接口已提供按时间分组的会话列表，
且 `HomePage.tsx` 中已有 `sessionSections` 渲染逻辑。

**结论**：**无需修改数据层**。现有 API 已经返回了所需的全部字段。
改动仅限于前端渲染样式（卡片 + 分组标题 + 预览文字显示）。

---

### 补充风险 6: Streaming 态进度反馈

**问题**：去掉 StatusBar 后，助手正在生成时的进度反馈不够明显。

**解决方案**：

1. **ContentHeader 添加 Streaming 指示器**：
   当 `turn_state === "running"` 时，在模型徽章旁显示一个脉冲动画点：
   ```
   Warwolf                          [Opus 4.1 ●] [Local]
   ```
   ● 为一个 CSS `animate-pulse` 的小圆点，颜色为 warwolf-orange。

2. **InputBar 状态变化**：
   - `isBusy=true` 时，输入框 placeholder 变为 "Thinking..."
   - Send 按钮变为 Stop 按钮（红色圆角），点击可中断生成
   - "Ask permissions" 按钮在 busy 时变灰不可点击

3. **消息列表底部**：保留现有的 "..." 跳动动画（CodeTerminal.tsx 中已有），
   作为最直接的流式反馈。

---

### 补充风险 7: 空状态（Empty State）

**问题**：没有任何 Session 打开时，Row 2 和内容区长什么样？

**解决方案**：

1. **Row 2 空状态**：
   显示一个不可关闭的占位标签 "New session"，点击即创建新会话：
   ```
   Row 2: [New session] [+]
   ```
   和目标截图一致（截图中就是这个状态）。

2. **内容区空状态**：
   当 `viewMode = { kind: "session", sessionId: null }` 时，
   显示 WelcomeScreen（已有，在 CodeTerminal.tsx 中），包含 4 个能力卡片。
   ContentHeader 正常显示（模型/环境从 workbench 默认值获取）。

---

### 补充风险 8: 响应式设计

**问题**：侧边栏在窄屏下如何处理？

**解决方案**：

1. **侧边栏折叠**：保留现有的 `sidebarOpen` 状态（ui.ts 中已有），
   在 CodePage 的 `SessionSidebar` 上加一个折叠按钮。
   折叠后侧边栏完全隐藏（不做图标模式，避免增加复杂度）。

2. **顶栏 Row 1 溢出**：
   7 个导航项 + Code 按钮在极窄窗口下可能溢出。
   给 Row 1 导航容器加 `overflow-x-auto scrollbar-none`，允许水平滚动。
   Tauri 配置的 `minWidth: 1180` 保证了正常情况不会溢出。

3. **不做移动端适配**：这是桌面应用，最小窗口 1180×760 已由 Tauri 配置保证。

---

## 六、深水区问题与对策（第二轮 Review）

### 深水区 1: 消息卡片对比度 — Accessibility 风险

**问题**：0.03（3%）明度差在某些高亮度或低质量显示器上几乎不可见。

**原方案缺陷**：仅依赖 Lightness 单一维度做区分。

**修正方案：明度 + 色相双维度区分**

不仅在明度上拉开差距，还在 Chroma（色度）和 Hue（色相）上做微妙偏移：

```
msg-user-bg:      oklch(0.93  0.018  75)   ← 偏暖灰/米色，chroma 较低
msg-assistant-bg:  oklch(0.96  0.008  60)   ← 偏冷白/象牙，chroma 更低且 hue 偏移
```

关键差异点：
- Lightness 差：0.03（明度）
- Chroma 差：0.01（色度）— 用户消息更"暖"，助手消息更"白净"
- Hue 差：15°（色相）— 用户 75°(黄橙)，助手 60°(黄) — 微妙但可感知

结合 Accent Bar，总共提供 **四个维度** 的视觉区分：
1. 背景明度差
2. 背景色相差
3. Accent Bar 颜色差（暖灰 vs 橙色）
4. 标签文字（USER vs ASSISTANT）

**验证方法**：实施后用 Chrome DevTools 的 "Rendering → Emulate vision deficiencies"
分别在 Protanopia/Deuteranopia/Tritanopia 模式下确认仍可区分。

---

### 深水区 2: Stop 按钮的中断逻辑

**问题**：Stop 按钮触发 UI 停止后，后端可能仍在占用资源生成 Token。

**完整中断链路设计**：

```
用户点击 Stop
    │
    ▼
InputBar.tsx: onStop() 回调
    │
    ▼
CodePage.tsx: cancelSession(sessionId)
    │
    ├──► 1. 立即设置本地 UI 状态：isCancelling = true
    │      → InputBar 显示 "Cancelling..." + 禁用按钮
    │      → 消息列表底部动画变为 "Stopping..."
    │
    ├──► 2. 发送取消请求到后端：
    │      POST /api/desktop/sessions/{id}/cancel
    │      → 后端关闭 SSE 流、中断 API 调用、清理 token 缓冲区
    │      → 返回 { cancelled: true, partial_response?: string }
    │
    └──► 3. 收到后端确认后：
           → turn_state 变为 "idle"
           → 如果有 partial_response，追加到消息列表（标记为 [interrupted]）
           → InputBar 恢复为 Send 模式
           → isCancelling = false
```

**需要新增的 API**：
- `lib/tauri.ts`：新增 `cancelSession(sessionId: string)` 函数
- 对应后端 endpoint（如果 Rust runtime 尚未支持，先做前端 stub：
  直接关闭 EventSource 连接 + 本地 reset turn_state）

**降级策略（后端未实现时）**：
```typescript
async function cancelSession(sessionId: string) {
  // 关闭 SSE 连接（已有的 dispose 函数）
  // 本地重置 session 的 turn_state
  // 追加一条 system message: "Generation interrupted by user"
}
```

**关键**：Stop 绝不能只改 UI 状态。至少要关闭 EventSource 连接，
否则后端产生的 token 仍会通过 SSE 推送过来，造成 UI "停了但消息还在蹦" 的怪异现象。

---

### 深水区 3: 顶部三层横条的"呼吸感"

**问题**：Row 1 + Row 2 + ContentHeader 三个条状组件堆叠，视觉头重脚轻。

**解决方案：分层 Elevation 系统**

三层组件使用不同的视觉分隔手段，形成层级递进：

```
┌─────────────────────────────────────────────────────┐
│ Row 1: Home  Search  Scheduled  ...         [Code]  │ ← 纯背景色，无阴影
├─────────────────────────────────────────────────────┤ ← 1px border-b（已有）
│ Row 2: [New session ×]  [+]                         │ ← 纯背景色，无阴影
╘═════════════════════════════════════════════════════╛ ← 底部 box-shadow: 0 1px 3px rgba(0,0,0,0.06)
│ ContentHeader: Warwolf           [Opus 4.1] [Local] │ ← 无背景色，透明融入内容区
│ /Users/champion/Documents/...                       │
│─────────────────────────────────────────────────────│ ← 无分隔线，直接过渡到消息列表
│ [消息列表]                                           │
```

具体实现：

1. **Row 1 → Row 2**：用 1px `border-b border-border/50`（半透明边框）分隔
   - 颜色极淡，暗示"同一区域的两部分"

2. **Row 2 底部**：加 `shadow-sm`（Tailwind 内置：`0 1px 2px 0 rgb(0 0 0 / 0.05)`）
   - 这个阴影将整个顶栏"浮起"，和下方内容区形成层级差
   - 这是关键的 Elevation 分割点

3. **ContentHeader**：**不加** 底部边框或阴影
   - ContentHeader 背景色和内容区一致（都是 `bg-background`）
   - 它视觉上是"内容区的一部分"，不是第三个横条
   - 和消息列表之间靠 padding（`pb-3`）自然过渡

效果：视觉上只有"两层" —— **浮起的顶栏**和**沉底的内容区**。
ContentHeader 融入内容区，不形成独立的第三层，解决头重脚轻的问题。

---

## 七、修订后的实施顺序

### Phase 0: 状态层准备
0. 修改 `store/slices/ui.ts`：引入 `ViewMode` 类型，替换 `homeSection` + `activeHomeSessionId`
   - 新增 `setViewMode` action
   - 保持向后兼容：`homeSection` 和 `activeHomeSessionId` 改为从 `viewMode` 派生的 getter
1. 修改 `features/workbench/tab-helpers.ts`：适配 ViewMode

### Phase 1: 主题配色（基础层）
2. 修改 `globals.css`：新增 `.theme-warwolf` 变量覆盖（不删除默认值）
   - 包含深水区 1 的双维度消息背景色（明度 + 色相偏移）
3. 修改 `App.tsx`：最外层加 `theme-warwolf` 类
4. Color Audit：检查所有 `components/ui/*.tsx` 确认无硬编码色值

### Phase 2: 顶栏重构（骨架层）
5. 修改 `store/slices/tabs.ts`：SYSTEM_TABS 简化
6. 重写 `shell/TabBar.tsx`：
   - 双行布局 + ViewMode 集成 + Row 2 条件折叠
   - Row 1→Row 2：`border-b border-border/50`
   - Row 2 底部：`shadow-sm`（深水区 3 Elevation 分层）
7. 微调 `shell/AppShell.tsx`

### Phase 3: 内容区组件（核心层）
8. 新建 `features/code/ContentHeader.tsx`：
   - 含 streaming 脉冲指示器
   - 无底部边框/阴影（融入内容区，深水区 3）
9. 重写 `features/code/MessageItem.tsx`：
   - 卡片样式 + 左侧 Accent Bar
   - 使用双维度背景色（深水区 1）
   - Protanopia/Deuteranopia 可区分
10. 重写 `features/code/InputBar.tsx`：
    - Ask permissions + Local + orange Send
    - Send ↔ Stop 状态切换（深水区 2）
    - Stop 触发：关闭 EventSource + 重置 turn_state + 追加中断消息
11. 修改 `lib/tauri.ts`：新增 `cancelSession()` 函数（降级 stub）
12. 修改 `features/code/CodeTerminal.tsx`：引入 ContentHeader
13. 修改 `features/code/CodePage.tsx`：移除 StatusBar，接入 ViewMode，传递 onCancel

### Phase 4: 侧边栏优化（完善层）
14. 修改 `features/workbench/HomePage.tsx`：
    - 移除 SECONDARY_ITEMS
    - 接入 ViewMode
    - 主内容区根据 viewMode.kind 切换渲染

### Phase 5: 验证和微调
15. 构建测试，检查所有页面
16. Accessibility 验证：Chrome DevTools vision deficiency emulation
17. 对比截图微调间距、字号、颜色
18. 极端场景测试：空状态、streaming 态、Stop 中断、长对话滚动、窗口缩放

---

## 八、改动文件完整清单（终版）

| # | 文件 | 操作 | Phase | 关键改动点 |
|---|------|------|-------|-----------|
| 0 | `store/slices/ui.ts` | 修改 | 0 | ViewMode 类型 + setViewMode action |
| 1 | `features/workbench/tab-helpers.ts` | 修改 | 0 | 适配 ViewMode |
| 2 | `globals.css` | 修改 | 1 | `.theme-warwolf` 变量覆盖 + 双维度消息色 |
| 3 | `App.tsx` | 修改 | 1 | 加 `theme-warwolf` 类 |
| 4 | `components/ui/*.tsx` | 审查 | 1 | Color Audit |
| 5 | `store/slices/tabs.ts` | 修改 | 2 | SYSTEM_TABS 简化 |
| 6 | `shell/TabBar.tsx` | 重写 | 2 | 双行 + ViewMode + Elevation |
| 7 | `shell/AppShell.tsx` | 微调 | 2 | 路由适配 |
| 8 | `features/code/ContentHeader.tsx` | 新建 | 3 | 标题 + 徽章 + streaming 指示器 |
| 9 | `features/code/MessageItem.tsx` | 重写 | 3 | 卡片 + Accent Bar + 双维度色 |
| 10 | `features/code/InputBar.tsx` | 重写 | 3 | 新布局 + Send/Stop 切换 |
| 11 | `lib/tauri.ts` | 新增函数 | 3 | cancelSession() stub |
| 12 | `features/code/CodeTerminal.tsx` | 修改 | 3 | 引入 ContentHeader |
| 13 | `features/code/CodePage.tsx` | 修改 | 3 | 移除 StatusBar + ViewMode + onCancel |
| 14 | `features/workbench/HomePage.tsx` | 修改 | 4 | 移除 SECONDARY + ViewMode 切换 |
| — | `features/code/StatusBar.tsx` | 保留不删 | — | 不渲染但保留文件，未来可复用 |
| — | `shell/TabItem.tsx` | 保留 | — | Row 2 可复用其逻辑 |

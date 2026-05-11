# Buddy 信息密度与视觉层次规范

**Status**: Draft v1 · 待评审
**Date**: 2026-05-11
**Author**: 基于 v0.1.14 实物 5 个 surface 截图诊断
**Replaces**: ad-hoc 各 feature 自己定的字号 / 颜色 / 折叠规则

---

## 0. 为什么要这份规范

E1-E22 的工作几乎都在数据 / 业务逻辑层。前端则是「每个 feature 加一块 UI 进来，找个看着合理的位置塞进去」——久了所有页面都堆成报表，**用户每次进来都要扫一大片才知道现在能做什么**。

v0.1.14 实物诊断结论（详见 audit notes）：

| Page | 密度 | 主要病因 |
|------|------|---------|
| Home (Pulse) | 🔴 严重 | 18 个 0 数据 tile + 大标题 + 多个 section 同等权重 |
| Ask 聊天 | 🟡 中等 | composer 11 元素一行 + status bar 8 元素 |
| Knowledge | 🟢 OK | (空态) |
| Drafts | 🟢 OK | — |
| Settings | 🟢 OK | 这是参考标准 |

这份规范把"什么该突出 / 什么该藏起来"写成可被 PR review 拒掉的规则。

---

## 1. 三层视觉等级 — Primary / Secondary / Tertiary

每个屏幕的每个元素必须能被分到三档之一。**不允许"暧昧第二档"**。

### 1.1 Primary —「用户来这页就为了它」
- **定义**: 这个 surface 唯一的核心动作或核心阅读对象。
- **数量约束**: 每个 surface **最多 1 个 primary 区域**（不是单个元素，是一个区域）。
- **视觉规则**:
  - 字号 ≥ 14px
  - 颜色 `var(--color-foreground)` (full opacity)
  - 如果是 button：`bg-primary text-primary-foreground`
  - 如果是 input：边框完整、占据视觉中心
  - **永远首屏可见**（不可滚动到才出现）
- **每页 primary 列表**:

| Page | Primary |
|------|---------|
| Home (Pulse) | "Top 3 建议动作" 列表（不是 stat cards） |
| Ask 聊天 | 输入框 + 发送 |
| Knowledge | 页面列表（用 search/filter 收窄后第一屏） |
| WikiArticle | body markdown |
| Drafts list | "新建草稿" CTA + 草稿列表 |
| DraftEditor | CodeMirror 编辑器 |
| Settings | 当前选中的 section 内容 |

### 1.2 Secondary —「需要时一眼能看到」
- **定义**: 不是用户主要意图，但属于"完成主要意图过程中可能要参考的信息"。
- **视觉规则**:
  - 字号 11-13px
  - 颜色 `var(--color-muted-foreground)`
  - 紧邻 primary 区域
  - 文字标签 OK，但不要 emoji icon + 文字 + ❓ 三件套（选一个）
- **每页 secondary 例子**:

| Page | Secondary |
|------|-----------|
| Home | 3 个 stat 行 (待审 / 质量 / Git) |
| Ask | model selector / context size / session token count |
| Knowledge | 筛选条件、page count |
| WikiArticle | metadata 行 (category / source_refs / verdict picker) |
| Drafts | target select + save status |

### 1.3 Tertiary —「需要时点开才出现」
- **定义**: 高级开关、debug info、罕用状态、批量操作。
- **视觉规则**:
  - **首屏不可直接展开**——必须藏在 `[···]` button / dropdown / expandable section 后
  - 展开后 max-height 限制（不挤掉 primary）
  - 点击 outside 自动收起
- **每页 tertiary 例子**:

| Page | Tertiary |
|------|----------|
| Home | "本周目的的流动" 6 个 purpose tile，"减熵脉冲" 完整面板，"投入回报"完整面板，"外部 AI 权限"管理 |
| Ask | composer 高级 pill (代码 / 计划 / 需要确认 / 继续前文 / 目的)，conversation navigator dots |
| WikiArticle | source_refs / expressed_in chip 完整列表 (默认收成 "+N refs" 折叠)，verdict picker popover |
| Settings | 开发者高级选项 (已经做对) |

### 1.4 反模式（PR review 直接 block）

❌ 一个 surface 里有 2 个或更多区域都按 primary 视觉处理——典型表现：3 个 stat card + 6 个 purpose tile + 3 个 funnel block 全用相同字号 + 相同 padding + 相同边框。

❌ 高级开关跟基础控件 inline 同等视觉处理——典型表现：composer 上 [代码] [计划] [需要确认] 跟 [发送] 颜色一样、大小一样。

❌ "0 数据 tile" 用 "正常 ✓" 绿色徽章——0 不是正常，是没数据。要么不显示（空态），要么用 muted 灰色。

❌ 同一段说明文案在一页内重复 3 次以上（Home 的"本周暂无新增, 可从已有 N 页继续提炼"出现 6 次）。

---

## 2. 字号阶梯 (Typography Scale)

抛弃当前各 page 自由发挥的字号，统一为 5 档：

| 角色 | 字号 (px) | 字重 | 颜色 | 用例 |
|------|----------|------|------|------|
| `text-page-title` | 20 | 600 | foreground | 页面顶部标题（取代当前 28-32px） |
| `text-section-title` | 14 | 600 | foreground | section header |
| `text-body` | 13 | 400 | foreground | 正文 / 按钮 |
| `text-meta` | 12 | 400 | muted-foreground | secondary 信息、metadata |
| `text-micro` | 11 | 400 | muted-foreground @ 0.7 | tertiary 信息、tooltip target |

**LR-1 复述**: 字号必须用 Tailwind class（`text-[20px]` 或 `text-base` 等），不要 inline `style={{ fontSize }}`。

**Page header 高度上限**: 80px（包含标题 + 副标题 + padding）。当前 Home 的 header 约 110px——超标。

---

## 3. 折叠规则 (Collapse Rules)

### 3.1 默认折叠的判定

写新 UI 时如果以下任意一条成立，**默认折叠**（隐藏在 `[···]` 或 `<details>` 后）：

1. **rarely-used**: 一周内被 < 10% 用户点击的功能（advanced toggles）
2. **all-zero**: section 内所有数值都是 0 / 空数组
3. **N+ items**: 列表项超过 5 个（chip / tag / link 等）显示前 5 个 + "+N more"
4. **重复说明**: 同一页内重复 3 次以上的描述文字

### 3.2 折叠交互模式

| 场景 | 控件 | 例子 |
|------|------|------|
| 复杂选项组 | popover button `[···]` | composer 高级 toggle |
| 长 chip 列表 | inline truncate + `+N` 链接 | wiki page source_refs |
| 数据 section | `<details>` 或自定义 collapsible | Home 的 "本周流动" 折叠到 section header |
| 单字段长文本 | `<details>` 内嵌 `<pre>` | health check 错误详情（已实现） |

### 3.3 reverse rule — 不该折叠的

❌ 不要把"primary 动作"折叠（永远 visible）
❌ 不要折叠当前 session 的关键 status (例：streaming 状态、错误 banner)

---

## 4. 状态栏 (Status Bar) 规范

**问题**: 当前底部 status bar 7-8 个元素同等权重，混合了 vault 全局 / session 状态 / 待办 三类信息。

### 4.1 新规范

```
┌─────────────────────────────────────────────────────────────────────┐
│ vault group              │           session group                  │
│ 📥 13 · 🔀 27 · 🎯 13   │    🛡 · 🤖 · ⏱ session · 23/28          │
└─────────────────────────────────────────────────────────────────────┘
```

- **左半**: vault 全局待办 (Inbox / Git / 待处理) — icon + count，hover 显文字
- **右半**: 当前 session 状态 (permission / mode / context / pages count) — icon-only，hover 显文字
- **中间 separator** (一根 1px 竖线)
- **全部 icon-only 默认**，hover 才显文字
- **总高度** ≤ 28px

### 4.2 共享组件
新建 `<StatusBar>` 组件，所有页面从中导入。每个 item 的 `<StatusBarItem>` 强制要求：
- `icon` (Lucide)
- `label` (string, 显示在 tooltip)
- `value?` (number, 可选 count)
- `tone?` ("default" | "warning" | "danger") — 触发不同颜色

---

## 5. 左侧 rail 规范

### 5.1 当前状态
8+ icons 平铺，无分组：B avatar / Home / Chat / Inbox / Books / Documents / Drafts / Sigma / Connections / 底部 Settings。

### 5.2 新规范

```
┌──────┐
│  B   │  avatar
├──────┤
│  🏠  │  ─ 主要工作 ─
│  💬  │   每天必走的路径
│  📥  │
│  📚  │
│  📝  │  Drafts
├──────┤  separator (1px)
│  Σ   │  ─ 进阶 ─
│  🔗  │   偶尔进
│  📄  │
├──────┤  flex-grow spacer
│  ⚙   │  ─ 系统 ─（底部固定）
└──────┘
```

- 用 `<hr>` 1px separator (`border-border/50`)
- "主要工作" 5 个 icon 默认 high-contrast
- "进阶" 3 个 icon 默认 muted (opacity 0.6)，hover full
- 系统 (Settings) 永远在底部 (margin-top: auto)

---

## 6. 各 surface 的应用 (Per-Page Application)

每页明确列出"哪些元素应该上提到 primary, 哪些应该下放到 secondary/tertiary, 哪些应该折叠"。

### 6.1 Home (Pulse) — E23 改造对象

**当前**: 6 purpose tile + 3 stat card + alert + 减熵 + 投入回报 + Top 3 + 外部 AI 权限 全部 visible 同等权重。

**重排**:

| 区域 | 当前等级 | 目标等级 | 改动 |
|------|---------|---------|------|
| 大标题 + 副标题 | (~110px 高) | secondary | 缩到 60-80px，副标题 12px muted |
| "审阅 13 条" CTA | primary | primary | 保留（但移到与标题同行） |
| 3 个 stat card | primary | secondary | 缩成一行 inline stats（icon + 数 + label）, 总高 ≤ 60px |
| **Top 3 建议动作** | tertiary（在底部） | **primary** | 上提到首屏，每条占独立 row, 大字号 + arrow |
| 本周目的的流动 6 tile | primary | tertiary | 默认折叠到 section header, expand 后只显示**有非零数据**的 tile, 其它合并 "+N purposes 本周静默" |
| 减熵脉冲 完整面板 | primary | tertiary | 折叠到 section header |
| 投入回报 完整面板 | primary | tertiary | 折叠到 section header |
| 外部 AI 权限 | primary | tertiary | 折叠到 section header |
| "21 页缺少 purpose lens" alert | secondary | secondary | 保留位置但缩高 |

**预期效果**: 进 Home 第一眼看到的是「Top 3 行动 + 13 条审阅 CTA」, 滚一屏才看见详细数据。

### 6.2 Ask 聊天 — E24 改造对象

**当前**: composer 一行 11 元素 + status bar 8 元素。

**重排**:

| 区域 | 当前 | 目标 | 改动 |
|------|------|------|------|
| 输入框 | primary | primary | 保留 |
| 📎 attachment | secondary | secondary | 保留 |
| 代码 / 计划 / 需要确认 / 继续前文 / 目的 (5 pills + 5 ❓) | secondary 同行 | tertiary 折叠 | 全部塞进 `[···]` popover button, 每个选项有 inline tooltip 不需要 ❓ |
| Enter 提示文字 | secondary | tertiary | 移到 popover 里, 或干脆删（用户输入两次就懂） |
| Model selector | secondary | secondary | 保留 |
| 发送 button | primary | primary | 保留 |
| Conversation navigator dots | tertiary | tertiary | 保留 (E19 已经做对) |
| Top 状态 "● 已完成" | secondary | secondary | 保留 |
| Top 右侧 "用时 / token" | tertiary | tertiary | 默认隐藏，hover top bar 才显示 |

**预期效果**: composer 视觉上只剩 `📎 [输入框 .....] [···] [Model▾] [→]` 5 个元素, 高级开关需要时点 `···`。

### 6.3 Knowledge / WikiArticle — E27 改造对象 (defer)

**当前**: 看不到（空态）。预期 23 页有数据时可能有 list density 问题。
**Defer**: 等填进数据再看。

### 6.4 Drafts — E27 改造对象 (defer)

**当前**: OK。
**只改一点**: 顶部 2 行说明在 0 草稿空态时显示, 有 ≥1 草稿时折叠到 (?) 提示 icon。

### 6.5 Settings — 已是参考标准

**不改**。Settings 的「左 group nav + 右 section content + 每个 row 字段精简」是 buddy 内最干净的页面，其它页面应该向它看齐。

---

## 7. 实施 sprint：E23-E27

按这份规范, 一次性 sprint：

| Epic | 内容 | 估时 | shipped 后用户感知 |
|------|------|------|------------------|
| **E23 — Home Pulse 重构** | §6.1 全部 | 4-6 hr | 极高 |
| **E24 — Composer 简化** | §6.2 composer 部分 | 1-2 hr | 高 |
| **E25 — Status Bar 重构** | §4 + 共享组件 | 2-3 hr | 中 |
| **E26 — Left Rail 分段** | §5 | 0.5-1 hr | 低 |
| **E27 — Typography 阶梯落地** | §2 在 globals.css 加 5 个 class，逐 page 替换 inline | 2-3 hr | 中（潜移默化） |

总时长 10-15 hr, 一个 sprint 完成。**E27 必须最先做**——其它 Epic 用到字号都要从 E27 的 token 引用。

---

## 8. 完成判定 (Done Criteria)

E23-E27 全部 ship 后, 由人手动跑一遍：

1. 进 Home 后能在 5 秒内说出"我现在要做的 3 件事是什么"
2. 进 Ask 后输入框立刻可见，眼睛不用扫过任何"开关"
3. 任意页面 status bar 都是同样的 8 个 icon (icon-only), hover 才出文字
4. 字号 grep `style={{\s*fontSize`: 0 命中
5. 字号 grep `text-\[`: 只有少数 special case 命中（不是页面级元素）

---

## 9. 不属于这份规范的事 (Out of Scope)

- **Token 颜色调整** — 现有 terracotta + cream 配色不动
- **新增 icon 设计** — 用现有 lucide-react
- **响应式 / 移动端** — buddy 是桌面应用, 不考虑窄屏
- **国际化文案** — 所有规则用中英混排示例, 不锁定语言
- **可达性 (a11y)** — `aria-label` / `role` 应该已经在组件里了, 这份规范不重复
- **动画 / transition** — 不属于密度问题, 后续单独规范

---

## 10. 评审清单

请回答这几个问题, 决定是否锁这版规范：

- [ ] Top 3 建议动作上提到 Home 首屏 — 同意？
- [ ] composer 5 个高级 pill 折叠到 `[···]` — 同意？还是想保留某几个常用的（如"目的"）？
- [ ] status bar 全部 icon-only + hover label — 同意？还是怕用户找不到？
- [ ] 字号最大 20px (取代当前 28-32px) — 同意？还是想 24px 折中？
- [ ] left rail 分 3 段 (主要 / 进阶 / 系统) — 同意？或者主要-系统两段就够？

回答完上面 5 个问题之后, 我把这版 spec 锁定 v1, 然后写 E23-E27 一次性 sprint plan。

# Warwolf Desktop Design Token 规范

> 基于 Claude Code v2.1.88 `src/utils/theme.ts` 逆向提取
> 原始定义：89 个语义化 token，6 套主题变体（light/dark × normal/daltonized/ansi）

---

## 一、品牌核心色

Claude Code 的品牌色体系以 **橙色（Claude Orange）** 为核心，配合 **蓝紫色** 做功能强调。

| Token | Light 模式 | Dark 模式 | 语义 |
|-------|-----------|----------|------|
| `claude` | `rgb(215,119,87)` | `rgb(215,119,87)` | **品牌主色** — 两个模式完全一致 |
| `claudeShimmer` | `rgb(245,149,117)` | `rgb(235,159,127)` | 品牌色浅版（动画/闪烁） |
| `permission` | `rgb(87,105,247)` | `rgb(177,185,249)` | 权限蓝（请求授权） |
| `permissionShimmer` | `rgb(137,155,255)` | `rgb(207,215,255)` | 权限蓝浅版 |
| `suggestion` | `rgb(87,105,247)` | `rgb(177,185,249)` | 建议/提示蓝 |
| `fastMode` | `rgb(255,106,0)` | `rgb(255,120,20)` | 快速模式橙 |
| `fastModeShimmer` | `rgb(255,150,50)` | `rgb(255,165,70)` | 快速模式浅版 |

**关键发现**：`claude: rgb(215,119,87)` 即 `#D77757` 是 Claude 品牌橙，**两种模式完全相同**。
转换为 CSS：`--color-claude-orange: oklch(0.62 0.12 46)`

---

## 二、语义反馈色

| Token | Light | Dark | 语义 |
|-------|-------|------|------|
| `success` | `rgb(44,122,57)` | `rgb(78,186,101)` | 成功/通过 |
| `error` | `rgb(171,43,63)` | `rgb(255,107,128)` | 错误/失败 |
| `warning` | `rgb(150,108,30)` | `rgb(255,193,7)` | 警告/注意 |
| `warningShimmer` | `rgb(200,158,80)` | `rgb(255,223,57)` | 警告浅版 |
| `merged` | `rgb(135,0,255)` | `rgb(175,135,255)` | 合并（紫色） |
| `autoAccept` | `rgb(135,0,255)` | `rgb(175,135,255)` | 自动接受（紫色） |

---

## 三、文字与基础色

| Token | Light | Dark | 语义 |
|-------|-------|------|------|
| `text` | `rgb(0,0,0)` | `rgb(255,255,255)` | **主文字色** |
| `inverseText` | `rgb(255,255,255)` | `rgb(0,0,0)` | 反色文字 |
| `inactive` | `rgb(102,102,102)` | `rgb(153,153,153)` | 非活跃/禁用 |
| `inactiveShimmer` | `rgb(142,142,142)` | `rgb(193,193,193)` | 非活跃浅版 |
| `subtle` | `rgb(175,175,175)` | `rgb(80,80,80)` | 次要/暗淡文字 |
| `remember` | `rgb(0,0,255)` | `rgb(177,185,249)` | 记忆/上下文 |

---

## 四、界面背景与消息色（桌面端核心）

| Token | Light | Dark | 语义 |
|-------|-------|------|------|
| `background` | `rgb(0,153,153)` | `rgb(0,204,204)` | 背景强调（青色） |
| `userMessageBackground` | `rgb(240,240,240)` | `rgb(55,55,55)` | **用户消息背景** |
| `userMessageBackgroundHover` | `rgb(252,252,252)` | `rgb(70,70,70)` | 用户消息 hover |
| `messageActionsBackground` | `rgb(232,236,244)` | `rgb(44,50,62)` | 消息操作栏背景（冷灰偏蓝） |
| `selectionBg` | `rgb(180,213,255)` | `rgb(38,79,120)` | 文本选中背景 |
| `bashMessageBackgroundColor` | `rgb(250,245,250)` | `rgb(65,60,65)` | Bash 输出背景 |
| `memoryBackgroundColor` | `rgb(230,245,250)` | `rgb(55,65,70)` | 记忆/上下文背景 |

---

## 五、边框与装饰色

| Token | Light | Dark | 语义 |
|-------|-------|------|------|
| `promptBorder` | `rgb(153,153,153)` | `rgb(136,136,136)` | 输入框边框 |
| `promptBorderShimmer` | `rgb(183,183,183)` | `rgb(166,166,166)` | 输入框边框浅版 |
| `bashBorder` | `rgb(255,0,135)` | `rgb(253,93,177)` | Bash 区域边框（粉色） |
| `planMode` | `rgb(0,102,102)` | `rgb(72,150,140)` | 计划模式（青绿色） |
| `ide` | `rgb(71,130,200)` | `rgb(71,130,200)` | IDE 强调色 |

---

## 六、Diff 色（代码对比）

| Token | Light | Dark | 语义 |
|-------|-------|------|------|
| `diffAdded` | `rgb(105,219,124)` | `rgb(34,92,43)` | 新增行背景 |
| `diffRemoved` | `rgb(255,168,180)` | `rgb(122,41,54)` | 删除行背景 |
| `diffAddedDimmed` | `rgb(199,225,203)` | `rgb(71,88,74)` | 新增行暗淡 |
| `diffRemovedDimmed` | `rgb(253,210,216)` | `rgb(105,72,77)` | 删除行暗淡 |
| `diffAddedWord` | `rgb(47,157,68)` | `rgb(56,166,96)` | 新增词高亮 |
| `diffRemovedWord` | `rgb(209,69,75)` | `rgb(179,89,107)` | 删除词高亮 |

---

## 七、Agent/子代理色（8 色）

| Token | 值（通用） | 语义 |
|-------|-----------|------|
| `red_FOR_SUBAGENTS_ONLY` | `rgb(220,38,38)` | Red 600 |
| `blue_FOR_SUBAGENTS_ONLY` | `rgb(37,99,235)` | Blue 600 |
| `green_FOR_SUBAGENTS_ONLY` | `rgb(22,163,74)` | Green 600 |
| `yellow_FOR_SUBAGENTS_ONLY` | `rgb(202,138,4)` | Yellow 600 |
| `purple_FOR_SUBAGENTS_ONLY` | `rgb(147,51,234)` | Purple 600 |
| `orange_FOR_SUBAGENTS_ONLY` | `rgb(234,88,12)` | Orange 600 |
| `pink_FOR_SUBAGENTS_ONLY` | `rgb(219,39,119)` | Pink 600 |
| `cyan_FOR_SUBAGENTS_ONLY` | `rgb(8,145,178)` | Cyan 600 |

---

## 八、标签色

| Token | Light | Dark | 语义 |
|-------|-------|------|------|
| `briefLabelYou` | `rgb(37,99,235)` | `rgb(122,180,232)` | "You" 标签（蓝色） |
| `briefLabelClaude` | `rgb(215,119,87)` | `rgb(215,119,87)` | "Claude" 标签（品牌橙） |

---

## 九、彩虹色（ultrathink 关键词高亮，14 色）

| Token | 值 |
|-------|-----|
| `rainbow_red` | `rgb(235,95,87)` |
| `rainbow_orange` | `rgb(245,139,87)` |
| `rainbow_yellow` | `rgb(250,195,95)` |
| `rainbow_green` | `rgb(145,200,130)` |
| `rainbow_blue` | `rgb(130,170,220)` |
| `rainbow_indigo` | `rgb(155,130,200)` |
| `rainbow_violet` | `rgb(200,130,180)` |
| Shimmer 版本 | 每色各有一个 `_shimmer` 变体，明度更高 |

---

## 十、其他功能色

| Token | Light | Dark | 语义 |
|-------|-------|------|------|
| `professionalBlue` | `rgb(106,155,204)` | `rgb(106,155,204)` | Grove 主题蓝 |
| `chromeYellow` | `rgb(251,188,4)` | `rgb(251,188,4)` | Chrome 品牌黄 |
| `clawd_body` | `rgb(215,119,87)` | `rgb(215,119,87)` | Clawd 助手颜色 |
| `clawd_background` | `rgb(0,0,0)` | `rgb(0,0,0)` | Clawd 背景 |
| `rate_limit_fill` | `rgb(87,105,247)` | `rgb(177,185,249)` | 速率限制填充 |
| `rate_limit_empty` | `rgb(39,47,111)` | `rgb(80,83,112)` | 速率限制空白 |

---

## 十一、UI 图标/符号（figures.ts）

| 常量 | 符号 | 用途 |
|------|------|------|
| `BLACK_CIRCLE` | `⏺` (macOS) / `●` (Win/Linux) | 状态指示器 |
| `BULLET_OPERATOR` | `∙` | 列表项 |
| `LIGHTNING_BOLT` | `↯` | 快速模式标识 |
| `EFFORT_LOW/MEDIUM/HIGH/MAX` | `○ ◐ ● ◉` | 努力级别 |
| `PLAY_ICON` / `PAUSE_ICON` | `▶` / `⏸` | 播放/暂停 |
| `BLOCKQUOTE_BAR` | `▎` | 引用条（左侧细竖线） |
| `DIAMOND_OPEN/FILLED` | `◇` / `◆` | 审查状态 |
| `FLAG_ICON` | `⚑` | Issue 标记 |

---

## 十二、主题架构

```
ThemeSetting (用户配置)
  ├── 'auto'  → 根据系统深浅色自动选择
  ├── 'dark'
  ├── 'light'
  ├── 'dark-daltonized'   (色盲友好)
  ├── 'light-daltonized'  (色盲友好)
  ├── 'dark-ansi'         (16 色终端)
  └── 'light-ansi'        (16 色终端)

系统主题检测:
  ├── OSC 11 查询终端背景色
  ├── $COLORFGBG 环境变量
  └── ITU-R BT.709 亮度公式 (> 0.5 = light)
```

---

## 十三、Design Token → CSS 变量映射方案

以下是将 Claude Code 终端 design token 转换为 Web CSS 变量的建议映射：

### 核心映射

```css
:root {
  /* ── Brand ── */
  --claude-orange:           rgb(215, 119, 87);   /* claude */
  --claude-orange-shimmer:   rgb(245, 149, 117);  /* claudeShimmer */
  --claude-blue:             rgb(87, 105, 247);   /* permission / suggestion */
  --claude-blue-shimmer:     rgb(137, 155, 255);  /* permissionShimmer */

  /* ── Semantic ── */
  --color-success:           rgb(44, 122, 57);    /* success */
  --color-error:             rgb(171, 43, 63);    /* error */
  --color-warning:           rgb(150, 108, 30);   /* warning */

  /* ── Text ── */
  --color-text-primary:      rgb(0, 0, 0);        /* text */
  --color-text-inverse:      rgb(255, 255, 255);  /* inverseText */
  --color-text-inactive:     rgb(102, 102, 102);  /* inactive */
  --color-text-subtle:       rgb(175, 175, 175);  /* subtle */

  /* ── Message backgrounds ── */
  --color-msg-user-bg:       rgb(240, 240, 240);  /* userMessageBackground */
  --color-msg-user-bg-hover: rgb(252, 252, 252);  /* userMessageBackgroundHover */
  --color-msg-actions-bg:    rgb(232, 236, 244);  /* messageActionsBackground */
  --color-msg-bash-bg:       rgb(250, 245, 250);  /* bashMessageBackgroundColor */
  --color-msg-memory-bg:     rgb(230, 245, 250);  /* memoryBackgroundColor */

  /* ── Selection ── */
  --color-selection-bg:      rgb(180, 213, 255);  /* selectionBg */

  /* ── Borders ── */
  --color-border-prompt:     rgb(153, 153, 153);  /* promptBorder */
  --color-border-bash:       rgb(255, 0, 135);    /* bashBorder */

  /* ── Diff ── */
  --color-diff-added:        rgb(105, 219, 124);  /* diffAdded */
  --color-diff-removed:      rgb(255, 168, 180);  /* diffRemoved */
  --color-diff-added-word:   rgb(47, 157, 68);    /* diffAddedWord */
  --color-diff-removed-word: rgb(209, 69, 75);    /* diffRemovedWord */

  /* ── Labels ── */
  --color-label-you:         rgb(37, 99, 235);    /* briefLabelYou */
  --color-label-claude:      rgb(215, 119, 87);   /* briefLabelClaude */

  /* ── Fast mode ── */
  --color-fast-mode:         rgb(255, 106, 0);    /* fastMode */

  /* ── Rate limit ── */
  --color-rate-fill:         rgb(87, 105, 247);   /* rate_limit_fill */
  --color-rate-empty:        rgb(39, 47, 111);    /* rate_limit_empty */
}

.dark {
  --claude-orange:           rgb(215, 119, 87);
  --claude-orange-shimmer:   rgb(235, 159, 127);
  --claude-blue:             rgb(177, 185, 249);
  --claude-blue-shimmer:     rgb(207, 215, 255);

  --color-success:           rgb(78, 186, 101);
  --color-error:             rgb(255, 107, 128);
  --color-warning:           rgb(255, 193, 7);

  --color-text-primary:      rgb(255, 255, 255);
  --color-text-inverse:      rgb(0, 0, 0);
  --color-text-inactive:     rgb(153, 153, 153);
  --color-text-subtle:       rgb(80, 80, 80);

  --color-msg-user-bg:       rgb(55, 55, 55);
  --color-msg-user-bg-hover: rgb(70, 70, 70);
  --color-msg-actions-bg:    rgb(44, 50, 62);
  --color-msg-bash-bg:       rgb(65, 60, 65);
  --color-msg-memory-bg:     rgb(55, 65, 70);

  --color-selection-bg:      rgb(38, 79, 120);

  --color-border-prompt:     rgb(136, 136, 136);
  --color-border-bash:       rgb(253, 93, 177);

  --color-diff-added:        rgb(34, 92, 43);
  --color-diff-removed:      rgb(122, 41, 54);
  --color-diff-added-word:   rgb(56, 166, 96);
  --color-diff-removed-word: rgb(179, 89, 107);

  --color-label-you:         rgb(122, 180, 232);
  --color-label-claude:      rgb(215, 119, 87);

  --color-fast-mode:         rgb(255, 120, 20);

  --color-rate-fill:         rgb(177, 185, 249);
  --color-rate-empty:        rgb(80, 83, 112);
}
```

### Agent 色板（通用）

```css
:root {
  --agent-red:    rgb(220, 38, 38);
  --agent-blue:   rgb(37, 99, 235);
  --agent-green:  rgb(22, 163, 74);
  --agent-yellow: rgb(202, 138, 4);
  --agent-purple: rgb(147, 51, 234);
  --agent-orange: rgb(234, 88, 12);
  --agent-pink:   rgb(219, 39, 119);
  --agent-cyan:   rgb(8, 145, 178);
}
```

---

## 十四、与 REFACTOR_PLAN.md 的关系

此 design token 规范 **替代** 了原计划中手动拟合的 oklch 暖色方案。

**关键变更**：
1. 不再用 oklch 暖色相猜测 → 直接使用 Claude Code 原始 RGB 值
2. `--color-msg-user-bg: rgb(240,240,240)` 替代原计划的 `oklch(0.93 0.018 75)`
3. `--claude-orange: rgb(215,119,87)` 替代原计划的 `oklch(0.65 0.14 45)`
4. 消息卡片 accent bar 可直接使用 `briefLabelYou`（蓝色）和 `briefLabelClaude`（橙色）

**注意**：Claude Code 原版是终端应用，背景色由终端本身决定（黑/白）。
桌面端的 `--color-background` 和 `--color-sidebar-background` 等 **布局色** 仍需自行定义，
但所有 **语义色**（消息、diff、状态、标签）应严格遵循此 token 规范。

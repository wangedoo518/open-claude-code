# ClawWiki · Desktop Shell **v2** — user-first IA

This is a reimagining of `desktop-shell/` for non-developer users — closer to **Notion / 飞书** than to a developer console. Technical terms are relocated to an on-demand "高级信息" drawer; default copy speaks in user actions.

Open `index.html` to try it.

## IA mapping · 旧 → 新

| v1 (developer view) | v2 (user view) |
|---|---|
| Dashboard          | **首页** · "工作起点" |
| Ask                | **问问题** |
| Inbox              | **待整理** (Maintainer 的提议) |
| Wiki · Graph · Raw | **知识库** → 页面 / 关系图 / 素材库 (tabs) |
| Schema             | Settings → 整理规则 (advanced) |
| WeChat Bridge      | **微信接入** (3-step onboarding) |
| Settings           | Settings (flattened, 技术选项 collapsed) |

## Glossary · 术语替换表

| 原文 | v2 | 备注 |
|---|---|---|
| Raw Library        | 素材库 |
| Wiki Pages         | 已整理的页面 |
| Schema             | 整理规则 | 用户层不再暴露 |
| WeChat Bridge      | 微信接入 |
| Session            | 对话 |
| Source             | 参考内容 |
| Permission mode    | 操作确认 |
| Compact session    | 精简对话历史 |
| Provider / Runtime / Pipeline | — | 仅在「高级信息」抽屉出现 |
| Maintainer         | 我 (the assistant's first person)  | "我注意到…""我的建议是…" |

## Default layer vs Advanced layer

- **Default** — 用户任务、当下状态、下一步行动。没有技术名词。
- **Advanced** (top-bar `{ }` toggle) — 打开侧边抽屉，显示 route、provider、runtime、storage、最近日志等。面向调试。

This matches the 分层原则 in the brief: 默认不展示 / 高级可查看 / 核心能力未被删除。

## Pages

| File | Screen |
|---|---|
| `Home.jsx`          | 首页 — 欢迎语 + 四张快速开始卡片 + 建议的问题 + 最近动态 |
| `Ask.jsx`           | 问问题 — 答案展示"参考内容"（不是 sources / citations） |
| `Review.jsx`        | 待整理 — 三种提议卡片（合并 / 新页面 / 去重），配"我的建议" + 按建议执行按钮，带 empty state |
| `KnowledgeBase.jsx` | 知识库 — 页面 / 关系图 / 素材库三个 tab |
| `Connect.jsx`       | 微信接入 — 3 步 onboarding + 二维码 + 「暂时不想连」逃生口；同文件含 SettingsPage |
| `Shell.jsx`         | SidebarV2 (4 主 + 2 工具) 与 TopBarV2 (标题 + 副说明 + 高级信息按钮) |
| `App.jsx`           | route switch + AdvancedDrawer |

## What did NOT change

- 设计系统 tokens（颜色/字体/间距/圆角/阴影）完全沿用 v1 的 `colors_and_type.css` 与 `kit.css`，只在末尾追加 v2 专属样式。
- Lucide 图标集 `Icons.jsx` 共用。
- 没有改底层业务逻辑 —— 这个 kit 仅重构"产品表达与信息架构"。

## Known simplifications

- 折叠后的 Sidebar (icon-only) 与 dark mode 未接入。
- 微信接入的二维码是装饰性 SVG，不是真实 QR 编码。
- 关系图是手绘风静态 SVG，不是 force-directed sim。
- AdvancedDrawer 内的日志/键值是演示数据。

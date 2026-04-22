# ClawWiki · Desktop Shell UI kit

An interactive, click-through recreation of the ClawWiki desktop app's three-pane shell. Visuals lifted from `claudewiki/apps/desktop-shell/src/`.

## Routes implemented

| Route | File | State |
|---|---|---|
| Dashboard | `Dashboard.jsx` | Hero greeting, 4 stat cards, activity feed, quick-ask list |
| Ask | `Ask.jsx` | Message list + composer; `⌘↵` sends; fake streaming reply after ~1.6s with source pills |
| Inbox | `Inbox.jsx` | Two-pane (list / detail) Maintainer Workbench; three sample proposals with mono-diff preview |
| Raw · Wiki · Graph · Schema · Bridge · Settings | `App.jsx PlaceholderPage` | Intentionally stubbed — see note in each |

## Components

- `Shell.jsx` → `<Sidebar>`, `<TopBar>`
- `Dashboard.jsx` → `<StatCard>`, `<ActivityRow>`, `<DashboardPage>`
- `Ask.jsx` → `<ChatMessage>`, `<Composer>`, `<AskPage>`
- `Inbox.jsx` → `<InboxPage>` (list + detail)
- `App.jsx` → route switch, `<PlaceholderPage>`

## How to run

Just open `index.html` in a browser. It loads React 18 + Babel from unpkg and imports each `.jsx` file separately — components are hung off `window` at the bottom of each file so they share scope.

## Known simplifications

- WeChat Bridge, Raw, Wiki, Graph, Schema, Settings routes are placeholder stubs — real implementations live in the codebase under `src/features/*/`.
- Collapsed sidebar mode (icon-only @ 48px) is not wired.
- Dark mode toggle is not wired; `[data-theme="dark"]` works if set manually.
- Streaming text animation is a single-state swap, not token-by-token — the pulsing Terracotta left-border carries the perception of motion.

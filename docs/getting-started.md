# Getting Started with open-claude-code

A Tauri 2.0 + Rust + React desktop client for running Claude Code-style
agent loops with local tool execution, MCP integration, and a built-in
HTTP API for scripting.

This guide walks you from a fresh clone to your first real conversation.

---

## 1. Prerequisites

| Requirement | Why |
|-------------|-----|
| **Rust toolchain** (stable, ≥ 1.75) | Backend (`desktop-core`, `desktop-server`, `desktop-cli`) |
| **Node.js 20+** + npm | Frontend (`apps/desktop-shell`) |
| **Anthropic API key** OR ChatGPT Plus / Qwen account | LLM credentials (see §3) |
| **Python 3** (optional) | Only needed if you run the included MCP mock server for testing |

Tauri 2.0 also needs platform-specific bundling tooling — see
[tauri.app/start/prerequisites](https://tauri.app/start/prerequisites/)
for Windows / macOS / Linux setup if you want to build the desktop bundle.

---

## 2. Build & run

```bash
git clone <repo-url>
cd open-claude-code

# Backend
cd rust
cargo build -p desktop-server -p desktop-cli
cargo test -p desktop-core   # 62 tests should pass

# Frontend
cd ../apps/desktop-shell
npm install
npx tsc --noEmit             # type check, should be clean
```

To start a local backend:

```bash
cd rust
cargo run -p desktop-server
# Listens on http://127.0.0.1:4357
```

To start the frontend dev server in the browser (without Tauri shell):

```bash
cd apps/desktop-shell
npm run dev
# Open the printed URL (usually http://localhost:5173)
```

To start the full Tauri desktop app:

```bash
cd apps/desktop-shell
npm run tauri:dev
```

---

## 3. Configuring credentials

The agentic loop walks a four-step priority chain to find LLM credentials:

### Option 1 — `ANTHROPIC_API_KEY` env var (easiest)

```bash
export ANTHROPIC_API_KEY=sk-ant-...
cargo run -p desktop-server
```

This uses "direct mode" — the backend talks to `api.anthropic.com`
without going through any OAuth flow.

### Option 2 — Per-project `direct_api_key` in `.claude/settings.json`

```json
{
  "direct_api_key": "sk-ant-...",
  "permission_mode": "workspace_write"
}
```

Useful for CI environments and per-project keys. **Plaintext** — for
security-critical setups, use Option 4 instead.

### Option 3 — `codex` CLI OAuth (ChatGPT Plus subscribers)

```bash
npm install -g @openai/codex
codex auth login
# Opens browser to ChatGPT login
```

The agentic loop will discover `~/.codex/auth.json` and use the OAuth
token. Tokens auto-refresh.

### Option 4 — `qwen` CLI OAuth (encrypted at rest)

```bash
qwen auth login
```

Credentials are stored in `~/.warwolf/qwen/profiles.json` encrypted
with AES-256-GCM (key in `~/.warwolf/.secret-key`, 0600 perms on Unix).

---

## 4. Your first conversation

With the backend running, use the CLI client:

```bash
# Sanity check
./target/debug/ocl health
# {status: ok}

# Create a session
./target/debug/ocl --json sessions new --title "My first chat"
# Returns a JSON blob with `id` like "desktop-session-1"

# Send a message
./target/debug/ocl sessions send desktop-session-1 "Read README.md and summarize it"

# Watch the streaming response and any tool calls
./target/debug/ocl sessions show desktop-session-1
```

Or use the Tauri desktop shell — same operations through the GUI.

---

## 5. Project layout (`.claude/` directory)

The backend reads several files from your project root:

```
my-project/
├── .claude/
│   ├── settings.json        # permission_mode, direct_api_key, hooks, mcpServers
│   ├── CLAUDE.md            # always-loaded instructions
│   └── skills/              # workspace skills (lazy-loaded)
│       ├── code-review.md          # file form
│       └── doc-writing/SKILL.md    # directory form
├── src/
└── ...
```

### `settings.json` reference

```json
{
  "permission_mode": "workspace_write",
  "direct_api_key": "sk-ant-...",
  "hooks": {
    "PreToolUse": [
      { "matcher": "bash", "command": "echo 'about to run bash'" }
    ]
  },
  "mcpServers": {
    "github": {
      "type": "stdio",
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"]
    }
  }
}
```

### Permission modes

| Mode | Behavior |
|------|----------|
| `read_only` | Only read tools (read_file, glob_search, grep_search) without prompting |
| `workspace_write` | (default) Read tools auto-allowed; write tools (write_file, edit_file, bash) prompt the user |
| `danger_full_access` | All tools auto-allowed (no prompts) — use only for trusted scripts |

### Workspace skills

Markdown files under `.claude/skills/`. The agent sees the name + first
paragraph in its system prompt and can invoke skills via the `Skill` tool.

```markdown
# code-review.md
Reviews code changes for bugs, style, and architecture.

When invoked, walks through the staged diff, checks each file,
runs the linter, and writes a structured review.
```

---

## 6. CLI reference (`ocl`)

```
ocl health
ocl sessions list | show <id> | new [--title] [--path]
ocl sessions send <id> <message>
ocl sessions cancel <id> | compact <id> | delete <id>
ocl sessions status <id> <todo|in_progress|needs_review|done|archived>
ocl sessions flag <id> <true|false>
ocl mcp probe <project_path>
ocl mcp call <qualified_name> <args_json>
ocl permission-mode [get | set <mode>]

Flags:
  --server <url>     Override base URL (env: OCL_SERVER)
  --json             Emit raw pretty JSON (pipe-friendly)
  -h, --help         Show usage
```

---

## 7. Architecture at a glance

```
┌─────────────────────────────────────────────────────────────┐
│  Tauri Desktop Shell (React + Zustand + React Query)        │
│                                                              │
│  - SessionWorkbenchSidebar  (lifecycle workflow + flag UI)  │
│  - InputBar                 (drag-drop attachments)         │
│  - WorkspaceSkillsPanel     (popover listing skills)        │
│  - StreamingIndicator       (RAF-batched text rendering)    │
│  - PermissionDialog         (async permission prompts)      │
└──────────────────┬──────────────────────────────────────────┘
                   │  HTTP + SSE
                   ▼
┌─────────────────────────────────────────────────────────────┐
│  desktop-server (Axum, port 4357, 50+ routes)                │
└──────────────────┬──────────────────────────────────────────┘
                   ▼
┌─────────────────────────────────────────────────────────────┐
│  desktop-core                                                │
│                                                              │
│  ┌──────────────┐  ┌──────────────┐  ┌─────────────────┐    │
│  │ agentic_loop │→ │ system_prompt│  │ secure_storage  │    │
│  │   PermGate   │  │  CLAUDE.md+  │  │ AES-256-GCM     │    │
│  │   SSE stream │  │  Skills      │  │                 │    │
│  └──────┬───────┘  └──────────────┘  └─────────────────┘    │
│         │                                                    │
│         ├─→ tools::execute_tool (39 vendored tools)         │
│         └─→ McpServerManager (stdio MCP, persistent)        │
└─────────────────────────────────────────────────────────────┘
                   │
                   ▼
            api.anthropic.com  /  ChatGPT-OAuth proxy  /  Qwen
```

`desktop-cli` is a separate binary that talks HTTP to `desktop-server`,
useful for automation.

---

## 8. Troubleshooting

### "no credentials available"

The agentic loop couldn't find any of: `ANTHROPIC_API_KEY` env var,
`direct_api_key` in `.claude/settings.json`, codex auth, or qwen auth.
Set up at least one — see §3.

### Session stuck in `running` after backend crash

Open the session — the next backend startup auto-resets stuck sessions
to `idle` via the reconcile pass (L-03 fix in commit `42cd302`).

### MCP server "tool_count: 0" but config exists

Check `.claude/settings.json` for valid `mcpServers` entries with
`type: stdio`, `command`, and `args`. Run
`ocl mcp probe <project_path>` to see error logs.

### Backend can't bind 4357

Another process is using the port. Stop it or set
`OPEN_CLAUDE_CODE_DESKTOP_ADDR=127.0.0.1:5757` (any free port).

---

## 9. Further reading

- [`docs/audit-lessons.md`](audit-lessons.md) — 15 archived bug stories with root cause + fix
- [`docs/performance-report.md`](performance-report.md) — long-session benchmark results
- [`tasks/plan.md`](../tasks/plan.md) — current implementation plan
- [`tasks/todo.md`](../tasks/todo.md) — task checklist

# AGENTS.md · multi-agent coordination rules

## Agents

| Agent | Role | Scope | Trigger |
|---|---|---|---|
| **wiki-maintainer** | Summarise raw sources into wiki pages | wiki/ layer only | `raw_ingested(source_id)` event |
| **ask-runtime** | Answer user questions using wiki as context | read wiki/, read raw/ | user message in Ask page |

## Coordination rules

1. Only one agent may write to `wiki/` at a time. The inbox mutex serializes all writes.
2. `ask-runtime` NEVER writes to `wiki/` directly. If an answer should be filed, it proposes via Inbox.
3. `wiki-maintainer` NEVER reads from `ask-sessions`. The two agents are isolated by design.
4. Both agents share the same `codex_broker` pool. Token allocation is round-robin, not priority-based.

## Adding a new agent

1. Define its role, scope, and trigger in this file.
2. Add a new `InboxKind` variant in `wiki_store` for its proposals.
3. Wire the trigger in `desktop-server` or `desktop-core`.
4. The agent must respect the schema layer (this file + CLAUDE.md + policies/).

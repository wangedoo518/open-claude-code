# CLAUDE.md — Claude Code compatibility bridge

> **Primary project context lives in [`CLAW.md`](CLAW.md).**
> **Task routing and agent rules live in [`AGENTS.md`](AGENTS.md).**
>
> This file exists because Claude Code's runtime discovers `CLAUDE.md`
> in ancestor directories. In this repository the product is named Claw
> (a Claude Code fork), so `CLAW.md` is the canonical project-level
> context file that was established first. This `CLAUDE.md` bridges the
> two naming conventions without duplicating content.

## Relationship summary

| File | Role | Who writes it |
|------|------|---------------|
| `CLAW.md` | **Behavioral constitution** — stack, verification commands, repo shape, working agreement | Human + intentional agent edits |
| `AGENTS.md` | **Task routing** — doc index, verification entry points, modification backfill rules | Human + agent |
| `CLAUDE.md` (this file) | **Bridge** — ensures Claude Code runtime loads context; points to CLAW.md | Auto-discovered by runtime |

## Quick-start for new agents

1. Read `CLAW.md` for stack + verification commands
2. Read `AGENTS.md` for doc map + routing
3. Check `rules/` for path risk and trigger rules
4. Check `memory/` for corrections, observations, and learned rules
5. Never modify `CLAW.md` content automatically (per its own §Working agreement)

## Note on schema/CLAUDE.md

References to `CLAUDE.md` inside `apps/desktop-shell/src/` and `.clawwiki/schema/`
refer to a **different** file: the wiki maintainer agent's behavior contract stored
at `.clawwiki/schema/CLAUDE.md`. That is NOT this root-level compatibility bridge.

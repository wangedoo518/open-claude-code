# Evolution Log

Chronological record of governance and rule-system changes.

---

## 2026-04-16 — Governance bootstrap

**What happened**: First-time creation of the governance skeleton for this repository.

**Files created**:
| File | Role | Why needed |
|------|------|------------|
| `CLAUDE.md` | Compatibility bridge | Claude Code runtime discovers `CLAUDE.md` in ancestors; this repo uses `CLAW.md` as primary context. Bridge ensures both work. |
| `rules/path-risk-matrix.md` | Risk classification | Agents had no shared reference for which paths are dangerous to modify. Matrix distilled from P0–P2 bug history. |
| `rules/trigger-map.md` | Validation triggers | Mandatory verification steps were only in commander prompts, not codified. Now agents can self-check. |
| `memory/corrections.jsonl` | Correction log (empty) | Append-only store for factual corrections. Initialized empty — no corrections needed this round. |
| `memory/observations.jsonl` | Pattern log (1 entry) | Seeded with "heading inline typography residual" — the systemic pattern that drove P1-6 through P2-1. |
| `memory/learned-rules.md` | Stable rules (3 rules) | LR-1 heading class-over-inline, LR-2 minimal ReactMarkdown components, LR-3 TreeNode action model. All backed by multi-round evidence. |
| `memory/evolution-log.md` | This file | Meta-record of governance changes. |
| `docs/commander-prompt.md` | Commander template | Structured prompt for the project commander role. Includes execution phases, validation requirements, memory protocol. |

**Existing files NOT modified**:
- `CLAW.md` — remains the behavioral constitution (per its own §Working agreement: "Do not overwrite automatically")
- `AGENTS.md` — remains the task routing document

**Relationship established**:
```
CLAW.md (behavioral constitution — stack, verification, working agreement)
  ↑ pointed to by
CLAUDE.md (Claude Code runtime bridge — discovers this, redirects to CLAW.md)
  ↔ sibling
AGENTS.md (task routing — doc index, modification backfill, agent rules)
```

**Rules bootstrap reasoning**:
- `path-risk-matrix.md` was written from actual bug sites: P0-1 Composer double-ingest, P0-2 query_done sources missing, v3 scroll-hidden bug, theme double-class bug, session pile-up bug, dead tree nodes. Every entry has a known incident.
- `trigger-map.md` mandatory triggers come from validation steps that caught real regressions. Conditional triggers encode patterns learned across P1-6 through P2-1.
- `learned-rules.md` only has 3 rules — each backed by systemic evidence (6+ files affected, multi-round fix). No speculative rules.

**What was NOT created**:
- No `rules/` for Rust-specific invariants (cargo test coverage is sufficient)
- No `memory/context-window-log.md` (session management is out of scope)
- No `.claude/` configuration changes (runtime config is user-owned)

---

## 2026-04-16 — Governance format hardening (P2-2)

**What happened**: JSONL files created during the bootstrap round contained
`//` comment headers inside the `.jsonl` files. This is not valid JSONL
(the format is one JSON object per line, nothing else). Comments were
removed and their content migrated to markdown documentation.

**Changes**:

| File | Change |
|------|--------|
| `memory/corrections.jsonl` | Removed 3 `//` comment lines → now empty file (no corrections recorded yet) |
| `memory/observations.jsonl` | Removed 3 `//` comment lines → file now contains only the 1 real JSON record |
| `docs/commander-prompt.md` | Added strict JSONL format spec + field schemas to the memory-closure section |
| `rules/trigger-map.md` | Tightened `memory/*.jsonl` trigger: "strict JSONL (no comments, no headers)" |
| `memory/evolution-log.md` | This entry |

**Why this matters**: Future tooling (append-only linter, CI JSONL parser,
`jq` pipelines) will fail on `//` comment lines. Fixing this now means
the format contract is machine-verifiable from day one.

**What was NOT changed**:
- The existing JSON observation record was preserved byte-for-byte
- `memory/learned-rules.md` was not modified (no new rule needed — this is a one-time format cleanup, not a recurring pattern)
- No product code was touched

---

## 2026-04-16 — JSONL local validator (P3-1)

**What happened**: Added `scripts/check-memory-jsonl.mjs` — a zero-dependency
Node script that reads `memory/corrections.jsonl` and `memory/observations.jsonl`,
parses every non-empty line as JSON, and exits non-zero on any failure.

**Changes**:

| File | Change |
|------|--------|
| `scripts/check-memory-jsonl.mjs` | New file (33 lines). Run with `node scripts/check-memory-jsonl.mjs` |
| `rules/trigger-map.md` | Appended run command to the `memory/*.jsonl` trigger row |
| `memory/evolution-log.md` | This entry |

**Design decisions**:
- No package.json modification: root package.json has no `scripts` section and is
  owned by the Electron build. Adding a governance-only script entry would be out of
  character. The bare `node scripts/...` command is just as easy.
- No git pre-commit hook: hooks require `.git/hooks/` or `husky` setup, which is
  heavier than warranted for two files. The manual command + trigger-map rule is sufficient
  at current scale.
- No Python dependency: Node is already required by the desktop-shell build; no
  additional runtime needed.

---

## 2026-04-16 — Validator contract tightening (P3-2)

**What happened**: P3-1's validator had two gaps vs the documented contract:
1. Missing file → `SKIP` (silent pass). Should be `FAIL`.
2. Interior blank lines → silently filtered out. Should be `FAIL`.

**Changes**:

| File | Change |
|------|--------|
| `scripts/check-memory-jsonl.mjs` | Missing file → FAIL + exit 1. Interior blank lines → FAIL. EOF trailing newline → allowed. Added optional CLI file args for testing. |
| `docs/commander-prompt.md` | Clarified: trailing newline OK, interior blanks not, missing files = violation, added validate command reference |
| `memory/evolution-log.md` | This entry |

**Contract now enforced by validator**:
- Missing file → `FAIL` (governance files must exist)
- Empty file (0 bytes or trailing newline only) → `PASS` (no records yet)
- Each non-empty line must be valid JSON
- Interior blank / whitespace-only lines → `FAIL`
- One trailing newline at EOF → allowed (text-file convention)

**CLI args added**: `node scripts/check-memory-jsonl.mjs [file...]` — pass
custom paths for negative testing without touching real governance files.

---

## 2026-04-16 — Append-only enforcement (P3-3)

**What happened**: Added `scripts/check-memory-append-only.mjs` — a companion
to the strict JSONL validator. It uses `git diff --numstat HEAD` to detect
whether any lines were deleted from tracked memory JSONL files.

**Relationship to previous rounds**:
- P2-2 established "strict JSONL" contract (no comments, one JSON per line)
- P3-1 added `check-memory-jsonl.mjs` to enforce format
- P3-2 tightened the validator (missing=fail, blank lines=fail)
- P3-3 (this round) adds the second half: "history is append-only"

Together the two scripts enforce: **valid JSON + never rewritten**.

**Changes**:

| File | Change |
|------|--------|
| `scripts/check-memory-append-only.mjs` | New (59 lines). `git diff --numstat` based. Missing→FAIL, untracked→PASS, deletions>0→FAIL. CLI file args supported. |
| `rules/trigger-map.md` | Updated `memory/*.jsonl` trigger row to reference both scripts |
| `docs/commander-prompt.md` | Added append-only validate command + description |
| `memory/evolution-log.md` | This entry |

**Design decisions**:
- Separate script (not merged into check-memory-jsonl.mjs): format validation
  and history-integrity are different concerns with different failure modes and
  different underlying tools (JSON.parse vs git diff).
- `git diff --numstat` chosen over line-by-line patch parsing: simpler, no
  regex on diff hunks, single number comparison.
- Untracked files pass: all governance memory files are currently untracked
  (never committed). Once committed, the append-only check activates. This
  avoids blocking the initial commit.

---

## 2026-04-16 — Multi-agent bug operations framework (S1)

**What happened**:
- Created `docs/multi-agent-bug-ops.md` — a concrete playbook for multi-agent bug detection and fixing phases
- Updated `.gitignore` to cover 6 known local-data / junk entries that were polluting `git status`
- Updated `rules/trigger-map.md` to reference the new playbook as a load requirement for bug phases
- This is a framework-only round — no actual bugs were hunted or fixed

**Why framework first, not direct bug hunt**:
- Previous P0-P2 rounds showed that without explicit subagent ownership protocols, write-set conflicts, and stop conditions, bug fixes tended to scope-creep
- The governance bootstrap (P2-2 through P3-3) established format and enforcement but lacked operational playbook for the bug-finding workflow itself
- Cleaning junk from git status is prerequisite for reliable commit-scope audits during bug phases

**Files created/modified this round**:

| File | Change |
|------|--------|
| `docs/multi-agent-bug-ops.md` | New — 8-section bug-phase playbook |
| `.gitignore` | Added 6 junk entries |
| `rules/trigger-map.md` | Added bug-phase load requirement |
| `memory/evolution-log.md` | This entry |

**What was NOT created**:
- No new enforcement scripts (existing JSONL + append-only validators are sufficient for bug phases)
- No changes to learned-rules.md (no new multi-round pattern evidence yet)
- No product code changes

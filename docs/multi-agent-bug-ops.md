# Multi-Agent Bug Operations Playbook

> Actionable protocol for detecting, fixing, and closing bugs across
> coordinated agents. Load this when the commander declares a bug phase.

## 1. Entry Criteria

Enter a bug phase when ANY of:

- Commander explicitly signals `BUG:` or `FIX:` in the task prompt
- `npx tsc --noEmit` or `cargo check --workspace` fails on a previously-passing codebase
- A user reports broken behavior (screenshot, video, description)
- A verification step from `rules/trigger-map.md` fails after an unrelated change
- An agent discovers a regression during routine validation (Phase 4 of `docs/commander-prompt.md`)

Do NOT enter a bug phase for: feature requests, refactors, or cosmetic preferences unless they mask a functional issue.

## 2. Bug Lifecycle

### Step 1 -- Reproduce

Confirm the bug exists. Acceptable evidence:

- Browser screenshot or DOM introspection showing broken state
- Console error (DevTools or `preview_console_logs`)
- Test output with failure trace
- `npx tsc --noEmit` or `cargo check` error text

If you cannot reproduce, stop. File it as "unconfirmed" and move on.

### Step 2 -- Scope

1. Identify affected files using grep/search
2. Cross-check each file against `rules/path-risk-matrix.md`
3. List all files the fix will touch (this becomes the write set)
4. Note any Critical or High paths -- these require extra care in Steps 4-6

### Step 3 -- Risk

Classify severity:

| Level | Meaning | Examples |
|-------|---------|---------|
| **P0** | Blocks a primary user flow | Ask session won't send, wiki won't save, app won't start |
| **P1** | Degraded but usable | Sources missing from response, sidebar highlighting wrong, theme flicker |
| **P2** | Cosmetic / polish | Wrong font weight, spacing off by a few px, tooltip misaligned |

P0 gets fixed immediately. P1 gets fixed in current session. P2 can be deferred.

### Step 4 -- Plan

Before touching code, declare in writing:

1. **Approach**: 1-3 sentences on what you will change and why
2. **Write set**: exact file paths
3. **Boundaries**: what you will NOT change (adjacent code, unrelated bugs)
4. **Risk paths hit**: which Critical/High paths from the matrix are involved

### Step 5 -- Implement

- Make the minimal change that fixes the bug
- Stay strictly within the declared write set
- If the fix requires touching a file not in the plan, stop and re-scope (Step 4)
- Follow existing code patterns -- do not refactor while fixing

### Step 6 -- Verify

Run ALL applicable triggers from `rules/trigger-map.md`:

- `.tsx`/`.ts` changed: `cd apps/desktop-shell && npx tsc --noEmit`
- `.rs` changed: `cd rust && cargo check -p <crate>`
- `globals.css` changed: visual smoke on 2 routes (light + dark)
- `memory/*.jsonl` changed: `node scripts/check-memory-jsonl.mjs` + `node scripts/check-memory-append-only.mjs`
- UI bugs: browser smoke test (screenshot before/after or DOM introspection)

Also run the pre-commit checklist from `rules/trigger-map.md` lines 36-43.

### Step 7 -- Memory Closure

Follow Phase 5 of `docs/commander-prompt.md`. See Section 8 below for bug-specific guidance.

## 3. Role Definitions

An agent may hold multiple roles. Solo agents typically hold all four.

| Role | Responsibility | Key constraint |
|------|---------------|----------------|
| **Planner** | Steps 1-4: reproduce, scope, classify, write the plan | Must not start implementation |
| **Implementer** | Step 5: write the fix within declared boundaries | Must not edit files outside write set |
| **Verifier** | Step 6: run triggers, check regressions, provide evidence | Must report evidence, not assertions |
| **Collector** | Step 7: decide memory updates, write evolution-log if governance changed | Must follow JSONL schema from `docs/commander-prompt.md` lines 59-60 |

Handoff rule: each role produces an artifact (plan doc, code diff, verification log, memory update) before the next role begins.

## 4. Subagent Protocol

### When to use subagents

| Subagent type | Use when | Example |
|---------------|----------|---------|
| **Explorer** | Bug scope is unclear; need read-only audit (grep, git log, DOM introspection) | "Is this CSS regression in globals.css or a component override?" |
| **Worker** | Multiple independent files need changing simultaneously | Worker A fixes `WikiArticle.tsx`, Worker B fixes `WikiExplorer.tsx` |

### When NOT to use subagents

- Write set is 3 files or fewer
- Files are interdependent (e.g., a CSS token in `globals.css` + the component consuming it)
- The bug is in a single function

### Write-set ownership rules

1. Every worker declares its exclusive file list before starting
2. Main agent resolves any overlap before launching workers
3. No worker may modify files outside its declared set -- violation = revert that worker's changes
4. Workers must not modify: `.gitignore`, `rules/*`, `memory/*`, `scripts/*`, `CLAW.md`, `CLAUDE.md`, `AGENTS.md` (unless that is their explicit assignment)
5. Main agent does final integration: merges changes, runs full verification (Step 6), commits

### Explorer subagent contract

- Read-only: no file writes
- Returns: list of affected files, suspected root cause, relevant code snippets
- Time-boxed: if no answer after examining 10 files, return "inconclusive" with findings so far

## 5. Bug Report Template

```
## Bug: [short title]
**Severity**: P0 / P1 / P2
**Symptom**: [what the user sees]
**Reproduction**: [steps to reproduce]
**Evidence**: [screenshot / DOM output / console error / test failure]
**Suspected root cause**: [file:line if known]
**Affected paths**: [from rules/path-risk-matrix.md]
```

## 6. Fix Report Template

```
## Fix: [short title]
**Bug ref**: [link or title]
**Files changed**: [list]
**Approach**: [1-3 sentences]
**Verification**: [commands run + results]
**Regression check**: [what was checked to ensure nothing else broke]
**Memory update**: [which memory files updated, or "none -- reason"]
```

## 7. Stop Conditions

Stop and escalate to commander when ANY of:

| Condition | Action |
|-----------|--------|
| Fix touches a Critical-risk path not in the original plan | Stop. Re-scope with commander before proceeding |
| Fix requires changing >5 files | Stop. Break into sub-tasks with separate write sets |
| Verifier finds a new bug introduced by the fix | Stop. Revert the fix or isolate the regression before continuing |
| Fix requires modifying `CLAW.md` or `AGENTS.md` | Stop. These require commander approval per governance rules |
| Context budget >80% consumed | Stop. Document current state, hand off with a status summary |
| Reproduction fails after scoping (bug is intermittent) | Stop. Document reproduction attempts, mark "unconfirmed" |

## 8. Memory Decision Guide (Bug-Specific)

| Situation | Action | Target file |
|-----------|--------|-------------|
| Bug was caused by a wrong assumption about code behavior | Write correction | `memory/corrections.jsonl` |
| Same bug pattern seen in 2+ files or 2+ rounds | Write observation | `memory/observations.jsonl` |
| Bug pattern has a proven, repeatable prevention strategy | Promote to learned rule | `memory/learned-rules.md` |
| Fix changed governance rules or enforcement | Write evolution-log entry | `memory/evolution-log.md` |
| Fix was a one-off cosmetic tweak | No memory update needed | -- |
| Fix touched `rules/*.md` | Record change in evolution-log | `memory/evolution-log.md` |

JSONL schemas (from `docs/commander-prompt.md`):
- `corrections.jsonl`: `{"date","session","what","file","before","after"}`
- `observations.jsonl`: `{"date","pattern","evidence","frequency","recommendation"}`

Validate after writing: `node scripts/check-memory-jsonl.mjs` and `node scripts/check-memory-append-only.mjs`.

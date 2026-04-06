# Desktop Shell Architecture Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor `apps/desktop-shell` so Router owns navigation identity, TanStack Query owns remote data, Redux shrinks to local UI state, and the desktop client layer is split into maintainable feature-oriented modules.

**Architecture:** This refactor is incremental. First create transport and feature-client seams without changing behavior. Then remove Redux as a shadow cache for remote data. Then normalize navigation so URL and Router explain the visible screen. Finally converge the UI stack and remove stale migration debt.

**Tech Stack:** React 19, React Router 7, TanStack Query 5, Redux Toolkit, Tauri 2, Tailwind 4, TypeScript 5

---

### Task 1: Split Desktop Transport From Feature Clients

**Files:**
- Create: `apps/desktop-shell/src/lib/desktop/bootstrap.ts`
- Create: `apps/desktop-shell/src/lib/desktop/transport.ts`
- Create: `apps/desktop-shell/src/features/session-workbench/api/client.ts`
- Create: `apps/desktop-shell/src/features/settings/api/client.ts`
- Create: `apps/desktop-shell/src/features/workbench/api/client.ts`
- Create: `apps/desktop-shell/src/features/code-tools/api/client.ts`
- Modify: `apps/desktop-shell/src/lib/tauri.ts`
- Test: `cd apps/desktop-shell && npm run build`

- [x] Extract the API base bootstrap logic out of `src/lib/tauri.ts` into `src/lib/desktop/bootstrap.ts` without changing runtime behavior.
- [x] Extract the generic `fetchJson` transport and retry handling into `src/lib/desktop/transport.ts`.
- [x] Move session-workbench-facing HTTP functions into `src/features/session-workbench/api/client.ts`.
- [x] Move settings-facing HTTP functions into `src/features/settings/api/client.ts`.
- [x] Move workbench-facing HTTP functions into `src/features/workbench/api/client.ts`.
- [x] Move code-tools-facing HTTP functions into `src/features/code-tools/api/client.ts`.
- [x] Keep `src/lib/tauri.ts` as a compatibility re-export layer during the transition so imports do not all need to move at once.
- [x] Run `cd apps/desktop-shell && npm run build` and confirm the build still passes.

### Task 2: Introduce Stable Query Modules

**Files:**
- Create: `apps/desktop-shell/src/features/session-workbench/api/query.ts`
- Create: `apps/desktop-shell/src/features/settings/api/query.ts`
- Create: `apps/desktop-shell/src/features/workbench/api/query.ts`
- Modify: `apps/desktop-shell/src/features/session-workbench/SessionWorkbenchPage.tsx`
- Modify: `apps/desktop-shell/src/features/workbench/HomePage.tsx`
- Modify: `apps/desktop-shell/src/features/settings/SettingsPage.tsx`
- Test: `cd apps/desktop-shell && npm run build`

- [x] Add feature-local query key factories so feature cache ownership is explicit.
- [x] Replace ad hoc string query keys in session-workbench with feature-local query helpers.
- [x] Replace ad hoc string query keys in workbench with feature-local query helpers.
- [x] Replace ad hoc string query keys in settings with feature-local query helpers.
- [x] Run `cd apps/desktop-shell && npm run build` and confirm the refactor preserves type safety.

### Task 3: Remove Redux As Remote Session Shadow Cache

**Files:**
- Modify: `apps/desktop-shell/src/store/slices/sessions.ts`
- Modify: `apps/desktop-shell/src/store/index.ts`
- Modify: `apps/desktop-shell/src/features/session-workbench/SessionWorkbenchPage.tsx`
- Modify: `apps/desktop-shell/src/features/session-workbench/SessionWorkbenchTerminal.tsx`
- Modify: any remaining files importing the sessions slice
- Test: `cd apps/desktop-shell && npm run build`

- [x] Identify all live imports of the `sessions` slice and verify which values are already available from Query.
- [x] Write a failing regression test if a test harness is introduced during this refactor; otherwise establish the build as the minimum safety gate for this stage.
- [x] Remove remote-session duplication from Redux usage sites so session list/detail come from Query-owned state.
- [x] Shrink or delete the `sessions` slice if no UI-only state remains.
- [x] Run `cd apps/desktop-shell && npm run build` and confirm the app compiles with Query as the session source of truth.

### Task 4: Normalize Navigation Ownership

**Files:**
- Modify: `apps/desktop-shell/src/store/slices/ui.ts`
- Modify: `apps/desktop-shell/src/store/slices/tabs.ts`
- Modify: `apps/desktop-shell/src/shell/AppShell.tsx`
- Modify: `apps/desktop-shell/src/shell/TabBar.tsx`
- Modify: `apps/desktop-shell/src/features/workbench/HomePage.tsx`
- Modify: `apps/desktop-shell/src/features/workbench/tab-helpers.ts`
- Modify: `apps/desktop-shell/src/features/session-workbench/SessionWorkbenchPage.tsx`
- Test: `cd apps/desktop-shell && npm run build`

- [x] Reduce `ui.viewMode` so it stops acting as the primary navigation truth.
- [x] Move session selection toward route-driven identity where practical.
- [x] Simplify `tabs` so it represents presentation state instead of product navigation state.
- [x] Update shell and workbench components to derive active views from Router-first logic.
- [x] Run `cd apps/desktop-shell && npm run build` and confirm navigation compiles coherently after the ownership shift.

### Task 5: Converge UI Stack

**Files:**
- Modify: `apps/desktop-shell/src/features/code-tools/CodeToolsPage.tsx`
- Modify: `apps/desktop-shell/src/features/code-tools/components/ModelSelector.tsx`
- Modify: `apps/desktop-shell/package.json`
- Modify: `apps/desktop-shell/src/main.tsx`
- Test: `cd apps/desktop-shell && npm run build`

- [x] Replace remaining `antd` usage with the existing UI primitives already used elsewhere in the shell.
- [x] Remove `antd` from `apps/desktop-shell/package.json`.
- [x] Remove `styled-components` and `@types/styled-components` from `apps/desktop-shell/package.json` if they remain unused.
- [x] Remove the Ant Design reset import from `src/main.tsx` once the last Antd component is gone.
- [x] Run `cd apps/desktop-shell && npm run build` and confirm the UI stack reduction does not break compilation.

### Task 6: Cleanup Migration Debt

**Files:**
- Modify: `apps/desktop-shell/src/**/*.ts`
- Modify: `apps/desktop-shell/src/**/*.tsx`
- Modify: `apps/desktop-shell/REFACTOR_PLAN.md` if needed
- Test: `cd apps/desktop-shell && npm run build`

- [ ] Remove stale comments that describe actively owned product files primarily as Cherry Studio ports where that wording no longer reflects the intended architecture.
- [ ] Keep historical provenance only where it still materially explains an interoperability constraint.
- [ ] Run `cd apps/desktop-shell && npm run build` as the final architecture cleanup verification.

### Task 7: Final Verification

**Files:**
- Verify only

- [x] Run `cd apps/desktop-shell && npm run build`.
- [x] Run `cd apps/desktop-shell/src-tauri && cargo check`.
- [x] Review the resulting diff and confirm the final state matches the design goals:
- [x] Router is the primary navigation source.
- [x] Query owns remote session and workbench data.
- [x] Redux is limited to local UI state.
- [x] `src/lib/tauri.ts` is no longer the monolithic desktop client implementation.
- [x] `antd` and `styled-components` are removed from `apps/desktop-shell`.

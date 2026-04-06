# Desktop Shell Architecture Refactor Design

**Goal**

Refactor `apps/desktop-shell` into a clearer 2026-style frontend architecture by shrinking Redux to local UI state, moving remote state ownership to TanStack Query, moving navigation/session selection toward Router-driven state, and splitting the current monolithic desktop client layer into feature-oriented modules.

**Scope**

This design applies only to `apps/desktop-shell`.

Out of scope unless the frontend is blocked:

- redesigning `rust/` service boundaries
- changing `desktop-server` API contracts in a broad way
- replacing Tauri
- introducing a new frontend framework

## Current Problems

### 1. Navigation has multiple competing sources of truth

The current shell spreads navigation responsibility across:

- React Router routes in `src/shell/AppShell.tsx`
- Redux tab state in `src/store/slices/tabs.ts`
- Redux view mode in `src/store/slices/ui.ts`
- feature-level orchestration in `src/shell/TabBar.tsx`
- feature-level orchestration in `src/features/workbench/HomePage.tsx`
- feature-level orchestration in `src/features/session-workbench/SessionWorkbenchPage.tsx`

This makes it harder to answer simple questions such as:

- What page is active?
- Which session is selected?
- Is a tab a durable concept or only a presentation artifact?

### 2. Remote state is partially mirrored into Redux

The app already uses TanStack Query for most backend data, but Redux slices still exist for state that is effectively remote or session-derived. This creates synchronization work without adding much value.

### 3. The desktop client layer is too centralized

`src/lib/tauri.ts` currently combines:

- Tauri bootstrap and `invoke` handling
- low-level HTTP transport
- TypeScript API contracts
- feature-facing service functions

This makes ownership unclear and causes unrelated features to couple through one file.

### 4. The UI stack is not fully converged

The intended UI direction is already mostly Tailwind + Radix/shadcn, but the desktop shell still carries:

- `antd` for a small subset of code tools UI
- `styled-components` dependency with no meaningful remaining usage in app code

This increases maintenance and contributor overhead.

### 5. Migration-era naming and provenance still leak into the product

A number of modules still describe themselves as Cherry Studio ports or compatibility layers. Historical provenance is useful in docs, but it should not keep shaping the active product architecture.

## Target Architecture

The target architecture is a four-layer frontend:

### 1. App Shell Layer

Location:

- `src/App.tsx`
- `src/shell/*`
- router entrypoints

Responsibilities:

- provider composition
- route definition
- layout shell
- top-level error boundaries
- global theme/bootstrap concerns

This layer should not own feature business logic beyond layout orchestration.

### 2. Feature Modules

Location:

- `src/features/*`

Each feature should own its own:

- pages
- components
- query hooks
- mutations
- local selectors/helpers
- feature-specific API client wrappers

Cross-feature imports should be minimized and should go through stable feature entrypoints when needed.

### 3. Shared UI and Utility Layer

Location:

- `src/components/ui/*`
- `src/components/*` for generic reusable components
- `src/lib/*` for pure utilities only

This layer should remain business-agnostic.

### 4. Desktop Client Layer

The existing `src/lib/tauri.ts` should be broken apart into:

- `src/lib/desktop/transport.ts`
- `src/lib/desktop/bootstrap.ts`
- feature-oriented client files under feature folders when appropriate

Examples:

- `src/features/session-workbench/api/client.ts`
- `src/features/settings/api/client.ts`
- `src/features/code-tools/api/client.ts`

Responsibilities:

- obtaining desktop API base URL
- low-level retry/timeout behavior
- request/response helpers
- narrowly scoped feature API functions

Non-responsibilities:

- feature page orchestration
- UI-specific branching
- giant shared contract dumping ground

## State Ownership Model

### Router owns navigational identity

Router should become the primary source of truth for:

- current page
- whether the user is in home/apps/code/settings flows
- selected session identifiers where route semantics make sense
- selected app or minapp identifiers where route semantics make sense

Tabs should increasingly be treated as a UI projection of routeable state, not an independent truth source.

### TanStack Query owns remote data

TanStack Query should own:

- workbench payloads
- session list and session detail
- settings/customize payloads
- auth/provider data
- backend mutation cache invalidation
- event-driven cache updates

Remote entities should not also be mirrored into Redux unless there is a very specific offline/UI-only reason.

### Redux owns only durable local UI state

Redux should remain only for local concerns such as:

- theme preference if still needed globally
- permission mode
- sidebar visibility preference
- lightweight UI toggles that are not worth encoding in the URL

Redux should not remain responsible for:

- session list snapshots
- active session remote payloads
- navigation truth
- route-derived tab identity

### Persistence should narrow

`redux-persist` should continue only if needed for a small set of UI preferences. If it remains, the persisted surface should become much smaller and more deliberate than the current store-wide setup.

## Technology Decisions

### Keep

- React 19
- React Router
- TanStack Query
- Tauri 2
- Tailwind 4
- Radix/shadcn-based UI components

### Reduce

- Redux scope
- `redux-persist` surface area
- cross-feature store coupling

### Remove over time

- `styled-components`
- `antd`
- Cherry Studio compatibility wording in active product code where it no longer reflects intended architecture

### Do not do in this refactor

- do not introduce Zustand or Jotai as the first move
- do not replace Redux before its responsibilities are reduced
- do not redesign Rust APIs unless strictly necessary to unblock frontend cleanup

The rationale is simple: moving bad ownership into a new state library is still bad ownership.

## Migration Strategy

The refactor should be incremental and always leave the app in a working state.

### Phase 1: Create architectural seams

- split `src/lib/tauri.ts` into transport/bootstrap plus feature-facing API modules
- add stable query keys and feature-local query helpers
- document state ownership rules in code comments where needed

Outcome:

The app still behaves the same, but the client layer stops being monolithic.

### Phase 2: Move remote/session state out of Redux

- identify which parts of `sessions` slice duplicate query-owned data
- move active session detail and session list reliance fully to Query
- retain only any local optimistic or UI-only session affordances if truly needed

Outcome:

Redux is no longer a shadow cache for backend state.

### Phase 3: Normalize navigation ownership

- reduce `ui.viewMode` so it no longer acts as primary navigation truth
- move session selection and route identity into Router where possible
- simplify `tabs` so tabs become presentation state rather than domain state

Outcome:

Navigation becomes easier to reason about because URL and router state explain the visible screen.

### Phase 4: UI stack convergence

- replace remaining `antd` usage with existing UI primitives
- remove unused `styled-components` dependency and related type packages

Outcome:

The frontend stack is more coherent and easier for contributors to extend.

### Phase 5: Cleanup and rename debt

- remove stale compatibility comments that describe current code as a port when the file is now product-owned
- update relevant docs inside `apps/desktop-shell`

Outcome:

The codebase communicates present intent rather than migration history.

## File-Level Direction

### Files expected to shrink or disappear in responsibility

- `apps/desktop-shell/src/store/slices/sessions.ts`
- `apps/desktop-shell/src/store/slices/ui.ts`
- `apps/desktop-shell/src/store/slices/tabs.ts`
- `apps/desktop-shell/src/store/index.ts`
- `apps/desktop-shell/src/lib/tauri.ts`

### Files expected to gain clearer responsibility

- `apps/desktop-shell/src/shell/AppShell.tsx`
- `apps/desktop-shell/src/shell/TabBar.tsx`
- `apps/desktop-shell/src/features/workbench/HomePage.tsx`
- `apps/desktop-shell/src/features/session-workbench/SessionWorkbenchPage.tsx`

### Files expected to be added

Representative examples:

- `apps/desktop-shell/src/lib/desktop/transport.ts`
- `apps/desktop-shell/src/lib/desktop/bootstrap.ts`
- `apps/desktop-shell/src/features/session-workbench/api/client.ts`
- `apps/desktop-shell/src/features/session-workbench/api/query.ts`
- `apps/desktop-shell/src/features/settings/api/client.ts`
- `apps/desktop-shell/src/features/code-tools/api/client.ts`

The exact filenames can adapt to existing project conventions, but the separation of concerns should remain.

## Testing Strategy

This refactor should add missing verification infrastructure where needed.

Minimum expected test direction:

- unit tests for extracted pure helpers
- component or hook tests for state ownership transitions
- regression tests around session selection and navigation behavior
- build verification for the desktop shell

Where code is difficult to test, that is usually evidence that boundaries are still too entangled.

## Success Criteria

The refactor is complete when all of the following are true:

1. A contributor can determine the active screen primarily from Router state.
2. Session list/detail remote data is not redundantly mirrored in Redux.
3. `src/lib/tauri.ts` is no longer the central dumping ground for all desktop API concerns.
4. Redux store contents are small, obvious, and UI-only.
5. `antd` and `styled-components` are removed from `apps/desktop-shell`.
6. The app still builds successfully and core user flows continue to work.

## Non-Goals and Constraints

- This is not a product redesign initiative.
- This is not a rewrite-from-scratch effort.
- This is not a Rust API modernization project.
- This is not a justification to reorganize unrelated features.

The guiding rule is incremental architectural cleanup that compounds maintainability without stalling delivery.

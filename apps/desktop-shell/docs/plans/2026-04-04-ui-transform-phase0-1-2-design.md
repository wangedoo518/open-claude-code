# UI Transform Design — Phase 0, 1, 2

> Approved 2026-04-04. Scope: ViewMode + Theme + TopBar dual-row.

## Phase 0: ViewMode Unified Navigation State

**Problem**: `homeSection` and `activeHomeSessionId` are independent fields in `ui.ts`, creating state conflict risk.

**Solution**: Replace with unified `ViewMode` discriminated union:

```typescript
type NavSection = "overview" | "session" | "search" | "scheduled" | "dispatch" | "customize" | "openclaw" | "settings";

type ViewMode =
  | { kind: "nav"; section: NavSection }
  | { kind: "session"; sessionId: string };
```

**Files changed**:
| File | Action |
|------|--------|
| `store/slices/ui.ts` | Replace `homeSection` + `activeHomeSessionId` with `viewMode` field |
| `features/workbench/tab-helpers.ts` | Update `openHomeSession` / `openHomeOverview` to dispatch `setViewMode` |
| `features/workbench/HomePage.tsx` | Read `viewMode` instead of old fields, derive `homeSection` + `activeHomeSessionId` |
| `features/workbench/AppsPage.tsx` | Use `setViewMode` instead of `setHomeSection` |
| `shell/TabBar.tsx` | Use `setViewMode` instead of `setHomeSection` + `setActiveHomeSessionId` |

**Default**: `{ kind: "nav", section: "overview" }` — equivalent to old `homeSection: "overview", activeHomeSessionId: null`.

**Migration**: ui slice is blacklisted from Redux Persist, so no migration needed.

---

## Phase 1: Theme System `.theme-warwolf`

**Strategy**: Dual-track. Add `.theme-warwolf` CSS class override on `<html>`. Original shadcn theme preserved, switchable.

**CSS variables added** (from DESIGN_TOKENS.md):
- Light: pure gray backgrounds (`rgb(250,250,250)`), not warm tones
- Dark: `rgb(25,25,25)` backgrounds
- Brand: `rgb(215,119,87)` Claude Orange (identical in both modes)
- Message-specific: `--color-msg-user`, `--color-msg-assistant`, `--color-msg-bash`
- Label-specific: `--color-label-you` (blue), `--color-label-claude` (orange)

**Files changed**:
| File | Action |
|------|--------|
| `globals.css` | Append `.theme-warwolf` and `.theme-warwolf.dark` variable blocks |
| `components/ThemeProvider.tsx` | Add `warwolfEnabled` state, toggle `theme-warwolf` class on `<html>` |
| `store/slices/settings.ts` | Add `warwolfTheme: boolean` setting (default `true`) |

**Activation**: `<html class="dark theme-warwolf">` or `<html class="light theme-warwolf">`.

---

## Phase 2: TopBar Dual-Row Restructure

**Current**: Single 40px row with cherry-studio tabs (首页/应用) + theme + settings.

**Target**: Two rows:
- **Row 1 (36px)**: `[traffic lights] [Nav: 首页 | 应用 | 设置] [spacer] [Theme toggle]` — always visible
- **Row 2 (32px)**: `[traffic lights spacer] [Session tabs...] [+ New]` — shown only when session tabs exist

**Key decisions**:
1. Row 1 nav items are fixed buttons (dispatch `setViewMode`), NOT Redux tabs
2. Row 2 only contains closable session/minapp tabs from Redux
3. Row 2 auto-hides when empty (saves 32px vertical space)
4. Elevation: Row 1 `border-b border-border/50`, Row 2 `shadow-sm`

**Files changed**:
| File | Action |
|------|--------|
| `shell/TabBar.tsx` | Rewrite to dual-row layout |
| `shell/TabItem.tsx` | Height 30px → 28px for Row 2 compactness |
| `store/slices/tabs.ts` | Remove SYSTEM_TABS (nav moves to Row 1 buttons); keep addTab/removeTab for session tabs |

---

## Dependency Order

```
Phase 0 (ViewMode) ──→ Phase 2 (TopBar uses ViewMode for nav state)
Phase 1 (Theme)    ──→ Phase 2 (TopBar uses warwolf colors)
Phase 0 ∥ Phase 1 are independent, but executed sequentially for clarity
```

Execution: **Phase 0 → Phase 1 → Phase 2**

# Display Density Sprint (E23–E27) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Apply the rules from `docs/desktop-shell/specs/2026-05-11-display-density-spec.md` to the 5 surfaces flagged in the v0.1.14 hands-on audit. After this sprint: Home shows actions before data, Composer hides advanced toggles by default, Status bar reads as 2 grouped clusters of icon-only badges, Left rail has visible section separators, every page-level title uses one of 5 typography tokens.

**Architecture:** No new infrastructure. Refactor existing components in-place + introduce 5 typography utility classes. The legacy `BuddyStatusBar` becomes the shared `<StatusBar>` (rename + extend). DashboardPage gets a layout reorder (data-only change to the existing return statement, no new components). Composer collapses 2 chips behind a `[···]` popover.

**Tech Stack:** React 19 + TypeScript + Tailwind 4 + lucide-react (no additions).

**Slicing:** 5 epics, executed in **mandatory dependency order**. E27 + E25 ship foundation that other epics consume; E23 + E24 are the user-facing paydirt.

| # | Epic | Why this order | User-visible impact |
|---|------|----------------|---------------------|
| 1 | **E27 — Typography ladder** | Other epics' string replacements depend on the new utility classes existing | Subtle (page titles shrink ~30%) |
| 2 | **E25 — StatusBar grouping** | Touches one shared component used by every page; doing later means redoing audits | Medium |
| 3 | **E26 — Left rail polish** | Tiny scoped change — clear it before the big DashboardPage edit | Low |
| 4 | **E23 — Home Pulse rebuild** | Biggest single payoff; uses E27 typography tokens | **Highest** |
| 5 | **E24 — Composer simplify** | Isolated to Composer.tsx + AskHeader.tsx | High (every chat message) |

**Why this design (over alternatives):**
- **No new component library** — buddy already has ad-hoc components per surface. Adding a "shadcn-style" component framework would 3x the scope. We refactor in-place.
- **Typography classes in `@theme`, not arbitrary values** — already the project's pattern; `text-[28px]` was the one violation (DashboardPage.tsx:375).
- **Composer collapses behind `[···]`, not removed** — `ResponseModeChip` and `PurposeLensChip` are real features users sometimes need; hiding ≠ deleting.

**Out of scope (defer to E28+):**
- Knowledge / WikiArticle density (waiting for real data — empty state today)
- Drafts list intro copy folding (cosmetic; <0.5h)
- Animation / transition rules (separate spec)
- Mobile/responsive (buddy is desktop)
- Color palette adjustments (kept on terracotta + cream)

---

## Slice E27 — Typography ladder

After this slice ships: 5 utility classes exist, one offending `text-[28px]` is replaced. Ground for E23/E24/E25 to use them.

### Task 1: Add 5 typography utility classes to globals.css

**Files:**
- Modify: `apps/desktop-shell/src/globals.css` (the `@theme` block, lines 1–100)

**Step 1: Locate the @theme block**

Open `apps/desktop-shell/src/globals.css`. Find the existing `--font-size-*` variable definitions inside `@theme` (around line 8–15). Confirm where the block ends (recon said line 90).

**Step 2: Add the 5 classes BELOW the existing font-size variables**

Insert into the `@theme` block:

```css
/* E27 — semantic typography ladder. All page-level text MUST use one
   of these 5 classes; no inline style={{fontSize}} or text-[Npx]
   arbitrary values. See specs/2026-05-11-display-density-spec.md §2. */
.text-page-title {
  font-size: 20px;
  font-weight: 600;
  color: var(--color-foreground);
  line-height: 1.3;
}
.text-section-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--color-foreground);
  line-height: 1.4;
}
.text-body {
  font-size: 13px;
  font-weight: 400;
  color: var(--color-foreground);
  line-height: 1.5;
}
.text-meta {
  font-size: 12px;
  font-weight: 400;
  color: var(--color-muted-foreground);
  line-height: 1.4;
}
.text-micro {
  font-size: 11px;
  font-weight: 400;
  color: var(--color-muted-foreground);
  opacity: 0.85;
  line-height: 1.3;
}
```

**Step 3: Verify the classes are picked up by the build**

Run: `cd apps/desktop-shell && npm run build 2>&1 | tail -3`
Expected: build succeeds, no Tailwind warnings about unknown classes.

**Step 4: Commit**

```bash
git add apps/desktop-shell/src/globals.css
git commit -m "$(cat <<'EOF'
feat(globals): add 5 semantic typography utility classes (E27.1)

Per docs/desktop-shell/specs/2026-05-11-display-density-spec.md §2.
Future PRs that introduce inline style={{fontSize}} or text-[Npx]
arbitrary values should be rejected — use these instead:

  text-page-title    20 / 600 / foreground
  text-section-title 14 / 600 / foreground
  text-body          13 / 400 / foreground
  text-meta          12 / 400 / muted-foreground
  text-micro         11 / 400 / muted-foreground @ 0.85

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: Replace the one inline-fontsize offender

**Files:**
- Modify: `apps/desktop-shell/src/features/dashboard/DashboardPage.tsx:375`

**Step 1: Find the line**

Run: `grep -n "text-\[28px\]" apps/desktop-shell/src/features/dashboard/DashboardPage.tsx`
Expected: 1 hit at line 375.

**Step 2: Replace**

The current line (recon):
```tsx
<h1 className="mt-2 text-[28px] font-semibold tracking-normal">{headline}</h1>
```
Becomes:
```tsx
<h1 className="mt-2 text-page-title tracking-normal">{headline}</h1>
```

(Drops `font-semibold` because `text-page-title` already sets `font-weight: 600`.)

**Step 3: Tsc + visual verify**

```bash
cd apps/desktop-shell && npx tsc --noEmit
```

Expected: clean.

Then `npm run tauri:dev` and confirm the Home page header is now smaller (~20px instead of 28px). The page should feel like its first ~80px of vertical space is more compact.

**Step 4: Commit**

```bash
git add apps/desktop-shell/src/features/dashboard/DashboardPage.tsx
git commit -m "fix(dashboard): use text-page-title token (E27.2)

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

### Task 3: Lint guard against future regressions

**Files:**
- Modify: `apps/desktop-shell/src/globals.css` (add a comment block at the top of the new section)

**Step 1: Verify no other inline-fontsize / arbitrary text-[…] hits exist**

```bash
cd apps/desktop-shell && grep -rn "style={{\s*fontSize" src/ | head -10
cd apps/desktop-shell && grep -rn 'text-\[\d' src/ | head -10
```
Expected: zero matches across both (recon confirmed). If new hits exist, replace them inline before continuing.

**Step 2: Add a script-friendly guard comment in globals.css**

The 5 utility classes block from Task 1 already has a `/* E27 — */` header. Make sure the comment includes the grep patterns to use:

```css
/* E27 — typography ladder. PR review check:
   grep -rn "style={{\s*fontSize" apps/desktop-shell/src/ → must be 0
   grep -rn "text-\[\d"            apps/desktop-shell/src/ → must be 0
   Use text-page-title / text-section-title / text-body / text-meta / text-micro instead.
*/
```

**Step 3: Commit (no code change, comment only)**

(If T1's comment was already detailed enough, skip this task — just confirm with grep.)

---

## Slice E25 — StatusBar grouping + icon-only

After this slice ships: bottom status bar reads as 2 visually distinct clusters (vault / session) of icon-only badges with hover labels.

### Task 4: Refactor BuddyStatusBar to grouped + icon-only

**Files:**
- Modify: `apps/desktop-shell/src/shell/BuddyStatusBar.tsx` (1–196)
- (Optionally rename to `apps/desktop-shell/src/shell/StatusBar.tsx` — defer rename to avoid import churn this slice)

**Step 1: Read the existing structure**

Skim `BuddyStatusBar.tsx` lines 76–132. Identify:
- Left items (vault group): HeartPulse / Inbox / GitBranch (lines 80–96)
- Right items (session group): PermissionIcon / Bot / Shield / CheckCircle2 (lines 99–129)

**Step 2: Refactor each item to icon-only + hover label**

For each `<div>` rendering an item with `<icon /> + <span>{label}</span> + <span>{count}</span>`, change to:

```tsx
<button
  type="button"
  className="ds-status-bar-item"
  title={`${label}${count !== undefined ? ` · ${count}` : ""}`}
  aria-label={`${label}${count !== undefined ? ` ${count}` : ""}`}
  onClick={onClick}
>
  <Icon className="size-3.5" />
  {count !== undefined && <span className="ml-1 text-micro">{count}</span>}
</button>
```

Keep the count visible (it's a glanceable signal); drop the prose label (it's available on hover via `title` and tooltip).

**Step 3: Add a vertical separator between left and right groups**

Insert between the left group's closing `</div>` and the right group's opening `<div>`:

```tsx
<div
  aria-hidden="true"
  className="mx-2 h-4 w-px self-center bg-border/60"
/>
```

**Step 4: Update CSS for `.ds-status-bar-item`**

In `globals.css`, find or add:

```css
.ds-status-bar-item {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  padding: 2px 6px;
  border-radius: 4px;
  color: var(--color-muted-foreground);
  background: transparent;
  border: 0;
  cursor: pointer;
  transition: background-color 120ms ease, color 120ms ease;
}
.ds-status-bar-item:hover {
  background: var(--color-muted);
  color: var(--color-foreground);
}
```

If `.ds-status-bar` itself needs a max-height adjustment (currently looks fine), set `min-height: 28px; max-height: 28px;`.

**Step 5: Verify all 5 surfaces still render the bar correctly**

`npm run tauri:dev`, navigate to Home / Ask / Knowledge / Drafts / Settings — bottom bar in each should look identical (icon + count, hover shows label).

**Step 6: Commit**

```bash
git add apps/desktop-shell/src/shell/BuddyStatusBar.tsx apps/desktop-shell/src/globals.css
git commit -m "$(cat <<'EOF'
feat(shell): StatusBar groups + icon-only + hover labels (E25)

Status bar is shared across all pages (BuddyStatusBar.tsx). Splits
into vault group (待处理 / Inbox / Git) and session group
(permission / mode / context / pages) with a 1px vertical
separator between them. Each item is icon-only by default; hover
shows the full label + count via title attribute. Total bar
height capped at 28px.

Per docs/desktop-shell/specs/2026-05-11-display-density-spec.md §4.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Slice E26 — Left rail polish

After this slice ships: left rail has a clearly visible 1px separator between "daily" and "tune" sections. ("tune" section opacity-muted by default to signal "not your daily path".)

### Task 5: Visible separator + muted "tune" group

**Files:**
- Modify: `apps/desktop-shell/src/shell/Sidebar.tsx` (52–143)
- Modify: `apps/desktop-shell/src/globals.css` (add `.ds-rail-separator` styles if absent / refresh if present)

**Step 1: Verify current separator visibility**

Recon said `<div className="ds-rail-separator" aria-hidden="true" />` already exists between the daily and tune `.map(...)` blocks. Open Sidebar.tsx and confirm. Then `grep "ds-rail-separator" apps/desktop-shell/src/globals.css` — does the class have visual styling?

If absent, add to globals.css:

```css
.ds-rail-separator {
  height: 1px;
  margin: 8px 12px;
  background: var(--color-border);
  opacity: 0.6;
}
```

If present but barely visible (e.g. `opacity: 0.2`), bump to 0.6.

**Step 2: Mute the "tune" group icons by default**

In `Sidebar.tsx`, find the tune-section `.map((route) => <RailItem ... />)` block (recon line 99–105). Wrap the items in a `data-rail-group="tune"` container:

```tsx
<div data-rail-group="tune" className="contents">
  {tuneItems.map((route) => <RailItem ... />)}
</div>
```

Then in globals.css:

```css
[data-rail-group="tune"] .ds-rail-item {
  opacity: 0.6;
  transition: opacity 120ms ease;
}
[data-rail-group="tune"] .ds-rail-item:hover,
[data-rail-group="tune"] .ds-rail-item[data-active="true"] {
  opacity: 1;
}
```

(Adjust class names to match what `RailItem` actually renders — read it first.)

**Step 3: Commit**

```bash
git add apps/desktop-shell/src/shell/Sidebar.tsx apps/desktop-shell/src/globals.css
git commit -m "$(cat <<'EOF'
feat(sidebar): visible group separator + muted tune section (E26)

The left rail already has a separator <div> between daily and tune
sections, but its CSS was too subtle to read at a glance. Bumps
opacity to 0.6 + adds data-rail-group="tune" muting (60% opacity,
restored on hover/active) so the user sees "5 daily icons + 2
secondary icons + Settings" rather than 8 equally-weighted dots.

Per docs/desktop-shell/specs/2026-05-11-display-density-spec.md §5.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Slice E23 — Home Pulse rebuild (the big one)

After this slice ships: a user opens Home and sees Top 3 actions in the first viewport. Stat row is one compact line. All other panels (purpose tiles / 减熵 / 投入回报 / 外部 AI) are collapsed `<details>` expandable sections.

### Task 6: Reorder DashboardPage layout — Top 3 above stats

**Files:**
- Modify: `apps/desktop-shell/src/features/dashboard/DashboardPage.tsx` (366–556)

**Step 1: Audit the current top-of-return JSX**

Open `DashboardPage.tsx` lines 366–556. The current order (per recon):

```
1. Header (369–390): title + primary CTA
2. Loading state (392–397)
3. Stat cards grid (399–439): 3 HealthCard
4. PurposeWeeklyDigest (441–445)
5. EntropyPulsePanel (447–451)
6. RoiPulsePanel (453)
7. Bottom grid: Top 3 + External AI (455–540)
8. Mini stat row (542–553)
```

Target order:

```
1. Header (compact, ≤80px tall)
2. Top 3 行动 (was at 455–491, MOVE to second position)
3. Stat row inline (compact: 3 stats in one row, ≤60px)
4. Collapsible: 本周目的的流动 (PurposeWeeklyDigest) — collapsed by default
5. Collapsible: 减熵脉冲 (EntropyPulsePanel) — collapsed by default
6. Collapsible: 投入回报 (RoiPulsePanel) — collapsed by default
7. Collapsible: 外部 AI 权限 (was at 493–527) — collapsed by default
8. Mini stat row (delete — duplicates section 3)
```

**Step 2: Extract the Top 3 block into a top-of-page position**

Cut the block at lines 455–491 (the `<Card><HeartPulse/>Top 3 行动...` block) and paste it as the FIRST element after the header (between current line 397 and 399).

Adjust styling: previously it was inside a 2-col grid sharing space with External AI; now it's full-width. Drop the grid wrapper for Top 3, give it `className="rounded-lg border border-border bg-card p-4"`.

**Step 3: Compress stat cards to inline row**

The 3 `HealthCard` components (lines 399–439) currently render as `grid md:grid-cols-3` with each card ~100px tall.

Replace with a single `<div className="flex items-center gap-4 rounded-md border border-border bg-card px-3 py-2">`:

```tsx
<div className="flex items-center gap-4 rounded-md border border-border bg-card px-3 py-2 text-meta">
  <div className="flex items-center gap-1.5">
    <Inbox className="size-3.5" />
    <span className="text-section-title">{inboxCount}</span>
    <span>待审</span>
  </div>
  <div className="h-4 w-px bg-border/60" />
  <div className="flex items-center gap-1.5">
    <Shield className="size-3.5" />
    <span className="text-section-title">{healthScore}</span>
    <span>知识质量</span>
  </div>
  <div className="h-4 w-px bg-border/60" />
  <div className="flex items-center gap-1.5">
    <GitBranch className="size-3.5" />
    <span className="text-section-title">{gitChanges}</span>
    <span>Git 改动</span>
  </div>
</div>
```

(Names like `inboxCount` come from existing variables in DashboardPage scope — find them first by reading the existing HealthCard prop bindings around lines 399–439.)

Delete the `HealthCard` JSX entirely. If `HealthCard` is used elsewhere, leave the component definition; if not, defer cleanup to a later sweep.

**Step 4: Wrap PurposeWeeklyDigest, EntropyPulsePanel, RoiPulsePanel, External AI in `<details>`**

For each of the 4 sections, wrap their existing JSX in a `<details>` whose summary acts as the section header:

```tsx
<details className="rounded-lg border border-border bg-card">
  <summary className="cursor-pointer px-4 py-2 text-section-title flex items-center gap-2 list-none [&::-webkit-details-marker]:hidden">
    <Heart className="size-3.5 text-primary" />
    本周目的的流动
    <span className="ml-auto text-meta">展开 ⌄</span>
  </summary>
  <div className="border-t border-border px-4 py-4">
    <PurposeWeeklyDigest ... />
  </div>
</details>
```

Repeat for the other 3 sections. The `summary` text should mirror the section's title; the inner div mounts the existing component.

**Step 5: Delete the duplicate mini stat row at 542–553**

Cut the block. The compact stat row in section 3 already covers it.

**Step 6: Tsc + visual verify**

```bash
cd apps/desktop-shell && npx tsc --noEmit && npm run build 2>&1 | tail -3
```

Then `npm run tauri:dev`, open Home. Verify:
- First viewport (no scrolling) shows: header + Top 3 actions list + 1 compact stat row
- 4 collapsible sections visible as headers below
- No second mini stat row at the bottom
- Page feels much shorter

**Step 7: Commit**

```bash
git add apps/desktop-shell/src/features/dashboard/DashboardPage.tsx
git commit -m "$(cat <<'EOF'
feat(dashboard): Home Pulse rebuild — actions before data (E23)

Reorders DashboardPage so the user's first viewport shows the Top
3 行动 list + a compact 3-stat inline row. Everything else (本周
目的的流动 / 减熵脉冲 / 投入回报 / 外部 AI 权限) becomes a
collapsible <details> section so it doesn't dilute the primary
action.

Detail changes:
  - Top 3 actions block extracted from the bottom 2-col grid and
    promoted to position 2 (right after the page header)
  - 3 HealthCard tall cards (~100px each) collapsed to one inline
    flex row (~32px total)
  - 4 sections wrapped in <details> + section-title summary
  - Mini stat row at bottom removed (duplicate of compact row)
  - Page header h1 already shrunk to text-page-title in E27

Per docs/desktop-shell/specs/2026-05-11-display-density-spec.md §6.1.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Slice E24 — Composer simplification

After this slice ships: composer toolbar reads as `📎 [输入框] [···] [Model▾] [→]` with all advanced toggles behind the `[···]` popover. Chat header drops the inline token counter (move to hover).

### Task 7: Move ResponseModeChip + PurposeLensChip behind a `[···]` popover

**Files:**
- Modify: `apps/desktop-shell/src/features/ask/Composer.tsx` (1031–1200, esp. 1140 / 1146)

**Step 1: Identify what's currently inline**

Read Composer.tsx 1031–1200. Locate (per recon):
- Line 1140: `<ResponseModeChip ... />` (the "需要确认" context mode UI)
- Line 1146: `<PurposeLensChip ... />` (the "目的" selection)
- Lines 1065–1091: 代码 / 计划 mode toggle buttons (inline — confirm if these stay or fold)

**Step 2: Decide what goes in the popover**

Per spec §6.2: 5 high-level pills should fold into `[···]`. Map to actual current Composer state:
- **Stays inline**: 📎 attachment, slash command `>`, model selector, send button
- **Goes to `[···]`**: 代码 toggle, 计划 toggle, ResponseModeChip ("需要确认"), PurposeLensChip ("目的")
- **"继续前文"**: not in toolbar; ignore (slash command)

If the 代码/计划 toggle buttons feel high-frequency, keep them inline and only fold the 2 chips. **Default decision: fold all 4 (代码/计划/needs-confirm/purpose) into `[···]`.** User can override this call during execution.

**Step 3: Build the popover**

Wrap the 4 elements in a popover. Use the existing popover pattern from VerdictPicker (`apps/desktop-shell/src/features/wiki/VerdictPicker.tsx`) — it's the same project's idiom.

```tsx
const [advancedOpen, setAdvancedOpen] = useState(false);
// ... in toolbar JSX:
<div className="relative">
  <button
    type="button"
    className="ask-composer-advanced-trigger"
    onClick={() => setAdvancedOpen((v) => !v)}
    title="高级选项：代码模式 / 计划模式 / 需要确认 / 目的"
    aria-label="高级选项"
  >
    <MoreHorizontal className="size-4" />
  </button>
  {advancedOpen && (
    <div className="ask-composer-advanced-popover" role="dialog" aria-label="高级选项">
      <div className="ask-composer-advanced-row">
        {/* 代码 toggle */}
        {/* 计划 toggle */}
      </div>
      <div className="ask-composer-advanced-row">
        <ResponseModeChip ... />
      </div>
      <div className="ask-composer-advanced-row">
        <PurposeLensChip ... />
      </div>
    </div>
  )}
</div>
```

Add `MoreHorizontal` to the lucide imports.

**Step 4: Add CSS to globals.css**

```css
.ask-composer-advanced-trigger {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: 6px;
  border: 1px solid var(--color-border);
  background: transparent;
  color: var(--color-muted-foreground);
  cursor: pointer;
  transition: background-color 120ms ease, color 120ms ease;
}
.ask-composer-advanced-trigger:hover,
.ask-composer-advanced-trigger[data-active="true"] {
  background: var(--color-muted);
  color: var(--color-foreground);
}
.ask-composer-advanced-popover {
  position: absolute;
  bottom: calc(100% + 6px);
  left: 0;
  z-index: 20;
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 280px;
  padding: 8px;
  border-radius: 8px;
  border: 1px solid var(--color-border);
  background: var(--color-card);
  box-shadow: 0 4px 16px rgb(0 0 0 / 0.08);
}
.ask-composer-advanced-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
```

**Step 5: Click-outside-to-close**

Use a ref + `useEffect` listening on `mousedown` outside the popover:

```tsx
const popoverRef = useRef<HTMLDivElement>(null);
useEffect(() => {
  if (!advancedOpen) return;
  const onDown = (e: MouseEvent) => {
    if (popoverRef.current && !popoverRef.current.contains(e.target as Node)) {
      setAdvancedOpen(false);
    }
  };
  document.addEventListener("mousedown", onDown);
  return () => document.removeEventListener("mousedown", onDown);
}, [advancedOpen]);
```

Attach `ref={popoverRef}` to the popover div.

**Step 6: Tsc + visual verify**

`npx tsc --noEmit` clean. `npm run tauri:dev` open Ask → composer shows fewer items inline; click `[···]` reveals 4 advanced controls.

**Step 7: Commit**

```bash
git add apps/desktop-shell/src/features/ask/Composer.tsx apps/desktop-shell/src/globals.css
git commit -m "$(cat <<'EOF'
feat(ask): composer advanced toggles fold into [···] popover (E24.1)

Reduces composer toolbar from 11 inline elements to 5 (📎, [···],
input, model, send). The 4 high-level toggles (代码 / 计划 /
ResponseModeChip / PurposeLensChip) now live in a popover behind a
[···] button — same idiom as VerdictPicker. Click-outside closes.

Per docs/desktop-shell/specs/2026-05-11-display-density-spec.md §6.2.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 8: Hide chat-header token metrics behind hover

**Files:**
- Modify: `apps/desktop-shell/src/features/ask/AskHeader.tsx` (66–71)

**Step 1: Wrap the metrics span in a hover-only display**

Currently (recon line 66–71):
```tsx
{metricsLabel && <span className="ask-chat-header-metrics">{metricsLabel}</span>}
```

Change to:
```tsx
{metricsLabel && (
  <span
    className="ask-chat-header-metrics opacity-0 transition-opacity duration-150 group-hover:opacity-100"
    title={metricsLabel}
  >
    {metricsLabel}
  </span>
)}
```

Then add `group` class to the `<header>` parent so the hover is scoped to the header.

(If the header doesn't currently have `group`, add it: `className="... group ..."`.)

**Step 2: Tsc + visual verify**

`npm run tauri:dev` Ask page → top right of chat header no longer shows "用时 1.3s · ↑1 ↓58 tokens" by default; hover the header reveals it.

**Step 3: Commit**

```bash
git add apps/desktop-shell/src/features/ask/AskHeader.tsx
git commit -m "feat(ask): chat header token metrics hover-reveal only (E24.2)

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
"
```

---

## Task 9: Bump v0.1.15 + plan link + ship

**Files:**
- Modify: `apps/desktop-shell/src-tauri/tauri.conf.json` (version)
- Modify: `apps/desktop-shell/src-tauri/Cargo.toml` (version)
- Modify: `docs/desktop-shell/plans/README.md` (link this plan)

**Step 1: Final verification**

```bash
cd rust && cargo test --workspace --quiet 2>&1 | grep -E "test result|FAILED" | head -15
cd apps/desktop-shell && npx tsc --noEmit && npm run build 2>&1 | tail -3
```
Expected: workspace tests still green (no Rust changes this sprint), tsc clean, build succeeds.

**Step 2: Bump version**

```bash
sed -i 's/"version": "0.1.14"/"version": "0.1.15"/' apps/desktop-shell/src-tauri/tauri.conf.json
sed -i 's/^version = "0.1.14"$/version = "0.1.15"/' apps/desktop-shell/src-tauri/Cargo.toml
cd apps/desktop-shell/src-tauri && cargo check 2>&1 | tail -2
```

**Step 3: Add plan link**

In `docs/desktop-shell/plans/README.md`:

```markdown
- [Display Density Sprint (E23-E27) Implementation Plan](./2026-05-11-display-density-sprint-plan.md)
```

**Step 4: Commit + tag + push**

```bash
git add docs/desktop-shell/plans/README.md docs/desktop-shell/plans/2026-05-11-display-density-sprint-plan.md \
  apps/desktop-shell/src-tauri/tauri.conf.json apps/desktop-shell/src-tauri/Cargo.toml \
  apps/desktop-shell/src-tauri/Cargo.lock
git commit -m "$(cat <<'EOF'
chore: bump desktop-shell to v0.1.15 + Display Density sprint plan

E23-E27 ships the first systematic UI density pass. Five epics:

  E27 — typography ladder (5 utility classes, replaces inline 28px)
  E25 — StatusBar grouped + icon-only across all pages
  E26 — Left rail visible separator + muted tune section
  E23 — Home Pulse rebuild: actions above data, panels collapsed
  E24 — Composer simplification: advanced toggles in [···] popover

Foundation for future UI work — the spec at
docs/desktop-shell/specs/2026-05-11-display-density-spec.md is now
the binding rulebook.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
git tag -a v0.1.15 -m "v0.1.15: Display Density Sprint (E23-E27)"
git push origin main
git push origin v0.1.15
```

---

## Done criteria

After all 9 tasks ship:

1. **Home**: open Home → first viewport shows header + Top 3 actions + 1 compact stat row + 4 collapsed `<details>` headers. No 6 purpose tiles dominating the page.
2. **Ask**: composer toolbar has 5 visible elements (📎, [···], input, model, send). Clicking [···] reveals 4 advanced toggles. Chat header doesn't show token counter unless you hover.
3. **Status bar**: every page shows the same icon-only bar with vault group / session group separated by a vertical line. Hover any icon shows label + count.
4. **Left rail**: visible 1px separator between daily and tune sections. Tune icons (Σ / connections) muted to ~60% opacity by default.
5. **Typography**: `grep -rn "style={{\s*fontSize" apps/desktop-shell/src/` returns 0. `grep -rn 'text-\[\d' apps/desktop-shell/src/` returns 0.
6. **Tests**: workspace cargo tests still green (no Rust changes); tsc clean; vite build clean.
7. **Visual smoke**: open Home, Ask, Knowledge, Drafts, Settings — each surface should feel calmer than v0.1.14.

## Risks called out

1. **`<details>` styling default browser look** — Chrome's `<details>` has a default disclosure triangle that looks bad next to our card design. Suppress via `summary { list-style: none } summary::-webkit-details-marker { display: none }`. Already in the Task 6 example.
2. **PurposeWeeklyDigest auto-collapse hides legitimate actions** — if a user has many active purposes, collapsing by default means they have to click to see weekly progress. Mitigation: auto-expand `<details>` if any tile inside has `weekly_count > 0` (use `defaultOpen` prop or `open` attribute in JSX).
3. **Composer popover loses keyboard focus when clicking outside** — verify Tab navigation through the popover works; if not, add focus trap (defer if minor).
4. **Chat header hover-reveal hides info from screen readers** — the metrics are still in the DOM via `<span>`, just `opacity:0`. Screen readers will read it (they don't honor opacity by default). If concerned, add `aria-hidden="true"` when not hovered. **Defer this call**.
5. **Dependency order strictness** — E27 must ship before E23/E24 because they reference the new typography classes. If executing out of order, build may break with "unknown CSS class" warnings (Tailwind 4 is more lenient than older versions, but still a risk).

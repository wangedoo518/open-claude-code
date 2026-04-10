# Maintenance Policy

## The 5 maintenance actions (canonical section 8 Triggers)

Every `raw_ingested(source_id)` event MUST fire these 5 actions:

1. **Summarise** the new source (max 200 words, quote max 15 consecutive words)
2. **Update affected** concept/people/topic/compare pages
3. **Add/update backlinks** (bidirectional: A->B implies B->A)
4. **Detect conflicts** -> `mark_conflict` -> Inbox (never silently merge)
5. **Append to changelog** (`changelog/YYYY-MM-DD.md`) + rebuild `index.md`

## Ordering

Actions 1-4 can run in any order. Action 5 runs last (after all writes).

## Failure handling

If any action fails, log the error and continue with the remaining actions.
Never let a changelog failure block a page write. Never let a backlink
failure block a summarise. The user can re-run from the Inbox.

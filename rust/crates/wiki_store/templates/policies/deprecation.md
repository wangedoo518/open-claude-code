# Deprecation Policy

## When to deprecate a page

- The page's content has been fully superseded by a newer, more accurate page
- The page was created from a source that has been retracted or corrected
- Two pages cover the same topic and should be merged into one

## Rules

1. NEVER deprecate a page without a replacement slug
2. Set `status: deprecated` in the frontmatter
3. Add a note at the top: `> This page has been superseded by [replacement](concepts/replacement.md).`
4. Keep the deprecated page on disk (do not delete) — it's part of the wiki's history
5. Remove the deprecated page from `index.md` on the next rebuild
6. The deprecation must go through the Inbox (InboxKind::Deprecate)

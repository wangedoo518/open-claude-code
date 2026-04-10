# Naming Policy

## Slugs

- Lowercase ASCII only: `a-z`, `0-9`, `-`
- No underscores, no dots, no spaces
- Max 64 characters
- Must be unique within their category directory
- Examples: `llm-wiki`, `rag-vs-llm-wiki`, `karpathy`

## Titles

- May contain any Unicode (CJK, accents, etc.)
- Should be concise: max ~60 characters
- Capitalize like a book title (first word + proper nouns)

## File paths

- `wiki/concepts/{slug}.md` for concept pages
- `wiki/people/{slug}.md` for people pages
- `wiki/topics/{slug}.md` for topic pages
- `wiki/compare/{slug}.md` for compare pages
- `wiki/changelog/YYYY-MM-DD.md` for daily changelogs

## Collision handling

If a slug already exists in the target category:
- The `write_wiki_page` function is idempotent — it overwrites
- This is correct for updates but NOT for creating a genuinely
  different page with the same slug
- When in doubt, append a disambiguator: `rag-2024` vs `rag-2026`

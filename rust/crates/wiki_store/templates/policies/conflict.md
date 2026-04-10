# Conflict Resolution Policy

## When to mark a conflict

A conflict exists when:
- A new source directly contradicts an existing wiki page's claims
- Two sources provide incompatible data about the same entity
- A page's "canonical" status is challenged by newer information

## What to do

1. NEVER silently overwrite the existing page
2. Call `mark_conflict` with the affected slugs and a clear reason
3. The user will see the conflict in their Inbox and decide:
   - Keep the existing page (reject the new information)
   - Update the page with new information (approve)
   - Mark the existing page as deprecated and create a new one

## Uncertainty

When uncertain whether something is a real conflict:
- Use `mark_conflict` with `reason="uncertain: {description}"`
- Let the user triage — false positives are better than silent overwrites

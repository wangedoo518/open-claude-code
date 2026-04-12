# ClawWiki Desktop LLM Gateway Integration

This path previously held a private gateway-integration draft.

It has been replaced with a public-safe placeholder because the
previous content described private gateway behavior, rollout details,
and non-public operational assumptions that should not be published as
part of the open-source repository.

Public replacement documents:

- [`../desktop-shell/specs/2026-04-12-desktop-shell-open-source-gateway-design.md`](../desktop-shell/specs/2026-04-12-desktop-shell-open-source-gateway-design.md)
- [`../desktop-shell/plans/2026-04-12-desktop-shell-open-source-gateway-plan.md`](../desktop-shell/plans/2026-04-12-desktop-shell-open-source-gateway-plan.md)

The public contract for gateway support is now intentionally generic:

- Anthropic-compatible gateway
- OpenAI-compatible gateway
- user-provided `base_url + api_key + model`

Private backend review should happen in a private documentation space,
not in this repo.

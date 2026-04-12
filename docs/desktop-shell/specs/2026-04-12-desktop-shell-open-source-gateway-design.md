---
title: Desktop Shell Open Source Boundary And API Key Gateway Design
doc_type: spec
status: active
owner: desktop-shell
last_verified: 2026-04-12
related:
  - docs/desktop-shell/README.md
  - docs/desktop-shell/specs/README.md
  - docs/desktop-shell/plans/README.md
---

# Desktop Shell Open Source Boundary And API Key Gateway Design

**Goal**

Define a reviewable design for open-sourcing `claudewiki` without leaking private backend or operations details, while preserving a first-class user flow for connecting any LLM gateway through `base_url + api_key`.

This document is for architecture review. It does not implement the change.

## Executive Conclusion

`claudewiki` should be open-sourced as a **general-purpose desktop shell plus general-purpose LLM provider client**, not as a bundled distribution of the current private cloud backend.

The recommended product split is:

- **Open-source core**
  - desktop UI
  - Ask runtime
  - multi-provider registry
  - generic Anthropic-compatible and OpenAI-compatible gateway support
  - local provider testing and local credential storage
- **Private extension / control plane**
  - user accounts
  - API key issuance
  - quota, billing, subscriptions, and rate-limit enforcement
  - account pools, scheduler, refresh loops, and upstream routing
  - deployment, monitoring, and operations

This split works because the current desktop already contains the right public abstraction:

- a local multi-provider config in `.claw/providers.json`
- protocol-level provider kinds instead of vendor-specific coupling
- a settings page that lets the user manage providers locally
- runtime resolution that can read the active provider on each turn

So the open-source path is primarily a **boundary cleanup and documentation problem**, not a net-new architecture invention.

## Evidence From Current Desktop Shell

The current desktop already has a generic provider model:

- `rust/crates/desktop-core/src/providers_config.rs` defines the on-disk schema for `.claw/providers.json`, with `kind`, `base_url`, `api_key`, `model`, and `max_tokens`.
- `apps/desktop-shell/src/features/settings/sections/MultiProviderSettings.tsx` already exposes a user-facing multi-provider settings panel and states that API keys remain local.
- `apps/desktop-shell/src/features/settings/api/client.ts` already has CRUD and test APIs for providers.
- `rust/crates/desktop-core/src/lib.rs` already resolves the active provider during runtime from `.claw/providers.json`.

These are exactly the building blocks an open-source desktop client needs.

## Evidence Of The Current Private Coupling

The current repository also contains material that is not suitable to ship as open-source core:

- `rust/crates/desktop-core/src/codex_broker.rs` implements a private cloud-account pool abstraction with encrypted local persistence.
- `rust/crates/desktop-core/src/managed_auth.rs` and `rust/crates/desktop-core/src/lib.rs` contain comments that directly tie generic gateway support to the current private product naming.
- `docs/clawwiki/desktop-llm-gateway-integration.md` and `docs/clawwiki/backend-design.md` document private backend topology, internal product assumptions, and rollout decisions that should not live in a public repo.

This means the repo is currently a hybrid of:

- reusable desktop-shell capabilities, and
- private product integration notes.

Open-sourcing requires separating those two layers explicitly.

## Evidence From The Reviewed Private Gateway

The reviewed private gateway implementation is useful as a **capability reference**, but not as something to publish through this repo.

Its reusable ideas are:

- user-managed API keys
- per-key policy controls such as expiry, quota, rate limits, and IP rules
- protocol-compatible gateway endpoints
- auth middleware that is independent from upstream account scheduling

Those ideas should influence the interface contract of the open-source desktop shell, but the shell must not depend on or expose any private implementation details, environment details, or operational workflows from that system.

## Problem Statement

The team needs one architecture that satisfies both constraints:

1. `claudewiki` must be publishable as open source.
2. End users must still be able to use a hosted LLM gateway via `api_key`.

The main risk is accidental leakage through the open-source repo itself:

- private backend names or assumptions embedded into code comments
- private domains or endpoints embedded into templates or examples
- tests that hit private services
- design docs that reveal internal control-plane structure
- operations documents, screenshots, or credentials copied into the public tree

## Design Principles

### 1. Protocol, not product, is the public contract

The open-source shell should depend only on wire protocol shape:

- `anthropic` for Messages API-compatible gateways
- `openai_compat` for ChatCompletions / Responses-compatible gateways

The shell must not know whether the server behind that endpoint is:

- a direct vendor API
- a self-hosted gateway
- a private scheduler over multiple upstreams
- a commercial managed service

### 2. The desktop client is not the control plane

The desktop shell may store a user-provided API key locally and attach it to outgoing requests, but it should not own:

- user lifecycle
- key issuance
- subscription state
- upstream account rotation
- quota accounting
- policy enforcement

Those belong to the gateway service.

### 3. OSS must not require private infrastructure to make sense

A new user reading the public repo should understand the product through:

- direct vendor APIs
- self-hosted compatible gateways
- custom gateway URLs

The public README and settings UX should not assume the existence of a specific private cloud product.

### 4. No private ops knowledge in the repo

Production operations documents, hostnames, credentials, internal admin procedures, and deployment topology are strictly out of scope for the open-source repo.

### 5. Private cloud features must be optional overlays

If the team keeps private cloud integrations in the same codebase temporarily, they must be isolated behind an explicit feature boundary so the open-source default build path is generic and reviewable.

## Proposed Open-Source Boundary

### Keep In OSS Core

- `apps/desktop-shell`
- generic settings UI for provider configuration
- `providers_config` schema and CRUD
- runtime provider resolution
- generic managed-auth providers that correspond to public vendor flows
- provider testing endpoints
- public docs for connecting standard APIs and compatible gateways

### Remove, Rewrite, Or Isolate From OSS Core

- private product branding in source comments and built-in descriptions
- private backend integration docs under `docs/clawwiki/`
- private cloud account pool logic when it is not required for generic local operation
- any tests or examples that depend on private staging or production services
- any templates or sample configs that reference private domains

### Recommended Code Boundary

The preferred shape is:

- `desktop-core` remains the generic runtime
- private cloud integration moves to a private crate, private feature flag, or downstream fork layer
- the open-source default binary exposes only generic provider paths

If the team cannot immediately split the code physically, the minimum acceptable interim boundary is:

- no private docs in the tree
- no private tests in the tree
- no private brand names in the public UX defaults
- a clearly documented feature gate for private cloud additions

## Proposed API Key Gateway User Flow

The user flow in the open-source product should be simple:

1. Open `Settings -> LLM Providers`
2. Choose `Anthropic-compatible gateway` or `OpenAI-compatible gateway`
3. Fill:
   - `display_name`
   - `base_url`
   - `api_key`
   - `model`
   - optional `max_tokens`
4. Click `Test`
5. Activate the provider
6. Ask runtime uses that provider on the next turn

This flow already matches the current product direction and requires no product-specific backend knowledge.

## Gateway Compatibility Contract

### Anthropic-Compatible Gateway

The open-source shell should document support for a gateway that provides:

- `POST {base_url}/v1/messages`
- `GET {base_url}/v1/models` or equivalent model discovery endpoint when available
- streaming responses compatible with the Anthropic Messages API event model
- `x-api-key` or bearer-style auth as configured by the provider kind

Required guarantees:

- stable terminal event semantics
- no leaking of upstream internal identifiers in error bodies
- protocol-correct error object shape

### OpenAI-Compatible Gateway

The open-source shell should document support for a gateway that provides:

- `POST {base_url}/chat/completions` or `POST {base_url}/v1/chat/completions`
- optional `POST {base_url}/responses` or `POST {base_url}/v1/responses`
- `GET {base_url}/models` or `GET {base_url}/v1/models`
- standard bearer-token auth

Required guarantees:

- correct stream termination
- compatible usage fields when available
- error bodies that do not reveal private scheduler internals

## Credential Handling

The current repository stores provider credentials in `.claw/providers.json`, with redacted debug output and best-effort restrictive file permissions.

This is acceptable as the short-term open-source baseline because:

- it is already implemented
- it is local-only
- it keeps the contract simple for self-hosters and power users

The follow-up hardening direction should be:

- migrate provider secrets to OS keychain or encrypted local storage
- keep `.claw/providers.json` for non-secret metadata plus provider ids
- preserve the current UX contract while improving at-rest storage

That storage hardening is a follow-up, not a prerequisite for publishing the repo.

## Documentation Strategy For OSS

The public documentation should explain only:

- how to connect direct vendor APIs
- how to connect custom compatible gateways
- what fields each provider type expects
- how local secret storage works today
- what guarantees the desktop makes and does not make

The public documentation should not explain:

- how the private cloud backend is deployed
- how upstream account pools are populated
- how subscriptions are fulfilled
- internal admin workflows
- internal domains, staging environments, or operational procedures

## Required Cleanup Before Publishing

### 1. Remove Or Rewrite Private Docs

These docs should not ship as public source-of-truth in their current form:

- `docs/clawwiki/desktop-llm-gateway-integration.md`
- `docs/clawwiki/backend-design.md`

They should either:

- move to a private documentation repository, or
- be rewritten into generic OSS-safe documents with no private product detail

### 2. Rename Product-Specific Comments

Code comments that currently frame generic gateway support as a private branded feature should be rewritten into generic protocol language.

### 3. Isolate Private Broker Logic

`codex_broker` and related cloud-account paths should be reviewed file-by-file and either:

- moved out of the OSS default build, or
- clearly feature-gated as private extensions

### 4. Scrub Tests And Examples

The team should search for:

- private domains
- private backend names
- internal ports
- staging URLs
- real request traces

Any such references must be deleted, mocked, or replaced with generic placeholders.

### 5. Add Public Gateway Docs

The repo should gain a public-facing guide for:

- configuring a custom Anthropic-compatible gateway
- configuring a custom OpenAI-compatible gateway
- debugging connection/auth/model errors locally

## Security And Confidentiality Rules

The following are hard rules for the open-source effort:

- no production operations document may be copied into this repo
- no credential, admin token, or database string may appear in docs, tests, screenshots, or examples
- no public example may point to a live private service
- no error message returned to the desktop should expose private upstream account identifiers or scheduler reasoning

## Rollout Shape

### Phase 1: Boundary Cleanup

- remove private docs from the repo
- scrub product-specific comments
- identify private-only modules and decide their feature boundary

### Phase 2: Public Gateway Positioning

- treat the multi-provider registry as the primary public configuration path
- polish provider templates and public docs
- verify generic gateway connectivity flows

### Phase 3: Storage Hardening

- move API key secrets toward keychain or encrypted storage
- preserve backward compatibility for existing local configs

### Phase 4: Open-Source Release Review

- legal review
- security review
- doc review
- final secret scan

## Review Questions

The team should explicitly review and decide:

1. Is the public product boundary defined as "desktop shell + generic provider client" rather than "desktop shell + private cloud service"?
2. Does `codex_broker` belong in the open-source default build?
3. Should provider secrets stay in `.claw/providers.json` for the first public release, or must keychain migration happen first?
4. Is the first public gateway contract limited to `anthropic` and `openai_compat`, with no private provider kinds?
5. Will `docs/clawwiki/` be removed from the public tree before release?
6. Who owns the final secret and operations scrub before the repo becomes public?

## Recommendation

Approve the open-source direction if and only if the team agrees on this boundary:

- `claudewiki` public repo = generic desktop shell and generic gateway client
- private backend = separate control plane with separate docs and operations

That boundary preserves the current product value, supports API-key-based gateway access cleanly, and avoids leaking private backend information through the open-source repository.

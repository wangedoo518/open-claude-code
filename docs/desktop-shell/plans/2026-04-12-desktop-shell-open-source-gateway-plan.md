---
title: Desktop Shell Open Source Boundary And API Key Gateway Plan
doc_type: plan
status: active
owner: desktop-shell
last_verified: 2026-04-12
related:
  - docs/desktop-shell/README.md
  - docs/desktop-shell/specs/2026-04-12-desktop-shell-open-source-gateway-design.md
  - docs/desktop-shell/plans/README.md
---

# Desktop Shell Open Source Boundary And API Key Gateway Plan

**Goal:** Prepare `claudewiki` for open-source release as a generic desktop shell with generic API-key-based gateway support, while removing or isolating private backend and operations details.

**Architecture:** Public repo keeps the generic provider registry and generic gateway runtime. Private cloud account pools, control-plane behaviors, and operations artifacts are moved out of the OSS default path or isolated behind explicit private boundaries.

**Tech Stack:** React, Tauri, Rust, existing `desktop-core` provider registry, existing settings UI, existing runtime provider resolution

---

### Task 1: Remove Private Documentation From The Public Tree

**Files:**
- Review: `docs/clawwiki/desktop-llm-gateway-integration.md`
- Review: `docs/clawwiki/backend-design.md`
- Review: `docs/clawwiki/`
- Create or update: OSS-safe replacement docs under `docs/desktop-shell/`

- [ ] Inventory all private product and backend design docs currently living under `docs/clawwiki/`.
- [ ] Move private-only material to a private documentation location or archive outside the public release scope.
- [ ] Replace any removed public entry points with generic OSS-safe documentation.
- [ ] Verify the public docs tree no longer explains private backend topology, internal operations, or private rollout procedures.

### Task 2: Scrub Product-Specific Gateway Language In Source

**Files:**
- Review and modify: `rust/crates/desktop-core/src/managed_auth.rs`
- Review and modify: `rust/crates/desktop-core/src/lib.rs`
- Review and modify: `apps/desktop-shell/src/features/settings/api/client.ts`
- Review and modify: other files found by repo search for private backend names

- [ ] Replace comments and descriptions that equate generic compatible gateways with a specific private product.
- [ ] Keep protocol-level wording such as `Anthropic-compatible gateway` and `OpenAI-compatible gateway`.
- [ ] Verify no built-in provider template or user-facing string ships a private domain or private service name by default.

### Task 3: Decide The Private Feature Boundary

**Files:**
- Review: `rust/crates/desktop-core/src/codex_broker.rs`
- Review: related cloud-account sync routes and UI surfaces
- Modify: feature flags, crate boundaries, or build wiring as chosen
- Verify: `cd rust && cargo check --workspace`

- [ ] Review `codex_broker` and adjacent cloud-account features to determine whether they are public, private, or split.
- [ ] Choose one boundary:
- [ ] move private logic to a private crate or downstream layer, or
- [ ] keep it behind an explicit private feature flag that is off by default in OSS builds.
- [ ] Verify the open-source default build remains functional without private cloud infrastructure.
- [ ] Run `cd rust && cargo check --workspace`.

### Task 4: Promote The Generic Provider Registry As The Public Path

**Files:**
- Review and modify: `rust/crates/desktop-core/src/providers_config.rs`
- Review and modify: `apps/desktop-shell/src/features/settings/sections/MultiProviderSettings.tsx`
- Review and modify: `apps/desktop-shell/src/features/settings/api/client.ts`
- Verify: `cd apps/desktop-shell && npm run build`
- Verify: `cd apps/desktop-shell/src-tauri && cargo check`

- [ ] Confirm the public provider story is centered on `.claw/providers.json` plus the settings UI.
- [ ] Add or polish public-safe built-in templates for generic compatible gateways if needed.
- [ ] Make sure the UI language explains local-only API key storage clearly.
- [ ] Verify provider CRUD, activation, and connectivity testing remain stable.
- [ ] Run `cd apps/desktop-shell && npm run build`.
- [ ] Run `cd apps/desktop-shell/src-tauri && cargo check`.

### Task 5: Harden Public Docs For Custom Gateway Setup

**Files:**
- Create: `docs/desktop-shell/operations/` or `docs/desktop-shell/architecture/` guidance as appropriate
- Update: `docs/desktop-shell/README.md`
- Update: spec and plan indexes if new docs are added

- [ ] Add a public guide for connecting an Anthropic-compatible gateway.
- [ ] Add a public guide for connecting an OpenAI-compatible gateway.
- [ ] Document expected fields, common auth mistakes, streaming expectations, and model-selection guidance.
- [ ] Keep all examples generic and offline-safe.

### Task 6: Run The Open-Source Safety Sweep

**Files:**
- Verify only

- [ ] Search the repo for private backend names, private domains, internal ports, and real endpoint traces.
- [ ] Search docs and code for credentials, tokens, connection strings, and operations screenshots.
- [ ] Verify tests do not require private networks or private services.
- [ ] Run a final secret scan before publication.

### Task 7: Storage Hardening Follow-Up

**Files:**
- Review and modify: `rust/crates/desktop-core/src/providers_config.rs`
- Review and modify: secure storage integration files
- Verify: `cd rust && cargo check --workspace`

- [ ] Decide whether the first OSS release uses `.claw/providers.json` as-is or introduces keychain/encrypted storage immediately.
- [ ] If storage hardening is in scope now, move secret material out of plaintext config while preserving backward compatibility.
- [ ] Run `cd rust && cargo check --workspace`.

### Task 8: Release Review

**Files:**
- Verify only

- [ ] Product review: confirm the repo tells a generic client story, not a private cloud story.
- [ ] Security review: confirm no private operations knowledge or credentials remain.
- [ ] Engineering review: confirm open-source default builds and runs without private infra.
- [ ] Documentation review: confirm public onboarding is complete for direct APIs and custom gateways.

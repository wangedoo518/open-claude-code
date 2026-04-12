---
title: ClaudeWiki WeChat Customer-Service Ingress Plan
doc_type: plan
status: active
owner: desktop-shell
last_verified: 2026-04-10
related:
  - docs/desktop-shell/README.md
  - docs/desktop-shell/specs/2026-04-10-claudewiki-wechat-customer-service-ingress-design.md
  - docs/desktop-shell/plans/README.md
---

# ClaudeWiki WeChat Customer-Service Ingress Plan

**Goal:** Add a ClaudeWiki WeChat customer-service ingress so ClaudeWiki can appear through WeChat's `客服消息` surface while preserving the current personal-WeChat `iLink` path.

**Architecture:** Introduce a dual-channel model. `wechat_ilink` remains for personal WeChat DM via WeChat ClawBot. A new `wechat_kefu` channel handles customer-service ingress. Both channels normalize into one shared ClaudeWiki turn pipeline.

**Tech Stack:** Rust, desktop-server HTTP routes, existing ClaudeWiki session runtime, existing desktop-shell WeChat Bridge UI

---

### Task 1: Confirm The Official Customer-Service Entry

**Files:**
- Document only

- [ ] Confirm which official WeChat customer-service surface ClaudeWiki will use in production.
- [ ] Confirm the tenant, account owner, credential model, and operational owner.
- [ ] Confirm how the customer-service entry is exposed to end users: entry link, QR, menu, or embedded handoff.
- [ ] Record the chosen surface and required credentials back into the design doc before implementation starts.

### Task 2: Introduce Shared WeChat Ingress Abstractions

**Files:**
- Create: `rust/crates/desktop-core/src/wechat_ingress/mod.rs`
- Create: `rust/crates/desktop-core/src/wechat_ingress/types.rs`
- Create: `rust/crates/desktop-core/src/wechat_ingress/session_map.rs`
- Create: `rust/crates/desktop-core/src/wechat_ingress/turn_runner.rs`
- Modify: `rust/crates/desktop-core/src/lib.rs`
- Modify: `rust/crates/desktop-core/src/wechat_ilink/desktop_handler.rs`
- Verify: `cd rust && cargo check --workspace`

- [ ] Extract a channel-agnostic `InboundEnvelope` model.
- [ ] Extract the shared desktop turn execution and session-resolution logic out of the current `iLink`-specific handler.
- [ ] Make session mapping channel-aware so `ilink:*` and `kefu:*` do not collide.
- [ ] Keep current `wechat_ilink` behavior unchanged after the refactor.
- [ ] Run `cd rust && cargo check --workspace`.

### Task 3: Implement `wechat_kefu` Ingress

**Files:**
- Create: `rust/crates/desktop-core/src/wechat_kefu/mod.rs`
- Create: `rust/crates/desktop-core/src/wechat_kefu/client.rs`
- Create: `rust/crates/desktop-core/src/wechat_kefu/account.rs`
- Create: `rust/crates/desktop-core/src/wechat_kefu/adapter.rs`
- Create: `rust/crates/desktop-core/src/wechat_kefu/monitor.rs` or `webhook.rs`
- Modify: `rust/crates/desktop-server/src/main.rs`
- Modify: `rust/crates/desktop-core/src/lib.rs`
- Verify: `cd rust && cargo check --workspace`

- [ ] Implement the chosen customer-service ingress transport.
- [ ] Normalize inbound customer-service messages into the shared `InboundEnvelope`.
- [ ] Implement customer-service outbound reply formatting.
- [ ] Persist channel credentials and runtime status separately from `wechat_ilink`.
- [ ] Start and stop the customer-service listener alongside existing server lifecycle management.
- [ ] Run `cd rust && cargo check --workspace`.

### Task 4: Expose Customer-Service Status In Desktop Shell

**Files:**
- Modify: `apps/desktop-shell/src/features/settings/sections/WeChatSettings.tsx`
- Modify: `apps/desktop-shell/src/features/wechat/WeChatBridgePage.tsx`
- Modify: `apps/desktop-shell/src/features/settings/api/client.ts`
- Verify: `cd apps/desktop-shell && npm run build`

- [ ] Add a separate customer-service section instead of overloading the existing ClawBot account card.
- [ ] Show customer-service health, bound account info, and entry link or QR state.
- [ ] Keep the current `iLink` account management UI working without regression.
- [ ] Run `cd apps/desktop-shell && npm run build`.

### Task 5: Add Message-Type Capability Gates

**Files:**
- Modify: `rust/crates/desktop-core/src/wechat_ingress/types.rs`
- Modify: `rust/crates/desktop-core/src/wechat_ilink/desktop_handler.rs`
- Modify: `rust/crates/desktop-core/src/wechat_kefu/adapter.rs`
- Verify: `cd rust && cargo check --workspace`

- [ ] Separate "customer-service entry exists" from "all message types are supported".
- [ ] Add explicit capability flags for text, image, file, card, and share handling.
- [ ] Keep user-facing fallback errors channel-specific and accurate.
- [ ] Run `cd rust && cargo check --workspace`.

### Task 6: End-To-End Verification

**Files:**
- Verify only

- [ ] Verify the existing personal WeChat `iLink` path still binds, receives text, and replies.
- [ ] Verify ClaudeWiki appears through the intended `客服消息` surface on device.
- [ ] Verify inbound customer-service text messages reach the same ClaudeWiki turn pipeline and produce replies.
- [ ] Verify no session collision occurs between `wechat_ilink` and `wechat_kefu`.
- [ ] Verify the desktop shell surfaces both channel states clearly.
- [ ] Run `cd apps/desktop-shell && npm run build`.
- [ ] Run `cd apps/desktop-shell/src-tauri && cargo check`.
- [ ] Run `cd rust && cargo check --workspace`.

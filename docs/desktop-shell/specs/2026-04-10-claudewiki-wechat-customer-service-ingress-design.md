---
title: ClaudeWiki WeChat Customer-Service Ingress Design
doc_type: spec
status: active
owner: desktop-shell
last_verified: 2026-04-10
related:
  - docs/desktop-shell/README.md
  - docs/desktop-shell/specs/README.md
  - docs/desktop-shell/plans/README.md
---

# ClaudeWiki WeChat Customer-Service Ingress Design

**Goal**

Give ClaudeWiki a WeChat entry that behaves like QClaw's `QClaw客服` in WeChat:

- it can appear under `转发给朋友 -> 客服消息`
- users can enter ClaudeWiki from the customer-service surface, not only from the personal WeChat ClawBot plugin
- inbound messages still flow into the existing ClaudeWiki desktop session and wiki-ingest pipeline

This document is for architecture review. It does not implement the feature.

## Executive Conclusion

ClaudeWiki's current WeChat Bridge is built on the official WeChat `iLink` bot protocol. That path gives us:

- QR binding through the WeChat ClawBot plugin
- personal WeChat direct-message ingress
- long-poll `getupdates` and downstream `sendmessage`

It does **not** give ClaudeWiki a `客服消息` entry in WeChat's forward picker.

QClaw's visible `QClaw客服` behavior comes from a different ingress model:

- a customer-service entry generated through `open_kfid`
- Tencent/QClaw backend contact-link generation
- a `wechat-access` WebSocket channel that receives `session.prompt` and returns `promptResponse.content`

So the key gap is not "ClaudeWiki's reply format is wrong". The gap is "ClaudeWiki is on the wrong WeChat-side surface". To match QClaw's UX, ClaudeWiki must add a **second WeChat ingress** for customer service, rather than trying to force the existing `iLink` bot into a surface that `iLink` does not own.

## Evidence From Current ClaudeWiki

ClaudeWiki's current implementation is explicitly an `iLink` integration:

- `apps/desktop-shell/src/features/settings/sections/WeChatSettings.tsx` says messages are forwarded through `iLink` long-poll and replied through the agentic loop.
- `apps/desktop-shell/src/features/wechat/WeChatBridgePage.tsx` labels the page as `个微 iLink 漏斗`.
- `rust/crates/desktop-core/src/wechat_ilink/mod.rs` states the module implements the protocol used by the WeChat ClawBot plugin and talks to `https://ilinkai.weixin.qq.com`.
- `rust/crates/desktop-core/src/wechat_ilink/login.rs` uses `ilink/bot/get_bot_qrcode` and `ilink/bot/get_qrcode_status`.
- `rust/crates/desktop-core/src/wechat_ilink/client.rs` uses `ilink/bot/getupdates` and `ilink/bot/sendmessage`.
- `rust/crates/desktop-server/src/main.rs` starts persisted `iLink` long-poll monitors and describes the login as "Bind a new WeChat ClawBot via QR code".

The message bridge also shows the current product boundary:

- `rust/crates/desktop-core/src/wechat_ilink/desktop_handler.rs` expects `from_user_id` and `context_token`.
- the same file currently rejects non-text input and sends `（暂不支持非文本消息，请发送文字）`.

That means ClaudeWiki currently supports a personal-WeChat DM funnel plus text-centric reply splitting. It does not currently implement a customer-service conversation model, nor a share-card/file/media adapter for that model.

## Evidence From QClaw

`qclaw-wechat-client` and the extracted QClaw channel code show that QClaw used two different WeChat paths:

### 1. `weixin` path: official `iLink`

The extracted `weixin` plugin is clearly the long-poll `iLink` path:

- `weixin/README.md` documents `getUpdates`, `sendMessage`, `getUploadUrl`, `getConfig`, and `sendTyping`
- `weixin/src/channel.ts` labels itself `openclaw-weixin (long-poll)`

This is the same family as ClaudeWiki's current `wechat_ilink`.

### 2. `wechat-access` path: customer-service / Tencent access channel

The QClaw reverse source shows the customer-service entry:

- `qclaw-wechat-client/src/index.ts` exposes `data/4018/forward` as `GENERATE_CONTACT_LINK`
- `qclaw-wechat-client/examples/full-flow.ts` hardcodes `WECOM_OPEN_KFID`
- the same example calls `generateContactLink({ open_id: WECOM_OPEN_KFID, contact_type: "open_kfid" })`
- the example comments explicitly say the generated link opens a **WeCom customer service chat**

The extracted `wechat-access` plugin then shows how that channel is handled:

- `wechat-access/index.ts` registers a separate `wechat-access` channel
- the same file starts an `AgpWebSocketClient` with `token`, `wsUrl`, `guid`, and `userId`
- `wechat-access/common/message-context.ts` normalizes this source as `wechat-access`, with its own session key format
- `wechat-access/websocket/message-handler.ts` accumulates reply text and sends the final answer through `promptResponse.content`

So QClaw's forward-list visibility is attached to the customer-service surface, not to the `iLink` surface.

## Why ClaudeWiki Cannot Get QClaw's UX By Tweaking The Current `iLink` Path

The screenshots and code line up on one point:

- WeChat shows `客服消息` as a separate forwarding container
- inside that container, QClaw appears as `QClaw客服`
- QClaw generates that entry through a customer-service link bound to `open_kfid`

ClaudeWiki's current `iLink` integration has none of those primitives:

- no customer-service account identity
- no `open_kfid`
- no customer-service contact link
- no service-side WebSocket session for that channel

Because of that, changing only these parts will **not** solve the problem:

- changing reply text format
- changing `context_token` handling
- changing message chunk size
- changing desktop-side session mapping

Those tweaks may improve the current bot conversation, but they cannot make the WeChat client place ClaudeWiki under `客服消息`.

## Why "Spoofing Customer Service By Editing iLink Message Format" Is Not A Viable Path

This option was considered explicitly because the customer-service route has operational constraints. After comparing the wire formats, the answer is still no for a production-grade solution.

### 1. The `iLink` wire schema has no customer-service identity slot

The `iLink` message envelope only exposes direct-conversation fields:

- `from_user_id`
- `to_user_id`
- `session_id`
- `group_id`
- `message_type`
- `message_state`
- `context_token`
- `item_list`

There is no field for:

- `open_kfid`
- customer-service account id
- service-account id
- customer-service scene or entrypoint

So there is nowhere in the `iLink` payload to honestly or dishonestly declare "this message belongs to a customer-service account".

### 2. The outbound `iLink` implementation is already structurally a bot reply

Both the standalone reverse client and the extracted `openclaw-weixin` implementation build replies as:

- `from_user_id: ""`
- `to_user_id: <wechat user>`
- `message_type: BOT`
- `message_state: FINISH`
- `context_token: <echo from inbound>`

This means ClaudeWiki is not choosing between multiple public surfaces at send time. It is already inside the only surface the `iLink` protocol exposes: bot direct-message reply.

Changing `message_type` does not help either. In the reverse schema, `message_type` only has:

- `0 = NONE`
- `1 = USER`
- `2 = BOT`

There is no "customer-service" enum value to switch to.

### 3. QClaw's customer-service path is not just "different fields"; it is a different session namespace and transport

QClaw's customer-service flow is not an `iLink sendmessage` with extra metadata. It is:

- `open_kfid` contact-link creation
- customer-service entry inside WeChat
- `wechat-access` WebSocket uplink/downlink
- `session.prompt` / `session.promptResponse`

That is a different ingress contract from `iLink`:

- different session identifier semantics
- different server entrypoint
- different account identity
- different reply method

So there is no evidence that a field-level mutation inside `iLink` can cross that boundary.

### 4. The forward-picker classification almost certainly happens before message rendering

This is an inference from the observed behavior and the available protocol surface:

- WeChat's `转发给朋友` chooser shows `客服消息` as a distinct container
- QClaw appears there only when entered through the customer-service path
- ClaudeWiki's `iLink` bot conversation does not appear there
- the `iLink` protocol has no customer-service identity field that could influence that grouping

The most likely explanation is that WeChat classifies targets in its contact/conversation directory using server-side account type plus entrypoint, not using individual message payload shape.

If that inference is correct, then modifying `sendmessage` payloads cannot change forward-list placement because the placement decision is made before the user picks a target and before a reply is rendered.

### 5. The only "spoof" paths left are high-risk client hacks

After excluding protocol-level spoofing, the remaining options are all non-product paths:

- patch the mobile WeChat client UI to inject a fake `客服消息` target
- intercept and modify the client's contact-directory/network responses
- distribute a modified WeChat build or rely on jailbreak/root-level hooking

These paths are unsuitable for ClaudeWiki productization because they are:

- device-dependent
- fragile across app upgrades
- high-maintenance
- likely to trigger compliance and account risk

## Decision On The Spoofing Question

The team should treat "edit `iLink` message format to masquerade as customer service" as **not viable** for the target outcome.

At most, it could change how a message is rendered inside an existing `iLink` conversation. It cannot reliably create the missing customer-service identity and therefore cannot reliably make ClaudeWiki appear in `转发给朋友 -> 客服消息`.

## Recommended Architecture

### Recommendation: Dual-channel WeChat ingress

Keep the existing `wechat_ilink` path, and add a new `wechat_kefu` path.

#### Channel A: `wechat_ilink`

Use the current implementation for:

- personal WeChat ClawBot direct chat
- low-risk continuity with today's feature
- compatibility with existing account storage and QR login flow

#### Channel B: `wechat_kefu`

Add a new customer-service ingress for:

- WeChat `客服消息` discoverability
- `转发给朋友` visibility aligned with QClaw
- future shared/customer-service style flows

The customer-service channel should be treated as a first-class ingress, not a UI alias over `wechat_ilink`.

### Core design principle: unify downstream, separate upstream

ClaudeWiki should share the downstream session and wiki-ingest pipeline, but keep separate upstream adapters.

Proposed model:

- `wechat_ilink` adapter
- `wechat_kefu` adapter
- shared normalized envelope
- shared session resolver
- shared ClaudeWiki turn executor
- per-channel outbound formatter

Suggested normalized envelope:

```text
InboundEnvelope {
  channel_kind: "ilink" | "kefu"
  external_user_id: string
  conversation_token: string
  message_items: NormalizedMessageItem[]
  display_name?: string
  raw_meta: serde_json::Value
}
```

This keeps the current desktop loop reusable while isolating the customer-service protocol differences.

### Session model

Do not reuse today's `openid -> desktop_session_id` mapping as-is for both channels.

Instead, namespace the mapping by channel:

- `ilink:{external_user_id}`
- `kefu:{external_user_id}`

If product later wants a unified "same human across channels" view, add a higher-level contact identity layer. Do not collapse the two transports into the same low-level mapping on day one.

### Outbound model

Customer-service outbound should not be modeled as `iLink sendmessage`.

Instead, the new adapter should expose a channel-specific send API that can:

- send final text blocks
- optionally send multiple text blocks for long replies
- later add media/card/file support based on the chosen official customer-service capability

The current `desktop_handler.rs` logic can be split into:

- shared turn execution
- `iLink` outbound formatter
- customer-service outbound formatter

## What ClaudeWiki Should Not Copy From QClaw

ClaudeWiki should learn from QClaw's architecture, but should **not** ship a production dependency on QClaw's private gateway shape:

- do not depend on `data/4018/forward`
- do not depend on `data/4058/forward`
- do not depend on QClaw-owned JWT or channel token semantics
- do not assume QClaw's `wsUrl`, `guid`, or backend protocol are stable

The correct production design is:

- ClaudeWiki owns its own customer-service account and backend integration
- ClaudeWiki implements an official customer-service ingress surface
- QClaw is used only as reverse-engineering evidence for product behavior and channel separation

This is especially important because `qclaw-wechat-client/README.md` already marks that path as unmaintained and recommends switching to the official `iLink` protocol for the personal-bot case.

## Proposed ClaudeWiki Module Layout

Suggested Rust-side structure:

```text
rust/crates/desktop-core/src/wechat_ingress/
  mod.rs
  types.rs              # InboundEnvelope / normalized items
  session_map.rs        # channel-aware external-user -> desktop session
  turn_runner.rs        # shared ClaudeWiki turn execution

rust/crates/desktop-core/src/wechat_ilink/
  ... existing code ...
  adapter.rs            # normalize iLink inbound/outbound into shared shape

rust/crates/desktop-core/src/wechat_kefu/
  mod.rs
  client.rs
  webhook.rs or ws.rs
  adapter.rs
  account.rs
```

Suggested frontend/UI changes:

- keep the current `WeChat Bridge` page, but make it show two entry types
- `个人微信 ClawBot`
- `微信客服入口`

The second card should expose operational state, not just QR binding:

- customer-service account status
- customer-service entry link / QR
- last inbound time
- outbound health

## Delivery Constraints And Risks

### 1. This is an account/product-operation change, not only a code change

ClaudeWiki needs a real customer-service identity to appear in `客服消息`.

So before implementation, the team must decide:

- which tenant owns the customer-service account
- whether the surface is WeCom customer service, service account, or another official WeChat customer-service capability
- who owns credentials, rotation, and compliance

### 2. Non-text support is a separate deliverable

Even after the customer-service entry is added, today's ClaudeWiki handler still rejects non-text messages. So there are really two milestones:

- milestone A: ClaudeWiki appears in `客服消息`
- milestone B: ClaudeWiki correctly handles shares, files, images, and cards from that surface

These should not be conflated in review.

### 3. Identity merging needs care

The same user may reach ClaudeWiki through:

- personal WeChat ClawBot
- customer-service entry

If both paths are enabled, the team must decide whether to:

- keep completely separate sessions
- link them under one higher-level contact
- or merge only in analytics, not in conversation memory

The safest first release is separate session histories with a later identity-linking layer.

## Decision

For ClaudeWiki to match QClaw's `QClaw客服` forwarding behavior, the team should approve this architectural direction:

1. Keep the current `wechat_ilink` integration as the personal WeChat path.
2. Add a new official customer-service ingress for `客服消息` visibility.
3. Normalize both channels into one downstream ClaudeWiki turn pipeline.
4. Treat non-text/customer-service media support as a follow-up scope, not as a hidden assumption.

If the team approves only item 1 and keeps `iLink` alone, ClaudeWiki should expect the current limitation to remain: it will work as a direct ClawBot conversation, but it will not show up like `QClaw客服` in WeChat's customer-service forwarding surface.

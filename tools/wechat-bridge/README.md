# wechat-bridge (prototype)

> **Status: prototype — proof-of-concept for Plan A from the WeChat integration review.**
> **Not production-ready. Do NOT expose on a public network.**

## What this is

A stand-alone Node.js HTTP service that bridges WeChat private messages to
the `open-claude-code` desktop server's Dispatch inbox.

- **Zero external dependencies** — uses only Node 18+ built-ins (`http`, `fs`, `fetch`).
- **Does NOT modify `open-claude-code`** beyond the tiny `source_kind` /
  `source_label` fields added to `CreateDesktopDispatchItemRequest`.
- **Reuses the existing Dispatch state machine**. Each WeChat message becomes
  a `Dispatch` item with `source_kind = "remote_bridge"` and a descriptive
  label like `"WeChat: 张三"`.
- **Human review preserved**: untrusted senders see their message sit in the
  Dispatch inbox waiting for manual approval. Trusted senders (configured
  via env var) are auto-delivered.
- **SSE reply routing**: when a session replies, the bridge picks it up
  via `/sessions/{id}/events` and stores it in a per-openid outbox for
  your WeChat sending client to poll.

## Why this design?

See the architecture discussion in the commit message of
`fix(audit-r2): Plan A WeChat bridge prototype` (or `tasks/plan.md` if it
was saved there).

TL;DR: the user asked whether to replace the Dispatch feature with
`wechat-acp`. That approach had five hard problems (protocol mismatch,
auto-approved permissions, no multi-tenant model, security hole, etc.).
This bridge takes the minimal-risk path by reusing the `RemoteBridge`
source kind that was already defined but unused in
`rust/crates/desktop-core/src/lib.rs` (enum variant
`DesktopDispatchSourceKind::RemoteBridge`).

## Architecture

```
  ┌─────────────┐
  │ WeChat user │
  └──────┬──────┘
         │ private message
         ▼
  ┌──────────────────────────┐
  │  Your WeChat SDK client   │   ← e.g. Wechaty, itchat, or a
  │  (out-of-scope here)     │     custom integration
  └──────┬───────────────────┘
         │ POST /inbound
         │ { openid, nickname, text }
         ▼
  ┌──────────────────────────┐
  │      wechat-bridge        │   ← this directory
  │    (127.0.0.1:4358)       │
  └──────┬───────────────────┘
         │ POST /api/desktop/dispatch
         │ { source_kind: "remote_bridge",
         │   source_label: "WeChat: <nickname>",
         │   ... }
         ▼
  ┌──────────────────────────┐
  │  open-claude-code server  │
  │    (127.0.0.1:4357)       │
  └──────┬───────────────────┘
         │
         │ Untrusted:  waits in Dispatch UI
         │ Trusted:    auto-delivered
         │
         ▼
     Dispatch item
         │
         │ (user or auto) → deliver_dispatch_item()
         │ → create_session / append_user_message
         │ → agentic_loop runs
         ▼
     assistant reply (as SSE message event)
         │
         ▼
  ┌──────────────────────────┐
  │      wechat-bridge        │   ← subscribes to
  │     (SSE consumer)        │     /sessions/{id}/events
  └──────┬───────────────────┘
         │ stores in outbox
         ▼
  ┌──────────────────────────┐
  │  Your WeChat SDK client   │   ← polls GET /outbox/:openid
  │                           │     and sends back to user
  └───────────────────────────┘
```

## Setup

```bash
cd tools/wechat-bridge

# Start the desktop server separately (in another terminal):
#   cargo run -p desktop-server

# Then launch the bridge:
node src/index.mjs

# With trusted senders (auto-deliver):
WECHAT_TRUSTED_OPENIDS="wxid_abc,wxid_xyz" node src/index.mjs
```

Environment variables:

| Variable | Default | Purpose |
|---|---|---|
| `DESKTOP_SERVER` | `http://127.0.0.1:4357` | Where to find open-claude-code |
| `BRIDGE_PORT` | `4358` | Bridge HTTP listen port (127.0.0.1 only) |
| `BRIDGE_STATE_FILE` | `.state/bridge-state.json` | Persistent mapping + outbox |
| `WECHAT_TRUSTED_OPENIDS` | (empty) | CSV of openids to auto-deliver |

## HTTP API

### `GET /health`
Bridge health + config dump.

### `POST /inbound`
Simulate an incoming WeChat private message.
```json
{ "openid": "wxid_abc", "nickname": "张三", "text": "请看 PR #42" }
```
Returns the created dispatch item and whether it was auto-delivered.

### `POST /deliver/:item_id`
Manually deliver a pending dispatch item (mirrors desktop-server but
also wires up SSE reply routing for the item's owner).

### `GET /outbox/:openid`
Poll pending replies for a specific user. Append `?drain=1` to atomically
remove them after reading.

```json
{
  "openid": "wxid_abc",
  "count": 1,
  "items": [{ "ts": 1234567890, "session_id": "desktop-session-7", "text": "..." }]
}
```

### `GET /state`
Debug dump of the full in-memory state (openid→session mapping, pending
items, outbox).

## Running the smoke test

Make sure both servers are up, then:

```bash
# Terminal 1
cd "open-claude-code(8)"
./rust/target/debug/desktop-server.exe

# Terminal 2
cd tools/wechat-bridge
node src/index.mjs

# Terminal 3 (smoke test)
cd tools/wechat-bridge
node src/smoke-test.mjs
```

Expected output:
```
=== wechat-bridge smoke test ===
[1] Preflight — both servers alive
  ✓ desktop-server /healthz
  ✓ bridge /health
  ✓ bridge points at http://127.0.0.1:4357
[2] Inbound from untrusted openid — expect Dispatch inbox
  ✓ item created
  ✓ untrusted openid is NOT auto-delivered
  ✓ source.kind === "remote_bridge"
  ✓ source.label starts with "WeChat:"
  ✓ status === "unread"
...
SMOKE TEST PASSED
```

## Security posture

**Defense-in-depth layers:**

1. **Bind to loopback only** (`127.0.0.1`). Bridge is not reachable from
   other hosts. If you need it externally, put a reverse proxy in front
   with TLS + HMAC signing + rate limits.
2. **Source label sanitization**: backend strips RTL overrides, zero-width
   chars, C0/C1 control characters, and caps at 120 chars. A malicious
   nickname cannot inject control chars into the Dispatch UI.
3. **Human review gate**: untrusted openids never auto-execute. A human
   operator must press "Deliver" in the Dispatch UI.
4. **Trusted list is explicit**: `WECHAT_TRUSTED_OPENIDS` must be set
   deliberately. Default is empty (everyone goes through manual review).
5. **Body size limit**: bridge rejects `/inbound` requests over 1 MiB.
   Desktop-server additionally enforces 15 MiB global + 10 MiB for
   attachments.
6. **No file upload from WeChat**: bridge only forwards the text of the
   message. Files, images, voice notes are ignored (would need a separate
   channel that runs through the attachment validator).

**Known gaps (prototype-level, must fix before production):**

- No HMAC signing on `/inbound`. Add `x-signature` header with
  `HMAC-SHA256(shared_secret, body)` verification.
- No rate limiting on `/inbound`. A single openid could flood the Dispatch
  inbox. Add a per-openid token bucket.
- No `.wechat-acp`-style persistent subprocess isolation per user. All
  WeChat users share the same `open-claude-code` process and permission
  gate. This is fine for single-operator usage but NOT for multi-tenant.
- The outbox is plaintext JSON on disk. If the bridge is used for
  sensitive conversations, encrypt it with the same `secret-key`
  mechanism used by `managed_auth.rs`.

## Integrating a real WeChat SDK

The bridge intentionally does NOT include a WeChat SDK. You can plug any
of these in:

### Option 1: Wechaty (Node.js, most common)
```bash
npm install wechaty wechaty-puppet-padlocal
```
Write a small adapter that listens for `message` events on a
`Wechaty` instance, filters for private (non-room) text messages, and
POSTs them to `http://127.0.0.1:4358/inbound`. Poll `/outbox/:openid`
on a timer (or subscribe via long-polling) and call `Contact.say()` to
reply.

### Option 2: itchat (Python)
Wrap `itchat.msg_register` and make the bridge calls via `requests`.
Same flow, different language.

### Option 3: WeCom / WeChat Work webhooks
If you're on WeChat Work, their official webhook API can POST directly
to `/inbound` — no SDK needed. Just verify the signature.

## Rollback

If this prototype doesn't work out:

```bash
git revert <commit-hash>
# or, if you committed the backend changes separately:
git revert <backend-commit>   # revert source_kind/label fields
rm -rf tools/wechat-bridge/    # remove bridge entirely
```

The backend changes are additive (optional fields with serde defaults),
so rollback cannot break any existing callers.

#!/usr/bin/env node
/**
 * wechat-bridge smoke test — end-to-end proof that the architecture works.
 *
 * Prerequisites:
 *   1. desktop-server running on http://127.0.0.1:4357
 *   2. wechat-bridge running on http://127.0.0.1:4358
 *      (run `node src/index.mjs` in another terminal)
 *
 * What this script verifies:
 *   1. POST /inbound creates a dispatch item with source=remote_bridge
 *   2. The item shows up in desktop-server's dispatch list with the
 *      correct source_label ("WeChat: <nickname>")
 *   3. Trusted openids auto-deliver (via WECHAT_TRUSTED_OPENIDS env var)
 *   4. Manual /deliver flow works for non-trusted openids
 *   5. Untrusted items can be reviewed via desktop-server and the delivery
 *      triggers an SSE subscription for reply routing
 *
 * Exit code: 0 on success, 1 on any assertion failure.
 */

const DESKTOP = process.env.DESKTOP_SERVER || "http://127.0.0.1:4357";
const BRIDGE = process.env.BRIDGE_URL || "http://127.0.0.1:4358";

let passed = 0;
let failed = 0;

function assert(cond, label) {
  if (cond) {
    console.log(`  ✓ ${label}`);
    passed++;
  } else {
    console.error(`  ✗ ${label}`);
    failed++;
  }
}

async function requireOk(url, init) {
  const res = await fetch(url, init);
  const text = await res.text();
  const body = text ? JSON.parse(text) : {};
  if (!res.ok) {
    throw new Error(`${url} returned ${res.status}: ${text}`);
  }
  return body;
}

async function main() {
  console.log("=== wechat-bridge smoke test ===");
  console.log(`  desktop-server: ${DESKTOP}`);
  console.log(`  bridge:         ${BRIDGE}`);
  console.log("");

  // ── Step 1: sanity checks ────────────────────────────────────────
  console.log("[1] Preflight — both servers alive");
  const deskHealth = await requireOk(`${DESKTOP}/healthz`);
  assert(deskHealth.status === "ok", "desktop-server /healthz");

  const bridgeHealth = await requireOk(`${BRIDGE}/health`);
  assert(bridgeHealth.status === "ok", "bridge /health");
  assert(
    bridgeHealth.desktop_server === DESKTOP,
    `bridge points at ${DESKTOP}`
  );

  // ── Step 2: inbound message from NON-trusted openid ──────────────
  console.log("");
  console.log("[2] Inbound from untrusted openid — expect Dispatch inbox");
  const untrusted = "openid-untrusted-" + Math.random().toString(36).slice(2, 8);
  const inboundResp = await requireOk(`${BRIDGE}/inbound`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      openid: untrusted,
      nickname: "Visitor",
      text: "Please analyze the project structure",
    }),
  });
  assert(inboundResp.item && inboundResp.item.id, "item created");
  assert(
    inboundResp.auto_delivered === false,
    "untrusted openid is NOT auto-delivered"
  );
  assert(
    inboundResp.item.source.kind === "remote_bridge",
    `source.kind === "remote_bridge" (got ${inboundResp.item.source.kind})`
  );
  assert(
    inboundResp.item.source.label.startsWith("WeChat:"),
    `source.label starts with "WeChat:" (got "${inboundResp.item.source.label}")`
  );
  assert(
    inboundResp.item.status === "unread",
    `status === "unread" (got ${inboundResp.item.status})`
  );
  const untrustedItemId = inboundResp.item.id;

  // ── Step 3: verify it appears in desktop-server dispatch list ────
  console.log("");
  console.log("[3] Verify desktop-server sees the item with correct source");
  const dispatchList = await requireOk(`${DESKTOP}/api/desktop/dispatch`);
  const allItems = dispatchList.dispatch.items || [];
  const ourItem = allItems.find((i) => i.id === untrustedItemId);
  assert(!!ourItem, `item ${untrustedItemId} in dispatch list`);
  if (ourItem) {
    assert(
      ourItem.source.kind === "remote_bridge",
      "dispatch list preserves source.kind"
    );
    assert(
      ourItem.source.label === inboundResp.item.source.label,
      "dispatch list preserves source.label"
    );
  }

  // ── Step 4: manual deliver via bridge ────────────────────────────
  console.log("");
  console.log("[4] Manually deliver via bridge → open session + wire SSE");
  const deliverResp = await requireOk(
    `${BRIDGE}/deliver/${encodeURIComponent(untrustedItemId)}`,
    { method: "POST" }
  );
  assert(
    deliverResp.delivered && deliverResp.delivered.status === "delivered",
    `item delivered (status: ${deliverResp.delivered?.status})`
  );
  assert(
    deliverResp.openid === untrusted,
    "bridge pending map routes reply back to original openid"
  );
  const targetSessionId =
    deliverResp.delivered.target && deliverResp.delivered.target.session_id;
  assert(!!targetSessionId, "delivered item has target.session_id");

  // ── Step 5: verify session got the user message ──────────────────
  console.log("");
  console.log("[5] Verify session received the user message");
  const sessionDetail = await requireOk(
    `${DESKTOP}/api/desktop/sessions/${encodeURIComponent(targetSessionId)}`
  );
  // Session detail shape: { session: { messages: [{ role, blocks: [{type, text}] }] } }
  const messages = (sessionDetail.session && sessionDetail.session.messages) || [];
  const found = messages.find(
    (m) =>
      m.role === "user" &&
      Array.isArray(m.blocks) &&
      m.blocks.some(
        (b) => b.type === "text" && b.text && b.text.includes("analyze the project")
      )
  );
  assert(!!found, "user message from WeChat landed in session");

  // ── Step 6: inbound from TRUSTED openid — auto-deliver ───────────
  console.log("");
  console.log("[6] Trusted openid (requires WECHAT_TRUSTED_OPENIDS set)");
  const trusted = process.env.SMOKE_TRUSTED_OPENID;
  if (!trusted) {
    console.log(
      "  (skip) set SMOKE_TRUSTED_OPENID=<id> and restart bridge with"
    );
    console.log("         WECHAT_TRUSTED_OPENIDS=<same-id> to exercise this path");
  } else {
    const autoResp = await requireOk(`${BRIDGE}/inbound`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        openid: trusted,
        nickname: "Owner",
        text: "Auto-delivery test",
      }),
    });
    assert(autoResp.auto_delivered === true, "auto_delivered=true for trusted");
    assert(
      autoResp.delivered && autoResp.delivered.status === "delivered",
      "delivered.status === 'delivered'"
    );
  }

  // ── Step 7: source label sanitization (RTL override) ─────────────
  console.log("");
  console.log("[7] RTL override in nickname is sanitized");
  const evilResp = await requireOk(`${BRIDGE}/inbound`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      openid: "openid-evil",
      nickname: "admin\u202Etxt.exe",
      text: "test rtl override",
    }),
  });
  const label = evilResp.item.source.label;
  assert(!label.includes("\u202E"), `RTL override stripped (got "${label}")`);
  assert(
    label.includes("admin"),
    "harmless characters preserved"
  );

  // ── Summary ──────────────────────────────────────────────────────
  console.log("");
  console.log("=== Summary ===");
  console.log(`  passed: ${passed}`);
  console.log(`  failed: ${failed}`);
  if (failed > 0) {
    console.error("");
    console.error("SMOKE TEST FAILED");
    process.exit(1);
  } else {
    console.log("");
    console.log("SMOKE TEST PASSED");
  }
}

main().catch((e) => {
  console.error("UNHANDLED ERROR:", e);
  process.exit(2);
});

#!/usr/bin/env node
/**
 * wechat-bridge — prototype WeChat ↔ open-claude-code Dispatch adapter
 *
 * This is a STAND-ALONE Node.js service with NO external dependencies.
 * It uses only built-in `http`, `fs`, and `crypto` modules so you can run
 * it with nothing more than Node 18+.
 *
 * Architecture (see tools/wechat-bridge/README.md for the full picture):
 *
 *   WeChat user
 *       │ private message
 *       ▼
 *   POST /inbound       ← inbound webhook (wired to your WeChat SDK)
 *       │
 *       ▼
 *   POST /api/desktop/dispatch  ← create dispatch item in open-claude-code
 *       │                         with source_kind="remote_bridge"
 *       │                              source_label="WeChat: <nickname>"
 *       ▼
 *   User (or auto-deliver rule) approves in the desktop Dispatch UI
 *       │
 *       ▼
 *   session.append_user_message → agentic_loop runs → assistant replies
 *       │
 *       ▼
 *   Bridge subscribes to /sessions/{id}/events SSE
 *       │
 *       ▼
 *   Assistant reply appears in GET /outbox/:openid
 *       │
 *       ▼
 *   Your WeChat SDK poller sends it back to the WeChat user
 *
 * IMPORTANT SECURITY NOTES:
 *   - This prototype has NO authentication on /inbound. In production you
 *     MUST add HMAC signing or a shared secret.
 *   - This prototype auto-approves delivery for items whose OpenID is in
 *     TRUSTED_OPENIDS (env: WECHAT_TRUSTED_OPENIDS="id1,id2"). For everyone
 *     else, the item sits in the Dispatch inbox waiting for manual review.
 *   - Do NOT expose this bridge on a public IP without a reverse proxy
 *     that terminates TLS and enforces rate limits.
 */

import http from "node:http";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// ── Configuration ────────────────────────────────────────────────────

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const DESKTOP_SERVER = process.env.DESKTOP_SERVER || "http://127.0.0.1:4357";
const BRIDGE_PORT = Number(process.env.BRIDGE_PORT || 4358);
const STATE_FILE =
  process.env.BRIDGE_STATE_FILE ||
  path.join(__dirname, "..", ".state", "bridge-state.json");
const TRUSTED_OPENIDS = new Set(
  (process.env.WECHAT_TRUSTED_OPENIDS || "")
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean)
);

// ── Persistent state ─────────────────────────────────────────────────

/**
 * Bridge state structure (persisted to disk):
 * {
 *   openid_to_session: { "<openid>": "desktop-session-N" },
 *   outbox: {
 *     "<openid>": [
 *       { ts: 1234567890, text: "assistant reply text" },
 *       ...
 *     ]
 *   },
 *   pending: {
 *     // dispatch_item_id -> openid (so we know who to route replies to)
 *     "dispatch-item-N": "<openid>"
 *   }
 * }
 */
function loadState() {
  try {
    const raw = fs.readFileSync(STATE_FILE, "utf8");
    return JSON.parse(raw);
  } catch {
    return { openid_to_session: {}, outbox: {}, pending: {} };
  }
}

function saveState(state) {
  fs.mkdirSync(path.dirname(STATE_FILE), { recursive: true });
  fs.writeFileSync(STATE_FILE, JSON.stringify(state, null, 2));
}

const state = loadState();

// ── Helpers ──────────────────────────────────────────────────────────

function jsonResponse(res, status, body) {
  res.writeHead(status, {
    "Content-Type": "application/json; charset=utf-8",
    "Cache-Control": "no-store",
  });
  res.end(JSON.stringify(body));
}

async function readJsonBody(req, maxBytes = 1024 * 1024) {
  return new Promise((resolve, reject) => {
    let chunks = [];
    let total = 0;
    req.on("data", (chunk) => {
      total += chunk.length;
      if (total > maxBytes) {
        req.destroy();
        reject(new Error("request body too large"));
        return;
      }
      chunks.push(chunk);
    });
    req.on("end", () => {
      try {
        resolve(chunks.length === 0 ? {} : JSON.parse(Buffer.concat(chunks).toString("utf8")));
      } catch (e) {
        reject(e);
      }
    });
    req.on("error", reject);
  });
}

async function desktopPost(path, body) {
  const response = await fetch(`${DESKTOP_SERVER}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  const text = await response.text();
  const data = text ? JSON.parse(text) : {};
  if (!response.ok) {
    throw new Error(
      `desktop-server ${path} returned ${response.status}: ${text}`
    );
  }
  return data;
}

async function desktopGet(path) {
  const response = await fetch(`${DESKTOP_SERVER}${path}`);
  const text = await response.text();
  if (!response.ok) {
    throw new Error(
      `desktop-server ${path} returned ${response.status}: ${text}`
    );
  }
  return JSON.parse(text);
}

// ── Core operations ──────────────────────────────────────────────────

/**
 * Called when a new WeChat message arrives (via /inbound webhook).
 *
 * Creates a dispatch item with source_kind="remote_bridge" and a descriptive
 * label. If the sender is in TRUSTED_OPENIDS, also auto-delivers so the
 * assistant reply comes back immediately; otherwise the item waits in the
 * Dispatch UI for manual review.
 */
async function handleInboundMessage({ openid, nickname, text }) {
  if (!openid || typeof openid !== "string") {
    throw new Error("openid is required");
  }
  if (!text || typeof text !== "string" || !text.trim()) {
    throw new Error("text is required");
  }

  const targetSessionId = state.openid_to_session[openid] || null;
  const sourceLabel = `WeChat: ${nickname || openid.slice(0, 8)}`;

  const createResp = await desktopPost("/api/desktop/dispatch", {
    title: `WeChat · ${(nickname || openid).slice(0, 20)}`,
    body: text,
    target_session_id: targetSessionId,
    priority: "normal",
    source_kind: "remote_bridge",
    source_label: sourceLabel,
  });

  const item = createResp.item;
  if (!item || !item.id) {
    throw new Error(
      `unexpected dispatch response: ${JSON.stringify(createResp)}`
    );
  }

  // Remember which openid this dispatch item belongs to so the SSE
  // subscriber can route the reply back.
  state.pending[item.id] = openid;
  saveState(state);

  let delivered = null;
  if (TRUSTED_OPENIDS.has(openid)) {
    // Trusted sender — auto-deliver right away.
    const deliverResp = await desktopPost(
      `/api/desktop/dispatch/items/${encodeURIComponent(item.id)}/deliver`,
      {}
    );
    delivered = deliverResp.item;
    if (delivered.target && delivered.target.session_id) {
      state.openid_to_session[openid] = delivered.target.session_id;
      saveState(state);
      // Open an SSE subscription to pick up the assistant reply.
      subscribeToSession(delivered.target.session_id, openid);
    }
  }

  return { item, delivered, auto_delivered: !!delivered };
}

// ── SSE subscription ─────────────────────────────────────────────────

// Map of session_id → AbortController so we don't open duplicate streams.
const sseControllers = new Map();

function subscribeToSession(sessionId, openid) {
  if (sseControllers.has(sessionId)) return;
  const ctrl = new AbortController();
  sseControllers.set(sessionId, ctrl);

  (async () => {
    try {
      const res = await fetch(
        `${DESKTOP_SERVER}/api/desktop/sessions/${encodeURIComponent(sessionId)}/events`,
        { signal: ctrl.signal }
      );
      if (!res.ok) {
        console.error(`[sse] subscribe ${sessionId} -> ${res.status}`);
        sseControllers.delete(sessionId);
        return;
      }
      console.error(`[sse] subscribed to ${sessionId} for openid=${openid}`);

      const reader = res.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        // SSE events are separated by double newlines
        let idx;
        while ((idx = buffer.indexOf("\n\n")) !== -1) {
          const rawEvent = buffer.slice(0, idx);
          buffer = buffer.slice(idx + 2);
          handleSseEvent(rawEvent, sessionId, openid);
        }
      }
    } catch (err) {
      if (err.name !== "AbortError") {
        console.error(`[sse] error on ${sessionId}:`, err.message);
      }
    } finally {
      sseControllers.delete(sessionId);
    }
  })();
}

function handleSseEvent(raw, sessionId, openid) {
  // Parse "event: X\ndata: Y"
  const lines = raw.split("\n");
  let eventName = "message";
  let dataLine = "";
  for (const line of lines) {
    if (line.startsWith("event:")) eventName = line.slice(6).trim();
    else if (line.startsWith("data:")) dataLine = line.slice(5).trim();
  }
  if (!dataLine) return;

  let payload;
  try {
    payload = JSON.parse(dataLine);
  } catch {
    return;
  }

  if (eventName !== "message") return;
  if (payload.type !== "message") return;
  if (!payload.message || payload.message.role !== "assistant") return;

  // Extract the assistant text and route to the openid's outbox.
  const text = extractAssistantText(payload.message);
  if (!text) return;

  if (!state.outbox[openid]) state.outbox[openid] = [];
  state.outbox[openid].push({
    ts: Date.now(),
    session_id: sessionId,
    text,
  });
  // Cap per-openid outbox at 50 entries to bound disk usage.
  if (state.outbox[openid].length > 50) {
    state.outbox[openid] = state.outbox[openid].slice(-50);
  }
  saveState(state);
  console.error(
    `[outbox] ${openid} ← "${text.slice(0, 60)}${text.length > 60 ? "…" : ""}"`
  );
}

function extractAssistantText(message) {
  // The desktop runtime emits messages with a `blocks` array, each block
  // having `type` and `text` fields. We only care about "text" blocks.
  if (!message || !Array.isArray(message.blocks)) return null;
  const parts = [];
  for (const block of message.blocks) {
    if (block && block.type === "text" && typeof block.text === "string") {
      parts.push(block.text);
    }
  }
  return parts.join("\n").trim() || null;
}

// ── HTTP server ──────────────────────────────────────────────────────

async function routeRequest(req, res) {
  const url = new URL(req.url, `http://${req.headers.host}`);
  const method = req.method || "GET";

  // GET /health
  if (method === "GET" && url.pathname === "/health") {
    return jsonResponse(res, 200, {
      status: "ok",
      desktop_server: DESKTOP_SERVER,
      trusted_openids: [...TRUSTED_OPENIDS],
      subscribed_sessions: [...sseControllers.keys()],
      outbox_keys: Object.keys(state.outbox),
    });
  }

  // POST /inbound — mock WeChat webhook
  if (method === "POST" && url.pathname === "/inbound") {
    try {
      const body = await readJsonBody(req);
      const result = await handleInboundMessage(body);
      return jsonResponse(res, 201, result);
    } catch (e) {
      return jsonResponse(res, 400, { error: String(e.message || e) });
    }
  }

  // POST /deliver/:item_id — manual approval bridge (mirrors desktop-server)
  const deliverMatch = url.pathname.match(/^\/deliver\/([a-zA-Z0-9_-]+)$/);
  if (method === "POST" && deliverMatch) {
    try {
      const itemId = deliverMatch[1];
      const openid = state.pending[itemId];
      const deliverResp = await desktopPost(
        `/api/desktop/dispatch/items/${encodeURIComponent(itemId)}/deliver`,
        {}
      );
      const delivered = deliverResp.item;
      if (openid && delivered.target && delivered.target.session_id) {
        state.openid_to_session[openid] = delivered.target.session_id;
        saveState(state);
        subscribeToSession(delivered.target.session_id, openid);
      }
      return jsonResponse(res, 200, { delivered, openid });
    } catch (e) {
      return jsonResponse(res, 400, { error: String(e.message || e) });
    }
  }

  // GET /outbox/:openid — poll for pending replies
  const outboxMatch = url.pathname.match(/^\/outbox\/(.+)$/);
  if (method === "GET" && outboxMatch) {
    const openid = decodeURIComponent(outboxMatch[1]);
    const drain = url.searchParams.get("drain") === "1";
    const items = state.outbox[openid] || [];
    if (drain) {
      delete state.outbox[openid];
      saveState(state);
    }
    return jsonResponse(res, 200, { openid, count: items.length, items });
  }

  // GET /state — debug dump
  if (method === "GET" && url.pathname === "/state") {
    return jsonResponse(res, 200, state);
  }

  return jsonResponse(res, 404, { error: "not found" });
}

const server = http.createServer((req, res) => {
  routeRequest(req, res).catch((e) => {
    console.error("[bridge] unhandled:", e);
    jsonResponse(res, 500, { error: "internal error" });
  });
});

server.listen(BRIDGE_PORT, "127.0.0.1", () => {
  console.error(`[bridge] listening on http://127.0.0.1:${BRIDGE_PORT}`);
  console.error(`[bridge] desktop server at ${DESKTOP_SERVER}`);
  console.error(`[bridge] trusted openids: ${[...TRUSTED_OPENIDS].join(", ") || "(none)"}`);
});

process.on("SIGINT", () => {
  console.error("[bridge] shutting down");
  for (const ctrl of sseControllers.values()) ctrl.abort();
  server.close(() => process.exit(0));
});

import { fetchJson } from "@/lib/desktop/transport";

// M5 WeChat bridge: health + group-scope config.
//
// Routes:
//   * GET /api/wechat/bridge/health
//   * GET /api/wechat/bridge/config
//   * POST /api/wechat/bridge/config

/** Group-scope config for the WeChat auto-ingest bridge. */
export interface WeChatIngestConfig {
  /** "all" passes every event; "whitelist" requires a known group_id. */
  enabled_mode: "all" | "whitelist";
  /** WeChat-side group ids allowed under `"whitelist"` mode. */
  enabled_group_ids: string[];
}

/** Per-channel health snapshot returned by the bridge health route. */
export interface WeChatChannelHealth {
  channel: "ilink" | "kefu";
  running: boolean;
  last_poll_unix_ms: number | null;
  last_inbound_unix_ms: number | null;
  last_ingest_unix_ms: number | null;
  consecutive_failures: number;
  last_error: string | null;
  processed_msg_count: number;
  dedupe_hit_count: number;
}

/** Envelope returned by `GET /api/wechat/bridge/health`. */
export interface WeChatBridgeHealthResponse {
  ilink: WeChatChannelHealth;
  kefu: WeChatChannelHealth;
  config: WeChatIngestConfig;
}

export async function fetchWeChatBridgeHealth(): Promise<WeChatBridgeHealthResponse> {
  return fetchJson<WeChatBridgeHealthResponse>("/api/wechat/bridge/health");
}

export async function fetchWeChatIngestConfig(): Promise<WeChatIngestConfig> {
  return fetchJson<WeChatIngestConfig>("/api/wechat/bridge/config");
}

export async function updateWeChatIngestConfig(
  config: WeChatIngestConfig,
): Promise<WeChatIngestConfig> {
  return fetchJson<WeChatIngestConfig>("/api/wechat/bridge/config", {
    method: "POST",
    body: JSON.stringify(config),
  });
}

// ── R1.3 reliability gate · aggregate WeChat health ────────────────
//
// Single endpoint that combines kefu monitor status, iLink account
// roster, and outbox counts under one verdict so the UI can render a
// "Connected / Degraded / Disconnected / Not configured" pill without
// re-implementing the rules.
//
// Route: GET /api/desktop/wechat/health
// Polled every 5s by `WeChatHealthPanel` while mounted.

export type WeChatHealthVerdict =
  | "connected"
  | "degraded"
  | "disconnected"
  | "not_configured";

/** Per-iLink-account snapshot embedded in the aggregate health. */
export interface WeChatHealthIlinkAccount {
  id: string;
  display_name: string;
  /** Wire status string from `WeChatAccountStatus::wire_tag`. */
  status: string;
  last_active_at: number | null;
}

/** Outbox status histogram embedded in the aggregate health. */
export interface WeChatHealthOutboxCounts {
  pending: number;
  sending: number;
  sent: number;
  failed: number;
  cancelled: number;
}

/** Kefu monitor snapshot — mirrors `KefuStatus` on the wire. */
export interface WeChatHealthKefu {
  configured: boolean;
  account_created: boolean;
  monitor_running: boolean;
  last_poll_unix_ms: number | null;
  last_inbound_unix_ms: number | null;
  consecutive_failures: number;
  last_error: string | null;
  capabilities: {
    text: boolean;
    url: boolean;
    query: boolean;
    commands: string[];
    file: boolean;
    image: boolean;
    card: boolean;
  };
}

/** Envelope returned by `GET /api/desktop/wechat/health`. */
export interface WeChatHealthSnapshot {
  /** Worst-wins overall verdict; the UI renders one pill from this. */
  health: WeChatHealthVerdict;
  kefu: WeChatHealthKefu;
  ilink_accounts: WeChatHealthIlinkAccount[];
  outbox: WeChatHealthOutboxCounts;
}

export async function fetchWeChatHealth(): Promise<WeChatHealthSnapshot> {
  return fetchJson<WeChatHealthSnapshot>("/api/desktop/wechat/health");
}

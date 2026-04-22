/**
 * S0.3 → S1 redesign: Ask page workbench.
 *
 * Architecture changes from S0.3:
 *   - ConversationScroller (use-stick-to-bottom) replaces manual scroll div
 *   - StreamingMessage component replaces inline StreamingIndicator
 *   - MessageList groups tool messages into ToolActionsGroup
 *   - SlashCommandPalette wired into Composer
 *   - DeepTutor warm visual language (#C35A2C + Lora serif + #FAF9F6)
 *
 * Preserved:
 *   - Welcome screen + demo mode
 *   - Permission dialog rendering
 *   - Tool / message flattening from runtime types
 *   - Subagent panel (MaintainerTaskTree)
 *   - All store wiring (settings, permissions, streaming)
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AlertTriangle, MessageSquare, Play, Sparkles } from "lucide-react";
import { AskHeader } from "./AskHeader";
import { useWikiQuery } from "./useWikiQuery";
import { WikiQueryMessage } from "./WikiQueryMessage";
import { MessageList } from "./MessageList";
import { Composer } from "./Composer";
import { ConversationScroller } from "./ConversationScroller";
import { ScrollToBottomButton } from "./ScrollToBottomButton";
import { WikiPermissionDialog } from "@/features/permission/WikiPermissionDialog";
import type { PermissionAction } from "@/features/permission/permission-types";
import {
  MaintainerTaskTree,
  extractSubagents,
} from "@/features/inbox/MaintainerTaskTree";
import { usePermissionsStore } from "@/state/permissions-store";
import { useStreamingStore } from "@/state/streaming-store";
import {
  forwardPermissionDecision,
  type ContentBlock,
  type ContextBasis,
  type ContextMode,
  type DesktopSessionDetail,
  type EnrichStatus,
  type RuntimeConversationMessage,
  type SourceRef,
} from "@/lib/tauri";
import {
  bindSourceToSession,
  clearSourceBinding,
  compactSession,
} from "./api/client";
import { useQueryClient } from "@tanstack/react-query";
import type { ConversationMessage } from "@/features/common/message-types";
import { MOCK_DEMO_MESSAGES } from "./mockDemoMessages";
import { formatIngestError } from "@/lib/ingest/format-error";
import { FailureBanner } from "@/components/ui/failure-banner";
import {
  classifyAskError,
  type AskErrorClassification,
} from "./ask-error-classifier";
import { useNavigate } from "react-router-dom";

/** Pull the optional `enrich_status` off a session detail. Returns
 * `undefined` when the session object isn't loaded yet, `null` when
 * the backend explicitly reports "no enrichment for this turn", and
 * an `EnrichStatus` otherwise. The canonical type now lives in
 * `@/lib/tauri` — see the wire-format comment there. */
function readEnrichStatus(
  detail: DesktopSessionDetail | null,
): EnrichStatus | null | undefined {
  if (!detail) return undefined;
  return detail.enrich_status ?? null;
}

/** Detect URLs in text and return the first one. */
function extractUrl(text: string): string | null {
  const match = text.match(/https?:\/\/[^\s，。！？]+/);
  return match ? match[0] : null;
}

/**
 * A2 — parse a handoff `bind=<kind>:<id-or-slug>` query string (with
 * optional `&title=...`) into a concrete `SourceRef`. Returns null when
 * the param is missing / malformed. URL format locked to
 *   bind=raw:123&title=Example%20Domain
 *   bind=wiki:foo-slug&title=Foo%20Page
 *   bind=inbox:42&title=Inbox%20Task
 * Worker C must emit links in exactly this shape.
 */
function parseBindParam(search: string): SourceRef | null {
  const params = new URLSearchParams(search);
  const bind = params.get("bind");
  if (!bind) return null;
  const colon = bind.indexOf(":");
  if (colon <= 0) return null;
  const kind = bind.slice(0, colon);
  const rest = bind.slice(colon + 1);
  if (!rest) return null;
  const title = params.get("title") ?? "";
  if (kind === "raw") {
    const id = Number.parseInt(rest, 10);
    if (!Number.isFinite(id)) return null;
    return { kind: "raw", id, title };
  }
  if (kind === "inbox") {
    const id = Number.parseInt(rest, 10);
    if (!Number.isFinite(id)) return null;
    return { kind: "inbox", id, title };
  }
  if (kind === "wiki") {
    return { kind: "wiki", slug: rest, title };
  }
  return null;
}

/**
 * A2 — strip the `bind` + `title` params from the current URL after a
 * handoff has been processed, without triggering a navigation.
 */
function stripBindFromUrl() {
  try {
    const url = new URL(window.location.href);
    const hash = url.hash; // e.g. "#/ask?bind=raw:123&title=..."
    const qIdx = hash.indexOf("?");
    if (qIdx < 0) return;
    const hashPath = hash.slice(0, qIdx);
    const hashSearch = hash.slice(qIdx + 1);
    const params = new URLSearchParams(hashSearch);
    params.delete("bind");
    params.delete("title");
    const rebuilt = params.toString();
    url.hash = rebuilt ? `${hashPath}?${rebuilt}` : hashPath;
    window.history.replaceState(null, "", url.toString());
  } catch {
    // Non-fatal — leave the URL alone if parsing fails.
  }
}

// v2 bugfix: fetchAndIngestUrl + isValidContent removed from the frontend.
// Backend's `maybe_enrich_url` (desktop-core) now handles the entire
// fetch → ingest → inject-to-system-prompt flow. The frontend only
// shows a progress hint via addSystemMessage in handleSendWithUrlFetch.

interface ProviderOption {
  id: string;
  label: string;
  model: string;
  isActive: boolean;
}

interface AskWorkbenchProps {
  session: DesktopSessionDetail | null;
  isLoadingSession: boolean;
  isSending: boolean;
  errorMessage?: string;
  onSend: (
    message: string,
    options?: { mode?: ContextMode },
  ) => void | Promise<void>;
  onStop?: () => void;
  onCreateSession?: () => void;
  modelLabel?: string;
  environmentLabel?: string;
  projectPath?: string;
  providers?: ProviderOption[];
  onSwitchProvider?: (id: string) => void;
  /**
   * A2 — hook-backed ensure-session-and-bind. When provided, the URL
   * handoff parser can attach a bind to the (possibly freshly created)
   * active session. Typically wired to `useAskSession`'s `onSend` ladder
   * so session lifecycle guarantees are preserved.
   */
  onEnsureAndBind?: (source: SourceRef) => Promise<DesktopSessionDetail>;
  /** v2: compact mode for the ChatSidePanel (Wiki mode right-side chat).
   * Tightens message spacing and Composer padding, hides the Agent panel
   * toggle. All state visualization (streaming cursor, token counts,
   * tool blocks, permission dialogs) is preserved. */
  compact?: boolean;
  /** v2: suppress the AskHeader row (model + agent-panel toggle).
   * Intended for compact contexts where the parent already shows
   * a label and where screen real estate is precious. */
  hideHeader?: boolean;
  /**
   * P1-2 fallback UX: when the providers registry resolves to an
   * empty list OR the settings query itself errors, the parent
   * (AskPage) hands `true` here. We render an explicit "设置未就绪"
   * banner under the header instead of letting the header silently
   * stay on a stale "Opus 4.6" placeholder.
   */
  settingsUnready?: boolean;
}

export function AskWorkbench({
  session,
  isLoadingSession,
  isSending,
  errorMessage,
  onSend,
  onStop,
  onCreateSession,
  modelLabel = "AI",
  environmentLabel = "Local",
  projectPath,
  providers,
  onSwitchProvider,
  onEnsureAndBind,
  compact = false,
  hideHeader = false,
  settingsUnready = false,
}: AskWorkbenchProps) {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  // Python deps auto-installed by backend on startup — no frontend action needed.

  const pendingPermission = usePermissionsStore(
    (state) => state.pendingRequest
  );
  const setPendingPermission = usePermissionsStore(
    (state) => state.setPendingPermission
  );
  const clearPendingPermission = usePermissionsStore(
    (state) => state.clearPendingPermission
  );
  const streamingContent = useStreamingStore((s) => s.streamingContent);
  const streamingThinking = useStreamingStore((s) => s.streamingThinking);

  const [showDemo, setShowDemo] = useState(false);
  const [localMessages, setLocalMessages] = useState<ConversationMessage[]>([]);
  const [showAgentPanel, setShowAgentPanel] = useState(false);

  const messages = useMemo(
    () =>
      flattenSessionMessages(
        session?.session.messages ?? [],
        // A1 integration patch: backend currently emits `context_basis`
        // on the top-level `DesktopSessionDetail` (per-Snapshot side
        // channel), not on individual `RuntimeConversationMessage`
        // entries. Pass the detail-level basis through so flatten can
        // fall back to it for the most recent assistant turn when the
        // message-level field is absent. See Worker A report §"SSE
        // ContextBasis 字段": basis is only stamped on the snapshot
        // triggered by append_user_message; subsequent snapshots carry
        // None, so this label is turn-local and disappears on reload.
        session?.context_basis ?? null
      ),
    [session?.session.messages, session?.context_basis]
  );
  const displayMessages = useMemo(() => {
    if (messages.length > 0) return [...messages, ...localMessages];
    if (showDemo) return [...MOCK_DEMO_MESSAGES, ...localMessages];
    return localMessages;
  }, [messages, showDemo, localMessages]);

  const agentCount = useMemo(
    () => extractSubagents(displayMessages).length,
    [displayMessages]
  );
  // isRunning: true while the AI is generating a response.
  // Keep it true for a few seconds after POST returns, bridging the gap
  // until the next poll detects turn_state=running.
  const [extendedRunning, setExtendedRunning] = useState(false);
  useEffect(() => {
    if (isSending) {
      setExtendedRunning(true);
    }
  }, [isSending]);
  useEffect(() => {
    if (!extendedRunning) return;
    if (session?.turn_state === "running") {
      // Backend confirmed running — no longer need the extension
      setExtendedRunning(false);
      return;
    }
    // Auto-clear after 5s if backend never reports running (very short reply)
    const t = setTimeout(() => setExtendedRunning(false), 5000);
    return () => clearTimeout(t);
  }, [extendedRunning, session?.turn_state]);
  const isRunning = session?.turn_state === "running" || isSending || extendedRunning;

  // Clear local messages when session changes
  useEffect(() => {
    setLocalMessages([]);
  }, [session?.id]);

  const handlePermissionDecision = useCallback(
    (action: PermissionAction) => {
      if (pendingPermission) {
        if (session?.id) {
          void forwardPermissionDecision(session.id, {
            requestId: pendingPermission.id,
            decision: action,
          })
            .then(() => {
              clearPendingPermission(pendingPermission.id);
            })
            .catch((error) => {
              console.warn("Failed to forward permission decision to backend", {
                sessionId: session.id,
                requestId: pendingPermission.id,
                decision: action,
                error,
              });
            });
          return;
        }

        clearPendingPermission(pendingPermission.id);
      }
    },
    [clearPendingPermission, pendingPermission, session?.id]
  );

  const addSystemMessage = useCallback(
    (text: string, idOverride?: string) => {
      const id =
        idOverride ?? `system-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
      setLocalMessages((prev) => [
        ...prev,
        {
          id,
          role: "system" as const,
          type: "text" as const,
          content: text,
          timestamp: Date.now(),
        },
      ]);
      return id;
    },
    [],
  );

  /** Update an existing local message by id, or drop it entirely when
   * `nextContent === null`. Used by the URL-enrich upgrade path so the
   * optimistic "⏳ 正在抓取..." hint can be promoted/downgraded/cleared
   * once the backend resolves. */
  const updateLocalMessage = useCallback(
    (id: string, nextContent: string | null) => {
      setLocalMessages((prev) => {
        if (nextContent === null) {
          return prev.filter((m) => m.id !== id);
        }
        return prev.map((m) =>
          m.id === id ? { ...m, content: nextContent } : m,
        );
      });
    },
    [],
  );

  /** Tracks the id of the pending "⏳ 正在抓取..." optimistic message so
   * the next enrich_status snapshot from the backend can replace it in
   * place. `null` means "no pending enrich on screen". */
  const pendingEnrichIdRef = useRef<string | null>(null);

  /** Last `enrich_status` we reconciled against, so we don't re-apply
   * the same status on every poll tick. The status shape is tiny so
   * JSON.stringify is fine for equality. */
  const lastEnrichKeyRef = useRef<string | null>(null);

  // When the session detail emits a new enrich_status, reconcile it
  // against the pending optimistic message. Supports (kind = wire value):
  //  - success             → "✓ 已抓取：<title> (raw #<id>)"
  //  - rejected_quality    → "⚠ 抓取失败（<reason>）。链接仍在，您可手动重试或继续发送。"
  //  - fetch_failed        → "⚠ 抓取失败（<reason>）"
  //  - prerequisite_missing → "⚠ 环境缺依赖（<dep>）：<hint>"
  //  - none                → drop the optimistic message (nothing to fetch)
  //  - null                → backend reports: no URL worth enriching, drop hint
  //  - undefined           → session not loaded yet; leave pending msg untouched
  const enrichStatus = readEnrichStatus(session);
  useEffect(() => {
    const pendingId = pendingEnrichIdRef.current;
    if (!pendingId) return;

    // Undefined means the backend hasn't shipped enrich_status yet.
    // Leave the optimistic message in place for legacy behaviour.
    if (enrichStatus === undefined) return;

    // Key includes discriminant + payload so the effect runs once per
    // distinct status (and not on every 1-s session poll tick).
    const key = JSON.stringify(enrichStatus ?? null);
    if (lastEnrichKeyRef.current === key) return;
    lastEnrichKeyRef.current = key;

    if (enrichStatus === null) {
      // Backend reports: no URL worth enriching. Drop the hint.
      updateLocalMessage(pendingId, null);
      pendingEnrichIdRef.current = null;
      return;
    }

    switch (enrichStatus.kind) {
      case "none":
        updateLocalMessage(pendingId, null);
        break;
      case "success":
        updateLocalMessage(
          pendingId,
          `✓ 已抓取：${enrichStatus.title} (raw #${enrichStatus.raw_id})`,
        );
        break;
      case "reused": {
        // M3 dedupe path: the URL-ingest orchestrator recognised a
        // prior raw for the same canonical URL and handed the existing
        // entry back rather than re-fetching.
        //
        // M4: the `reason` field is now a short prefix-tagged string
        // (e.g. "pending:...", "refreshed:prev=42", "content_duplicate:src=..."),
        // which lets us surface the *why* of each reuse path. We branch on
        // the prefix to render a precise hint; anything unrecognised falls
        // back to the pre-M4 wording so we stay compatible with older
        // backends that still emit untagged reasons.
        const idStr = String(enrichStatus.raw_id).padStart(5, "0");
        const reason = enrichStatus.reason ?? "";
        let message: string;
        if (reason.startsWith("refreshed:prev=")) {
          const prev = reason.substring("refreshed:prev=".length);
          message = `⟳ 内容已更新,新建 raw #${idStr}(原 raw #${prev} 保留)`;
        } else if (reason.startsWith("content_duplicate:src=")) {
          message = `✓ 相同内容已存在于 raw #${idStr}(来自不同 URL)`;
        } else if (reason.startsWith("pending:")) {
          message = `✓ 已复用 raw #${idStr},inbox 中已有待处理任务`;
        } else if (reason.startsWith("approved:")) {
          message = `✓ 已复用 raw #${idStr}(此前已审批入库)`;
        } else if (reason.startsWith("rejected:")) {
          message = `✓ 已复用 raw #${idStr}(此前被拒;如需重抓请使用 force)`;
        } else if (reason.startsWith("silent:")) {
          message = `✓ 已复用 raw #${idStr}(${enrichStatus.title})`;
        } else {
          // Pre-M4 / untagged reason — fall back to the original wording.
          message = `✓ 已复用此前入库的素材 raw #${idStr}(${enrichStatus.title})`;
        }
        updateLocalMessage(pendingId, message);
        break;
      }
      case "rejected_quality":
        updateLocalMessage(
          pendingId,
          `⚠ 抓取失败（${formatIngestError(enrichStatus.reason)}）。链接仍在，您可手动重试或继续发送。`,
        );
        break;
      case "fetch_failed":
        updateLocalMessage(
          pendingId,
          `⚠ 抓取失败（${formatIngestError(enrichStatus.reason)}）`,
        );
        break;
      case "prerequisite_missing":
        updateLocalMessage(
          pendingId,
          `⚠ 环境缺依赖（${enrichStatus.dep}）：${enrichStatus.hint}`,
        );
        break;
    }

    // The hint has been upgraded/downgraded into its final text; any
    // further enrich_status snapshot (e.g. a later turn) should find a
    // fresh optimistic id, so clear the ref.
    pendingEnrichIdRef.current = null;
  }, [enrichStatus, updateLocalMessage]);

  // ─── A2: source-binding integration ───────────────────────────────
  //
  // Persistent binding state comes from the session detail (never from
  // the query string). The URL-handoff parser lives in an effect below
  // and is a ONE-SHOT: it consumes `?bind=...` on mount, calls into
  // `onEnsureAndBind`, and strips the param so the next render no
  // longer sees it. Critical: we do NOT touch `useAskSession` — the
  // ensure-create ladder stays owned by that hook.
  const binding = session?.source_binding ?? null;
  const bindHandledRef = useRef(false);

  useEffect(() => {
    if (bindHandledRef.current) return;
    if (!onEnsureAndBind) return;
    // Hash-based routing: the query string lives inside `location.hash`
    // (e.g. "#/ask?bind=raw:123&title=X"). Fall back to search for dev.
    const hash = window.location.hash;
    const qIdx = hash.indexOf("?");
    const search = qIdx >= 0 ? hash.slice(qIdx) : window.location.search;
    const parsed = parseBindParam(search);
    if (!parsed) return;
    bindHandledRef.current = true;
    void onEnsureAndBind(parsed)
      .then(() => stripBindFromUrl())
      .catch((err) => {
        console.warn("[ask:bind] handoff bind failed", err);
        // Still strip so we don't retry the failing param on next mount.
        stripBindFromUrl();
      });
  }, [onEnsureAndBind]);

  const handleClearBinding = useCallback(async () => {
    if (!session?.id) return;
    try {
      const next = await clearSourceBinding(session.id);
      queryClient.setQueryData(
        ["clawwiki", "ask", "session", session.id],
        next,
      );
    } catch (err) {
      console.warn("[ask:bind] clear failed", err);
    }
  }, [session?.id, queryClient]);

  // A3 — promote a turn-local auto-binding to a persistent session
  // binding. Invoked from the inline "📌 固定到会话" button inside
  // <UsedSourcesBar> on an assistant message with
  // `context_basis.auto_bound === true`. After the POST succeeds the
  // next SSE snapshot will reflect `source_binding` populated +
  // `auto_bound=false`, so the chip tone transitions from blue (A3)
  // to orange (A2) automatically.
  const handlePromoteToSession = useCallback(
    async (source: SourceRef) => {
      if (!session?.id) return;
      try {
        const next = await bindSourceToSession(session.id, source);
        queryClient.setQueryData(
          ["clawwiki", "ask", "session", session.id],
          next,
        );
      } catch (err) {
        console.warn("[a3:promote] failed", err);
      }
    },
    [session?.id, queryClient],
  );

  // Slash command handlers
  const handleClear = useCallback(() => {
    setLocalMessages([]);
    onCreateSession?.();
  }, [onCreateSession]);

  const handleNewSession = useCallback(() => {
    onCreateSession?.();
  }, [onCreateSession]);

  const handleCompact = useCallback(() => {
    if (!session?.id) return;
    void compactSession(session.id)
      .then(() => addSystemMessage("会话已压缩。"))
      .catch((err) => addSystemMessage(`压缩失败：${err instanceof Error ? err.message : String(err)}`));
  }, [session?.id, addSystemMessage]);

  // Input ref for focus shortcut
  const inputRef = useRef<HTMLTextAreaElement>(null);

  // v2: Wiki query hook for ?-prefixed questions.
  const wikiQuery = useWikiQuery();

  // Wrap onSend: detect ? prefix → wiki query; detect URLs → fetch; else → session send
  const handleSendWithUrlFetch = useCallback(async (
    message: string,
    options?: { mode?: ContextMode },
  ) => {
    // ?-prefix: route to /query (wiki knowledge Q&A)
    const trimmed = message.trimStart();
    if (trimmed.startsWith("?") || trimmed.startsWith("/query ")) {
      const question = trimmed.startsWith("/query ")
        ? trimmed.slice(7).trim()
        : trimmed.slice(1).trim();
      if (question) {
        wikiQuery.queryWiki(question);
        return;
      }
    }

    // v2 bugfix: unified URL handling. The backend's `maybe_enrich_url`
    // (desktop-core::append_user_message) handles fetch + ingest + inject
    // into system prompt. The frontend's only job is to show a progress
    // hint so the user knows the backend is working.
    //
    // Previously this function called `/api/desktop/wechat-fetch` directly
    // and inlined the enriched content into the user message, which both
    // polluted session history AND raced with the backend's own enrichment.
    //
    // M2 upgrade: the optimistic "⏳ 正在抓取..." hint is given a
    // stable id we can later promote/demote via `pendingEnrichIdRef`
    // once the backend reports `enrich_status` in the next session
    // detail snapshot. When `enrich_status` never arrives (Worker A
    // backend still in flight), the hint stays as-is — no regression.
    const url = extractUrl(message);
    if (url) {
      const pendingId = `system-url-enrich-${Date.now()}-${Math.random()
        .toString(36)
        .slice(2, 6)}`;
      const isWeChat = url.includes("mp.weixin.qq.com") || url.includes("weixin.qq.com");
      if (isWeChat) {
        addSystemMessage("⏳ 正在抓取微信文章（Playwright，最长 45 秒）...", pendingId);
      } else {
        addSystemMessage(
          `⏳ 正在抓取 ${url.slice(0, 60)}${url.length > 60 ? "…" : ""}`,
          pendingId,
        );
      }
      pendingEnrichIdRef.current = pendingId;
      // Reset the reconciler key so the next enrich_status snapshot
      // is treated as fresh even if it happens to match a prior one.
      lastEnrichKeyRef.current = null;
      // Fall through to backend. maybe_enrich_url will handle fetch/ingest
      // and inject content into the turn's system prompt.
    }
    await onSend(message, options);
  }, [onSend, addSystemMessage, wikiQuery]);

  return (
    <div className={`flex flex-1 overflow-hidden ${compact ? "compact-ask" : ""}`}>
     <div className="flex flex-1 flex-col overflow-hidden">
      {!hideHeader && (
        <AskHeader
          projectPath={projectPath}
          modelLabel={modelLabel}
          environmentLabel={environmentLabel}
          isStreaming={isRunning}
          agentCount={agentCount}
          showAgentPanel={showAgentPanel}
          onToggleAgentPanel={() => setShowAgentPanel((v) => !v)}
        />
      )}
      {settingsUnready && !hideHeader && (
        <div
          className="mx-4 mb-2 mt-1 flex items-center gap-2 rounded-md border px-3 py-1.5 text-caption"
          role="status"
          style={{
            borderColor:
              "color-mix(in srgb, var(--color-warning) 35%, transparent)",
            backgroundColor:
              "color-mix(in srgb, var(--color-warning) 8%, transparent)",
            color: "var(--color-warning)",
          }}
        >
          <AlertTriangle
            className="size-3.5 shrink-0"
            strokeWidth={1.5}
            aria-hidden="true"
          />
          <span className="font-medium">设置未就绪</span>
          <span className="text-muted-foreground/90">
            · 还没有解析到模型服务。
          </span>
          <a
            href="#/settings"
            className="ml-auto underline underline-offset-2 hover:no-underline"
            style={{ color: "var(--color-warning)" }}
          >
            打开设置 →
          </a>
        </div>
      )}

      {displayMessages.length === 0 && !isLoadingSession ? (
        <WelcomeScreen onShowDemo={() => setShowDemo(true)} />
      ) : (
        <ConversationScroller>
          <div className="flex min-h-full flex-col">
            {showDemo && (
              <div className="mb-2 flex items-center justify-between rounded-lg border border-border/30 bg-muted/10 px-3 py-1.5">
                <span className="text-label text-muted-foreground">
                  演示模式 — 展示示例对话
                </span>
                <div className="flex items-center gap-2">
                  {!pendingPermission && (
                    <button
                      className="text-label font-medium text-muted-foreground hover:text-foreground hover:underline"
                      onClick={() =>
                        setPendingPermission({
                          id: `demo-perm-${Date.now()}`,
                          toolName: "Bash",
                          toolInput: {
                            command: "npm install @radix-ui/react-dialog",
                          },
                          riskLevel: "high",
                        })
                      }
                    >
                      测试权限
                    </button>
                  )}
                  <button
                    className="text-label font-medium text-foreground hover:underline"
                    onClick={() => setShowDemo(false)}
                  >
                    退出演示
                  </button>
                </div>
              </div>
            )}

            {isLoadingSession && <MessageSkeleton />}

            <MessageList
              key={session?.id ?? "ask-empty"}
              sessionKey={session?.id ?? "ask-empty"}
              messages={displayMessages}
              streamingContent={streamingContent}
              streamingThinking={streamingThinking}
              isStreaming={isRunning && !pendingPermission}
              onPromoteToSession={handlePromoteToSession}
            />

          {/* v2: Wiki query result (? prefix) — not stored in session history */}
          {(wikiQuery.isQuerying || wikiQuery.answer || wikiQuery.error) && (
            <WikiQueryMessage
              question={wikiQuery.question}
              answer={wikiQuery.answer}
              sources={wikiQuery.sources}
              isStreaming={wikiQuery.isQuerying}
              error={wikiQuery.error}
            />
          )}

          {/* Permission dialog rendered inside scroller for flow */}
          {pendingPermission && (
            <WikiPermissionDialog
              request={pendingPermission}
              onDecision={handlePermissionDecision}
            />
          )}

          {/* Error banner — R1 trust layer: classifier-driven friendly
              `FailureBanner`. Known kinds (credentials, broker, session)
              get bespoke titles + recovery CTAs; unknown kinds fall
              through to a generic message with the raw string parked
              under the technical-detail `<details>`. */}
            {errorMessage && (
              <AskFailureBannerSwitch
                classification={classifyAskError(errorMessage)}
                onOpenSettings={() => navigate("/settings")}
                onNewSession={() => {
                  onCreateSession?.();
                  addSystemMessage("已新建对话，请重试你的消息。");
                }}
                onDismiss={() =>
                  addSystemMessage("错误已忽略。你可以重试上一条消息。")
                }
              />
            )}

          </div>
          <ScrollToBottomButton />
        </ConversationScroller>
      )}

      <Composer
        onSend={handleSendWithUrlFetch}
        onStop={onStop}
        isBusy={isRunning || !!pendingPermission}
        modelLabel={modelLabel}
        environmentLabel={environmentLabel}
        providers={providers}
        onSwitchProvider={onSwitchProvider}
        inputRef={inputRef}
        onClear={handleClear}
        onNewSession={handleNewSession}
        onCompact={handleCompact}
        binding={binding}
        onClearBinding={handleClearBinding}
      />

      {/* StatusLine removed — Composer's inline toolbar now shows model + permission + environment */}
     </div>

      {/* Maintainer task tree side panel (CCD soul ④) */}
      {showAgentPanel && !compact && (
        <MaintainerTaskTree
          messages={displayMessages}
          onClose={() => setShowAgentPanel(false)}
        />
      )}
    </div>
  );
}

/* ─── Loading skeleton ──────────────────────────────────────────── */

function MessageSkeleton() {
  return (
    <div className="space-y-3">
      {/* User message skeleton */}
      <div className="rounded-lg border border-border/30 bg-muted/10 p-3">
        <div className="mb-2 h-2.5 w-12 animate-pulse rounded bg-muted/40" />
        <div className="space-y-1.5">
          <div className="h-3 w-3/4 animate-pulse rounded bg-muted/30" />
          <div className="h-3 w-1/2 animate-pulse rounded bg-muted/30" />
        </div>
      </div>
      {/* Assistant message skeleton */}
      <div className="rounded-lg border border-border/30 bg-muted/10 p-3">
        <div className="mb-2 h-2.5 w-16 animate-pulse rounded bg-muted/40" />
        <div className="space-y-1.5">
          <div className="h-3 w-full animate-pulse rounded bg-muted/30" />
          <div className="h-3 w-5/6 animate-pulse rounded bg-muted/30" />
          <div className="h-3 w-2/3 animate-pulse rounded bg-muted/30" />
        </div>
      </div>
      {/* Tool use skeleton */}
      <div className="rounded-lg border border-border/30 bg-muted/10 p-2.5">
        <div className="flex items-center gap-2">
          <div className="size-3.5 animate-pulse rounded bg-muted/40" />
          <div className="h-3 w-20 animate-pulse rounded bg-muted/40" />
          <div className="h-3 flex-1 animate-pulse rounded bg-muted/20" />
        </div>
      </div>
    </div>
  );
}

/* ─── Welcome Screen ─────────────────────────────────────────────── */

// A5 — the pre-A5 welcome screen was a 6-card capability grid that
// read like marketing copy. Claude's empty state is closer to "just
// start a conversation"; the surface gives you a greeting, a few
// concrete starter prompts, and the composer below stays the real
// call-to-action. We keep the 演示模式 button because it's how the
// product ships mock data for gray-test users.
const STARTER_PROMPTS = [
  "帮我总结今天微信里入库的素材，给一个两段式摘要。",
  "把以下链接整理成一篇知识页，保持客观语气：",
  "我想问一个长期问题：ClawWiki 应该怎么处理长尾知识？",
];

function WelcomeScreen({ onShowDemo }: { onShowDemo?: () => void }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-6 px-6 py-10">
      {/* Greeting */}
      <div className="flex flex-col items-center gap-3 text-center">
        <div
          className="flex size-12 items-center justify-center rounded-2xl"
          style={{
            background:
              "linear-gradient(135deg, var(--deeptutor-primary, var(--claude-orange)), var(--deeptutor-primary-hi, var(--claude-orange-shimmer)))",
          }}
        >
          <MessageSquare className="size-6 text-white" />
        </div>
        <div>
          <h2 className="ask-serif text-xl font-semibold text-foreground">
            有什么我能帮你的？
          </h2>
          <p className="mt-1.5 max-w-md text-sm text-muted-foreground/80">
            直接在下面输入框发一条问题，或粘贴一条链接、一条微信素材，让 AI
            帮你整理。
          </p>
        </div>
      </div>

      {/* Starter prompts — Claude-style suggestion strip */}
      <ul className="flex w-full max-w-lg flex-col gap-1.5">
        {STARTER_PROMPTS.map((prompt) => (
          <li key={prompt}>
            <div className="group/prompt flex items-start gap-2 rounded-md border border-border/30 bg-card/30 px-3 py-2 text-[13px] leading-relaxed text-muted-foreground/80 transition-colors hover:border-border/60 hover:bg-accent/30 hover:text-foreground">
              <Sparkles
                className="mt-0.5 size-3.5 shrink-0 text-muted-foreground/40 transition-colors group-hover/prompt:text-[color:var(--deeptutor-primary,var(--claude-orange))]"
                aria-hidden
              />
              <span className="min-w-0 flex-1">{prompt}</span>
            </div>
          </li>
        ))}
      </ul>

      {/* Shortcut hint + demo fallback */}
      <div className="flex flex-col items-center gap-2 text-[11px] text-muted-foreground/50">
        <div className="flex items-center gap-1.5">
          <kbd className="rounded border border-border/40 bg-muted/20 px-1.5 py-0.5 font-mono">
            Enter
          </kbd>
          <span>发送</span>
          <span className="mx-1 opacity-40">·</span>
          <kbd className="rounded border border-border/40 bg-muted/20 px-1.5 py-0.5 font-mono">
            Shift+Enter
          </kbd>
          <span>换行</span>
          <span className="mx-1 opacity-40">·</span>
          <kbd className="rounded border border-border/40 bg-muted/20 px-1.5 py-0.5 font-mono">
            /
          </kbd>
          <span>命令</span>
        </div>
        {onShowDemo && (
          <button
            type="button"
            className="flex items-center gap-1 rounded-md px-2 py-0.5 text-[11px] text-muted-foreground/50 transition-colors hover:text-foreground"
            onClick={onShowDemo}
          >
            <Play className="size-2.5" />
            查看演示对话
          </button>
        )}
      </div>
    </div>
  );
}

/* ─── Message flattening ─────────────────────────────────────────── */

function flattenSessionMessages(
  source: RuntimeConversationMessage[],
  detailContextBasis: ContextBasis | null = null
): ConversationMessage[] {
  const items: ConversationMessage[] = [];
  let order = 0;

  for (const message of source) {
    let usageAttached = false;
    // A1 sprint — context-basis attaches to the FIRST assistant text
    // block of each message, same policy as usage. Legacy messages
    // without the field skip this branch (UI tolerates null).
    let basisAttached = false;
    for (const block of message.blocks) {
      order += 1;
      const entry = toDisplayMessage(message.role, block, order);
      if (entry) {
        // Attach usage data to the first text block of assistant messages
        if (!usageAttached && message.usage && entry.role === "assistant" && entry.type === "text") {
          entry.usage = {
            inputTokens: message.usage.input_tokens,
            outputTokens: message.usage.output_tokens,
          };
          usageAttached = true;
        }
        if (
          !basisAttached &&
          message.context_basis != null &&
          entry.role === "assistant" &&
          entry.type === "text"
        ) {
          entry.contextBasis = message.context_basis;
          basisAttached = true;
        }
        items.push(entry);
      }
    }
  }

  // A1 integration fallback: if no message-level context_basis was
  // found (current backend reality — see Worker A/B contract gap)
  // but the session detail carries a top-level basis, attach it to
  // the LAST assistant-text entry as the "turn-local" basis.
  if (detailContextBasis) {
    for (let i = items.length - 1; i >= 0; i--) {
      const entry = items[i];
      if (entry.role === "assistant" && entry.type === "text" && !entry.contextBasis) {
        entry.contextBasis = detailContextBasis;
        break;
      }
    }
  }

  return items;
}

function toDisplayMessage(
  role: RuntimeConversationMessage["role"],
  block: ContentBlock,
  order: number
): ConversationMessage | null {
  if (block.type === "text") {
    return {
      id: `${role}-text-${order}`,
      role: role === "user" ? "user" : role === "system" ? "system" : "assistant",
      type: "text",
      content: block.text,
      timestamp: order,
    };
  }

  if (block.type === "tool_use") {
    return {
      id: block.id,
      role: "assistant",
      type: "tool_use",
      content: block.input,
      timestamp: order,
      toolUse: {
        toolUseId: block.id,
        toolName: block.name,
        toolInput: block.input,
      },
    };
  }

  return {
    id: `${block.tool_use_id}-result-${order}`,
    role: "assistant",
    type: "tool_result",
    content: block.output,
    timestamp: order,
    toolResult: {
      toolUseId: block.tool_use_id,
      toolName: block.tool_name,
      output: block.output,
      isError: block.is_error,
    },
  };
}

/**
 * R1 trust-layer helper — picks the right `FailureBanner` copy + CTAs
 * based on the classifier output. Scoped to this file because the
 * copy is Ask-specific ("设置", "新建对话", "文档", etc.); other
 * surfaces (Maintainer, Graph) pass their own strings.
 *
 * All unknown kinds fall through to a generic banner so we never
 * silently drop a runtime error — the raw string is preserved under
 * the technical-detail `<details>`.
 */
function AskFailureBannerSwitch({
  classification,
  onOpenSettings,
  onNewSession,
  onDismiss,
}: {
  classification: AskErrorClassification;
  onOpenSettings: () => void;
  onNewSession: () => void;
  onDismiss: () => void;
}) {
  const { kind, raw } = classification;
  if (kind === "credentials_missing") {
    return (
      <FailureBanner
        severity="error"
        title="🔐 还没连接大模型账号"
        description={
          <>
            Ask 需要一个大模型账号来生成回答。当前没找到有效的 API
            key（<code className="font-mono">ANTHROPIC_AUTH_TOKEN</code>{" "}
            或 <code className="font-mono">ANTHROPIC_API_KEY</code>）。
          </>
        }
        actions={[
          { label: "打开设置", onClick: onOpenSettings, variant: "primary" },
          {
            label: "查看文档",
            href: "https://docs.anthropic.com/claude/reference/getting-started-with-the-api",
            variant: "secondary",
          },
        ]}
        technicalDetail={raw}
        dismissible
        onDismiss={onDismiss}
      />
    );
  }
  if (kind === "broker_empty") {
    return (
      <FailureBanner
        severity="warning"
        title="🪫 大模型账号池空"
        description="暂时没有可用的 Claude 账号来处理这一轮。稍等片刻再重试，或在设置里补充账号。"
        actions={[
          { label: "打开设置", onClick: onOpenSettings, variant: "primary" },
        ]}
        technicalDetail={raw}
        dismissible
        onDismiss={onDismiss}
      />
    );
  }
  if (kind === "session_not_found") {
    return (
      <FailureBanner
        severity="warning"
        title="📝 对话不存在或已过期"
        description="后端找不到这个会话 id。可能是服务重启清空了内存状态，或会话已被删除。新建一个对话即可继续。"
        actions={[
          { label: "新建对话", onClick: onNewSession, variant: "primary" },
        ]}
        technicalDetail={raw}
        dismissible
        onDismiss={onDismiss}
      />
    );
  }
  if (kind === "url_enrich_failed") {
    return (
      <FailureBanner
        severity="warning"
        title="🔗 链接抓取失败"
        description="没能从你发的链接里取到正文。可能是网站挡了 bot、超时，或者这不是一个可读的网页。"
        technicalDetail={raw}
        dismissible
        onDismiss={onDismiss}
      />
    );
  }
  // Unknown — preserve the raw string visibly but still in the nicer
  // banner shell so layout is consistent.
  return (
    <FailureBanner
      severity="error"
      title="出错了"
      description="后端在处理这一轮时失败了。你可以忽略后重试上一条消息，或查看下面的技术细节。"
      technicalDetail={raw}
      dismissible
      onDismiss={onDismiss}
    />
  );
}

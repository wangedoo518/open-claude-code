/**
 * Subagent Panel — shows active and completed subagents in the current session.
 *
 * Derives subagent state from Agent tool_use / tool_result messages in the
 * conversation stream. Follows docs/desktop-shell/tokens/functional-tokens.md §4.
 */

import { useMemo } from "react";
import {
  Brain,
  Search,
  FileSearch,
  CheckCircle2,
  Shield,
  BookOpen,
  Loader2,
  AlertCircle,
  X,
  ChevronRight,
  ChevronDown,
} from "lucide-react";
import { useState } from "react";
import type { ConversationMessage } from "./types";

/* ─── Types ────────────────────────────────────────────────────── */

export type SubagentType =
  | "general-purpose"
  | "plan"
  | "explore"
  | "verification"
  | "claude-code-guide";

export type SubagentStatus = "running" | "completed" | "error";

export interface SubagentInfo {
  id: string;
  type: SubagentType;
  description: string;
  model?: string;
  status: SubagentStatus;
  background?: boolean;
  isolation?: string;
  resultPreview?: string;
}

/* ─── Agent type metadata ──────────────────────────────────────── */

const AGENT_TYPE_META: Record<
  SubagentType,
  { icon: typeof Brain; label: string; color: string }
> = {
  "general-purpose": {
    icon: Brain,
    label: "General",
    color: "var(--agent-purple)",
  },
  plan: {
    icon: FileSearch,
    label: "Plan",
    color: "var(--color-warning)",
  },
  explore: {
    icon: Search,
    label: "Explore",
    color: "var(--agent-cyan)",
  },
  verification: {
    icon: Shield,
    label: "Verify",
    color: "var(--color-success)",
  },
  "claude-code-guide": {
    icon: BookOpen,
    label: "Guide",
    color: "var(--claude-blue)",
  },
};

/* ─── Extract subagents from messages ──────────────────────────── */

export function extractSubagents(
  messages: ConversationMessage[]
): SubagentInfo[] {
  const agents: SubagentInfo[] = [];
  const agentToolUses = new Map<string, SubagentInfo>();

  for (const msg of messages) {
    if (msg.type === "tool_use" && msg.toolUse?.toolName === "Agent") {
      let parsed: Record<string, unknown> = {};
      try {
        parsed =
          typeof msg.toolUse.toolInput === "string"
            ? (JSON.parse(msg.toolUse.toolInput) as Record<string, unknown>)
            : {};
      } catch {
        /* noop */
      }

      const agent: SubagentInfo = {
        id: msg.id,
        type: (parsed.subagent_type as SubagentType) ?? "general-purpose",
        description:
          (parsed.description as string) ?? "Subagent task",
        model: parsed.model as string | undefined,
        status: "running",
        background: parsed.run_in_background as boolean | undefined,
        isolation: parsed.isolation as string | undefined,
      };
      agentToolUses.set(msg.id, agent);
      agents.push(agent);
    }

    if (
      msg.type === "tool_result" &&
      msg.toolResult?.toolName === "Agent"
    ) {
      // Match to an existing agent by scanning for the most recent unresolved one
      for (let i = agents.length - 1; i >= 0; i--) {
        if (agents[i].status === "running") {
          agents[i].status = msg.toolResult.isError ? "error" : "completed";
          agents[i].resultPreview = msg.toolResult.output;
          break;
        }
      }
    }
  }

  return agents;
}

/* ─── Panel Component ──────────────────────────────────────────── */

interface SubagentPanelProps {
  messages: ConversationMessage[];
  onClose: () => void;
}

export function SubagentPanel({ messages, onClose }: SubagentPanelProps) {
  const agents = useMemo(() => extractSubagents(messages), [messages]);

  const running = agents.filter((a) => a.status === "running");
  const completed = agents.filter((a) => a.status !== "running");

  return (
    <div className="flex h-full w-[280px] shrink-0 flex-col border-l border-border/50 bg-background">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-border/50 px-3 py-2">
        <div className="flex items-center gap-2">
          <Brain
            className="size-4"
            style={{ color: "var(--agent-purple)" }}
          />
          <span className="text-body-sm font-semibold text-foreground">
            Subagents
          </span>
          {agents.length > 0 && (
            <span className="rounded-full bg-muted px-1.5 py-0.5 text-caption text-muted-foreground">
              {agents.length}
            </span>
          )}
        </div>
        <button
          className="rounded p-0.5 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          onClick={onClose}
        >
          <X className="size-3.5" />
        </button>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto">
        {agents.length === 0 ? (
          <EmptyState />
        ) : (
          <div className="p-2">
            {/* Running section */}
            {running.length > 0 && (
              <div className="mb-3">
                <div className="mb-1.5 px-1 text-caption font-semibold uppercase tracking-wider text-muted-foreground">
                  Active ({running.length})
                </div>
                <div className="space-y-1">
                  {running.map((agent) => (
                    <AgentCard key={agent.id} agent={agent} />
                  ))}
                </div>
              </div>
            )}

            {/* Completed section */}
            {completed.length > 0 && (
              <div>
                <div className="mb-1.5 px-1 text-caption font-semibold uppercase tracking-wider text-muted-foreground">
                  Completed ({completed.length})
                </div>
                <div className="space-y-1">
                  {completed.map((agent) => (
                    <AgentCard key={agent.id} agent={agent} />
                  ))}
                </div>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

/* ─── Agent Card ───────────────────────────────────────────────── */

function AgentCard({ agent }: { agent: SubagentInfo }) {
  const [expanded, setExpanded] = useState(false);
  const meta = AGENT_TYPE_META[agent.type] ?? AGENT_TYPE_META["general-purpose"];
  const Icon = meta.icon;

  return (
    <div className="rounded-lg border border-border/40 bg-muted/10 transition-colors hover:bg-muted/20">
      <button
        className="flex w-full items-center gap-2 px-2.5 py-2 text-left"
        onClick={() => setExpanded(!expanded)}
      >
        {expanded ? (
          <ChevronDown className="size-3 shrink-0 text-muted-foreground" />
        ) : (
          <ChevronRight className="size-3 shrink-0 text-muted-foreground" />
        )}
        <Icon className="size-3.5 shrink-0" style={{ color: meta.color }} />
        <div className="min-w-0 flex-1">
          <div className="truncate text-label font-medium text-foreground">
            {agent.description}
          </div>
          <div className="flex items-center gap-1.5 text-caption text-muted-foreground">
            <span>{meta.label}</span>
            {agent.model && (
              <>
                <span className="opacity-40">·</span>
                <span>{agent.model}</span>
              </>
            )}
            {agent.background && (
              <>
                <span className="opacity-40">·</span>
                <span>BG</span>
              </>
            )}
          </div>
        </div>
        <StatusBadge status={agent.status} />
      </button>

      {expanded && agent.resultPreview && (
        <div className="max-h-[300px] overflow-auto border-t border-border/30 px-2.5 py-2">
          <pre className="whitespace-pre-wrap font-mono text-caption leading-relaxed text-muted-foreground">
            {agent.resultPreview}
          </pre>
        </div>
      )}

      {expanded && !agent.resultPreview && agent.status === "running" && (
        <div className="border-t border-border/30 px-2.5 py-2">
          <div className="flex items-center gap-2 text-caption text-muted-foreground">
            <Loader2 className="size-3 animate-spin" />
            <span>Agent is working...</span>
          </div>
        </div>
      )}
    </div>
  );
}

/* ─── Status Badge ─────────────────────────────────────────────── */

function StatusBadge({ status }: { status: SubagentStatus }) {
  switch (status) {
    case "running":
      return (
        <div className="flex items-center gap-1 rounded-full bg-[color:var(--agent-purple,rgb(147,51,234))]/10 px-1.5 py-0.5">
          <Loader2
            className="size-2.5 animate-spin"
            style={{ color: "var(--agent-purple)" }}
          />
          <span
            className="text-nano font-medium"
            style={{ color: "var(--agent-purple)" }}
          >
            Running
          </span>
        </div>
      );
    case "completed":
      return (
        <div className="flex items-center gap-1 rounded-full bg-[color:var(--color-success,rgb(44,122,57))]/10 px-1.5 py-0.5">
          <CheckCircle2
            className="size-2.5"
            style={{ color: "var(--color-success)" }}
          />
          <span
            className="text-nano font-medium"
            style={{ color: "var(--color-success)" }}
          >
            Done
          </span>
        </div>
      );
    case "error":
      return (
        <div className="flex items-center gap-1 rounded-full bg-[color:var(--color-error,rgb(171,43,63))]/10 px-1.5 py-0.5">
          <AlertCircle
            className="size-2.5"
            style={{ color: "var(--color-error)" }}
          />
          <span
            className="text-nano font-medium"
            style={{ color: "var(--color-error)" }}
          >
            Error
          </span>
        </div>
      );
  }
}

/* ─── Empty State ──────────────────────────────────────────────── */

function EmptyState() {
  return (
    <div className="flex flex-col items-center justify-center gap-2 p-6 text-center">
      <Brain className="size-8 text-muted-foreground/30" />
      <div>
        <div className="text-body-sm font-medium text-muted-foreground">
          No subagents
        </div>
        <div className="mt-0.5 text-label text-muted-foreground/60">
          Subagents appear here when the AI spawns parallel workers for complex
          tasks.
        </div>
      </div>
    </div>
  );
}

import type { RawEntry } from "@/api/wiki/types";

export type RawEntropyStatusKey =
  | "retained"
  | "observing"
  | "duplicate_candidate"
  | "crystallizable"
  | "safe_archive";

export interface RawEntropyStatusMeta {
  label: string;
  description: string;
  tone: "success" | "muted" | "warning" | "primary" | "archive";
  reversible: boolean;
}

export interface RawEntropyStatus {
  key: RawEntropyStatusKey;
  meta: RawEntropyStatusMeta;
}

export const RAW_ENTROPY_STATUS_META: Record<
  RawEntropyStatusKey,
  RawEntropyStatusMeta
> = {
  retained: {
    label: "已留存",
    description: "这条素材已被复用或进入知识结构，保留来源即可。",
    tone: "success",
    reversible: true,
  },
  observing: {
    label: "观察中",
    description: "目前证据不足，先保留来源，等待后续共性或复用信号。",
    tone: "muted",
    reversible: true,
  },
  duplicate_candidate: {
    label: "疑似重复",
    description: "内容与既有 raw 高度相同，后续整理时优先合并或降噪。",
    tone: "warning",
    reversible: true,
  },
  crystallizable: {
    label: "可结晶",
    description: "已有待整理任务或新鲜信号，可以进入 Inbox 判断是否成页。",
    tone: "primary",
    reversible: true,
  },
  safe_archive: {
    label: "可冷却",
    description: "此前被拒、静默复用或信号较弱，可降权观察。",
    tone: "archive",
    reversible: true,
  },
};

type RawDecisionKind =
  | "created_new"
  | "reused_with_pending_inbox"
  | "reused_approved"
  | "reused_after_reject"
  | "reused_silent"
  | "explicit_reingest"
  | "refreshed_content"
  | "content_duplicate";

function decisionKind(entry: RawEntry): RawDecisionKind | null {
  const decision = entry.last_ingest_decision;
  if (!decision || typeof decision !== "object" || !("kind" in decision)) {
    return null;
  }
  const kind = String((decision as { kind?: unknown }).kind);
  if (
    kind === "created_new" ||
    kind === "reused_with_pending_inbox" ||
    kind === "reused_approved" ||
    kind === "reused_after_reject" ||
    kind === "reused_silent" ||
    kind === "explicit_reingest" ||
    kind === "refreshed_content" ||
    kind === "content_duplicate"
  ) {
    return kind;
  }
  return null;
}

export function deriveRawEntropyStatus(
  entry: RawEntry,
  options: { pendingRawIds?: Set<number> } = {},
): RawEntropyStatus {
  const kind = decisionKind(entry);
  let key: RawEntropyStatusKey = "observing";

  if (kind === "content_duplicate") {
    key = "duplicate_candidate";
  } else if (kind === "reused_approved") {
    key = "retained";
  } else if (kind === "reused_after_reject" || kind === "reused_silent") {
    key = "safe_archive";
  } else if (
    options.pendingRawIds?.has(entry.id) ||
    kind === "created_new" ||
    kind === "reused_with_pending_inbox" ||
    kind === "explicit_reingest" ||
    kind === "refreshed_content"
  ) {
    key = "crystallizable";
  }

  return {
    key,
    meta: RAW_ENTROPY_STATUS_META[key],
  };
}

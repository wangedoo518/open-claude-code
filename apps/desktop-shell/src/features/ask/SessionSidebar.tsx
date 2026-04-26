/**
 * SessionSidebar - Ask conversation history column.
 *
 * The sidebar is intentionally denser than the main workspace: search,
 * a single primary "new conversation" CTA, grouped history, and a quiet
 * bottom utility bar. The data contract still comes from listSessions().
 */

import { useState, useCallback, useRef, useEffect, useMemo } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  Check,
  Loader2,
  PanelLeftClose,
  Pencil,
  Plus,
  Search,
  Sparkles,
  Trash2,
  X,
} from "lucide-react";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import {
  listSessions,
  deleteSession,
  deleteSessions,
  renameSession,
  cleanupEmptySessions,
} from "./api/client";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import type { DesktopSessionSummary } from "@/lib/tauri";

const sessionListKeys = {
  all: ["clawwiki", "sessions", "list"] as const,
};

type SessionGroupKey = "today" | "yesterday" | "week" | "month" | "older";

const GROUP_LABELS: Record<SessionGroupKey, string> = {
  today: "今天",
  yesterday: "昨天",
  week: "7 天内",
  month: "30 天内",
  older: "更早",
};

interface SessionSidebarProps {
  activeSessionId: string | null;
  onSelectSession: (id: string) => void;
  onNewSession: () => void;
  /** @deprecated Shell sidebar now handles collapse. Ignored. */
  collapsed?: boolean;
  /** Collapse the Ask history column while keeping a small restore handle. */
  onToggleCollapse?: () => void;
}

export function SessionSidebar({
  activeSessionId,
  onSelectSession,
  onNewSession,
  onToggleCollapse,
}: SessionSidebarProps) {
  const queryClient = useQueryClient();
  const [query, setQuery] = useState("");

  const listQuery = useQuery({
    queryKey: sessionListKeys.all,
    queryFn: () => listSessions(),
    staleTime: 10_000,
    refetchInterval: 30_000,
  });

  const sessions = listQuery.data?.sessions ?? [];
  const filteredSessions = useMemo(
    () => filterSessions(sessions, query),
    [sessions, query],
  );
  const groupedSessions = useMemo(
    () => groupSessions(filteredSessions),
    [filteredSessions],
  );

  const [cleanupConfirmOpen, setCleanupConfirmOpen] = useState(false);
  const cleanupMut = useMutation({
    mutationFn: () => cleanupEmptySessions(activeSessionId),
    onSuccess: (result) => {
      void queryClient.invalidateQueries({ queryKey: sessionListKeys.all });
      const count = result.deleted_count;
      if (count === 0) {
        toast.info("没有可清理的空会话");
      } else {
        toast.success(`已清理 ${count} 条空会话`);
      }
    },
    onError: (err) => {
      toast.error(
        `清理失败：${err instanceof Error ? err.message : String(err)}`,
      );
    },
  });

  const [selectionMode, setSelectionMode] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(() => new Set());
  const [bulkDeleteConfirmOpen, setBulkDeleteConfirmOpen] = useState(false);

  const exitSelectionMode = useCallback(() => {
    setSelectionMode(false);
    setSelectedIds(new Set());
    setBulkDeleteConfirmOpen(false);
  }, []);

  const toggleSessionSelection = useCallback((id: string) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }, []);

  const selectAllSessions = useCallback(() => {
    setSelectedIds(new Set(filteredSessions.map((session) => session.id)));
  }, [filteredSessions]);

  const clearSelection = useCallback(() => {
    setSelectedIds(new Set());
  }, []);

  useEffect(() => {
    const liveIds = new Set(sessions.map((session) => session.id));
    setSelectedIds((prev) => {
      let changed = false;
      const next = new Set<string>();
      for (const id of prev) {
        if (liveIds.has(id)) {
          next.add(id);
        } else {
          changed = true;
        }
      }
      return changed ? next : prev;
    });
  }, [sessions]);

  const bulkDeleteMut = useMutation({
    mutationFn: (ids: string[]) => deleteSessions(ids),
    onSuccess: (result) => {
      if (result.deleted_count > 0) {
        void queryClient.invalidateQueries({ queryKey: sessionListKeys.all });
      }

      if (activeSessionId && result.deleted_ids.includes(activeSessionId)) {
        onNewSession();
      }

      if (result.failed_count === 0) {
        toast.success(`已删除 ${result.deleted_count} 条对话`);
        exitSelectionMode();
        return;
      }

      setSelectedIds(new Set(result.failed.map((item) => item.id)));
      if (result.deleted_count > 0) {
        toast.warning(
          `已删除 ${result.deleted_count} 条，${result.failed_count} 条删除失败`,
        );
      } else {
        toast.error(
          `批量删除失败：${result.failed[0]?.error ?? "未知错误"}`,
        );
      }
    },
    onError: (err) => {
      toast.error(
        `批量删除失败：${err instanceof Error ? err.message : String(err)}`,
      );
    },
  });

  const selectedCount = selectedIds.size;
  const allSelected =
    filteredSessions.length > 0 && selectedCount === filteredSessions.length;

  return (
    <div className="ask-history-sidebar flex h-full w-full flex-col">
      <div className="ask-history-titlebar flex items-center justify-between">
        <span className="ask-history-title">对话历史</span>
        {onToggleCollapse && (
          <button
            type="button"
            onClick={onToggleCollapse}
            className="ask-history-icon-button"
            title="收起对话历史"
            aria-label="收起对话历史"
          >
            <PanelLeftClose className="size-3.5" />
          </button>
        )}
      </div>

      <div className="ask-history-search-wrap">
        <label className="ask-history-search">
          <Search className="size-3.5 shrink-0" aria-hidden="true" />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="搜索对话…"
            className="min-w-0 flex-1 bg-transparent text-[12px] outline-none placeholder:text-[#888780]"
          />
          <kbd>⌘K</kbd>
        </label>
      </div>

      <div className="ask-history-new-wrap">
        <button type="button" className="ask-history-new" onClick={onNewSession}>
          <span className="inline-flex items-center gap-1.5">
            <Plus className="size-3.5" />
            新对话
          </span>
          <kbd>⌘N</kbd>
        </button>
      </div>

      {selectionMode && (
        <div className="ask-history-selection">
          <div className="flex items-center justify-between gap-2">
            <span className="min-w-0 truncate text-[11px] text-[#5F5E5A]">
              {selectedCount > 0
                ? `已选择 ${selectedCount} / ${filteredSessions.length} 条`
                : "选择要删除的对话"}
            </span>
            <button
              type="button"
              onClick={allSelected ? clearSelection : selectAllSessions}
              className="shrink-0 rounded px-1.5 py-0.5 text-[10.5px] text-[#5F5E5A] hover:bg-[rgba(44,44,42,0.06)]"
            >
              {allSelected ? "取消全选" : "全选"}
            </button>
          </div>
          <div className="mt-2 flex items-center gap-1.5">
            <button
              type="button"
              onClick={() => setBulkDeleteConfirmOpen(true)}
              disabled={selectedCount === 0 || bulkDeleteMut.isPending}
              className="inline-flex flex-1 items-center justify-center gap-1 rounded-md bg-[#C44545] px-2 py-1.5 text-[11px] font-medium text-white disabled:cursor-not-allowed disabled:opacity-40"
            >
              {bulkDeleteMut.isPending ? (
                <Loader2 className="size-3 animate-spin" />
              ) : (
                <Trash2 className="size-3" />
              )}
              删除所选
            </button>
            <button
              type="button"
              onClick={exitSelectionMode}
              disabled={bulkDeleteMut.isPending}
              className="rounded-md border border-[rgba(44,44,42,0.12)] px-2 py-1.5 text-[11px] text-[#5F5E5A] hover:bg-[rgba(44,44,42,0.04)] disabled:opacity-40"
            >
              退出
            </button>
          </div>
        </div>
      )}

      <ConfirmDialog
        open={cleanupConfirmOpen}
        onOpenChange={setCleanupConfirmOpen}
        title="清理空会话？"
        description="清理所有没有发过消息的空会话。当前正在使用的对话不会被删除。"
        confirmLabel="清理"
        onConfirm={() => {
          cleanupMut.mutate();
          setCleanupConfirmOpen(false);
        }}
      />

      <ConfirmDialog
        open={bulkDeleteConfirmOpen}
        onOpenChange={setBulkDeleteConfirmOpen}
        title="删除所选对话？"
        description={`将删除 ${selectedCount} 条对话。删除后无法恢复。`}
        confirmLabel="删除"
        variant="destructive"
        onConfirm={() => {
          const ids = Array.from(selectedIds);
          if (ids.length === 0) return;
          bulkDeleteMut.mutate(ids);
        }}
      />

      <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
        {listQuery.isLoading ? (
          <div className="flex items-center gap-2 px-2 py-4 text-[11px] text-[#888780]">
            <Loader2 className="size-3 animate-spin" />
            加载中…
          </div>
        ) : sessions.length === 0 ? (
          <div className="px-2 py-4 text-[11px] text-[#888780]">
            暂无对话记录
          </div>
        ) : filteredSessions.length === 0 ? (
          <div className="px-2 py-4 text-[11px] text-[#888780]">
            没有匹配的对话
          </div>
        ) : (
          (Object.keys(GROUP_LABELS) as SessionGroupKey[]).map((key) => {
            const group = groupedSessions[key];
            if (group.length === 0) return null;
            return (
              <SessionGroup
                key={key}
                label={GROUP_LABELS[key]}
                sessions={group}
                activeId={activeSessionId}
                onSelect={onSelectSession}
                queryClient={queryClient}
                selectionMode={selectionMode}
                selectedIds={selectedIds}
                onToggleSelected={toggleSessionSelection}
              />
            );
          })
        )}
      </div>

      <div className="ask-history-footer">
        <button
          type="button"
          className="ask-history-cleanup"
          onClick={() => setCleanupConfirmOpen(true)}
          disabled={cleanupMut.isPending}
        >
          {cleanupMut.isPending ? (
            <Loader2 className="size-3 animate-spin" />
          ) : (
            <Sparkles className="size-3" />
          )}
          AI 整理对话
        </button>
        <button
          type="button"
          className={cn(
            "ask-history-icon-button",
            selectionMode && "bg-[rgba(216,90,48,0.08)] text-[#D85A30]",
          )}
          onClick={() => {
            if (selectionMode && selectedCount > 0) {
              setBulkDeleteConfirmOpen(true);
            } else {
              setSelectionMode((value) => !value);
            }
          }}
          disabled={sessions.length === 0 || bulkDeleteMut.isPending}
          title={selectionMode ? "删除所选对话" : "批量删除对话"}
          aria-label={selectionMode ? "删除所选对话" : "批量删除对话"}
        >
          <Trash2 className="size-3.5" />
        </button>
      </div>
    </div>
  );
}

function SessionGroup({
  label,
  sessions,
  activeId,
  onSelect,
  queryClient,
  selectionMode,
  selectedIds,
  onToggleSelected,
}: {
  label: string;
  sessions: DesktopSessionSummary[];
  activeId: string | null;
  onSelect: (id: string) => void;
  queryClient: ReturnType<typeof useQueryClient>;
  selectionMode: boolean;
  selectedIds: Set<string>;
  onToggleSelected: (id: string) => void;
}) {
  return (
    <section className="ask-history-group">
      <div className="ask-history-group-label">
        <span>{label}</span>
        <span className="ask-history-group-count">{sessions.length}</span>
      </div>
      <div className="space-y-px">
        {sessions.map((session) => (
          <SessionItem
            key={session.id}
            session={session}
            isActive={session.id === activeId}
            isSelected={selectedIds.has(session.id)}
            onSelect={() => onSelect(session.id)}
            onToggleSelected={() => onToggleSelected(session.id)}
            queryClient={queryClient}
            selectionMode={selectionMode}
          />
        ))}
      </div>
    </section>
  );
}

function SessionItem({
  session,
  isActive,
  isSelected,
  onSelect,
  onToggleSelected,
  queryClient,
  selectionMode,
}: {
  session: DesktopSessionSummary;
  isActive: boolean;
  isSelected: boolean;
  onSelect: () => void;
  onToggleSelected: () => void;
  queryClient: ReturnType<typeof useQueryClient>;
  selectionMode: boolean;
}) {
  const [isEditing, setIsEditing] = useState(false);
  const [editValue, setEditValue] = useState(session.title);
  const [confirmDeleteOpen, setConfirmDeleteOpen] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const model = getSessionModel(session);

  const renameMut = useMutation({
    mutationFn: (title: string) => renameSession(session.id, title),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: sessionListKeys.all });
      setIsEditing(false);
    },
  });

  const deleteMut = useMutation({
    mutationFn: () => deleteSession(session.id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: sessionListKeys.all });
      toast.success("会话已删除");
    },
    onError: (err) => {
      toast.error(
        `删除失败：${err instanceof Error ? err.message : String(err)}`,
      );
    },
  });

  const handleRename = useCallback(() => {
    const trimmed = editValue.trim();
    if (trimmed && trimmed !== session.title) {
      renameMut.mutate(trimmed);
    } else {
      setIsEditing(false);
    }
  }, [editValue, session.title, renameMut]);

  useEffect(() => {
    setEditValue(session.title);
  }, [session.title]);

  useEffect(() => {
    if (isEditing && inputRef.current) {
      inputRef.current.focus();
      inputRef.current.select();
    }
  }, [isEditing]);

  if (isEditing) {
    return (
      <div className="ask-history-edit">
        <input
          ref={inputRef}
          value={editValue}
          onChange={(event) => setEditValue(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") handleRename();
            if (event.key === "Escape") setIsEditing(false);
          }}
          className="min-w-0 flex-1 rounded-md border border-[rgba(44,44,42,0.15)] bg-white px-2 py-1 text-[12px] text-[#2C2C2A] outline-none focus:border-[#D85A30]"
        />
        <button
          onClick={handleRename}
          className="rounded p-1 text-[#888780] hover:bg-[rgba(44,44,42,0.05)] hover:text-[#2C2C2A]"
          aria-label="保存名称"
        >
          <Check className="size-3" />
        </button>
        <button
          onClick={() => setIsEditing(false)}
          className="rounded p-1 text-[#888780] hover:bg-[rgba(44,44,42,0.05)] hover:text-[#2C2C2A]"
          aria-label="取消重命名"
        >
          <X className="size-3" />
        </button>
      </div>
    );
  }

  return (
    <div
      className={cn(
        "ask-history-item group",
        isActive && "ask-history-item--active",
        selectionMode && isSelected && "ask-history-item--selected",
      )}
      data-active={isActive || undefined}
      data-selected={isSelected || undefined}
      onClick={selectionMode ? onToggleSelected : onSelect}
      onDoubleClick={(event) => {
        if (selectionMode) return;
        event.stopPropagation();
        setIsEditing(true);
      }}
    >
      {selectionMode && (
        <input
          type="checkbox"
          checked={isSelected}
          onChange={(event) => {
            event.stopPropagation();
            onToggleSelected();
          }}
          onClick={(event) => event.stopPropagation()}
          className="mt-0.5 size-3 shrink-0 accent-[#D85A30]"
          aria-label={`选择对话 ${session.title || "新对话"}`}
        />
      )}

      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span
            className="min-w-0 flex-1 truncate text-[12.5px] font-medium text-[#2C2C2A]"
            title={session.title || "新对话"}
          >
            {session.title || "新对话"}
          </span>
          <span className="ask-history-turn-badge">
            {getTurnLabel(session)}
          </span>
        </div>
        <div className="mt-1 flex min-w-0 items-center gap-1.5 text-[10.5px] text-[#888780]">
          <span
            className="size-1.5 shrink-0 rounded-full"
            style={{ backgroundColor: model.isOnline ? "#1D9E75" : "#B7B2A8" }}
            aria-hidden="true"
          />
          <span className="min-w-0 truncate">
            {model.label} · {formatSessionTime(session.updated_at)}
          </span>
          {session.turn_state === "running" && (
            <span className="shrink-0 text-[#D85A30]">生成中</span>
          )}
        </div>
      </div>

      {!selectionMode && (
        <div className="ask-history-row-actions">
          <button
            onClick={(event) => {
              event.stopPropagation();
              setEditValue(session.title);
              setIsEditing(true);
            }}
            className="ask-history-row-action"
            title="重命名"
            aria-label="重命名"
          >
            <Pencil className="size-3" />
          </button>
          <button
            onClick={(event) => {
              event.stopPropagation();
              setConfirmDeleteOpen(true);
            }}
            className="ask-history-row-action hover:text-[#C44545]"
            title="删除"
            aria-label="删除"
          >
            <Trash2 className="size-3" />
          </button>
        </div>
      )}

      <ConfirmDialog
        open={confirmDeleteOpen}
        onOpenChange={setConfirmDeleteOpen}
        title="删除这条对话？"
        description={`删除后无法恢复。会话标题：${session.title || "新对话"}`}
        confirmLabel="删除"
        variant="destructive"
        onConfirm={() => {
          deleteMut.mutate();
          setConfirmDeleteOpen(false);
        }}
      />
    </div>
  );
}

function filterSessions(
  sessions: DesktopSessionSummary[],
  query: string,
): DesktopSessionSummary[] {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return sessions;

  return sessions.filter((session) => {
    const haystack = [
      session.title,
      session.preview,
      session.model_label,
      session.environment_label,
      session.project_name,
    ]
      .join(" ")
      .toLowerCase();
    return haystack.includes(normalized);
  });
}

function groupSessions(
  sessions: DesktopSessionSummary[],
): Record<SessionGroupKey, DesktopSessionSummary[]> {
  const now = new Date();
  const todayStart = startOfDay(now).getTime();
  const yesterdayStart = todayStart - 24 * 60 * 60 * 1000;
  const weekStart = todayStart - 7 * 24 * 60 * 60 * 1000;
  const monthStart = todayStart - 30 * 24 * 60 * 60 * 1000;

  const groups: Record<SessionGroupKey, DesktopSessionSummary[]> = {
    today: [],
    yesterday: [],
    week: [],
    month: [],
    older: [],
  };

  for (const session of sessions) {
    const updatedAt = normalizeTimestamp(session.updated_at);
    if (updatedAt >= todayStart) {
      groups.today.push(session);
    } else if (updatedAt >= yesterdayStart) {
      groups.yesterday.push(session);
    } else if (updatedAt >= weekStart) {
      groups.week.push(session);
    } else if (updatedAt >= monthStart) {
      groups.month.push(session);
    } else {
      groups.older.push(session);
    }
  }

  return groups;
}

function startOfDay(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

function normalizeTimestamp(value: number): number {
  return value < 1_000_000_000_000 ? value * 1000 : value;
}

function formatSessionTime(value: number): string {
  const ms = normalizeTimestamp(value);
  const date = new Date(ms);
  const now = new Date();
  const todayStart = startOfDay(now).getTime();
  const yesterdayStart = todayStart - 24 * 60 * 60 * 1000;
  const weekStart = todayStart - 7 * 24 * 60 * 60 * 1000;

  const clock = new Intl.DateTimeFormat("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  }).format(date);

  if (ms >= todayStart) return clock;
  if (ms >= yesterdayStart) return `昨天 ${clock}`;
  if (ms >= weekStart) {
    return new Intl.DateTimeFormat("zh-CN", { weekday: "short" }).format(date);
  }

  return new Intl.DateTimeFormat("zh-CN", {
    month: "numeric",
    day: "numeric",
  }).format(date);
}

function getSessionModel(session: DesktopSessionSummary): {
  label: string;
  isOnline: boolean;
} {
  const raw = session.model_label || session.environment_label || "Local";
  if (/deepseek/i.test(raw)) return { label: "DeepSeek", isOnline: true };
  if (/local|本地/i.test(raw)) return { label: "Local", isOnline: false };
  return { label: raw.replace(/\s+chat$/i, ""), isOnline: true };
}

function getTurnLabel(session: DesktopSessionSummary): string {
  const extended = session as DesktopSessionSummary & {
    turn_count?: number;
    message_count?: number;
    messages_count?: number;
  };
  const count =
    extended.turn_count ?? extended.message_count ?? extended.messages_count;
  if (typeof count === "number" && Number.isFinite(count) && count > 0) {
    return `${Math.max(1, Math.ceil(count / 2))} 轮`;
  }
  if (session.turn_state === "running") return "进行中";
  return "1 轮";
}

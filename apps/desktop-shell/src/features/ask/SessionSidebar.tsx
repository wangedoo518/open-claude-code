/**
 * SessionSidebar — 会话历史列表，类似 Claude Code Desktop 左侧面板。
 * 显示今天/昨天/更早的分组，支持新建、切换、删除、重命名。
 */

import { useState, useCallback, useRef, useEffect } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  Plus,
  Trash2,
  Pencil,
  Check,
  X,
  MessageSquare,
  Loader2,
  Sparkles,
  PanelLeftClose,
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

  const listQuery = useQuery({
    queryKey: sessionListKeys.all,
    queryFn: () => listSessions(),
    staleTime: 10_000,
    refetchInterval: 30_000,
  });

  const sessions = listQuery.data?.sessions ?? [];

  // One-click cleanup for leftover empty "new conversation" sessions.
  // Preserves the session the user currently has active (if any).
  //
  // A5-Polish: replaced `window.alert` feedback with `sonner` toasts
  // and `window.confirm` with the R1 `ConfirmDialog` primitive so the
  // interaction matches the rest of the product instead of looking
  // like a native OS alert.
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
    setSelectedIds(new Set(sessions.map((session) => session.id)));
  }, [sessions]);

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
        toast.error(`批量删除失败：${result.failed[0]?.error ?? "未知错误"}`);
      }
    },
    onError: (err) => {
      toast.error(
        `批量删除失败：${err instanceof Error ? err.message : String(err)}`,
      );
    },
  });

  // Group by bucket
  const today = sessions.filter((s) => s.bucket === "today");
  const yesterday = sessions.filter((s) => s.bucket === "yesterday");
  const older = sessions.filter((s) => s.bucket === "older");
  const selectedCount = selectedIds.size;
  const allSelected = sessions.length > 0 && selectedCount === sessions.length;

  return (
    <div className="flex h-full w-full flex-col">
      {/* Header + collapse + new button */}
      <div className="flex items-center justify-between border-b border-border/50 px-3 py-2.5">
        <span className="text-[12px] font-semibold text-sidebar-foreground">
          对话历史
        </span>
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={onNewSession}
            className="rounded-md p-1 text-sidebar-foreground transition-colors hover:bg-sidebar-accent"
            title="新建对话"
          >
            <Plus className="size-3.5" />
          </button>
          {onToggleCollapse && (
            <button
              type="button"
              onClick={onToggleCollapse}
              className="rounded-md p-1 text-sidebar-foreground transition-colors hover:bg-sidebar-accent"
              title="收起对话历史"
              aria-label="收起对话历史"
            >
              <PanelLeftClose className="size-3.5" />
            </button>
          )}
          <button
            type="button"
            onClick={selectionMode ? exitSelectionMode : () => setSelectionMode(true)}
            disabled={sessions.length === 0 || bulkDeleteMut.isPending}
            className="rounded-md p-1 text-sidebar-foreground transition-colors hover:bg-sidebar-accent disabled:opacity-40"
            title={selectionMode ? "退出批量选择" : "批量删除对话"}
          >
            {selectionMode ? (
              <X className="size-3.5" />
            ) : (
              <Trash2 className="size-3.5" />
            )}
          </button>
          <button
            type="button"
            onClick={() => setCleanupConfirmOpen(true)}
            disabled={cleanupMut.isPending}
            className="rounded-md p-1 text-sidebar-foreground transition-colors hover:bg-sidebar-accent disabled:opacity-40"
            title="清理空会话（保留当前会话）"
          >
            {cleanupMut.isPending ? (
              <Loader2 className="size-3.5 animate-spin" />
            ) : (
              <Sparkles className="size-3.5" />
            )}
          </button>
          {/* Collapse is now handled by the shell sidebar */}
        </div>
      </div>

      {selectionMode && (
        <div className="border-b border-border/50 px-3 py-2">
          <div className="flex items-center justify-between gap-2">
            <span className="min-w-0 truncate text-[11px] text-muted-foreground">
              {selectedCount > 0
                ? `已选择 ${selectedCount} / ${sessions.length} 条`
                : "选择要删除的对话"}
            </span>
            <button
              type="button"
              onClick={allSelected ? clearSelection : selectAllSessions}
              className="shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium text-sidebar-foreground hover:bg-sidebar-accent"
            >
              {allSelected ? "取消全选" : "全选"}
            </button>
          </div>
          <div className="mt-2 flex items-center gap-1.5">
            <button
              type="button"
              onClick={() => setBulkDeleteConfirmOpen(true)}
              disabled={selectedCount === 0 || bulkDeleteMut.isPending}
              className="inline-flex flex-1 items-center justify-center gap-1 rounded-md px-2 py-1 text-[11px] font-medium text-white disabled:cursor-not-allowed disabled:opacity-40"
              style={{ backgroundColor: "var(--color-error)" }}
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
              className="rounded-md border border-border px-2 py-1 text-[11px] text-sidebar-foreground hover:bg-sidebar-accent disabled:opacity-40"
            >
              退出
            </button>
          </div>
        </div>
      )}

      <ConfirmDialog
        open={cleanupConfirmOpen}
        onOpenChange={setCleanupConfirmOpen}
        title="清理空会话"
        description="清理所有没有发送过消息的空会话。当前正在使用的对话不会被删除。"
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

      {/* Session list */}
      <div className="min-h-0 flex-1 overflow-y-auto">
        {listQuery.isLoading ? (
          <div className="flex items-center gap-2 px-3 py-4 text-[11px] text-muted-foreground">
            <Loader2 className="size-3 animate-spin" />
            加载中…
          </div>
        ) : sessions.length === 0 ? (
          <div className="px-3 py-4 text-[11px] text-muted-foreground">
            暂无对话记录
          </div>
        ) : (
          <>
            {today.length > 0 && (
              <SessionGroup
                label="今天"
                sessions={today}
                activeId={activeSessionId}
                onSelect={onSelectSession}
                queryClient={queryClient}
                selectionMode={selectionMode}
                selectedIds={selectedIds}
                onToggleSelected={toggleSessionSelection}
              />
            )}
            {yesterday.length > 0 && (
              <SessionGroup
                label="昨天"
                sessions={yesterday}
                activeId={activeSessionId}
                onSelect={onSelectSession}
                queryClient={queryClient}
                selectionMode={selectionMode}
                selectedIds={selectedIds}
                onToggleSelected={toggleSessionSelection}
              />
            )}
            {older.length > 0 && (
              <SessionGroup
                label="更早"
                sessions={older}
                activeId={activeSessionId}
                onSelect={onSelectSession}
                queryClient={queryClient}
                selectionMode={selectionMode}
                selectedIds={selectedIds}
                onToggleSelected={toggleSessionSelection}
              />
            )}
          </>
        )}
      </div>
    </div>
  );
}

/* ─── Session group ────────────────────────────────────────────── */

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
    <div className="py-1">
      <div className="px-3 py-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground/60">
        {label}
      </div>
      {sessions.map((s) => (
        <SessionItem
          key={s.id}
          session={s}
          isActive={s.id === activeId}
          isSelected={selectedIds.has(s.id)}
          onSelect={() => onSelect(s.id)}
          onToggleSelected={() => onToggleSelected(s.id)}
          queryClient={queryClient}
          selectionMode={selectionMode}
        />
      ))}
    </div>
  );
}

/* ─── Single session item ──────────────────────────────────────── */

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
  const [showActions, setShowActions] = useState(false);
  // A5-Polish — single-click delete on a hover-revealed icon is a
  // footgun. Gate it behind the R1 ConfirmDialog so an accidental
  // hover-click on the trash doesn't silently wipe a session.
  const [confirmDeleteOpen, setConfirmDeleteOpen] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

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
    if (isEditing && inputRef.current) {
      inputRef.current.focus();
      inputRef.current.select();
    }
  }, [isEditing]);

  if (isEditing) {
    return (
      <div className="flex items-center gap-1 px-2 py-1">
        <input
          ref={inputRef}
          value={editValue}
          onChange={(e) => setEditValue(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") handleRename();
            if (e.key === "Escape") setIsEditing(false);
          }}
          className="min-w-0 flex-1 rounded border border-border bg-background px-1.5 py-0.5 text-[11px] text-foreground outline-none focus:border-ring"
        />
        <button onClick={handleRename} className="p-0.5 text-muted-foreground hover:text-foreground">
          <Check className="size-3" />
        </button>
        <button onClick={() => setIsEditing(false)} className="p-0.5 text-muted-foreground hover:text-foreground">
          <X className="size-3" />
        </button>
      </div>
    );
  }

  return (
    <div
      className={cn(
        // Left 2px border kept always-present (transparent when inactive)
        // so active/inactive transitions don't shift the row horizontally.
        // Matches the DS terracotta-left-bar pattern used by InboxRow +
        // RawEntryCard; using 2px instead of 3px here because the session
        // sidebar is narrower and the 3px stripe competes with rounded-md.
        "group flex cursor-pointer items-center gap-2 rounded-md border-l-[2px] px-3 py-1.5 text-[11px] transition-colors",
        isActive
          ? "border-primary bg-sidebar-accent text-sidebar-accent-foreground"
          : "border-l-transparent text-sidebar-foreground hover:bg-sidebar-accent/50",
        selectionMode && isSelected && !isActive && "bg-sidebar-accent/70"
      )}
      onClick={selectionMode ? onToggleSelected : onSelect}
      onMouseEnter={() => setShowActions(true)}
      onMouseLeave={() => setShowActions(false)}
    >
      {selectionMode ? (
        <input
          type="checkbox"
          checked={isSelected}
          onChange={(e) => {
            e.stopPropagation();
            onToggleSelected();
          }}
          onClick={(e) => e.stopPropagation()}
          className="size-3 shrink-0 accent-primary"
          aria-label={`选择对话 ${session.title || "新对话"}`}
        />
      ) : (
        <MessageSquare className="size-3 shrink-0 opacity-40" />
      )}
      <span className="min-w-0 flex-1 truncate">{session.title || "新对话"}</span>

      {!selectionMode && showActions && (
        <div className="flex shrink-0 items-center gap-0.5">
          <button
            onClick={(e) => {
              e.stopPropagation();
              setEditValue(session.title);
              setIsEditing(true);
            }}
            className="rounded p-0.5 text-muted-foreground hover:text-foreground"
            title="重命名"
          >
            <Pencil className="size-2.5" />
          </button>
          <button
            onClick={(e) => {
              e.stopPropagation();
              setConfirmDeleteOpen(true);
            }}
            className="rounded p-0.5 text-muted-foreground hover:text-destructive"
            title="删除"
          >
            <Trash2 className="size-2.5" />
          </button>
        </div>
      )}

      <ConfirmDialog
        open={confirmDeleteOpen}
        onOpenChange={setConfirmDeleteOpen}
        title="删除这条对话？"
        description={`删除后无法恢复。会话标题：「${session.title || "新对话"}」`}
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

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
} from "lucide-react";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import {
  listSessions,
  deleteSession,
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
  /** @deprecated Shell sidebar now handles collapse. Ignored. */
  onToggleCollapse?: () => void;
}

export function SessionSidebar({
  activeSessionId,
  onSelectSession,
  onNewSession,
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

  // Group by bucket
  const today = sessions.filter((s) => s.bucket === "today");
  const yesterday = sessions.filter((s) => s.bucket === "yesterday");
  const older = sessions.filter((s) => s.bucket === "older");

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
              />
            )}
            {yesterday.length > 0 && (
              <SessionGroup
                label="昨天"
                sessions={yesterday}
                activeId={activeSessionId}
                onSelect={onSelectSession}
                queryClient={queryClient}
              />
            )}
            {older.length > 0 && (
              <SessionGroup
                label="更早"
                sessions={older}
                activeId={activeSessionId}
                onSelect={onSelectSession}
                queryClient={queryClient}
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
}: {
  label: string;
  sessions: DesktopSessionSummary[];
  activeId: string | null;
  onSelect: (id: string) => void;
  queryClient: ReturnType<typeof useQueryClient>;
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
          onSelect={() => onSelect(s.id)}
          queryClient={queryClient}
        />
      ))}
    </div>
  );
}

/* ─── Single session item ──────────────────────────────────────── */

function SessionItem({
  session,
  isActive,
  onSelect,
  queryClient,
}: {
  session: DesktopSessionSummary;
  isActive: boolean;
  onSelect: () => void;
  queryClient: ReturnType<typeof useQueryClient>;
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
          : "border-l-transparent text-sidebar-foreground hover:bg-sidebar-accent/50"
      )}
      onClick={onSelect}
      onMouseEnter={() => setShowActions(true)}
      onMouseLeave={() => setShowActions(false)}
    >
      <MessageSquare className="size-3 shrink-0 opacity-40" />
      <span className="min-w-0 flex-1 truncate">{session.title || "新对话"}</span>

      {showActions && (
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

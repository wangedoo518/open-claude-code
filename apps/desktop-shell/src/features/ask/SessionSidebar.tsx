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
  PanelLeftClose,
  PanelLeftOpen,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { listSessions, deleteSession, renameSession } from "./api/client";
import type { DesktopSessionSummary } from "@/lib/tauri";

const sessionListKeys = {
  all: ["clawwiki", "sessions", "list"] as const,
};

interface SessionSidebarProps {
  activeSessionId: string | null;
  onSelectSession: (id: string) => void;
  onNewSession: () => void;
  collapsed?: boolean;
  onToggleCollapse?: () => void;
}

export function SessionSidebar({
  activeSessionId,
  onSelectSession,
  onNewSession,
  collapsed = false,
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

  // Group by bucket
  const today = sessions.filter((s) => s.bucket === "today");
  const yesterday = sessions.filter((s) => s.bucket === "yesterday");
  const older = sessions.filter((s) => s.bucket === "older");

  // Collapsed state: show only a narrow strip with expand button
  if (collapsed) {
    return (
      <div className="flex h-full w-10 shrink-0 flex-col items-center border-r border-border/50 bg-sidebar-background pt-2.5">
        <button
          type="button"
          onClick={onToggleCollapse}
          className="rounded-md p-1.5 text-sidebar-foreground transition-colors hover:bg-sidebar-accent"
          title="展开对话历史"
        >
          <PanelLeftOpen className="size-4" />
        </button>
        <button
          type="button"
          onClick={onNewSession}
          className="mt-2 rounded-md p-1.5 text-sidebar-foreground transition-colors hover:bg-sidebar-accent"
          title="新建对话"
        >
          <Plus className="size-4" />
        </button>
      </div>
    );
  }

  return (
    <div className="flex h-full w-[220px] shrink-0 flex-col border-r border-border/50 bg-sidebar-background">
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
            onClick={onToggleCollapse}
            className="rounded-md p-1 text-sidebar-foreground transition-colors hover:bg-sidebar-accent"
            title="收起"
          >
            <PanelLeftClose className="size-3.5" />
          </button>
        </div>
      </div>

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
        "group flex cursor-pointer items-center gap-2 rounded-md px-3 py-1.5 text-[11px] transition-colors",
        isActive
          ? "bg-sidebar-accent text-sidebar-accent-foreground"
          : "text-sidebar-foreground hover:bg-sidebar-accent/50"
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
              deleteMut.mutate();
            }}
            className="rounded p-0.5 text-muted-foreground hover:text-destructive"
            title="删除"
          >
            <Trash2 className="size-2.5" />
          </button>
        </div>
      )}
    </div>
  );
}

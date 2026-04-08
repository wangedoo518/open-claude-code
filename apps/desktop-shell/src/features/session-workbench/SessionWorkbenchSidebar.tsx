import { useState, useRef, useEffect, useCallback } from "react";
import {
  Loader2,
  MessageSquare,
  PanelLeftClose,
  Plus,
  Search,
  Clock,
  Zap,
  FileText,
  FileJson,
  Trash2,
  Flag,
  CheckCircle2,
  CircleDashed,
  Eye,
  Archive,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { truncate, cn } from "@/lib/utils";
import type {
  DesktopLifecycleStatus,
  DesktopSessionSection,
  DesktopSessionSummary,
} from "@/lib/tauri";
import { useSettingsStore } from "@/state/settings-store";

interface SessionWorkbenchSidebarProps {
  sessionSections: DesktopSessionSection[];
  activeSessionId?: string | null;
  projectLabel: string;
  onSelectSession: (sessionId: string) => void;
  onCreateSession: () => void;
  onDeleteSession?: (sessionId: string) => void;
  onExportSession?: (sessionId: string, format: "markdown" | "json") => void;
  /** Inbox lifecycle status change. Backend wires the value to disk. */
  onSetSessionStatus?: (
    sessionId: string,
    status: DesktopLifecycleStatus,
  ) => void;
  /** Toggle the flagged bit on a session. */
  onToggleSessionFlag?: (sessionId: string, flagged: boolean) => void;
  isCreatingSession: boolean;
}

/** Visual config for each lifecycle status badge. */
const STATUS_META: Record<
  DesktopLifecycleStatus,
  { label: string; icon: typeof CircleDashed; color: string }
> = {
  todo: { label: "Todo", icon: CircleDashed, color: "var(--muted-foreground)" },
  in_progress: { label: "In Progress", icon: Zap, color: "var(--claude-orange)" },
  needs_review: { label: "Needs Review", icon: Eye, color: "var(--claude-blue)" },
  done: { label: "Done", icon: CheckCircle2, color: "var(--color-success)" },
  archived: { label: "Archived", icon: Archive, color: "var(--muted-foreground)" },
};

export function SessionWorkbenchSidebar({
  sessionSections,
  activeSessionId,
  projectLabel,
  onSelectSession,
  onCreateSession,
  onDeleteSession,
  onExportSession,
  onSetSessionStatus,
  onToggleSessionFlag,
  isCreatingSession,
}: SessionWorkbenchSidebarProps) {
  const setShowSessionSidebar = useSettingsStore(
    (state) => state.setShowSessionSidebar
  );
  const [searchQuery, setSearchQuery] = useState("");
  const [contextMenu, setContextMenu] = useState<{
    x: number;
    y: number;
    session: DesktopSessionSummary;
  } | null>(null);
  const [deleteConfirm, setDeleteConfirm] = useState<DesktopSessionSummary | null>(null);
  const contextMenuRef = useRef<HTMLDivElement>(null);

  // Close context menu on click outside.
  //
  // SG-06: Previously this effect re-registered on every `contextMenu`
  // change, which could race if the user rapidly right-clicked multiple
  // rows. We now:
  //  1. Keep `[contextMenu]` in the dep array so the effect only runs
  //     when the menu transitions from null → open (early return on null).
  //  2. Wrap `addEventListener` in `setTimeout(..., 0)` so the same
  //     mousedown event that opened the menu doesn't immediately close it.
  //  3. Use `capture: true` so we see the event before any child handler.
  useEffect(() => {
    if (!contextMenu) return;

    const handler = (e: MouseEvent) => {
      if (
        contextMenuRef.current &&
        !contextMenuRef.current.contains(e.target as Node)
      ) {
        setContextMenu(null);
      }
    };

    const timer = window.setTimeout(() => {
      document.addEventListener("mousedown", handler, { capture: true });
    }, 0);

    return () => {
      window.clearTimeout(timer);
      document.removeEventListener("mousedown", handler, { capture: true });
    };
  }, [contextMenu]);

  const handleContextMenu = useCallback(
    (e: React.MouseEvent, session: DesktopSessionSummary) => {
      e.preventDefault();
      setContextMenu({ x: e.clientX, y: e.clientY, session });
    },
    []
  );

  // Filter sessions by search query
  const filteredSections = sessionSections
    .map((section) => ({
      ...section,
      sessions: section.sessions.filter((s) => {
        if (!searchQuery) return true;
        const q = searchQuery.toLowerCase();
        return (
          s.title.toLowerCase().includes(q) ||
          s.preview.toLowerCase().includes(q)
        );
      }),
    }))
    .filter((section) => section.sessions.length > 0);

  const totalSessions = sessionSections.reduce(
    (sum, s) => sum + s.sessions.length,
    0
  );

  return (
    <div className="flex h-full w-[260px] shrink-0 flex-col border-r border-border bg-sidebar-background">
      {/* Header */}
      <div className="flex items-center justify-between px-3 py-2">
        <span className="text-body-sm font-medium text-sidebar-foreground">
          Sessions
          {totalSessions > 0 && (
            <span className="ml-1.5 text-caption text-muted-foreground">
              ({totalSessions})
            </span>
          )}
        </span>
        <div className="flex gap-0.5">
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                className="size-6"
                onClick={onCreateSession}
                disabled={isCreatingSession}
              >
                {isCreatingSession ? (
                  <Loader2 className="size-3.5 animate-spin" />
                ) : (
                  <Plus className="size-3.5" />
                )}
              </Button>
            </TooltipTrigger>
            <TooltipContent>New Session</TooltipContent>
          </Tooltip>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                className="size-6"
                onClick={() => setShowSessionSidebar(false)}
              >
                <PanelLeftClose className="size-3.5" />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Hide Sidebar</TooltipContent>
          </Tooltip>
        </div>
      </div>

      {/* Search */}
      {totalSessions > 3 && (
        <div className="px-2 pb-2">
          <div className="flex items-center gap-1.5 rounded-md border border-border/50 bg-muted/10 px-2 py-1">
            <Search className="size-3 text-muted-foreground" />
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search sessions..."
              className="w-full bg-transparent text-label text-foreground outline-none placeholder:text-muted-foreground"
            />
          </div>
        </div>
      )}

      {/* Session list */}
      <ScrollArea className="flex-1">
        <div className="px-1.5 pb-2">
          <div className="px-2 py-1 text-caption font-semibold uppercase tracking-wider text-muted-foreground">
            {projectLabel}
          </div>

          {filteredSections.map((section) => (
            <div key={section.id} className="mb-2">
              <div className="flex items-center gap-1.5 px-2 py-1">
                <Clock className="size-2.5 text-muted-foreground/50" />
                <span className="text-caption font-semibold uppercase tracking-wider text-muted-foreground">
                  {section.label}
                </span>
              </div>
              <div className="space-y-0.5">
                {section.sessions.map((session) => (
                  <button
                    key={session.id}
                    className={cn(
                      "group w-full rounded-md px-2 py-1.5 text-left transition-colors",
                      session.id === activeSessionId
                        ? "bg-sidebar-accent text-sidebar-accent-foreground"
                        : "text-sidebar-foreground hover:bg-sidebar-accent/50"
                    )}
                    onClick={() => onSelectSession(session.id)}
                    onContextMenu={(e) => handleContextMenu(e, session)}
                  >
                    <div className="flex items-center gap-1.5">
                      <MessageSquare className="size-3 shrink-0 opacity-40" />
                      <span className="flex-1 truncate text-body-sm font-medium">
                        {truncate(session.title, 28)}
                      </span>
                      {session.flagged && (
                        <Flag
                          className="size-2.5 shrink-0 fill-current"
                          style={{ color: "var(--color-warning)" }}
                        />
                      )}
                      {session.turn_state === "running" && (
                        <span className="flex items-center gap-1 rounded-full px-1.5 py-0.5 text-nano"
                          style={{
                            backgroundColor: "color-mix(in srgb, var(--claude-orange) 15%, transparent)",
                            color: "var(--claude-orange)",
                          }}
                        >
                          <Zap className="size-2" />
                          Active
                        </span>
                      )}
                      {session.lifecycle_status &&
                        session.lifecycle_status !== "todo" &&
                        session.turn_state !== "running" && (
                          <LifecycleBadge status={session.lifecycle_status} />
                        )}
                    </div>
                    <div className="mt-0.5 pl-[18px] text-caption text-muted-foreground">
                      <div className="truncate">{truncate(session.preview, 40)}</div>
                      <div className="mt-0.5 flex items-center gap-1 text-muted-foreground/70">
                        <span>{session.model_label}</span>
                        <span>·</span>
                        <span>{session.environment_label}</span>
                      </div>
                    </div>
                  </button>
                ))}
              </div>
            </div>
          ))}

          {filteredSections.length === 0 && searchQuery && (
            <div className="px-2 py-6 text-center text-label text-muted-foreground">
              No sessions match "{searchQuery}"
            </div>
          )}

          {totalSessions === 0 && (
            <div className="px-2 py-8 text-center text-label text-muted-foreground">
              No sessions yet.
              <br />
              Start a conversation to create one.
            </div>
          )}
        </div>
      </ScrollArea>

      {/* Bottom: new session button */}
      <div className="border-t border-sidebar-border p-2">
        <Button
          variant="ghost"
          className="h-7 w-full justify-start gap-2 text-label"
          onClick={onCreateSession}
          disabled={isCreatingSession}
        >
          {isCreatingSession ? (
            <Loader2 className="size-3 animate-spin" />
          ) : (
            <Plus className="size-3" />
          )}
          New Session
        </Button>
      </div>

      {/* Context menu */}
      {contextMenu && (
        <div
          ref={contextMenuRef}
          className="fixed z-[100] min-w-[160px] rounded-md border border-border bg-popover py-1 shadow-lg"
          style={{ left: contextMenu.x, top: contextMenu.y }}
        >
          {onExportSession && (
            <>
              <button
                className="flex w-full items-center gap-2 px-3 py-1.5 text-label text-popover-foreground transition-colors hover:bg-accent"
                onClick={() => {
                  onExportSession(contextMenu.session.id, "markdown");
                  setContextMenu(null);
                }}
              >
                <FileText className="size-3" />
                Export as Markdown
              </button>
              <button
                className="flex w-full items-center gap-2 px-3 py-1.5 text-label text-popover-foreground transition-colors hover:bg-accent"
                onClick={() => {
                  onExportSession(contextMenu.session.id, "json");
                  setContextMenu(null);
                }}
              >
                <FileJson className="size-3" />
                Export as JSON
              </button>
            </>
          )}
          {onSetSessionStatus && (
            <>
              <div className="my-1 h-px bg-border" />
              <div className="px-3 py-1 text-caption font-semibold text-muted-foreground">
                Move to
              </div>
              {(
                ["todo", "in_progress", "needs_review", "done", "archived"] as const
              ).map((status) => {
                const meta = STATUS_META[status];
                const StatusIcon = meta.icon;
                const isCurrent = contextMenu.session.lifecycle_status === status;
                return (
                  <button
                    key={status}
                    className={cn(
                      "flex w-full items-center gap-2 px-3 py-1.5 text-label text-popover-foreground transition-colors hover:bg-accent",
                      isCurrent && "bg-accent/50",
                    )}
                    onClick={() => {
                      onSetSessionStatus(contextMenu.session.id, status);
                      setContextMenu(null);
                    }}
                  >
                    <StatusIcon className="size-3" style={{ color: meta.color }} />
                    {meta.label}
                    {isCurrent && (
                      <span className="ml-auto text-caption text-muted-foreground">
                        current
                      </span>
                    )}
                  </button>
                );
              })}
            </>
          )}
          {onToggleSessionFlag && (
            <>
              <div className="my-1 h-px bg-border" />
              <button
                className="flex w-full items-center gap-2 px-3 py-1.5 text-label text-popover-foreground transition-colors hover:bg-accent"
                onClick={() => {
                  onToggleSessionFlag(
                    contextMenu.session.id,
                    !contextMenu.session.flagged,
                  );
                  setContextMenu(null);
                }}
              >
                <Flag
                  className={cn(
                    "size-3",
                    contextMenu.session.flagged && "fill-current",
                  )}
                  style={{ color: "var(--color-warning)" }}
                />
                {contextMenu.session.flagged ? "Unflag" : "Flag for attention"}
              </button>
            </>
          )}
          {onDeleteSession && (
            <>
              <div className="my-1 h-px bg-border" />
              <button
                className="flex w-full items-center gap-2 px-3 py-1.5 text-label transition-colors hover:bg-accent"
                style={{ color: "var(--color-error)" }}
                onClick={() => {
                  setDeleteConfirm(contextMenu.session);
                  setContextMenu(null);
                }}
              >
                <Trash2 className="size-3" />
                Delete Session
              </button>
            </>
          )}
        </div>
      )}

      {/* Delete confirmation dialog */}
      {onDeleteSession && (
        <ConfirmDialog
          open={!!deleteConfirm}
          onOpenChange={(open) => { if (!open) setDeleteConfirm(null); }}
          title="Delete session"
          description="This session and its conversation history will be permanently deleted. This action cannot be undone."
          confirmLabel="Delete"
          variant="destructive"
          onConfirm={() => {
            if (deleteConfirm) onDeleteSession(deleteConfirm.id);
            setDeleteConfirm(null);
          }}
        />
      )}
    </div>
  );
}

/** Inline status badge shown on the right of each session row. */
function LifecycleBadge({ status }: { status: DesktopLifecycleStatus }) {
  const meta = STATUS_META[status];
  const Icon = meta.icon;
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <span
          className="flex shrink-0 items-center rounded-full p-0.5"
          style={{
            backgroundColor: `color-mix(in srgb, ${meta.color} 14%, transparent)`,
            color: meta.color,
          }}
        >
          <Icon className="size-2.5" />
        </span>
      </TooltipTrigger>
      <TooltipContent side="left">{meta.label}</TooltipContent>
    </Tooltip>
  );
}

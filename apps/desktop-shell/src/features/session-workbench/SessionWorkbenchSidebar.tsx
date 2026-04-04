import { Loader2, MessageSquare, PanelLeftClose, Plus } from "lucide-react";
import { useAppDispatch } from "@/store";
import { setShowSessionSidebar } from "@/store/slices/settings";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { truncate } from "@/lib/utils";
import { cn } from "@/lib/utils";
import type { DesktopSessionSection } from "@/lib/tauri";

interface SessionWorkbenchSidebarProps {
  sessionSections: DesktopSessionSection[];
  activeSessionId?: string | null;
  projectLabel: string;
  onSelectSession: (sessionId: string) => void;
  onCreateSession: () => void;
  isCreatingSession: boolean;
}

export function SessionWorkbenchSidebar({
  sessionSections,
  activeSessionId,
  projectLabel,
  onSelectSession,
  onCreateSession,
  isCreatingSession,
}: SessionWorkbenchSidebarProps) {
  const dispatch = useAppDispatch();

  return (
    <div className="flex h-full w-[280px] shrink-0 flex-col border-r border-border bg-sidebar-background">
      <div className="flex items-center justify-between px-3 py-2">
        <span className="text-xs font-medium text-sidebar-foreground">
          Sessions
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
                onClick={() => dispatch(setShowSessionSidebar(false))}
              >
                <PanelLeftClose className="size-3.5" />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Hide Sidebar</TooltipContent>
          </Tooltip>
        </div>
      </div>

      <ScrollArea className="flex-1">
        <div className="px-2 pb-2">
          <div className="px-2 py-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
            {projectLabel}
          </div>

          {sessionSections.map((section) => (
            <div key={section.id} className="mb-3">
              <div className="px-2 py-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">
                {section.label}
              </div>
              {section.sessions.map((session) => (
                <div
                  key={session.id}
                  className={cn(
                    "group cursor-pointer rounded-md px-2 py-1.5 text-xs transition-colors",
                    session.id === activeSessionId
                      ? "bg-sidebar-accent text-sidebar-accent-foreground"
                      : "text-sidebar-foreground hover:bg-sidebar-accent/50"
                  )}
                  onClick={() => onSelectSession(session.id)}
                >
                  <div className="flex items-center gap-2">
                    <MessageSquare className="size-3.5 shrink-0 opacity-50" />
                    <span className="flex-1 truncate font-medium">
                      {truncate(session.title, 30)}
                    </span>
                    {session.turn_state === "running" && (
                      <span className="rounded-full bg-primary/10 px-1.5 py-0.5 text-[10px] text-primary">
                        Running
                      </span>
                    )}
                  </div>
                  <div className="mt-1 pl-[22px] text-[11px] text-muted-foreground">
                    <div className="truncate">{truncate(session.preview, 48)}</div>
                    <div className="mt-0.5 flex items-center gap-1.5">
                      <span>{session.model_label}</span>
                      <span>·</span>
                      <span>{session.environment_label}</span>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          ))}

          {sessionSections.every((section) => section.sessions.length === 0) && (
            <div className="px-2 py-8 text-center text-xs text-muted-foreground">
              No sessions yet.
              <br />
              Start a conversation to create one.
            </div>
          )}
        </div>
      </ScrollArea>

      <div className="border-t border-sidebar-border p-2">
        <Button
          variant="ghost"
          className="w-full justify-start gap-2 text-xs"
          onClick={onCreateSession}
          disabled={isCreatingSession}
        >
          {isCreatingSession ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : (
            <Plus className="size-3.5" />
          )}
          New Session
        </Button>
      </div>
    </div>
  );
}

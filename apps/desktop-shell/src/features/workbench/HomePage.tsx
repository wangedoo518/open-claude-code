import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { useLocation, useNavigate } from "react-router-dom";
import {
  BadgeCheck,
  CalendarClock,
  ChevronRight,
  Inbox,
  MessageSquare,
  PanelLeftClose,
  Plus,
  Search,
  Wrench,
  Zap,
} from "lucide-react";
import { getWorkbench } from "@/lib/tauri";
import { workbenchKeys } from "./api/query";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { truncate } from "@/lib/utils";
import {
  buildHomeSectionHref,
  openHomeSession,
  parseHomeRouteState,
  type NavSection,
} from "./tab-helpers";
import { SearchPage } from "./SearchPage";
import { ScheduledPage } from "./ScheduledPage";
import { DispatchPage } from "./DispatchPage";
import { CustomizePage } from "./CustomizePage";
import { OpenClawPage } from "./OpenClawPage";
import { SettingsPage } from "@/features/settings/SettingsPage";
import { SessionWorkbenchPage } from "@/features/session-workbench/SessionWorkbenchPage";

const PRIMARY_ITEMS = [
  { id: "search", label: "Search", icon: Search },
  { id: "scheduled", label: "Scheduled", icon: CalendarClock },
  { id: "dispatch", label: "Dispatch", icon: Inbox },
  { id: "customize", label: "Customize", icon: Wrench },
] as const;


export function HomePage() {
  const navigate = useNavigate();
  const location = useLocation();
  const { section: homeSection, sessionId: activeHomeSessionId } =
    parseHomeRouteState(location.search);
  const workbenchQuery = useQuery({
    queryKey: workbenchKeys.root(),
    queryFn: getWorkbench,
  });

  const workbench = workbenchQuery.data;
  const sessionSections = useMemo(
    () => workbench?.session_sections ?? [],
    [workbench]
  );

  const hideSidebar = homeSection === "settings";

  return (
    <div className="flex h-full overflow-hidden bg-background">
      {!hideSidebar && (
      <aside className="flex w-[220px] shrink-0 flex-col border-r border-border bg-sidebar-background">
        {/* New session button */}
        <div className="px-2 py-2">
          <Button
            variant="ghost"
            className="h-7 w-full justify-start gap-2 text-body-sm"
            onClick={() => openHomeSession(navigate, null)}
          >
            <Plus className="size-3" />
            New session
          </Button>
        </div>

        <ScrollArea className="flex-1">
          <div className="space-y-3 px-1.5 pb-3">
            {/* Navigation */}
            <nav className="space-y-0.5">
              {PRIMARY_ITEMS.map((item) => (
                <HomeRailButton
                  key={item.id}
                  label={item.label}
                  icon={item.icon}
                  active={homeSection === item.id}
                  onClick={() => navigate(buildHomeSectionHref(item.id))}
                />
              ))}
            </nav>

            {/* Session sections */}
            <section className="space-y-1.5">
              <div className="flex items-center justify-between px-2 text-caption text-muted-foreground">
                <span>{workbench?.project_label ?? "All projects"}</span>
                <PanelLeftClose className="size-3 opacity-40" />
              </div>

              {sessionSections.length === 0 && (
                <div className="px-2 py-4 text-center text-label text-muted-foreground">
                  No sessions yet
                </div>
              )}
              {sessionSections.map((section) => (
                <div key={section.id} className="space-y-0.5">
                  <div className="px-2 text-nano font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                    {section.label}
                  </div>
                  <div className="space-y-0.5">
                    {section.sessions.map((session) => (
                      <button
                        key={session.id}
                        className="w-full rounded-md bg-transparent px-2 py-1.5 text-left transition hover:bg-muted/30"
                        onClick={() => openHomeSession(navigate, session.id)}
                      >
                        <div className="flex items-center gap-1.5">
                          <MessageSquare className="size-3 shrink-0 opacity-30" />
                          <span className="truncate text-body-sm font-medium text-foreground">
                            {session.title}
                          </span>
                          {session.turn_state === "running" && (
                            <Zap className="size-2.5 shrink-0" style={{ color: "var(--claude-orange)" }} />
                          )}
                        </div>
                        <div className="mt-0.5 pl-[18px] text-caption text-muted-foreground">
                          {truncate(session.preview, 32)}
                        </div>
                      </button>
                    ))}
                  </div>
                </div>
              ))}
            </section>
          </div>
        </ScrollArea>

        {/* Bottom panel — compact */}
        <div className="border-t border-sidebar-border px-1.5 py-1.5">
          <div className="flex items-center gap-2 rounded-md px-2 py-1.5">
            <div
              className="flex size-5 shrink-0 items-center justify-center rounded"
              style={{
                backgroundColor: "color-mix(in srgb, var(--color-success) 12%, transparent)",
                color: "var(--color-success)",
              }}
            >
              <BadgeCheck className="size-3" />
            </div>
            <div className="min-w-0 flex-1">
              <div className="text-label font-medium text-foreground">
                {workbench?.account.name ?? "Warwolf"}
              </div>
              <div className="text-nano text-muted-foreground">
                {workbench?.account.plan_label ?? "Desktop"} · {workbench?.update_banner.version ?? "latest"}
              </div>
            </div>
            <div className="rounded-full bg-muted px-1.5 py-0.5 text-micro uppercase tracking-[0.14em] text-muted-foreground">
              {workbench?.composer.environment_label ?? "Local"}
            </div>
          </div>
        </div>
      </aside>
      )}

      <main className="min-w-0 flex-1 overflow-hidden">
        {homeSection === "overview" ? (
          <HomeOverview />
        ) : homeSection === "session" ? (
          <SessionWorkbenchPage
            tabId="home-session"
            sessionId={activeHomeSessionId ?? undefined}
            showSessionSidebar={false}
            syncTabState={false}
            autoSelectFallbackSession={false}
          />
        ) : homeSection === "search" ? (
          <SearchPage />
        ) : homeSection === "scheduled" ? (
          <ScheduledPage />
        ) : homeSection === "dispatch" ? (
          <DispatchPage />
        ) : homeSection === "customize" ? (
          <CustomizePage />
        ) : homeSection === "openclaw" ? (
          <OpenClawPage />
        ) : (
          <SettingsPage />
        )}
      </main>
    </div>
  );
}

function HomeOverview() {
  const navigate = useNavigate();
  const workbenchQuery = useQuery({
    queryKey: workbenchKeys.root(),
    queryFn: getWorkbench,
  });

  const workbench = workbenchQuery.data;
  const overviewCards = [
    {
      id: "search",
      label: "Search",
      body: "Search sessions, project history, and transcript content.",
      icon: Search,
    },
    {
      id: "scheduled",
      label: "Scheduled",
      body: "Create and manage scheduled code tasks.",
      icon: CalendarClock,
    },
    {
      id: "dispatch",
      label: "Dispatch",
      body: "Handle inbox continuations and deliver them.",
      icon: Inbox,
    },
    {
      id: "customize",
      label: "Customize",
      body: "Inspect hooks, MCP servers, and plugins.",
      icon: Wrench,
    },
  ] as const;

  return (
    <div className="h-full overflow-auto bg-background">
      <div className="mx-auto flex max-w-4xl flex-col gap-3 px-5 py-5">
        {/* Welcome header */}
        <section className="rounded-xl border border-border bg-muted/10 p-5">
          <div className="flex items-center gap-2">
            <div
              className="flex size-8 items-center justify-center rounded-lg"
              style={{
                background: "linear-gradient(135deg, var(--claude-orange), var(--claude-orange-shimmer))",
              }}
            >
              <MessageSquare className="size-4 text-white" />
            </div>
            <div>
              <h1 className="text-head font-semibold tracking-tight text-foreground">
                Warwolf Desktop
              </h1>
              <p className="text-label text-muted-foreground">
                Claude Code style desktop workspace
              </p>
            </div>
          </div>
        </section>

        {/* Feature cards */}
        <div className="grid gap-2 md:grid-cols-2">
          {overviewCards.map((card) => (
            <button
              key={card.id}
              className="flex items-start gap-3 rounded-lg border border-border bg-background p-3 text-left transition hover:border-foreground/15 hover:bg-muted/10"
              onClick={() =>
                navigate(buildHomeSectionHref(card.id as NavSection))
              }
            >
              <card.icon className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
              <div className="min-w-0 flex-1">
                <div className="text-body-sm font-semibold text-foreground">
                  {card.label}
                </div>
                <div className="mt-0.5 text-label leading-snug text-muted-foreground">
                  {card.body}
                </div>
              </div>
              <ChevronRight className="mt-0.5 size-3 shrink-0 text-muted-foreground/50" />
            </button>
          ))}
        </div>

        {/* Quick start */}
        <section className="rounded-lg border border-border bg-background p-3">
          <div className="text-body-sm font-semibold text-foreground">Quick start</div>
          <div className="mt-2 grid gap-2 lg:grid-cols-[0.9fr_1.1fr]">
            <Button
              className="h-7 justify-start gap-2 text-body-sm"
              onClick={() => openHomeSession(navigate, null)}
            >
              <Plus className="size-3" />
              New Code session
            </Button>
            <div className="flex items-center rounded-md border border-border bg-muted/20 px-2.5 py-1.5 text-label text-muted-foreground">
              {workbench?.composer.permission_mode_label ?? "Ask permissions"} ·{" "}
              {workbench?.composer.model_label ?? "Opus 4.6"} ·{" "}
              {workbench?.composer.environment_label ?? "Local"}
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}

function HomeRailButton({
  label,
  icon: Icon,
  active,
  onClick,
}: {
  label: string;
  icon: typeof Search;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      className={
        active
          ? "flex w-full items-center gap-2 rounded-md bg-sidebar-accent px-2 py-1.5 text-body-sm font-medium text-sidebar-accent-foreground"
          : "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-body-sm text-sidebar-foreground transition hover:bg-sidebar-accent/50"
      }
      onClick={onClick}
    >
      <Icon className="size-3.5" />
      <span>{label}</span>
    </button>
  );
}

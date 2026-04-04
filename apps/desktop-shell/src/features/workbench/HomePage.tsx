import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  BadgeCheck,
  CalendarClock,
  ChevronRight,
  Inbox,
  PanelLeftClose,
  Plus,
  Search,
  Wrench,
} from "lucide-react";
import { useAppDispatch, useAppSelector } from "@/store";
import { getWorkbench } from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { setViewMode, type NavSection } from "@/store/slices/ui";
import { truncate } from "@/lib/utils";
import { openHomeSession } from "./tab-helpers";
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
  const dispatch = useAppDispatch();
  const viewMode = useAppSelector((state) => state.ui.viewMode);

  // Derive legacy-compatible values from unified ViewMode
  const homeSection: NavSection =
    viewMode.kind === "nav" ? viewMode.section : "session";
  const activeHomeSessionId =
    viewMode.kind === "session" ? viewMode.sessionId : null;
  const workbenchQuery = useQuery({
    queryKey: ["desktop-workbench"],
    queryFn: getWorkbench,
  });

  const workbench = workbenchQuery.data;
  const sessionSections = useMemo(
    () => workbench?.session_sections ?? [],
    [workbench]
  );

  return (
    <div className="flex h-full overflow-hidden bg-background">
      <aside className="flex w-[240px] shrink-0 flex-col border-r border-border bg-sidebar-background">
        <div className="px-3 py-2">
          <Button
            variant="ghost"
            className="h-8 w-full justify-start gap-2 text-[13px]"
            onClick={() => openHomeSession(dispatch, null)}
          >
            <Plus className="size-3.5" />
            New session
          </Button>
        </div>

        <ScrollArea className="flex-1">
          <div className="space-y-4 px-2 pb-3">
            <nav className="space-y-0.5">
              {PRIMARY_ITEMS.map((item) => (
                <HomeRailButton
                  key={item.id}
                  label={item.label}
                  icon={item.icon}
                  active={homeSection === item.id}
                  onClick={() => dispatch(setViewMode({ kind: "nav", section: item.id }))}
                />
              ))}
            </nav>

            <section className="space-y-2">
              <div className="flex items-center justify-between px-2 text-[11px] text-muted-foreground">
                <span>{workbench?.project_label ?? "All projects"}</span>
                <PanelLeftClose className="size-3 opacity-40" />
              </div>

              {sessionSections.map((section) => (
                <div key={section.id} className="space-y-1">
                  <div className="px-2 text-[10px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                    {section.label}
                  </div>
                  <div className="space-y-0.5">
                    {section.sessions.map((session) => (
                      <button
                        key={session.id}
                        className="w-full rounded-lg border border-transparent bg-muted/20 px-2.5 py-1.5 text-left transition hover:border-foreground/10 hover:bg-muted/30"
                        onClick={() => openHomeSession(dispatch, session.id)}
                      >
                        <div className="flex items-center gap-1.5">
                          <span className="h-1.5 w-1.5 rounded-full border border-border bg-background" />
                          <span className="truncate text-[13px] font-medium text-foreground">
                            {session.title}
                          </span>
                        </div>
                        <div className="mt-0.5 pl-3 text-[11px] text-muted-foreground">
                          {truncate(session.preview, 36)}
                        </div>
                      </button>
                    ))}
                  </div>
                </div>
              ))}
            </section>
          </div>
        </ScrollArea>

        <div className="border-t border-sidebar-border p-2">
          <div className="rounded-xl border border-border bg-background p-2.5">
            <div className="flex items-start gap-2">
              <div className="flex size-8 items-center justify-center rounded-xl bg-emerald-50 text-emerald-700">
                <BadgeCheck className="size-4" />
              </div>
              <div className="min-w-0 flex-1">
                <div className="text-[13px] font-medium text-foreground">
                  Updated to {workbench?.update_banner.version ?? "latest"}
                </div>
                <div className="mt-0.5 text-[11px] text-muted-foreground">
                  {workbench?.update_banner.body ?? "Desktop build is ready."}
                </div>
              </div>
            </div>
            <Button variant="outline" className="mt-2 h-7 w-full text-[12px]">
              {workbench?.update_banner.cta_label ?? "Relaunch"}
            </Button>
          </div>

          <div className="mt-2 flex items-center justify-between rounded-xl border border-border bg-background px-2.5 py-2">
            <div>
              <div className="text-[13px] font-medium text-foreground">
                {workbench?.account.name ?? "Warwolf"}
              </div>
              <div className="text-[11px] text-muted-foreground">
                {workbench?.account.plan_label ?? "Desktop"}
              </div>
            </div>
            <div className="rounded-full bg-muted px-1.5 py-0.5 text-[10px] uppercase tracking-[0.14em] text-muted-foreground">
              {workbench?.composer.environment_label ?? "Local"}
            </div>
          </div>
        </div>
      </aside>

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
  const dispatch = useAppDispatch();
  const workbenchQuery = useQuery({
    queryKey: ["desktop-workbench"],
    queryFn: getWorkbench,
  });

  const workbench = workbenchQuery.data;
  const overviewCards = [
    {
      id: "search",
      label: "Search",
      body: "Search sessions, project history, and transcript content from the local runtime.",
    },
    {
      id: "scheduled",
      label: "Scheduled",
      body: "Create and manage local-first scheduled Code tasks.",
    },
    {
      id: "dispatch",
      label: "Dispatch",
      body: "Handle inbox continuations and deliver them into active Code sessions.",
    },
    {
      id: "customize",
      label: "Customize",
      body: "Inspect runtime-backed hooks, MCP servers, and plugins.",
    },
    {
      id: "openclaw",
      label: "OpenClaw",
      body: "Provider hub and future clawhub123 integration surface.",
    },
    {
      id: "settings",
      label: "Settings",
      body: "Organize model, MCP, display, and data information in the cherry-style layout.",
    },
  ] as const;

  return (
    <div className="h-full overflow-auto bg-background">
      <div className="mx-auto flex max-w-5xl flex-col gap-4 px-6 py-6">
        <section className="rounded-2xl border border-border bg-muted/20 p-6">
          <div className="text-[10px] font-semibold uppercase tracking-[0.18em] text-muted-foreground">
            Home
          </div>
          <h1 className="mt-2 text-xl font-semibold tracking-tight text-foreground">
            Claude Code style home workspace
          </h1>
          <p className="mt-2 max-w-3xl text-[13px] leading-5 text-muted-foreground">
            Search, Scheduled, Dispatch, Customize, OpenClaw, and Settings now live inside the Home tab, while the top bar stays trimmed to the cherry-style `首页 / 应用` model.
          </p>
        </section>

        <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
          {overviewCards.map((card) => (
            <button
              key={card.id}
              className="rounded-xl border border-border bg-background p-4 text-left transition hover:border-foreground/20 hover:bg-muted/20"
              onClick={() => dispatch(setViewMode({ kind: "nav", section: card.id as NavSection }))}
            >
              <div className="text-[13px] font-semibold text-foreground">
                {card.label}
              </div>
              <div className="mt-1.5 text-[12px] leading-5 text-muted-foreground">
                {card.body}
              </div>
              <div className="mt-3 inline-flex items-center gap-1 text-[11px] font-medium text-foreground">
                Open <ChevronRight className="size-3" />
              </div>
            </button>
          ))}
        </div>

        <section className="rounded-xl border border-border bg-background p-4">
          <div className="text-[13px] font-semibold text-foreground">Quick start</div>
          <div className="mt-3 grid gap-2 lg:grid-cols-[0.9fr_1.1fr]">
            <Button
              className="h-8 justify-start gap-2 text-[13px]"
              onClick={() => openHomeSession(dispatch, null)}
            >
              <Plus className="size-3.5" />
              New Code session
            </Button>
            <div className="rounded-lg border border-border bg-muted/20 px-3 py-2 text-[12px] text-muted-foreground">
              {workbench?.composer.permission_mode_label ?? "Danger full access"} ·{" "}
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
          ? "flex w-full items-center gap-2 rounded-lg bg-sidebar-accent px-2.5 py-1.5 text-[13px] font-medium text-sidebar-accent-foreground"
          : "flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-[13px] text-sidebar-foreground transition hover:bg-sidebar-accent/50"
      }
      onClick={onClick}
    >
      <Icon className="size-3.5" />
      <span>{label}</span>
    </button>
  );
}

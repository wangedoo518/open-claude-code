import { useMemo, useState, type FormEvent } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import {
  createScheduledTask,
  getScheduled,
  getWorkbench,
  runScheduledTaskNow,
  updateScheduledTaskEnabled,
  type DesktopScheduledSchedule,
  type DesktopWeekday,
} from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Panel, SummaryCard, SummaryGrid, SurfacePage, formatTimestamp } from "./shared";
import { workbenchKeys } from "./api/query";
import { openHomeSession } from "./tab-helpers";

const WEEKDAY_OPTIONS: Array<{ label: string; value: DesktopWeekday }> = [
  { label: "Mon", value: "monday" },
  { label: "Tue", value: "tuesday" },
  { label: "Wed", value: "wednesday" },
  { label: "Thu", value: "thursday" },
  { label: "Fri", value: "friday" },
  { label: "Sat", value: "saturday" },
  { label: "Sun", value: "sunday" },
];

export function ScheduledPage() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const scheduledQuery = useQuery({
    queryKey: workbenchKeys.scheduled(),
    queryFn: getScheduled,
  });
  const workbenchQuery = useQuery({
    queryKey: workbenchKeys.root(),
    queryFn: getWorkbench,
  });

  const sessions = useMemo(
    () => workbenchQuery.data?.session_sections.flatMap((section) => section.sessions) ?? [],
    [workbenchQuery.data]
  );

  const [title, setTitle] = useState("Morning workspace scan");
  const [prompt, setPrompt] = useState(
    "Review the workspace, summarize the highest-value next step, and continue if the path is clear."
  );
  const [scheduleKind, setScheduleKind] = useState<"hourly" | "weekly">("hourly");
  const [intervalHours, setIntervalHours] = useState("4");
  const [weeklyHour, setWeeklyHour] = useState("09");
  const [weeklyMinute, setWeeklyMinute] = useState("00");
  const [weeklyDays, setWeeklyDays] = useState<DesktopWeekday[]>([
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
  ]);
  const [targetSessionId, setTargetSessionId] = useState("");
  const [error, setError] = useState<string | null>(null);

  const createMutation = useMutation({
    mutationFn: () => {
      const selectedSession = sessions.find((session) => session.id === targetSessionId);
      const schedule: DesktopScheduledSchedule =
        scheduleKind === "hourly"
          ? {
              kind: "hourly",
              interval_hours: Number(intervalHours || "1"),
            }
          : {
              kind: "weekly",
              days: weeklyDays,
              hour: Number(weeklyHour || "0"),
              minute: Number(weeklyMinute || "0"),
            };

      return createScheduledTask({
        title,
        prompt,
        project_name:
          selectedSession?.project_name ?? workbenchQuery.data?.project_name,
        project_path:
          selectedSession?.project_path ?? scheduledQuery.data?.scheduled.project_path,
        target_session_id: targetSessionId || null,
        schedule,
      });
    },
    onSuccess: async () => {
      setTitle("Morning workspace scan");
      setPrompt(
        "Review the workspace, summarize the highest-value next step, and continue if the path is clear."
      );
      setError(null);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: workbenchKeys.scheduled() }),
        queryClient.invalidateQueries({ queryKey: workbenchKeys.root() }),
      ]);
    },
    onError: (mutationError) => {
      setError(errorMessage(mutationError));
    },
  });

  const toggleMutation = useMutation({
    mutationFn: ({ taskId, enabled }: { taskId: string; enabled: boolean }) =>
      updateScheduledTaskEnabled(taskId, enabled),
    onSuccess: async () => {
      setError(null);
      await queryClient.invalidateQueries({ queryKey: workbenchKeys.scheduled() });
    },
    onError: (mutationError) => {
      setError(errorMessage(mutationError));
    },
  });

  const runNowMutation = useMutation({
    mutationFn: (taskId: string) => runScheduledTaskNow(taskId),
    onSuccess: async () => {
      setError(null);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: workbenchKeys.scheduled() }),
        queryClient.invalidateQueries({ queryKey: workbenchKeys.root() }),
      ]);
    },
    onError: (mutationError) => {
      setError(errorMessage(mutationError));
    },
  });

  const scheduled = scheduledQuery.data?.scheduled ?? null;

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    await createMutation.mutateAsync();
  }

  return (
    <SurfacePage
      eyebrow="Scheduled"
      title="Local-first scheduled Code tasks"
      description="These schedules live in the Rust desktop runtime, persist locally, and can either reopen an existing Code session or spin up a fresh one."
    >
      <SummaryGrid>
        <SummaryCard
          label="Total tasks"
          value={String(scheduled?.summary.total_task_count ?? 0)}
        />
        <SummaryCard
          label="Running"
          value={String(scheduled?.summary.running_task_count ?? 0)}
        />
        <SummaryCard
          label="Blocked"
          value={String(scheduled?.summary.blocked_task_count ?? 0)}
        />
        <SummaryCard
          label="Trusted paths"
          value={String(scheduled?.trusted_project_paths.length ?? 0)}
        />
      </SummaryGrid>

      <div className="grid gap-6 xl:grid-cols-[0.92fr_1.08fr]">
        <Panel title="Create a scheduled task">
          <form className="space-y-4" onSubmit={handleSubmit}>
            <label className="block space-y-2 text-sm">
              <span className="text-muted-foreground">Title</span>
              <Input value={title} onChange={(event) => setTitle(event.target.value)} />
            </label>

            <label className="block space-y-2 text-sm">
              <span className="text-muted-foreground">Prompt</span>
              <textarea
                className="min-h-28 w-full rounded-xl border border-input bg-background px-3 py-2 text-sm outline-none focus:border-ring focus:ring-1 focus:ring-ring"
                value={prompt}
                onChange={(event) => setPrompt(event.target.value)}
              />
            </label>

            <label className="block space-y-2 text-sm">
              <span className="text-muted-foreground">Target session</span>
              <select
                className="w-full rounded-xl border border-input bg-background px-3 py-2 text-sm outline-none"
                value={targetSessionId}
                onChange={(event) => setTargetSessionId(event.target.value)}
              >
                <option value="">Start a fresh session every run</option>
                {sessions.map((session) => (
                  <option key={session.id} value={session.id}>
                    {session.title} · {session.project_name}
                  </option>
                ))}
              </select>
            </label>

            <div className="grid gap-3 md:grid-cols-3">
              <label className="block space-y-2 text-sm md:col-span-1">
                <span className="text-muted-foreground">Cadence</span>
                <select
                  className="w-full rounded-xl border border-input bg-background px-3 py-2 text-sm outline-none"
                  value={scheduleKind}
                  onChange={(event) =>
                    setScheduleKind(event.target.value as "hourly" | "weekly")
                  }
                >
                  <option value="hourly">Hourly</option>
                  <option value="weekly">Weekly</option>
                </select>
              </label>

              {scheduleKind === "hourly" ? (
                <label className="block space-y-2 text-sm md:col-span-2">
                  <span className="text-muted-foreground">Interval (hours)</span>
                  <Input
                    type="number"
                    min="1"
                    max="24"
                    value={intervalHours}
                    onChange={(event) => setIntervalHours(event.target.value)}
                  />
                </label>
              ) : (
                <>
                  <label className="block space-y-2 text-sm">
                    <span className="text-muted-foreground">Hour</span>
                    <Input
                      type="number"
                      min="0"
                      max="23"
                      value={weeklyHour}
                      onChange={(event) => setWeeklyHour(event.target.value)}
                    />
                  </label>
                  <label className="block space-y-2 text-sm">
                    <span className="text-muted-foreground">Minute</span>
                    <Input
                      type="number"
                      min="0"
                      max="59"
                      value={weeklyMinute}
                      onChange={(event) => setWeeklyMinute(event.target.value)}
                    />
                  </label>
                </>
              )}
            </div>

            {scheduleKind === "weekly" ? (
              <div className="space-y-2">
                <div className="text-sm text-muted-foreground">Days</div>
                <div className="flex flex-wrap gap-2">
                  {WEEKDAY_OPTIONS.map((day) => (
                    <button
                      key={day.value}
                      type="button"
                      className={
                        weeklyDays.includes(day.value)
                          ? "rounded-full bg-primary px-3 py-1 text-xs font-medium text-primary-foreground"
                          : "rounded-full border border-border px-3 py-1 text-xs text-muted-foreground"
                      }
                      onClick={() =>
                        setWeeklyDays((current) =>
                          current.includes(day.value)
                            ? current.filter((value) => value !== day.value)
                            : [...current, day.value]
                        )
                      }
                    >
                      {day.label}
                    </button>
                  ))}
                </div>
              </div>
            ) : null}

            {error ? (
              <div className="rounded-xl border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
                {error}
              </div>
            ) : null}

            <Button type="submit" disabled={createMutation.isPending}>
              {createMutation.isPending ? "Saving…" : "Create scheduled task"}
            </Button>
          </form>
        </Panel>

        <Panel title="Active tasks">
          <div className="space-y-3">
            {scheduled?.tasks.map((task) => (
              <div
                key={task.id}
                className="rounded-2xl border border-border bg-muted/10 p-4"
              >
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div>
                    <div className="text-sm font-semibold text-foreground">
                      {task.title}
                    </div>
                    <div className="mt-1 text-xs text-muted-foreground">
                      {task.schedule_label}
                    </div>
                  </div>
                  <div className="flex items-center gap-2">
                    <span className="rounded-full bg-muted px-2 py-0.5 text-caption uppercase tracking-[0.14em] text-muted-foreground">
                      {task.status}
                    </span>
                    <span className="rounded-full bg-muted px-2 py-0.5 text-caption uppercase tracking-[0.14em] text-muted-foreground">
                      {task.enabled ? "Enabled" : "Paused"}
                    </span>
                  </div>
                </div>
                <p className="mt-3 text-sm leading-6 text-muted-foreground">
                  {task.prompt}
                </p>
                <div className="mt-3 grid gap-2 text-xs text-muted-foreground md:grid-cols-2">
                  <div>Next run: {formatTimestamp(task.next_run_at)}</div>
                  <div>Last run: {formatTimestamp(task.last_run_at)}</div>
                  <div>Target: {task.target.label}</div>
                  <div>
                    Outcome: {task.last_outcome ?? task.last_run_status ?? "Waiting"}
                  </div>
                </div>
                {task.blocked_reason ? (
                  <div className="mt-3 rounded-xl border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
                    {task.blocked_reason}
                  </div>
                ) : null}
                <div className="mt-4 flex flex-wrap gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() =>
                      void toggleMutation.mutateAsync({
                        taskId: task.id,
                        enabled: !task.enabled,
                      })
                    }
                    disabled={toggleMutation.isPending}
                  >
                    {task.enabled ? "Pause" : "Enable"}
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => void runNowMutation.mutateAsync(task.id)}
                    disabled={runNowMutation.isPending}
                  >
                    Run now
                  </Button>
                  {task.target.session_id ? (
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => openHomeSession(navigate, task.target.session_id!)}
                    >
                      Open session
                    </Button>
                  ) : null}
                </div>
              </div>
            ))}

            {!scheduled?.tasks.length && (
              <div className="rounded-2xl border border-dashed border-border px-4 py-10 text-center text-sm text-muted-foreground">
                No scheduled tasks configured yet.
              </div>
            )}
          </div>
        </Panel>
      </div>
    </SurfacePage>
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "Unexpected error";
}

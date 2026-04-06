import { useMemo, useState, type FormEvent } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import {
  createDispatchItem,
  deliverDispatchItem,
  getDispatch,
  getWorkbench,
  updateDispatchItemStatus,
  type DesktopDispatchPriority,
  type DesktopDispatchStatus,
} from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { openHomeSession } from "./tab-helpers";
import { workbenchKeys } from "./api/query";
import { Panel, SummaryCard, SummaryGrid, SurfacePage, formatTimestamp } from "./shared";
import { truncate } from "@/lib/utils";

const PRIORITIES: DesktopDispatchPriority[] = ["low", "normal", "high"];

export function DispatchPage() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const dispatchQuery = useQuery({
    queryKey: workbenchKeys.dispatch(),
    queryFn: getDispatch,
  });
  const workbenchQuery = useQuery({
    queryKey: workbenchKeys.root(),
    queryFn: getWorkbench,
  });

  const sessions = useMemo(
    () => workbenchQuery.data?.session_sections.flatMap((section) => section.sessions) ?? [],
    [workbenchQuery.data]
  );

  const [title, setTitle] = useState("Continue this code review");
  const [body, setBody] = useState(
    "Review the current workspace state, summarize the next important action, and continue the implementation if the path is clear."
  );
  const [priority, setPriority] = useState<DesktopDispatchPriority>("normal");
  const [targetSessionId, setTargetSessionId] = useState("");
  const [error, setError] = useState<string | null>(null);

  const createMutation = useMutation({
    mutationFn: () => {
      const selectedSession = sessions.find((session) => session.id === targetSessionId);
      return createDispatchItem({
        title,
        body,
        priority,
        target_session_id: targetSessionId || null,
        project_name:
          selectedSession?.project_name ?? workbenchQuery.data?.project_name,
        project_path:
          selectedSession?.project_path ?? dispatchQuery.data?.dispatch.project_path,
      });
    },
    onSuccess: async () => {
      setError(null);
      setTitle("Continue this code review");
      setBody(
        "Review the current workspace state, summarize the next important action, and continue the implementation if the path is clear."
      );
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: workbenchKeys.dispatch() }),
        queryClient.invalidateQueries({ queryKey: workbenchKeys.root() }),
      ]);
    },
    onError: (mutationError) => setError(errorMessage(mutationError)),
  });

  const statusMutation = useMutation({
    mutationFn: ({
      itemId,
      status,
    }: {
      itemId: string;
      status: DesktopDispatchStatus;
    }) => updateDispatchItemStatus(itemId, status),
    onSuccess: async () => {
      setError(null);
      await queryClient.invalidateQueries({ queryKey: workbenchKeys.dispatch() });
    },
    onError: (mutationError) => setError(errorMessage(mutationError)),
  });

  const deliverMutation = useMutation({
    mutationFn: (itemId: string) => deliverDispatchItem(itemId),
    onSuccess: async () => {
      setError(null);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: workbenchKeys.dispatch() }),
        queryClient.invalidateQueries({ queryKey: workbenchKeys.root() }),
      ]);
    },
    onError: (mutationError) => setError(errorMessage(mutationError)),
  });

  const dispatchState = dispatchQuery.data?.dispatch ?? null;

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    await createMutation.mutateAsync();
  }

  return (
    <SurfacePage
      eyebrow="Dispatch"
      title="Inbox and continuation queue"
      description="This is the desktop-side inbox for deferred Code work. Items can target an existing session or reopen the work in a fresh one."
    >
      <SummaryGrid>
        <SummaryCard
          label="Inbox items"
          value={String(dispatchState?.summary.total_item_count ?? 0)}
        />
        <SummaryCard
          label="Unread"
          value={String(dispatchState?.summary.unread_item_count ?? 0)}
        />
        <SummaryCard
          label="Pending"
          value={String(dispatchState?.summary.pending_item_count ?? 0)}
        />
        <SummaryCard
          label="Delivered"
          value={String(dispatchState?.summary.delivered_item_count ?? 0)}
        />
      </SummaryGrid>

      <div className="grid gap-6 xl:grid-cols-[0.9fr_1.1fr]">
        <Panel title="Create a dispatch item">
          <form className="space-y-4" onSubmit={handleSubmit}>
            <label className="block space-y-2 text-sm">
              <span className="text-muted-foreground">Title</span>
              <Input value={title} onChange={(event) => setTitle(event.target.value)} />
            </label>
            <label className="block space-y-2 text-sm">
              <span className="text-muted-foreground">Body</span>
              <textarea
                className="min-h-28 w-full rounded-xl border border-input bg-background px-3 py-2 text-sm outline-none focus:border-ring focus:ring-1 focus:ring-ring"
                value={body}
                onChange={(event) => setBody(event.target.value)}
              />
            </label>
            <div className="grid gap-3 md:grid-cols-2">
              <label className="block space-y-2 text-sm">
                <span className="text-muted-foreground">Priority</span>
                <select
                  className="w-full rounded-xl border border-input bg-background px-3 py-2 text-sm outline-none"
                  value={priority}
                  onChange={(event) =>
                    setPriority(event.target.value as DesktopDispatchPriority)
                  }
                >
                  {PRIORITIES.map((entry) => (
                    <option key={entry} value={entry}>
                      {entry}
                    </option>
                  ))}
                </select>
              </label>
              <label className="block space-y-2 text-sm">
                <span className="text-muted-foreground">Target session</span>
                <select
                  className="w-full rounded-xl border border-input bg-background px-3 py-2 text-sm outline-none"
                  value={targetSessionId}
                  onChange={(event) => setTargetSessionId(event.target.value)}
                >
                  <option value="">Deliver into a fresh session</option>
                  {sessions.map((session) => (
                    <option key={session.id} value={session.id}>
                      {truncate(session.title, 30)} · {truncate(session.project_name, 15)}
                    </option>
                  ))}
                </select>
              </label>
            </div>

            {error ? (
              <div className="rounded-xl border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
                {error}
              </div>
            ) : null}

            <Button type="submit" disabled={createMutation.isPending}>
              {createMutation.isPending ? "Saving…" : "Create dispatch item"}
            </Button>
          </form>
        </Panel>

        <Panel title="Inbox">
          <div className="space-y-3">
            {dispatchState?.items.map((item) => (
              <div
                key={item.id}
                className="rounded-2xl border border-border bg-muted/10 p-4"
              >
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div>
                    <div className="text-sm font-semibold text-foreground">
                      {item.title}
                    </div>
                    <div className="mt-1 text-xs text-muted-foreground">
                      {item.source.label} · {item.priority}
                    </div>
                  </div>
                  <div className="flex items-center gap-2">
                    <span className="rounded-full bg-muted px-2 py-0.5 text-caption uppercase tracking-[0.14em] text-muted-foreground">
                      {item.status}
                    </span>
                    <span className="rounded-full bg-muted px-2 py-0.5 text-caption uppercase tracking-[0.14em] text-muted-foreground">
                      {item.target.label}
                    </span>
                  </div>
                </div>
                <p className="mt-3 text-sm leading-6 text-muted-foreground">
                  {item.body}
                </p>
                <div className="mt-3 grid gap-2 text-xs text-muted-foreground md:grid-cols-2">
                  <div>Created: {formatTimestamp(item.created_at)}</div>
                  <div>Delivered: {formatTimestamp(item.delivered_at)}</div>
                  <div>Workspace: {item.project_name}</div>
                  <div>Outcome: {item.last_outcome ?? "Pending"}</div>
                </div>
                <div className="mt-4 flex flex-wrap gap-2">
                  {item.status !== "delivered" && item.status !== "archived" ? (
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => void deliverMutation.mutateAsync(item.id)}
                      disabled={deliverMutation.isPending}
                    >
                      Deliver now
                    </Button>
                  ) : null}
                  {item.status === "unread" ? (
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() =>
                        void statusMutation.mutateAsync({
                          itemId: item.id,
                          status: "read",
                        })
                      }
                      disabled={statusMutation.isPending}
                    >
                      Mark read
                    </Button>
                  ) : null}
                  {item.status !== "archived" ? (
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() =>
                        void statusMutation.mutateAsync({
                          itemId: item.id,
                          status: "archived",
                        })
                      }
                      disabled={statusMutation.isPending}
                    >
                      Archive
                    </Button>
                  ) : null}
                  {item.target.session_id ? (
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => openHomeSession(navigate, item.target.session_id!)}
                    >
                      Open session
                    </Button>
                  ) : null}
                </div>
              </div>
            ))}

            {!dispatchState?.items.length && (
              <div className="rounded-2xl border border-dashed border-border px-4 py-10 text-center text-sm text-muted-foreground">
                No dispatch items queued yet.
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

import { useDeferredValue, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";
import { Search as SearchIcon } from "lucide-react";
import { Input } from "@/components/ui/input";
import { getWorkbench, searchSessions } from "@/lib/tauri";
import { workbenchKeys } from "./api/query";
import { openHomeSession } from "./tab-helpers";
import { Panel, SurfacePage } from "./shared";

export function SearchPage() {
  const navigate = useNavigate();
  const [query, setQuery] = useState("");
  const deferredQuery = useDeferredValue(query);
  const workbenchQuery = useQuery({
    queryKey: workbenchKeys.root(),
    queryFn: getWorkbench,
  });
  const searchQuery = useQuery({
    queryKey: workbenchKeys.search(deferredQuery),
    queryFn: () => searchSessions(deferredQuery),
    enabled: deferredQuery.trim().length > 0,
  });

  const sections = useMemo(
    () => workbenchQuery.data?.session_sections ?? [],
    [workbenchQuery.data]
  );

  return (
    <SurfacePage
      eyebrow="Search"
      title="Search sessions and workspace history"
      description="Search titles, previews, and message content from the Rust desktop runtime. When no query is active, this view doubles as a fast resume surface."
    >
      <Panel title="Search" description="Find by title, preview, or transcript content.">
        <div className="space-y-4">
          <div className="relative">
            <SearchIcon className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
            <Input
              className="pl-9"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Search sessions or start from a recent thread"
            />
          </div>

          {deferredQuery.trim() ? (
            <div className="space-y-2">
              {searchQuery.data?.results.length ? (
                searchQuery.data.results.map((result) => (
                  <button
                    key={`${result.session_id}-${result.updated_at}`}
                    className="w-full rounded-2xl border border-border bg-muted/20 px-4 py-3 text-left transition hover:border-foreground/20 hover:bg-muted/30"
                    onClick={() => openHomeSession(navigate, result.session_id)}
                  >
                    <div className="text-sm font-medium text-foreground">
                      {result.title}
                    </div>
                    <div className="mt-1 text-xs text-muted-foreground">
                      {result.snippet}
                    </div>
                    <div className="mt-2 text-label uppercase tracking-[0.14em] text-muted-foreground">
                      {result.project_name} · {result.bucket}
                    </div>
                  </button>
                ))
              ) : (
                <div className="rounded-2xl border border-dashed border-border px-4 py-10 text-center text-sm text-muted-foreground">
                  No sessions matched “{deferredQuery.trim()}”.
                </div>
              )}
            </div>
          ) : (
            <div className="grid gap-4 lg:grid-cols-3">
              {sections.map((section) => (
                <div key={section.id} className="rounded-2xl border border-border bg-muted/10 p-4">
                  <div className="mb-3 text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                    {section.label}
                  </div>
                  <div className="space-y-2">
                    {section.sessions.map((session) => (
                      <button
                        key={session.id}
                        className="w-full rounded-xl border border-border bg-background px-3 py-2 text-left transition hover:border-foreground/20"
                        onClick={() => openHomeSession(navigate, session.id)}
                      >
                        <div className="text-sm font-medium text-foreground">
                          {session.title}
                        </div>
                        <div className="mt-1 text-xs text-muted-foreground">
                          {session.preview}
                        </div>
                      </button>
                    ))}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </Panel>
    </SurfacePage>
  );
}

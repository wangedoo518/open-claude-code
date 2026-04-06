import { useQuery } from "@tanstack/react-query";
import { getCustomize } from "@/lib/tauri";
import { workbenchKeys } from "./api/query";
import { Panel, SummaryCard, SummaryGrid, SurfacePage } from "./shared";

export function CustomizePage() {
  const customizeQuery = useQuery({
    queryKey: workbenchKeys.customize(),
    queryFn: getCustomize,
  });

  const customize = customizeQuery.data?.customize ?? null;

  return (
    <SurfacePage
      eyebrow="Customize"
      title="Runtime-backed desktop configuration"
      description="This surface reads the same merged model defaults, hooks, MCP definitions, and plugin registry that the Rust runtime uses for Code sessions."
    >
      <SummaryGrid>
        <SummaryCard
          label="Config files"
          value={String(customize?.summary.loaded_config_count ?? 0)}
        />
        <SummaryCard
          label="MCP servers"
          value={String(customize?.summary.mcp_server_count ?? 0)}
        />
        <SummaryCard
          label="Enabled plugins"
          value={
            customize
              ? `${customize.summary.enabled_plugin_count}/${customize.summary.plugin_count}`
              : "0/0"
          }
        />
        <SummaryCard
          label="Plugin tools"
          value={String(customize?.summary.plugin_tool_count ?? 0)}
        />
      </SummaryGrid>

      <div className="grid gap-6 xl:grid-cols-2">
        <Panel title="Runtime identity">
          <div className="space-y-2 text-sm">
            <div className="flex justify-between gap-4">
              <span className="text-muted-foreground">Model</span>
              <span>{customize?.model_label ?? "Unavailable"}</span>
            </div>
            <div className="flex justify-between gap-4">
              <span className="text-muted-foreground">Permission mode</span>
              <span>{customize?.permission_mode ?? "Unavailable"}</span>
            </div>
            <div className="flex justify-between gap-4">
              <span className="text-muted-foreground">Workspace</span>
              <span className="max-w-[420px] truncate text-right">
                {customize?.project_path ?? "Unavailable"}
              </span>
            </div>
          </div>
        </Panel>

        <Panel title="Hooks">
          <div className="space-y-4">
            <div>
              <div className="text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                PreToolUse
              </div>
              <div className="mt-2 space-y-2">
                {customize?.hooks.pre_tool_use.length ? (
                  customize.hooks.pre_tool_use.map((hook) => (
                    <code
                      key={hook}
                      className="block rounded-lg border border-border bg-muted/20 px-3 py-2 text-xs"
                    >
                      {hook}
                    </code>
                  ))
                ) : (
                  <div className="text-sm text-muted-foreground">No pre hooks configured.</div>
                )}
              </div>
            </div>

            <div>
              <div className="text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">
                PostToolUse
              </div>
              <div className="mt-2 space-y-2">
                {customize?.hooks.post_tool_use.length ? (
                  customize.hooks.post_tool_use.map((hook) => (
                    <code
                      key={hook}
                      className="block rounded-lg border border-border bg-muted/20 px-3 py-2 text-xs"
                    >
                      {hook}
                    </code>
                  ))
                ) : (
                  <div className="text-sm text-muted-foreground">No post hooks configured.</div>
                )}
              </div>
            </div>
          </div>
        </Panel>

        <Panel title="Loaded settings files">
          <div className="space-y-2">
            {customize?.loaded_configs.length ? (
              customize.loaded_configs.map((config) => (
                <div
                  key={config.path}
                  className="rounded-xl border border-border bg-muted/20 px-3 py-2"
                >
                  <div className="text-sm font-medium">{config.source}</div>
                  <code className="mt-1 block text-xs text-muted-foreground">
                    {config.path}
                  </code>
                </div>
              ))
            ) : (
              <div className="text-sm text-muted-foreground">No config files loaded.</div>
            )}
          </div>
        </Panel>

        <Panel title="Plugins and MCP">
          <div className="space-y-3">
            {customize?.mcp_servers.length ? (
              customize.mcp_servers.map((server) => (
                <div
                  key={`${server.name}-${server.target}`}
                  className="rounded-xl border border-border bg-muted/20 px-3 py-2"
                >
                  <div className="text-sm font-medium">{server.name}</div>
                  <div className="mt-1 text-xs text-muted-foreground">
                    {server.scope} · {server.transport}
                  </div>
                  <code className="mt-2 block text-xs">{server.target}</code>
                </div>
              ))
            ) : (
              <div className="text-sm text-muted-foreground">No MCP servers configured.</div>
            )}
            {customize?.plugins.length ? (
              customize.plugins.map((plugin) => (
                <div
                  key={plugin.id}
                  className="rounded-xl border border-border bg-background px-3 py-3"
                >
                  <div className="flex items-center justify-between gap-3">
                    <div className="text-sm font-medium">{plugin.name}</div>
                    <span className="rounded-full bg-muted px-2 py-0.5 text-caption uppercase tracking-[0.14em] text-muted-foreground">
                      {plugin.enabled ? "Enabled" : "Disabled"}
                    </span>
                  </div>
                  <div className="mt-1 text-xs text-muted-foreground">
                    {plugin.kind} · v{plugin.version} · {plugin.source}
                  </div>
                  <div className="mt-2 text-sm">{plugin.description}</div>
                </div>
              ))
            ) : (
              <div className="text-sm text-muted-foreground">No plugins installed.</div>
            )}
          </div>
        </Panel>
      </div>
    </SurfacePage>
  );
}

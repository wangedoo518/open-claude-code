import { Badge } from "@/components/ui/badge";
import { SettingGroup } from "../components/SettingGroup";
import type { DesktopCustomizeState } from "@/lib/tauri";

interface McpSettingsProps {
  customize: DesktopCustomizeState | null;
  error?: string;
}

export function McpSettings({ customize, error }: McpSettingsProps) {
  const servers = customize?.mcp_servers ?? [];

  return (
    <div className="space-y-4">
      <SettingGroup
        title="MCP Servers"
        description="Servers discovered from the current runtime configuration"
      >
        <div className="space-y-2">
          {servers.map((server) => (
            <div
              key={`${server.scope}-${server.name}-${server.target}`}
              className="rounded-md border border-border bg-muted/20 px-3 py-2"
            >
              <div className="flex items-center justify-between gap-3">
                <div>
                  <div className="text-sm font-medium">{server.name}</div>
                  <div className="text-xs text-muted-foreground">
                    {server.target}
                  </div>
                </div>
                <div className="flex items-center gap-1">
                  <Badge variant="secondary" className="text-[10px]">
                    {server.scope}
                  </Badge>
                  <Badge variant="outline" className="text-[10px]">
                    {server.transport}
                  </Badge>
                </div>
              </div>
            </div>
          ))}

          {servers.length === 0 && (
            <div className="py-4 text-center text-xs text-muted-foreground">
              No MCP servers configured.
            </div>
          )}
        </div>
      </SettingGroup>

      {(error || customize?.warnings.length) && (
        <SettingGroup title="Warnings">
          <div className="space-y-2 text-xs text-muted-foreground">
            {error && <div>{error}</div>}
            {customize?.warnings.map((warning) => (
              <div key={warning}>{warning}</div>
            ))}
          </div>
        </SettingGroup>
      )}
    </div>
  );
}

import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  Settings,
  Key,
  Plug,
  Shield,
  Keyboard,
  Database,
  Info,
  Loader2,
} from "lucide-react";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { cn } from "@/lib/utils";
import { GeneralSettings } from "./sections/GeneralSettings";
import { ProviderSettings } from "./sections/ProviderSettings";
import { McpSettings } from "./sections/McpSettings";
import { PermissionSettings } from "./sections/PermissionSettings";
import { DataSettings } from "./sections/DataSettings";
import { AboutSection } from "./sections/AboutSection";
import {
  getBootstrap,
  getCustomize,
  getSettings,
  type DesktopBootstrap,
  type DesktopCustomizeState,
  type DesktopSettingsState,
} from "@/lib/tauri";

type SettingsSection =
  | "general"
  | "provider"
  | "mcp"
  | "permissions"
  | "shortcuts"
  | "data"
  | "about";

interface MenuItem {
  id: SettingsSection;
  label: string;
  icon: typeof Settings;
}

const MENU_ITEMS: MenuItem[] = [
  { id: "general", label: "常规", icon: Settings },
  { id: "provider", label: "模型服务", icon: Key },
  { id: "mcp", label: "MCP 服务", icon: Plug },
  { id: "permissions", label: "权限", icon: Shield },
  { id: "shortcuts", label: "快捷键", icon: Keyboard },
  { id: "data", label: "数据", icon: Database },
  { id: "about", label: "关于", icon: Info },
];

export function SettingsPage() {
  const [active, setActive] = useState<SettingsSection>("general");

  const bootstrapQuery = useQuery({
    queryKey: ["desktop-bootstrap"],
    queryFn: getBootstrap,
  });

  const settingsQuery = useQuery({
    queryKey: ["desktop-settings"],
    queryFn: getSettings,
  });

  const customizeQuery = useQuery({
    queryKey: ["desktop-customize"],
    queryFn: getCustomize,
  });

  const isLoading =
    bootstrapQuery.isLoading || settingsQuery.isLoading || customizeQuery.isLoading;
  const error = extractErrorMessage(
    bootstrapQuery.error,
    settingsQuery.error,
    customizeQuery.error
  );

  return (
    <div className="flex h-full">
      <div className="flex w-[220px] shrink-0 flex-col border-r border-border bg-sidebar-background">
        <div className="px-4 py-3">
          <h2 className="text-sm font-semibold text-foreground">设置</h2>
        </div>
        <Separator />
        <nav className="flex-1 px-2 py-2">
          {MENU_ITEMS.map((item) => (
            <button
              key={item.id}
              className={cn(
                "flex w-full items-center gap-2.5 rounded-md px-3 py-1.5 text-sm transition-colors",
                active === item.id
                  ? "bg-sidebar-accent font-medium text-sidebar-accent-foreground"
                  : "text-sidebar-foreground hover:bg-sidebar-accent/50"
              )}
              onClick={() => setActive(item.id)}
            >
              <item.icon className="size-4" />
              {item.label}
            </button>
          ))}
        </nav>
      </div>

      <ScrollArea className="flex-1">
        <div
          className={cn(
            "px-8 py-6",
            active === "provider" ? "max-w-none px-6" : "mx-auto max-w-3xl"
          )}
        >
          <h2 className="mb-4 text-lg font-semibold text-foreground">
            {MENU_ITEMS.find((m) => m.id === active)?.label}
          </h2>

          {isLoading && (
            <div className="flex items-center gap-2 rounded-lg border border-border bg-muted/20 px-4 py-3 text-sm text-muted-foreground">
              <Loader2 className="size-4 animate-spin" />
              <span>正在加载桌面设置...</span>
            </div>
          )}

          {!isLoading && (
            <SettingsContent
              section={active}
              bootstrap={bootstrapQuery.data}
              settings={settingsQuery.data?.settings ?? null}
              customize={customizeQuery.data?.customize ?? null}
              error={error}
            />
          )}
        </div>
      </ScrollArea>
    </div>
  );
}

function SettingsContent({
  section,
  bootstrap,
  settings,
  customize,
  error,
}: {
  section: SettingsSection;
  bootstrap: DesktopBootstrap | undefined;
  settings: DesktopSettingsState | null;
  customize: DesktopCustomizeState | null;
  error?: string;
}) {
  switch (section) {
    case "general":
      return <GeneralSettings />;
    case "provider":
      return (
        <ProviderSettings
          customize={customize}
          error={error}
        />
      );
    case "mcp":
      return <McpSettings customize={customize} error={error} />;
    case "permissions":
      return <PermissionSettings customize={customize} error={error} />;
    case "data":
      return <DataSettings settings={settings} error={error} />;
    case "about":
      return (
        <AboutSection
          productName={bootstrap?.product_name}
          error={error}
          settings={settings}
        />
      );
    default:
      return (
        <div className="py-8 text-center text-sm text-muted-foreground">
          即将支持
        </div>
      );
  }
}

function extractErrorMessage(...errors: Array<unknown>): string | undefined {
  for (const error of errors) {
    if (error instanceof Error) {
      return error.message;
    }
  }
  return undefined;
}

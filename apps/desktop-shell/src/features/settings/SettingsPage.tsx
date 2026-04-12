import { useState, useEffect } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import {
  Settings,
  Key,
  ServerCog,
  MessageCircle,
  Plug,
  Shield,
  Keyboard,
  Database,
  HardDrive,
  Info,
  Loader2,
} from "lucide-react";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { cn } from "@/lib/utils";
import { GeneralSettings } from "./sections/GeneralSettings";
import { ProviderSettings } from "./sections/ProviderSettings";
import { MultiProviderSettings } from "./sections/MultiProviderSettings";
import { SubscriptionCodexPool } from "./sections/private-cloud/SubscriptionCodexPool";
import { WeChatSettings } from "./sections/WeChatSettings";
import { McpSettings } from "./sections/McpSettings";
import { PermissionSettings } from "./sections/PermissionSettings";
import { DataSettings } from "./sections/DataSettings";
import { StorageSettings } from "./sections/StorageSettings";
import { ShortcutsSettings } from "./sections/ShortcutsSettings";
import { AboutSection } from "./sections/AboutSection";
import { settingsKeys } from "./api/query";
import {
  getBootstrap,
  getCustomize,
  getSettings,
  type DesktopBootstrap,
  type DesktopCustomizeState,
  type DesktopSettingsState,
} from "@/lib/tauri";
import { useSettingsStore } from "@/state/settings-store";

type SettingsSection =
  | "general"
  | "provider"
  | "multi-provider"
  | "codex-pool"
  | "wechat"
  | "mcp"
  | "permissions"
  | "shortcuts"
  | "storage"
  | "data"
  | "about";

interface MenuItem {
  id: SettingsSection;
  i18nKey: string;
  icon: typeof Settings;
  labelOverride?: string;
}

const MENU_ITEMS: MenuItem[] = [
  { id: "general", i18nKey: "settings.general", icon: Settings },
  { id: "provider", i18nKey: "settings.provider", icon: Key },
  {
    id: "multi-provider",
    i18nKey: "settings.multiProvider",
    icon: ServerCog,
    labelOverride: "LLM Gateway",
  },
  // S2: Codex pool read-only panel. The broker lives in the Rust
  // process (canonical §9.2); this entry is the only user-facing
  // surface — there is no provider picker, no API-key paste form.
  {
    id: "codex-pool",
    i18nKey: "settings.codexPool",
    icon: ServerCog,
    labelOverride: "订阅池",
  },
  // Phase 6C: WeChat account management (list + QR login + delete).
  // Sits between multi-provider (backend config) and MCP (tool config)
  // because it's a per-user "which channels do you talk through" setting.
  {
    id: "wechat",
    i18nKey: "settings.wechat",
    icon: MessageCircle,
    labelOverride: "WeChat 账号",
  },
  { id: "mcp", i18nKey: "settings.mcp", icon: Plug },
  { id: "permissions", i18nKey: "settings.permissions", icon: Shield },
  { id: "shortcuts", i18nKey: "settings.shortcuts", icon: Keyboard },
  {
    id: "storage",
    i18nKey: "settings.storage",
    icon: HardDrive,
    labelOverride: "数据存储",
  },
  { id: "data", i18nKey: "settings.data", icon: Database },
  { id: "about", i18nKey: "settings.about", icon: Info },
];

export function SettingsPage() {
  const [active, setActive] = useState<SettingsSection>("general");
  const { t, i18n } = useTranslation();

  const language = useSettingsStore((state) => state.language);
  useEffect(() => {
    void i18n.changeLanguage(language);
  }, [language, i18n]);

  const bootstrapQuery = useQuery({
    queryKey: settingsKeys.bootstrap(),
    queryFn: getBootstrap,
  });

  const settingsQuery = useQuery({
    queryKey: settingsKeys.settings(),
    queryFn: getSettings,
  });

  const customizeQuery = useQuery({
    queryKey: settingsKeys.customize(),
    queryFn: getCustomize,
  });

  const privateCloudEnabled =
    bootstrapQuery.data?.private_cloud_enabled === true;
  const menuItems = MENU_ITEMS.filter(
    (item) => privateCloudEnabled || item.id !== "codex-pool"
  );

  useEffect(() => {
    if (!privateCloudEnabled && active === "codex-pool") {
      setActive("general");
    }
  }, [active, privateCloudEnabled]);

  // Treat error states as "loaded with null data" — pages have fallback values
  const isLoading =
    (bootstrapQuery.isLoading && !bootstrapQuery.isError) ||
    (settingsQuery.isLoading && !settingsQuery.isError) ||
    (customizeQuery.isLoading && !customizeQuery.isError);
  const error = extractErrorMessage(
    bootstrapQuery.error,
    settingsQuery.error,
    customizeQuery.error
  );

  return (
    <div className="flex h-full">
      <div className="flex w-[200px] shrink-0 flex-col border-r border-border/50">
        <div className="px-4 py-3">
          <h2 className="uppercase tracking-widest text-muted-foreground/60" style={{ fontSize: 11 }}>{t("settings.title")}</h2>
        </div>
        <Separator className="opacity-50" />
        <nav className="flex-1 px-1.5 py-1.5">
          {menuItems.map((item) => (
            <button
              key={item.id}
              className={cn(
                "flex w-full items-center gap-2 rounded-none px-3 py-1.5 transition-colors",
                active === item.id
                  ? "border-l-[3px] border-l-primary text-foreground"
                  : "border-l-[3px] border-l-transparent text-muted-foreground hover:text-foreground"
              )}
              style={{ fontSize: 13, fontWeight: active === item.id ? 500 : 400 }}
              onClick={() => setActive(item.id)}
            >
              <item.icon className="size-3.5" />
              {item.labelOverride ?? t(item.i18nKey)}
            </button>
          ))}
        </nav>
      </div>

      <ScrollArea className="flex-1">
        <div
          className={cn(
            "px-6 py-4",
            active === "provider" ? "max-w-none px-5" : "mx-auto max-w-3xl"
          )}
        >
          <h2 className="mb-4 text-foreground" style={{ fontSize: 18, fontWeight: 600 }}>
            {(() => {
              const current =
                menuItems.find((m) => m.id === active) ?? menuItems[0];
              return current?.labelOverride ?? t(current?.i18nKey ?? "");
            })()}
          </h2>

          <SettingsContent
            section={active}
            isLoading={isLoading}
            bootstrap={bootstrapQuery.data}
            settings={settingsQuery.data?.settings ?? null}
            customize={customizeQuery.data?.customize ?? null}
            error={error}
          />
        </div>
      </ScrollArea>
    </div>
  );
}

/** Loading placeholder for sections that depend on backend data */
function SectionLoading() {
  const { t } = useTranslation();
  return (
    <div className="flex items-center gap-2 rounded-md border border-border/40 px-4 py-3 text-muted-foreground/60" style={{ fontSize: 13 }}>
      <Loader2 className="size-4 animate-spin" />
      <span>{t("settings.loading")}</span>
    </div>
  );
}

function SettingsContent({
  section,
  isLoading,
  bootstrap,
  settings,
  customize,
  error,
}: {
  section: SettingsSection;
  isLoading: boolean;
  bootstrap: DesktopBootstrap | undefined;
  settings: DesktopSettingsState | null;
  customize: DesktopCustomizeState | null;
  error?: string;
}) {
  // GeneralSettings and ShortcutsSettings use Redux / static data — no backend needed
  if (section === "general") return <GeneralSettings />;
  if (section === "shortcuts") return <ShortcutsSettings />;
  // S2 Codex pool has its own React Query hooks (broker status +
  // account list + clear mutation) and is not blocked by bootstrap.
  if (section === "codex-pool" && !bootstrap?.private_cloud_enabled) {
    return (
      <div className="rounded-md border border-border/40 px-4 py-3 text-muted-foreground/70" style={{ fontSize: 13 }}>
        当前构建未启用私有订阅池能力。公开版本默认使用 LLM Gateway 配置。
      </div>
    );
  }
  if (section === "codex-pool") return <SubscriptionCodexPool />;
  if (section === "multi-provider") return <MultiProviderSettings />;
  // Same story for WeChat accounts — fully self-contained React Query +
  // polling, never blocked on bootstrap/settings/customize.
  if (section === "wechat") return <WeChatSettings />;

  // Other sections need backend data
  if (isLoading) return <SectionLoading />;

  switch (section) {
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
    case "storage":
      return <StorageSettings settings={settings} error={error} />;
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
        <ComingSoon />
      );
  }
}

function ComingSoon() {
  const { t } = useTranslation();
  return (
    <div className="py-8 text-center text-body-sm text-muted-foreground">
      {t("settings.comingSoon")}
    </div>
  );
}

function extractErrorMessage(...errors: Array<unknown>): string | undefined {
  for (const error of errors) {
    if (error instanceof Error) {
      return error.message;
    }
  }
  return undefined;
}

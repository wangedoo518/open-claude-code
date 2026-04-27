/**
 * SettingsPage — DS1.4 · editorial settings center.
 *
 * IA mapping (pre-DS1.4 → DS1.4):
 *
 *   general          ─┐
 *   shortcuts        ─┴→  外观与快捷键       (appearance)
 *
 *   provider         ─┐
 *   multi-provider   ─┤
 *   codex-pool       ─┴→  账户与模型          (account-model)
 *
 *   wechat           ──→  微信接入            (wechat)
 *
 *   permissions      ──→  权限与安全          (security)
 *
 *   storage          ─┐
 *   data             ─┤
 *   about            ─┴→  数据与备份          (data-backup)
 *
 *   mcp              ──→  高级                (advanced)
 *
 * Deep-link aliases: legacy `?tab=` query values are routed to the
 * new group and, where a group contains multiple old sections, scroll
 * the right pane to the relevant card. Nothing old gets removed.
 *
 * Visual contract:
 *   - serif h1 "设置" + caption at top (editorial header)
 *   - 176 px left nav + flexible content (max 860 px wide), LEFT-aligned
 *   - Account/model now uses a compact single-column provider registry.
 *   - Section bodies use lightweight headings + row content, not card-in-card.
 *   - PermissionSettings keeps the localised DS1.4 copy where the English
 *     "Permission Mode / Runtime value" text was user-visible noise.
 */

import { useCallback, useEffect, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { useSearchParams } from "react-router-dom";
import {
  Cpu,
  MessageCircle,
  ShieldCheck,
  Palette,
  Database,
  Wrench,
  Loader2,
  type LucideIcon,
} from "lucide-react";

import { GeneralSettings } from "./sections/GeneralSettings";
import { AccountModelSettings } from "./sections/AccountModelSettings";
import { WeChatSettings } from "./sections/WeChatSettings";
import { McpSettings } from "./sections/McpSettings";
import { RuntimeHealthSection } from "./sections/RuntimeHealthSection";
import { ToolCapabilitySection } from "./sections/ToolCapabilitySection";
import { PermissionSettings } from "./sections/PermissionSettings";
import { DataSettings } from "./sections/DataSettings";
import { ShortcutsSettings } from "./sections/ShortcutsSettings";
import { settingsKeys } from "./api/query";
import {
  getBootstrap,
  getCustomize,
  getSettings,
  type DesktopCustomizeState,
  type DesktopSettingsState,
} from "@/lib/tauri";
import { useSettingsStore } from "@/state/settings-store";

/* ─── Group taxonomy ──────────────────────────────────────────── */

type GroupId =
  | "account-model"
  | "wechat"
  | "security"
  | "appearance"
  | "data-backup"
  | "advanced";

interface GroupMeta {
  id: GroupId;
  kicker: string;
  label: string;
  caption: string;
  icon: LucideIcon;
  developer?: boolean;
}

// DS1.5 · short chapter-style settings groups.
const GROUPS: readonly GroupMeta[] = [
  {
    id: "account-model",
    kicker: "ACCOUNTS & MODELS",
    label: "账户与模型",
    caption: "选择 AI 服务并登录账号 · 高级用户可在底部添加自定义 Provider",
    icon: Cpu,
  },
  {
    id: "wechat",
    kicker: "WECHAT",
    label: "微信接入",
    caption: "管理已绑定的微信小号 · 第一次接入请到「连接微信」页",
    icon: MessageCircle,
  },
  {
    id: "security",
    kicker: "PERMISSION & SAFETY",
    label: "权限与安全",
    caption: "限制 AI 可以读写的文件。",
    icon: ShieldCheck,
  },
  {
    id: "appearance",
    kicker: "APPEARANCE",
    label: "外观与快捷键",
    caption: "调整主题、字体、语言和快捷键。",
    icon: Palette,
  },
  {
    id: "data-backup",
    kicker: "DATA & BACKUP",
    label: "数据与备份",
    caption: "备份、恢复、迁移本地数据。",
    icon: Database,
  },
  {
    id: "advanced",
    kicker: "ADVANCED",
    label: "高级",
    caption: "运行环境、扩展工具和实验功能。",
    icon: Wrench,
    developer: true,
  },
];

/**
 * Map a legacy `?tab=` value to its new group + optional anchor. When
 * a link from an old surface lands here we still honour it.
 */
function aliasLegacyTab(legacy: string | null): {
  group: GroupId;
  anchor: string | null;
} {
  switch (legacy) {
    case "general":
    case "shortcuts":
    case "appearance":
      return { group: "appearance", anchor: legacy === "shortcuts" ? "shortcuts" : null };
    case "provider":
    case "multi-provider":
    case "codex-pool":
    case "account-model":
      return { group: "account-model", anchor: legacy };
    case "wechat":
      return { group: "wechat", anchor: null };
    case "permissions":
    case "security":
      return { group: "security", anchor: null };
    case "storage":
    case "data":
    case "about":
    case "data-backup":
      return { group: "data-backup", anchor: legacy === "about" ? "about" : null };
    case "mcp":
    case "advanced":
      return { group: "advanced", anchor: null };
    default:
      return { group: "account-model", anchor: null };
  }
}

/* ─── Page component ─────────────────────────────────────────── */

export function SettingsPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const language = useSettingsStore((state) => state.language);
  const { i18n } = useTranslation();

  // Seed default group from legacy `?tab=` alias so deep-links keep
  // working. When the URL has no tab query, default to 账户与模型.
  const initial = useMemo(
    () => aliasLegacyTab(searchParams.get("tab")),
    [searchParams],
  );
  const activeGroup: GroupId = initial.group;
  const anchor = initial.anchor;

  const setGroup = useCallback(
    (next: GroupId) => {
      const params = new URLSearchParams(searchParams);
      if (next === "account-model") {
        params.delete("tab");
      } else {
        params.set("tab", next);
      }
      setSearchParams(params, { replace: true });
    },
    [searchParams, setSearchParams],
  );

  // Keep i18n in sync with the user-preferred language (pre-DS1.4
  // behaviour: Settings was often the first page users land on, and
  // if they'd changed the language persisted in the store we want it
  // reflected everywhere on load).
  useEffect(() => {
    void i18n.changeLanguage(language);
  }, [language, i18n]);

  // Shared backend data — fetched once per page load. Individual
  // sections read the slices they need.
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

  const isLoading =
    (bootstrapQuery.isLoading && !bootstrapQuery.isError) ||
    (settingsQuery.isLoading && !settingsQuery.isError) ||
    (customizeQuery.isLoading && !customizeQuery.isError);
  const error = extractErrorMessage(
    bootstrapQuery.error,
    settingsQuery.error,
    customizeQuery.error,
  );

  const currentMeta = GROUPS.find((g) => g.id === activeGroup) ?? GROUPS[0];

  return (
    <div className="ds-settings-shell ds-canvas">
      <div className="ds-settings-layout">
        <nav className="ds-settings-nav" aria-label="设置分组">
          <div className="ds-settings-nav-head">
            <div className="ds-settings-kicker">SETTINGS</div>
            <h1 className="ds-settings-title">设置</h1>
            <p className="ds-settings-subtitle">
              调整模型、权限与本地数据
            </p>
          </div>

          <div className="ds-settings-nav-list">
            {GROUPS.map((g) => {
              const Icon = g.icon;
              return (
                <button
                  key={g.id}
                  type="button"
                  onClick={() => setGroup(g.id)}
                  className="ds-settings-nav-item"
                  data-active={g.id === activeGroup || undefined}
                  aria-current={g.id === activeGroup ? "page" : undefined}
                >
                  <Icon
                    className="size-3.5 shrink-0"
                    strokeWidth={1.5}
                    aria-hidden="true"
                  />
                  <span className="ds-settings-nav-label">{g.label}</span>
                  {g.developer ? (
                    <span className="ds-settings-nav-pill">开发者</span>
                  ) : null}
                </button>
              );
            })}
          </div>
        </nav>

        <div className="ds-settings-content">
          <div className="ds-settings-content-inner">
            <div className="ds-settings-section-head">
              <div className="ds-settings-section-kicker">
                {currentMeta.kicker}
              </div>
              <h2 className="ds-settings-section-h">{currentMeta.label}</h2>
              {currentMeta.caption && (
                <p className="ds-settings-section-help">
                  {currentMeta.caption}
                </p>
              )}
            </div>

            <GroupBody
              group={activeGroup}
              anchor={anchor}
              privateCloudEnabled={privateCloudEnabled}
              isLoading={isLoading}
              settings={settingsQuery.data?.settings ?? null}
              customize={customizeQuery.data?.customize ?? null}
              error={error}
            />
          </div>
        </div>
      </div>
    </div>
  );
}

/* ─── Group body — stacks the relevant existing sections ─────── */

function GroupBody({
  group,
  anchor,
  privateCloudEnabled,
  isLoading,
  settings,
  customize,
  error,
}: {
  group: GroupId;
  anchor: string | null;
  privateCloudEnabled: boolean;
  isLoading: boolean;
  settings: DesktopSettingsState | null;
  customize: DesktopCustomizeState | null;
  error?: string;
}) {
  // Scroll to anchor when the user arrives with a legacy `?tab=` alias.
  useEffect(() => {
    if (!anchor) return;
    const el = document.getElementById(`ds-settings-anchor-${anchor}`);
    if (el) {
      el.scrollIntoView({ behavior: "smooth", block: "start" });
    }
  }, [anchor]);

  if (group === "appearance") {
    return (
      <>
        <div id="ds-settings-anchor-appearance">
          <GeneralSettings />
        </div>
        <div id="ds-settings-anchor-shortcuts">
          <ShortcutsSettings />
        </div>
      </>
    );
  }

  if (group === "account-model") {
    if (isLoading) return <SectionLoading />;
    return (
      <AccountModelSettings
        privateCloudEnabled={privateCloudEnabled}
        error={error}
      />
    );
  }

  if (group === "wechat") {
    return (
      <div id="ds-settings-anchor-wechat">
        <WeChatSettings />
      </div>
    );
  }

  if (group === "security") {
    if (isLoading) return <SectionLoading />;
    return (
      <div id="ds-settings-anchor-permissions">
        <PermissionSettings customize={customize} error={error} />
      </div>
    );
  }

  if (group === "data-backup") {
    if (isLoading) return <SectionLoading />;
    return (
      <div id="ds-settings-anchor-data">
        <DataSettings settings={settings} error={error} />
      </div>
    );
  }

  if (group === "advanced") {
    if (isLoading) return <SectionLoading />;
    return (
      <>
        <div id="ds-settings-anchor-runtime-health">
          <RuntimeHealthSection />
        </div>
        <div id="ds-settings-anchor-tool-capability">
          <ToolCapabilitySection />
        </div>
        <div id="ds-settings-anchor-mcp">
          <McpSettings customize={customize} error={error} />
        </div>
      </>
    );
  }

  return null;
}

/* ─── Loading skeleton ───────────────────────────────────────── */

function SectionLoading() {
  return (
    <div className="flex items-center gap-2 rounded-md border border-border/40 px-4 py-3 text-muted-foreground/60" style={{ fontSize: 13 }}>
      <Loader2 className="size-4 animate-spin" strokeWidth={1.5} />
      <span>加载中…</span>
    </div>
  );
}

/* ─── Helpers ────────────────────────────────────────────────── */

function extractErrorMessage(...errors: Array<unknown>): string | undefined {
  for (const error of errors) {
    if (error instanceof Error) {
      return error.message;
    }
  }
  return undefined;
}

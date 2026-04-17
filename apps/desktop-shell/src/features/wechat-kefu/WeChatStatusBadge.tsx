import { useSettingsStore } from "@/state/settings-store";
import {
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar";
import { useKefuStatus, type ChannelStatus } from "./useKefuStatus";

/* ── Relative-time helper (Chinese) ──────────────────────────── */

function relativeTime(unixMs: number): string {
  const diff = Date.now() - unixMs;
  if (diff < 0) return "刚刚";
  const seconds = Math.floor(diff / 1000);
  if (seconds < 60) return "刚刚";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} 分钟前`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} 小时前`;
  const days = Math.floor(hours / 24);
  return `${days} 天前`;
}

/* ── Status visual config ────────────────────────────────────── */

interface StatusVisual {
  dotClass: string;
  label: string;
  pulseClass?: string;
}

function getStatusVisual(
  status: ChannelStatus,
  lastInboundMs: number | null,
): StatusVisual {
  switch (status) {
    case "not_connected":
      return {
        dotClass: "bg-muted-foreground/50",
        label: "⚡ 微信助手 · 未连接",
      };
    case "connecting":
      return {
        dotClass: "bg-amber-400",
        label: "🔄 微信助手 · 连接中...",
        pulseClass: "animate-pulse",
      };
    case "connected": {
      const suffix =
        lastInboundMs != null ? ` · ${relativeTime(lastInboundMs)}` : "";
      return {
        dotClass: "bg-green-500",
        label: `微信助手 · 在线${suffix}`,
      };
    }
    case "disconnected":
      return {
        dotClass: "bg-orange-400",
        label: "微信助手 · 已停止",
      };
    case "error":
      return {
        dotClass: "bg-red-500",
        label: "⚠ 微信助手 · 异常",
      };
  }
}

/* ── Component ───────────────────────────────────────────────── */

export function WeChatStatusBadge() {
  const { channelStatus, raw } = useKefuStatus(30_000);
  const openConnectWeChatModal = useSettingsStore(
    (s) => s.openConnectWeChatModal,
  );
  const openChannelStatusModal = useSettingsStore(
    (s) => s.openChannelStatusModal,
  );

  const lastInboundMs = raw?.last_inbound_unix_ms ?? null;
  const visual = getStatusVisual(channelStatus, lastInboundMs);

  const handleClick = () => {
    if (
      channelStatus === "not_connected" ||
      channelStatus === "connecting"
    ) {
      openConnectWeChatModal();
    } else {
      openChannelStatusModal();
    }
  };

  return (
    <SidebarMenuItem>
      <SidebarMenuButton tooltip={visual.label} onClick={handleClick}>
        {/* Status dot — always visible, sole indicator in icon-collapsed mode */}
        <span
          className={`inline-block h-2 w-2 flex-shrink-0 rounded-full ${visual.dotClass} ${visual.pulseClass ?? ""}`}
          aria-hidden="true"
        />
        <span className="truncate">{visual.label}</span>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
}

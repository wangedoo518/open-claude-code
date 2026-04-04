import { Terminal, Globe, AppWindow } from "lucide-react";
import { CherryOpenClawIcon } from "@/components/icons/CherryIcons";
import type { MinAppType } from "@/types/minapp";

interface MinAppIconProps {
  app: MinAppType;
  size?: number;
}

/**
 * Renders the icon for a MinApp.
 *
 * Matches cherry-studio's MinAppIcon pattern:
 * - If app has a logo URL → render as <img> with rounded corners
 * - If no logo → render lucide icon with gradient background
 */
export function MinAppIcon({ app, size = 60 }: MinAppIconProps) {
  // Logo mode: render image directly (cherry-studio pattern)
  if (app.logo) {
    return (
      <img
        src={app.logo}
        className="select-none rounded-2xl"
        style={{
          width: size,
          height: size,
          border: app.bordered
            ? "0.5px solid var(--color-border)"
            : "none",
          userSelect: "none",
          ...app.style,
        }}
        draggable={false}
        alt={app.name || "MinApp Icon"}
      />
    );
  }

  // Icon mode: lucide icon with gradient background
  const iconSize = size * 0.47;
  const Icon = resolveIcon(app);

  return (
    <div
      className="flex items-center justify-center rounded-2xl text-white shadow-[0_2px_4px_rgba(0,0,0,0.1)]"
      style={{
        width: size,
        height: size,
        background:
          app.gradient || "linear-gradient(135deg, #6366f1, #8b5cf6)",
        ...app.style,
      }}
    >
      <Icon style={{ width: iconSize, height: iconSize }} />
    </div>
  );
}

function resolveIcon(app: MinAppType) {
  switch (app.iconName) {
    case "Terminal":
      return Terminal;
    case "Shell":
      return CherryOpenClawIcon;
    case "Globe":
      return Globe;
    default:
      return AppWindow;
  }
}

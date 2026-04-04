import { X, Terminal, House, LayoutGrid } from "lucide-react";
import { CherryOpenClawIcon } from "@/components/icons/CherryIcons";
import { cn } from "@/lib/utils";

interface TabItemProps {
  id: string;
  title: string;
  type: string;
  active: boolean;
  closable: boolean;
  onSelect: () => void;
  onClose: () => void;
  onMiddleClick: () => void;
}

/**
 * Tab icon mapping matching cherry-studio's getTabIcon function.
 * Icons are 14px (size-3.5) matching cherry-studio.
 */
function getTabIcon(type: string, id: string) {
  switch (type) {
    case "home":
      return <House className="size-3.5 shrink-0" />;
    case "apps":
      return <LayoutGrid className="size-3.5 shrink-0" />;
    case "code":
      return <Terminal className="size-3.5 shrink-0" />;
    case "minapp":
      if (id.includes("openclaw")) {
        return <CherryOpenClawIcon className="size-3.5 shrink-0" />;
      }
      if (id.includes("code")) {
        return <Terminal className="size-3.5 shrink-0" />;
      }
      return <LayoutGrid className="size-3.5 shrink-0" />;
    default:
      return <Terminal className="size-3.5 shrink-0" />;
  }
}

/**
 * Individual tab component matching cherry-studio's Tab styling:
 * Row 2 session tab — compact sizing for dual-row layout.
 * - Height: 28px (down from 30px)
 * - Min-width: 80px
 * - Font-size: 13px
 * - Active: solid background with subtle shadow
 * - Hover: accent background
 * - Close button visible on group hover
 */
export function TabItem({
  id,
  title,
  type,
  active,
  closable,
  onSelect,
  onClose,
  onMiddleClick,
}: TabItemProps) {
  return (
    <div
      role="tab"
      aria-selected={active}
      className={cn(
        "group relative flex h-7 min-w-[80px] max-w-[180px] cursor-pointer items-center gap-1.5 rounded-md px-2.5 text-[13px] transition-colors select-none",
        active
          ? "bg-background text-foreground shadow-sm"
          : "text-muted-foreground hover:bg-accent/50 hover:text-foreground"
      )}
      onClick={onSelect}
      onMouseDown={(e) => {
        if (e.button === 1) {
          e.preventDefault();
          onMiddleClick();
        }
      }}
    >
      <div className="flex items-center gap-1.5 min-w-0 flex-1">
        {getTabIcon(type, id)}
        <span className="flex-1 truncate">{title}</span>
      </div>
      {closable && (
        <button
          className="ml-0.5 flex items-center justify-center size-3.5 rounded-sm opacity-0 transition-opacity hover:bg-muted group-hover:opacity-70 hover:!opacity-100"
          onClick={(e) => {
            e.stopPropagation();
            onClose();
          }}
        >
          <X className="size-3" />
        </button>
      )}
    </div>
  );
}

import { useNavigate } from "react-router-dom";
import { useMinappPopup } from "@/hooks/useMinappPopup";
import { MinAppIcon } from "./MinAppIcon";
import type { MinAppType } from "@/types/minapp";
import { cn } from "@/lib/utils";

interface MinAppProps {
  app: MinAppType;
  onClick?: () => void;
  size?: number;
}

/**
 * Individual MinApp tile displayed in the Apps gallery grid.
 *
 * Clicking navigates to `/apps/:id` (top-tab mode) and calls
 * openMinappKeepAlive to add the app to the webview pool.
 *
 * Mirrors cherry-studio's MinApp.tsx component.
 */
export function MinApp({ app, onClick, size = 60 }: MinAppProps) {
  const { openMinappKeepAlive, openedKeepAliveApps, currentAppId } =
    useMinappPopup();
  const navigate = useNavigate();

  const isOpened = openedKeepAliveApps.some((a) => a.id === app.id);
  const isActive = currentAppId === app.id;

  const handleClick = () => {
    // Top-tab mode: navigate to the app detail route
    openMinappKeepAlive(app);
    navigate(`/apps/${app.id}`);
    onClick?.();
  };

  return (
    <button
      className="group flex cursor-pointer flex-col items-center justify-center overflow-hidden border-0 bg-transparent"
      style={{ minHeight: 85 }}
      onClick={handleClick}
    >
      <div className="relative flex items-center justify-center">
        <MinAppIcon app={app} size={size} />
        {isOpened && (
          <div className="absolute -bottom-0.5 -right-0.5 rounded-full bg-background p-[2px]">
            <div
              className={cn(
                "h-1.5 w-1.5 rounded-full bg-green-500",
                !isActive && "animate-pulse"
              )}
            />
          </div>
        )}
      </div>
      <div className="mt-[5px] w-full max-w-[80px] select-none truncate text-center text-xs text-muted-foreground">
        {app.name}
      </div>
    </button>
  );
}

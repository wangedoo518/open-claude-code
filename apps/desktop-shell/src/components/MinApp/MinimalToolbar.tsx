import {
  ArrowLeft,
  ArrowRight,
  RotateCw,
  ExternalLink,
  Minimize2,
} from "lucide-react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import type { MinAppType } from "@/types/minapp";

interface MinimalToolbarProps {
  app: MinAppType;
  onReload?: () => void;
  /** Whether the content under this toolbar is a native component (not iframe) */
  isNative?: boolean;
}

export const TOOLBAR_HEIGHT = 35;

/**
 * Top toolbar shown above the app content in tab mode.
 * Provides navigation controls, reload, and minimize.
 *
 * Mirrors cherry-studio's MinimalToolbar.tsx (35px height).
 */
export function MinimalToolbar({
  app,
  onReload,
  isNative = false,
}: MinimalToolbarProps) {
  const navigate = useNavigate();

  return (
    <div
      className="flex shrink-0 items-center gap-1 border-b border-border bg-muted/30 px-2"
      style={{ height: TOOLBAR_HEIGHT }}
    >
      {/* Navigation buttons (disabled for native apps) */}
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className="size-6"
            disabled={isNative}
          >
            <ArrowLeft className="size-3.5" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>Back</TooltipContent>
      </Tooltip>

      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className="size-6"
            disabled={isNative}
          >
            <ArrowRight className="size-3.5" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>Forward</TooltipContent>
      </Tooltip>

      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className="size-6"
            onClick={onReload}
            disabled={isNative}
          >
            <RotateCw className="size-3.5" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>Reload</TooltipContent>
      </Tooltip>

      {/* App name in the center */}
      <div className="flex-1 text-center">
        <span className="text-xs font-medium text-muted-foreground">
          {app.name}
        </span>
      </div>

      {/* Right side controls */}
      {!isNative && app.url && !app.url.startsWith("warwolf://") && (
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="icon"
              className="size-6"
              onClick={() => window.open(app.url, "_blank")}
            >
              <ExternalLink className="size-3.5" />
            </Button>
          </TooltipTrigger>
          <TooltipContent>Open Externally</TooltipContent>
        </Tooltip>
      )}

      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className="size-6"
            onClick={() => navigate("/apps")}
          >
            <Minimize2 className="size-3.5" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>Back to Apps</TooltipContent>
      </Tooltip>
    </div>
  );
}

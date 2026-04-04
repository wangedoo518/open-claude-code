import { Brain, Coins, PanelLeftOpen, Shield } from "lucide-react";
import { useAppDispatch, useAppSelector } from "@/store";
import { setShowSessionSidebar } from "@/store/slices/settings";
import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { Badge } from "@/components/ui/badge";

interface StatusBarProps {
  modelLabel: string;
  permissionModeLabel: string;
  environmentLabel: string;
  isRunning: boolean;
}

export function StatusBar({
  modelLabel,
  permissionModeLabel,
  environmentLabel,
  isRunning,
}: StatusBarProps) {
  const dispatch = useAppDispatch();
  const showSidebar = useAppSelector((s) => s.settings.showSessionSidebar);

  return (
    <div className="flex h-7 items-center justify-between border-t border-border bg-muted/20 px-3 text-[11px] text-muted-foreground">
      <div className="flex items-center gap-3">
        {!showSidebar && (
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                className="size-5"
                onClick={() => dispatch(setShowSessionSidebar(true))}
              >
                <PanelLeftOpen className="size-3" />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Show Sidebar</TooltipContent>
          </Tooltip>
        )}
        <div className="flex items-center gap-1">
          <Brain className="size-3" />
          <span>{modelLabel}</span>
        </div>
        <div className="flex items-center gap-1">
          <Shield className="size-3" />
          <span>{permissionModeLabel}</span>
        </div>
        <div className="flex items-center gap-1">
          <span>{environmentLabel}</span>
        </div>
      </div>
      <div className="flex items-center gap-3">
        {isRunning && (
          <Badge variant="secondary" className="h-4 px-1.5 text-[10px]">
            Running
          </Badge>
        )}
        <div className="flex items-center gap-1">
          <Coins className="size-3" />
          <span>0 tokens</span>
        </div>
      </div>
    </div>
  );
}

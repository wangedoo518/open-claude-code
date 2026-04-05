/**
 * ProjectPathSelector — allows users to browse and select a project directory.
 *
 * Uses @tauri-apps/plugin-dialog for native folder picker,
 * with fallback text input for when dialog is unavailable.
 */

import { useState, useCallback } from "react";
import {
  FolderOpen,
  ChevronRight,
  X,
  FolderTree,
  Loader2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { useAppDispatch, useAppSelector } from "@/store";
import { setDefaultProjectPath } from "@/store/slices/settings";

interface ProjectPathSelectorProps {
  value?: string;
  onChange?: (path: string) => void;
  /** If true, also persists to Redux settings */
  persistToSettings?: boolean;
  compact?: boolean;
  className?: string;
}

export function ProjectPathSelector({
  value,
  onChange,
  persistToSettings = false,
  compact = false,
  className,
}: ProjectPathSelectorProps) {
  const dispatch = useAppDispatch();
  const storedPath = useAppSelector((s) => s.settings.defaultProjectPath);
  const currentPath = value ?? storedPath ?? "";
  const [isSelecting, setIsSelecting] = useState(false);
  const [showManualInput, setShowManualInput] = useState(false);
  const [manualPath, setManualPath] = useState(currentPath);

  const handleSelect = useCallback(async () => {
    setIsSelecting(true);
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const result = await open({
        directory: true,
        multiple: false,
        title: "Select project directory",
      });

      if (result && !Array.isArray(result)) {
        onChange?.(result);
        if (persistToSettings) {
          dispatch(setDefaultProjectPath(result));
        }
        setManualPath(result);
      }
    } catch {
      // Dialog not available (web mode), show manual input
      setShowManualInput(true);
    } finally {
      setIsSelecting(false);
    }
  }, [onChange, persistToSettings, dispatch]);

  const handleManualSubmit = useCallback(() => {
    const trimmed = manualPath.trim();
    if (trimmed) {
      onChange?.(trimmed);
      if (persistToSettings) {
        dispatch(setDefaultProjectPath(trimmed));
      }
    }
    setShowManualInput(false);
  }, [manualPath, onChange, persistToSettings, dispatch]);

  const handleClear = useCallback(() => {
    onChange?.("");
    if (persistToSettings) {
      dispatch(setDefaultProjectPath(""));
    }
    setManualPath("");
  }, [onChange, persistToSettings, dispatch]);

  if (compact) {
    return (
      <button
        className={cn(
          "flex items-center gap-1.5 rounded-md border border-border/50 px-2 py-1 text-[11px] text-muted-foreground transition-colors hover:bg-accent hover:text-foreground",
          className
        )}
        onClick={() => void handleSelect()}
        disabled={isSelecting}
      >
        {isSelecting ? (
          <Loader2 className="size-3 animate-spin" />
        ) : (
          <FolderOpen className="size-3" />
        )}
        <span className="max-w-[200px] truncate">
          {currentPath ? shortenPath(currentPath) : "Select folder..."}
        </span>
      </button>
    );
  }

  return (
    <div className={cn("space-y-2", className)}>
      {/* Current path display */}
      <div className="flex items-center gap-2">
        <div
          className={cn(
            "flex min-w-0 flex-1 items-center gap-2 rounded-md border px-3 py-2",
            currentPath
              ? "border-border bg-muted/10"
              : "border-dashed border-border/50 bg-muted/5"
          )}
        >
          <FolderTree
            className="size-4 shrink-0"
            style={{
              color: currentPath
                ? "var(--claude-orange, rgb(215,119,87))"
                : "var(--color-muted-foreground)",
            }}
          />
          {currentPath ? (
            <div className="min-w-0 flex-1">
              <div className="truncate text-[12px] font-medium text-foreground">
                {getDirectoryName(currentPath)}
              </div>
              <div className="truncate text-[10px] text-muted-foreground">
                {currentPath}
              </div>
            </div>
          ) : (
            <span className="text-[12px] text-muted-foreground">
              No project directory selected
            </span>
          )}
          {currentPath && (
            <button
              className="rounded p-0.5 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
              onClick={handleClear}
              title="Clear"
            >
              <X className="size-3" />
            </button>
          )}
        </div>
        <Button
          variant="outline"
          size="sm"
          className="gap-1.5 text-[11px]"
          onClick={() => void handleSelect()}
          disabled={isSelecting}
        >
          {isSelecting ? (
            <Loader2 className="size-3 animate-spin" />
          ) : (
            <FolderOpen className="size-3.5" />
          )}
          Browse
        </Button>
      </div>

      {/* Manual path input fallback */}
      {showManualInput && (
        <div className="flex items-center gap-2">
          <input
            type="text"
            value={manualPath}
            onChange={(e) => setManualPath(e.target.value)}
            placeholder="/path/to/project"
            className="flex-1 rounded-md border border-input bg-background px-2.5 py-1.5 font-mono text-[12px] text-foreground outline-none focus:border-ring focus:ring-1 focus:ring-ring/50"
            onKeyDown={(e) => {
              if (e.key === "Enter") handleManualSubmit();
              if (e.key === "Escape") setShowManualInput(false);
            }}
          />
          <Button
            size="sm"
            className="text-[11px]"
            onClick={handleManualSubmit}
          >
            Set
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="text-[11px]"
            onClick={() => setShowManualInput(false)}
          >
            Cancel
          </Button>
        </div>
      )}

      {/* Recent paths */}
      {!showManualInput && !currentPath && (
        <button
          className="flex items-center gap-1 text-[11px] text-muted-foreground transition-colors hover:text-foreground"
          onClick={() => setShowManualInput(true)}
        >
          <ChevronRight className="size-3" />
          Enter path manually
        </button>
      )}
    </div>
  );
}

/* ─── Helpers ─────────────────────────────────────────────────── */

function getDirectoryName(path: string): string {
  const sep = path.includes("\\") ? "\\" : "/";
  const parts = path.split(sep).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

function shortenPath(path: string): string {
  const sep = path.includes("\\") ? "\\" : "/";
  const parts = path.split(sep).filter(Boolean);
  if (parts.length <= 2) return path;
  return `...${sep}${parts.slice(-2).join(sep)}`;
}

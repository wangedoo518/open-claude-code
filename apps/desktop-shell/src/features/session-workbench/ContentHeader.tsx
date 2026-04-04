import { Badge } from "@/components/ui/badge";

interface ContentHeaderProps {
  /** Product title, e.g. "Warwolf" */
  title?: string;
  /** Project path, e.g. "/Users/champion/Documents/develop/..." */
  projectPath?: string;
  /** Model label badge, e.g. "Opus 4.6" */
  modelLabel?: string;
  /** Environment label badge, e.g. "Local" */
  environmentLabel?: string;
  /** Whether the session is currently streaming */
  isStreaming?: boolean;
}

/**
 * Content area header — Claude Code desktop style.
 *
 * Sits at the top of the code terminal area (below the top bar).
 * Transparent background, no border/shadow — merges visually with content.
 *
 * Layout:
 *   [Warwolf]                                [Opus 4.6] [Local]
 *   /path/to/project                         [● streaming]
 */
export function ContentHeader({
  title = "Warwolf",
  projectPath,
  modelLabel = "Opus 4.6",
  environmentLabel = "Local",
  isStreaming = false,
}: ContentHeaderProps) {
  return (
    <div className="flex items-start justify-between px-4 pb-2 pt-3">
      {/* Left: title + project path */}
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <h1 className="text-sm font-semibold text-foreground">{title}</h1>
          {isStreaming && (
            <span className="flex items-center gap-1 text-[10px] text-primary">
              <span className="inline-block size-1.5 animate-pulse rounded-full bg-primary" />
              Streaming
            </span>
          )}
        </div>
        {projectPath && (
          <p className="mt-0.5 truncate text-xs text-muted-foreground">
            {projectPath}
          </p>
        )}
      </div>

      {/* Right: badges */}
      <div className="flex shrink-0 items-center gap-1.5 pt-0.5">
        <Badge
          variant="secondary"
          className="h-5 rounded-md px-2 text-[10px] font-medium"
        >
          {modelLabel}
        </Badge>
        <Badge
          variant="outline"
          className="h-5 rounded-md px-2 text-[10px] font-medium"
        >
          {environmentLabel}
        </Badge>
      </div>
    </div>
  );
}

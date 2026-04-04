import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

interface SettingGroupProps {
  title: string;
  description?: string;
  children: ReactNode;
  className?: string;
}

export function SettingGroup({
  title,
  description,
  children,
  className,
}: SettingGroupProps) {
  return (
    <div className={cn("rounded-lg border border-border p-4", className)}>
      <div className="mb-3">
        <h3 className="text-sm font-medium text-foreground">{title}</h3>
        {description && (
          <p className="mt-0.5 text-xs text-muted-foreground">{description}</p>
        )}
      </div>
      <div className="space-y-3">{children}</div>
    </div>
  );
}

interface SettingRowProps {
  label: string;
  description?: string;
  children: ReactNode;
  className?: string;
}

export function SettingRow({
  label,
  description,
  children,
  className,
}: SettingRowProps) {
  return (
    <div
      className={cn(
        "flex items-center justify-between gap-4 py-1",
        className
      )}
    >
      <div className="flex-1">
        <div className="text-sm text-foreground">{label}</div>
        {description && (
          <div className="text-xs text-muted-foreground">{description}</div>
        )}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

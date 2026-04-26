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
    <section className={cn("settings-lite-section", className)}>
      <div className="settings-lite-section-head">
        <h3 className="settings-lite-section-title">{title}</h3>
        {description && (
          <p className="settings-lite-section-desc">{description}</p>
        )}
      </div>
      <div className="settings-lite-section-body">{children}</div>
    </section>
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
        "settings-lite-row",
        className
      )}
    >
      <div className="settings-lite-row-copy">
        <div className="settings-lite-row-label">{label}</div>
        {description && (
          <div className="settings-lite-row-desc">{description}</div>
        )}
      </div>
      <div className="settings-lite-row-control">{children}</div>
    </div>
  );
}

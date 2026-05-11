import { ArrowRight, Sparkles } from "lucide-react";
import { Link } from "react-router-dom";
import type { Observation } from "./today-observation";

/**
 * Slice E28.3 — Home hero observation card.
 *
 * Sits between the page header and Top 3 actions. Single soft-
 * tinted card that surfaces ONE in-context observation per visit
 * (computed by pickObservation). The tint is kept restrained:
 * 6% opacity on the bg, 35% on the border, full on the icon — so
 * the card reads as "a friendly note", not "an alert".
 *
 * Renders nothing for the "default" observation when caller wants
 * to omit a placeholder; pass `hideDefault` to opt in.
 */
export function TodayObservationCard({
  observation,
  hideDefault = false,
}: {
  observation: Observation;
  hideDefault?: boolean;
}) {
  if (hideDefault && observation.id === "default") return null;

  // Map observation tone to CSS color tokens.
  const toneToken =
    observation.tone === "warning"
      ? "color-warning"
      : observation.tone === "success"
        ? "color-success"
        : observation.tone === "primary"
          ? "color-primary"
          : "color-muted-foreground";

  const content = (
    <>
      <div
        className="grid size-9 shrink-0 place-items-center rounded-full"
        style={{
          backgroundColor: `color-mix(in srgb, var(--${toneToken}) 18%, transparent)`,
          color: `var(--${toneToken})`,
        }}
        aria-hidden="true"
      >
        <Sparkles className="size-4" />
      </div>
      <div className="min-w-0 flex-1">
        <div className="text-[10px] uppercase tracking-[0.14em] text-muted-foreground">
          今日观察
        </div>
        <div className="mt-0.5 text-[13px] leading-snug text-foreground">
          {observation.message}
        </div>
      </div>
      {observation.href ? (
        <ArrowRight
          className="size-4 shrink-0 text-muted-foreground transition-transform group-hover:translate-x-0.5"
          aria-hidden="true"
        />
      ) : null}
    </>
  );

  const className = "group flex items-center gap-3 rounded-lg border px-4 py-3";
  const style = {
    borderColor: `color-mix(in srgb, var(--${toneToken}) 35%, var(--color-border))`,
    backgroundColor: `color-mix(in srgb, var(--${toneToken}) 6%, var(--color-card))`,
  };

  if (observation.href) {
    return (
      <Link
        to={observation.href}
        className={`${className} hover:shadow-sm`}
        style={style}
      >
        {content}
      </Link>
    );
  }
  return (
    <div className={className} style={style}>
      {content}
    </div>
  );
}

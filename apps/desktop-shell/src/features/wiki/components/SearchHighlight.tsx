import { cn } from "@/lib/utils";

interface SearchHighlightProps {
  text: string;
  query: string;
  className?: string;
}

/**
 * Highlight matched substrings within text using <mark>.
 * Case-insensitive. Safe for arbitrary user input via regex escaping.
 */
export function SearchHighlight({ text, query, className }: SearchHighlightProps) {
  const trimmedQuery = query.trim();
  if (!trimmedQuery) {
    return <span className={className}>{text}</span>;
  }

  const escapedQuery = escapeRegex(trimmedQuery);
  const splitRegex = new RegExp(`(${escapedQuery})`, "gi");
  const matchRegex = new RegExp(`^${escapedQuery}$`, "i");
  const parts = text.split(splitRegex);

  return (
    <span className={className}>
      {parts.map((part, index) => {
        if (!part) return null;
        if (matchRegex.test(part)) {
          return (
            <mark
              key={`${part}-${index}`}
              className={cn(
                "rounded px-0.5",
                "bg-amber-100 text-amber-900",
                "dark:bg-amber-900/40 dark:text-amber-100",
              )}
            >
              {part}
            </mark>
          );
        }
        return <span key={`${part}-${index}`}>{part}</span>;
      })}
    </span>
  );
}

function escapeRegex(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

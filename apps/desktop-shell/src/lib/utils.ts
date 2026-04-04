import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export function generateId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
}

export function formatRelativeDate(timestamp: number): string {
  const now = Date.now();
  const diff = now - timestamp;
  const day = 86400000;

  if (diff < day) return "Today";
  if (diff < day * 2) return "Yesterday";
  if (diff < day * 7) return "This Week";
  return "Older";
}

export function truncate(str: string, maxLen: number): string {
  if (str.length <= maxLen) return str;
  return str.slice(0, maxLen - 1) + "\u2026";
}

export function platformModKey(): string {
  return navigator.userAgent.includes("Mac") ? "\u2318" : "Ctrl";
}

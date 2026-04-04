import type { CSSProperties } from "react";

export interface MinAppType {
  /** Unique identifier for the app */
  id: string;
  /** Display name */
  name: string;
  /** i18n key (optional) */
  nameKey?: string;
  /** URL loaded inside webview/iframe */
  url: string;
  /** Logo image URL or icon component name */
  logo: string;
  /** App type */
  type?: "builtin" | "custom";
  /** Whether icon needs a border */
  bordered?: boolean;
  /** Custom icon style */
  style?: CSSProperties;
  /** Gradient string for the icon background */
  gradient?: string;
  /** Description text */
  description?: string;
  /** Icon component name (for lucide icons) */
  iconName?: string;
  /** Timestamp when custom app was added */
  addTime?: string;
}

export interface WebviewState {
  loaded: boolean;
  url: string | null;
}

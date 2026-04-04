import type { SVGProps } from "react";

// Source-aligned with cherry-studio:
// /Users/champion/Documents/develop/Warwolf/cherry-studio/src/renderer/src/components/Icons/SVGIcon.tsx
// LaunchpadPage uses OpenClawIcon, while the sidebar uses OpenClawSidebarIcon.
export function CherryOpenClawIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width="24"
      height="24"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...props}
    >
      <path d="M14.4 5.2a5.3 5.3 0 0 1 2.022-2.548L16.8 2.4" />
      <path d="M14.56 8.933v1" />
      <path d="m15.1 18.933.81 2.65" />
      <path d="M18.56 8.433c.833.333 2 1 2 2" />
      <path d="M5.56 8.433c-.833.333-2 1-2 2" />
      <path d="m7.91 18.933-.81 2.65" />
      <path d="M9.56 8.933v1" />
      <path d="M9.6 5.2a5.3 5.3 0 0 0-2.022-2.548L7.2 2.4" />
      <circle cx="12" cy="12" r="7.2" />
    </svg>
  );
}

export function CherryOpenClawSidebarIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width="24"
      height="24"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      {...props}
    >
      <path d="M14.4 5.2a5.3 5.3 0 0 1 2.022-2.548L16.8 2.4" />
      <path d="M14.56 8.933v1" />
      <path d="m15.1 18.933.81 2.65" />
      <path d="M18.56 8.433c.833.333 2 1 2 2" />
      <path d="M5.56 8.433c-.833.333-2 1-2 2" />
      <path d="m7.91 18.933-.81 2.65" />
      <path d="M9.56 8.933v1" />
      <path d="M9.6 5.2a5.3 5.3 0 0 0-2.022-2.548L7.2 2.4" />
      <circle cx="12" cy="12" r="7.2" />
    </svg>
  );
}

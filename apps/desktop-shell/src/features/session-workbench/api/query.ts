export const sessionWorkbenchKeys = {
  all: ["desktop-session"] as const,
  detail: (sessionId: string | null | undefined) =>
    ["desktop-session", sessionId ?? "missing"] as const,
};

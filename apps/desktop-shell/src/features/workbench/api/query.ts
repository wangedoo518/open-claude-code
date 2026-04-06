export const workbenchKeys = {
  all: ["desktop-workbench"] as const,
  root: () => ["desktop-workbench"] as const,
  search: (query: string) => ["desktop-search", query] as const,
  scheduled: () => ["desktop-scheduled"] as const,
  dispatch: () => ["desktop-dispatch"] as const,
  customize: () => ["desktop-customize"] as const,
};

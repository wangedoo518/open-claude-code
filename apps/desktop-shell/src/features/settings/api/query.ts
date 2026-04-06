export const settingsKeys = {
  bootstrap: () => ["desktop-bootstrap"] as const,
  settings: () => ["desktop-settings"] as const,
  customize: () => ["desktop-customize"] as const,
  managedAuthProviders: () => ["managed-auth-providers"] as const,
  managedAuthAccounts: (providerId: string) =>
    ["managed-auth-accounts", providerId] as const,
  codexRuntime: () => ["codex-runtime"] as const,
  codexAuthOverview: () => ["codex-auth-overview"] as const,
};

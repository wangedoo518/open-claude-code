export const kefuQueryKeys = {
  status: () => ["kefu", "status"] as const,
  config: () => ["kefu", "config"] as const,
  pipeline: () => ["kefu", "pipeline"] as const,
  contactUrl: () => ["kefu", "contact-url"] as const,
};

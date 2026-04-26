export const DEFAULT_PROFILE_NAME = "Pumbaa";
export const PROFILE_NAME_STORAGE_KEY = "outer-brain.profile-name";
export const PROFILE_NAME_CHANGE_EVENT = "outer-brain:profile-name-change";

export function normalizeProfileName(value: string | null | undefined): string | null {
  const normalized = value?.trim().replace(/\s+/g, " ");
  if (!normalized) return null;
  return normalized.slice(0, 24);
}

export function getStoredProfileName(): string {
  if (typeof window === "undefined") return DEFAULT_PROFILE_NAME;
  return (
    normalizeProfileName(window.localStorage.getItem(PROFILE_NAME_STORAGE_KEY)) ??
    DEFAULT_PROFILE_NAME
  );
}

export function saveStoredProfileName(value: string): string {
  const nextName = normalizeProfileName(value) ?? DEFAULT_PROFILE_NAME;
  if (typeof window !== "undefined") {
    window.localStorage.setItem(PROFILE_NAME_STORAGE_KEY, nextName);
    window.dispatchEvent(
      new CustomEvent(PROFILE_NAME_CHANGE_EVENT, { detail: { name: nextName } }),
    );
  }
  return nextName;
}

export function subscribeProfileName(callback: (name: string) => void): () => void {
  if (typeof window === "undefined") return () => {};

  const handleStorage = (event: StorageEvent) => {
    if (event.key === PROFILE_NAME_STORAGE_KEY) {
      callback(normalizeProfileName(event.newValue) ?? DEFAULT_PROFILE_NAME);
    }
  };

  const handleCustom = (event: Event) => {
    const detail = (event as CustomEvent<{ name?: string }>).detail;
    callback(normalizeProfileName(detail?.name) ?? getStoredProfileName());
  };

  window.addEventListener("storage", handleStorage);
  window.addEventListener(PROFILE_NAME_CHANGE_EVENT, handleCustom);

  return () => {
    window.removeEventListener("storage", handleStorage);
    window.removeEventListener(PROFILE_NAME_CHANGE_EVENT, handleCustom);
  };
}

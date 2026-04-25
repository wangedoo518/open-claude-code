import { invoke } from "@tauri-apps/api/core";

const DEFAULT_API_BASE = "http://127.0.0.1:4357";
const ENV_API_BASE = import.meta.env.VITE_DESKTOP_API_BASE;

let apiBasePromise: Promise<string> | null = null;

export async function getDesktopApiBase(): Promise<string> {
  if (ENV_API_BASE) {
    return ENV_API_BASE;
  }

  if (!apiBasePromise) {
    apiBasePromise = (async () => {
      try {
        return await invoke<string>("desktop_server_ensure");
      } catch {
        try {
          return await invoke<string>("desktop_api_base");
        } catch {
          return DEFAULT_API_BASE;
        }
      }
    })();
  }

  return apiBasePromise;
}

export function resetDesktopApiBaseCache() {
  apiBasePromise = null;
}

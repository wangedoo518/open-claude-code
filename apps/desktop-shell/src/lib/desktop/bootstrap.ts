import { invoke } from "@tauri-apps/api/core";

// In dev mode (Vite hot reload, no Tauri), an empty base means
// requests stay relative (e.g. `/api/wiki/...`) and Vite's `proxy`
// config in `vite.config.ts` forwards them to the backend. This
// avoids the dual-base trap where some calls go through the proxy
// and others hit a hardcoded `127.0.0.1:4357` that isn't running.
//
// In production (Tauri), the IPC bridge resolves the actual port the
// sidecar bound to (which can differ from any default), so the
// `desktop_server_ensure` invoke wins.
const DEV_RELATIVE_BASE = "";
const PROD_DEFAULT_BASE = "http://127.0.0.1:4357";
const ENV_API_BASE = import.meta.env.VITE_DESKTOP_API_BASE;
const IS_DEV = import.meta.env.DEV === true;

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
          // No Tauri bridge. In dev, return "" so Vite proxy handles
          // every `/api/...` URL. In a production build that somehow
          // runs without Tauri, fall back to the historical default
          // so behavior matches pre-fix builds.
          return IS_DEV ? DEV_RELATIVE_BASE : PROD_DEFAULT_BASE;
        }
      }
    })();
  }

  return apiBasePromise;
}

export function resetDesktopApiBaseCache() {
  apiBasePromise = null;
}

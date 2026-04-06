import { getDesktopApiBase, resetDesktopApiBaseCache } from "./bootstrap";

async function readError(response: Response): Promise<string> {
  try {
    const payload = (await response.json()) as { error?: string };
    if (payload.error) {
      return payload.error;
    }
  } catch {
    // Fall back to reading response text.
  }

  try {
    const text = await response.text();
    if (text) {
      return text;
    }
  } catch {
    // Ignore text parse failure too.
  }

  return `Request failed with status ${response.status}`;
}

function isRetryableNetworkError(error: unknown): boolean {
  if (!(error instanceof Error)) {
    return false;
  }
  const message = error.message.toLowerCase();
  return (
    message.includes("failed to fetch") ||
    message.includes("networkerror") ||
    message.includes("network request failed") ||
    message.includes("load failed")
  );
}

export async function fetchJson<T>(
  path: string,
  init?: RequestInit,
  timeout = 30_000
): Promise<T> {
  const base = await getDesktopApiBase();
  const attempt = async (requestBase: string): Promise<T> => {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeout);

    try {
      const response = await fetch(`${requestBase}${path}`, {
        ...init,
        signal: init?.signal ?? controller.signal,
        headers: {
          Accept: "application/json",
          ...(init?.body ? { "Content-Type": "application/json" } : {}),
          ...(init?.headers ?? {}),
        },
      });

      if (!response.ok) {
        throw new Error(await readError(response));
      }

      return (await response.json()) as T;
    } finally {
      clearTimeout(timer);
    }
  };

  try {
    return await attempt(base);
  } catch (error) {
    if (!isRetryableNetworkError(error)) {
      throw error;
    }

    resetDesktopApiBaseCache();
    const ensuredBase = await getDesktopApiBase();
    return attempt(ensuredBase);
  }
}

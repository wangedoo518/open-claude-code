/**
 * Global webview state manager.
 *
 * Tracks loaded state per app-id so that MinAppPage, MinAppTabsPool
 * and MinimalToolbar can all check / subscribe without coupling to
 * Redux or React context.
 *
 * Ported from cherry-studio's webviewStateManager.ts.
 */

type Listener = (loaded: boolean) => void;

const states = new Map<string, boolean>();
const listeners = new Map<string, Set<Listener>>();

export function setWebviewLoaded(appId: string, loaded: boolean) {
  states.set(appId, loaded);
  const subs = listeners.get(appId);
  if (subs) {
    for (const fn of subs) {
      fn(loaded);
    }
  }
}

export function getWebviewLoaded(appId: string): boolean {
  return states.get(appId) ?? false;
}

export function clearWebviewState(appId: string) {
  states.delete(appId);
  listeners.delete(appId);
}

/**
 * Subscribe to loaded state changes for a given app.
 * Returns an unsubscribe function.
 */
export function onWebviewStateChange(
  appId: string,
  listener: Listener
): () => void {
  if (!listeners.has(appId)) {
    listeners.set(appId, new Set());
  }
  listeners.get(appId)!.add(listener);
  return () => {
    const subs = listeners.get(appId);
    if (subs) {
      subs.delete(listener);
      if (subs.size === 0) listeners.delete(appId);
    }
  };
}

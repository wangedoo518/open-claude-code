import type { DesktopSessionEvent } from "@/api/contracts/desktop";

export type AbsorbEvent = Extract<
  DesktopSessionEvent,
  { type: "absorb_progress" | "absorb_complete" }
>;

function isAbsorbEvent(event: DesktopSessionEvent): event is AbsorbEvent {
  return event.type === "absorb_progress" || event.type === "absorb_complete";
}

/**
 * Subscribe to the session-agnostic absorb SSE stream.
 *
 * This deliberately does not depend on Ask session events: `/wiki`
 * users can trigger maintenance before any Ask session exists, and the
 * progress UI should not create an empty conversation just to hold SSE.
 */
export function subscribeToAbsorbEvents(
  onEvent: (event: AbsorbEvent) => void,
  onError?: (error: Error) => void,
): AbortController {
  const controller = new AbortController();

  void (async () => {
    const { getDesktopApiBase } = await import("@/lib/desktop/bootstrap");
    const base = await getDesktopApiBase();

    try {
      const response = await fetch(`${base}/api/wiki/absorb/events`, {
        signal: controller.signal,
        headers: { Accept: "text/event-stream" },
      });

      if (!response.ok || !response.body) {
        onError?.(new Error(`Absorb SSE failed: ${response.status}`));
        return;
      }

      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";
      let dataLines: string[] = [];

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split("\n");
        buffer = lines.pop() ?? "";

        for (const line of lines) {
          if (line.startsWith("data: ")) {
            dataLines.push(line.slice(6));
            continue;
          }

          if (line === "" && dataLines.length > 0) {
            const json = dataLines.join("\n");
            dataLines = [];
            try {
              const event = JSON.parse(json) as DesktopSessionEvent;
              if (isAbsorbEvent(event)) onEvent(event);
            } catch (err) {
              console.warn("[absorb-sse] dropped malformed event", err);
            }
          }
        }
      }

      if (!controller.signal.aborted) {
        onError?.(new Error("Absorb SSE closed"));
      }
    } catch (err) {
      if ((err as Error).name !== "AbortError") {
        onError?.(err instanceof Error ? err : new Error(String(err)));
      }
    }
  })();

  return controller;
}

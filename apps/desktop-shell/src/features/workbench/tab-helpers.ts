import type { AppDispatch } from "@/store";
import { setViewMode } from "@/store/slices/ui";

export function openHomeSession(
  dispatch: AppDispatch,
  sessionId: string | null
) {
  if (sessionId) {
    dispatch(setViewMode({ kind: "session", sessionId }));
  } else {
    dispatch(setViewMode({ kind: "nav", section: "session" }));
  }
}

export function openHomeOverview(dispatch: AppDispatch) {
  dispatch(setViewMode({ kind: "nav", section: "overview" }));
}

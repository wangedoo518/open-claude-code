import type { AppDispatch } from "@/store";
import { setActiveTab } from "@/store/slices/tabs";
import { setActiveHomeSessionId, setHomeSection } from "@/store/slices/ui";

export function openHomeSession(
  dispatch: AppDispatch,
  sessionId: string | null
) {
  dispatch(setActiveTab("home"));
  dispatch(setActiveHomeSessionId(sessionId));
  dispatch(setHomeSection("session"));
}

export function openHomeOverview(dispatch: AppDispatch) {
  dispatch(setActiveTab("home"));
  dispatch(setActiveHomeSessionId(null));
  dispatch(setHomeSection("overview"));
}

import { useAppSelector, useAppDispatch } from "@/store";
import {
  setEnabledApps,
  setDisabledApps,
  setPinnedApps,
} from "@/store/slices/minapps";
import type { MinAppType } from "@/types/minapp";

/**
 * Hook for reading and managing the MinApp catalog.
 * Mirrors cherry-studio's useMinapps.ts
 */
export function useMinapps() {
  const dispatch = useAppDispatch();
  const minapps = useAppSelector((s) => s.minapps.enabled);
  const disabled = useAppSelector((s) => s.minapps.disabled);
  const pinned = useAppSelector((s) => s.minapps.pinned);

  const updateMinapps = (apps: MinAppType[]) => {
    dispatch(setEnabledApps(apps));
  };

  const updateDisabledMinapps = (apps: MinAppType[]) => {
    dispatch(setDisabledApps(apps));
  };

  const updatePinnedMinapps = (apps: MinAppType[]) => {
    dispatch(setPinnedApps(apps));
  };

  return {
    minapps,
    disabled,
    pinned,
    updateMinapps,
    updateDisabledMinapps,
    updatePinnedMinapps,
  };
}

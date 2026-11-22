import type { Settings, SettingsPatch } from "@/lib/ipc";

import { useCallback, useSyncExternalStore } from "react";

import { getSettings, updateSettings } from "@/lib/ipc";
import { createIpcStore } from "@/lib/ipc/store";

const settingsStore = createIpcStore(
  getSettings,
  "settings:changed",
  (settings) => settings
);

/**
 * The current settings (`undefined` until loaded) and a function to change
 * them. Every component in the window shares one copy, kept current by
 * `settings:changed`.
 */
export function useSettings(): {
  settings: Settings | undefined;
  update: (patch: SettingsPatch) => Promise<Settings>;
} {
  const settings = useSyncExternalStore(
    settingsStore.subscribe,
    settingsStore.getSnapshot
  );
  const update = useCallback(async (patch: SettingsPatch) => {
    const next = await updateSettings(patch);
    settingsStore.set(next);
    return next;
  }, []);
  return { settings, update };
}

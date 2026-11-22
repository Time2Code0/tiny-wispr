import type { Settings, SettingsPatch } from "@/lib/ipc";

import { useCallback, useOptimistic, useTransition } from "react";

import { toastManager } from "@/components/ui/toast";
import { useSettings } from "@/hooks/use-settings";

const merge = (current: Settings | undefined, patch: SettingsPatch) =>
  current ? { ...current, ...patch } : current;

/**
 * Settings that save as soon as they change: the new value shows at once and
 * snaps back, with a toast, if the backend refuses it.
 */
export function useSavedSettings(): {
  save: (patch: SettingsPatch) => void;
  settings: Settings | undefined;
} {
  const { settings, update } = useSettings();
  const [optimistic, apply] = useOptimistic(settings, merge);
  const [, startTransition] = useTransition();

  const save = useCallback(
    (patch: SettingsPatch) => {
      startTransition(async () => {
        apply(patch);
        try {
          await update(patch);
        } catch (error) {
          toastManager.add({
            description: error instanceof Error ? error.message : undefined,
            title: "Couldn't save that change",
            type: "error",
          });
        }
      });
    },
    [apply, update]
  );

  return { save, settings: optimistic };
}

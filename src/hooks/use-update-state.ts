import type { UpdateState } from "@/lib/ipc";

import { useSyncExternalStore } from "react";

import { getUpdateState } from "@/lib/ipc";
import { createIpcStore } from "@/lib/ipc/store";

const IDLE: UpdateState = { state: "idle" };

const updateStore = createIpcStore(
  getUpdateState,
  "updates:state",
  (state) => state
);

/** What the updater is doing, kept current by `updates:state`. */
export function useUpdateState(): UpdateState {
  return (
    useSyncExternalStore(updateStore.subscribe, updateStore.getSnapshot) ?? IDLE
  );
}

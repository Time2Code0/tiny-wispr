import type { DictationState } from "@/lib/ipc";

import { useSyncExternalStore } from "react";

import { getDictationState } from "@/lib/ipc";
import { createIpcStore } from "@/lib/ipc/store";

const IDLE: DictationState = {
  elapsedMs: 0,
  mode: null,
  notice: null,
  phase: "idle",
  stage: null,
};

const dictationStore = createIpcStore(
  getDictationState,
  "dictation:state",
  (state) => state
);

/** The pill's state, kept current by `dictation:state`. */
export function useDictationState(): DictationState {
  return (
    useSyncExternalStore(
      dictationStore.subscribe,
      dictationStore.getSnapshot
    ) ?? IDLE
  );
}

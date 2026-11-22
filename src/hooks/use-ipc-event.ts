import type { EventName, Events } from "@/lib/ipc";

import { useEffect, useRef } from "react";

import { on } from "@/lib/ipc";

/** Calls `handler` for every `event` while the component is mounted. */
export function useIpcEvent<E extends EventName>(
  event: E,
  handler: (payload: Events[E]) => void
): void {
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  useEffect(() => on(event, (payload) => handlerRef.current(payload)), [event]);
}

import type { EventName, Events } from "./types";

import { on } from "./index";

export interface IpcStore<T> {
  getSnapshot: () => T | undefined;
  /** Re-reads the value from the backend. */
  refresh: () => Promise<void>;
  set: (value: T) => void;
  subscribe: (onChange: () => void) => () => void;
}

/**
 * A value loaded once from the backend and kept current by an event, shared
 * by every component in the window. Read it with `useSyncExternalStore`.
 */
export function createIpcStore<T, E extends EventName>(
  load: () => Promise<T>,
  event: E,
  fromEvent: (payload: Events[E]) => T
): IpcStore<T> {
  let value: T | undefined;
  let started = false;
  let version = 0;
  const subscribers = new Set<() => void>();

  const set = (next: T) => {
    version += 1;
    value = next;
    for (const notify of subscribers) {
      notify();
    }
  };

  const refresh = async () => {
    const before = version;
    const next = await load();
    // An event that arrived while loading is newer than the loaded value.
    if (version === before) {
      set(next);
    }
  };

  const start = () => {
    if (started) {
      return;
    }
    started = true;
    on(event, (payload) => set(fromEvent(payload)));
    refresh().catch((error: unknown) => {
      console.error(`Could not load the initial value for ${event}`, error);
    });
  };

  return {
    getSnapshot: () => value,
    refresh,
    set,
    subscribe(onChange) {
      start();
      subscribers.add(onChange);
      return () => {
        subscribers.delete(onChange);
      };
    },
  };
}

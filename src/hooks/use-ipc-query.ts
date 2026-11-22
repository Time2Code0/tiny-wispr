import type { EventName } from "@/lib/ipc";

import { useCallback, useEffect, useRef, useState } from "react";

import { on } from "@/lib/ipc";
import { IpcError } from "@/lib/ipc/errors";

export interface IpcQuery<T> {
  data: T | undefined;
  error: IpcError | undefined;
  loading: boolean;
  refetch: () => Promise<void>;
}

/**
 * Loads `fetcher()` on mount and again whenever one of `refetchOn` fires.
 * Pass a stable `fetcher` (a command from `@/lib/ipc` or a `useCallback`).
 *
 * @example
 * const models = useIpcQuery(listModels, ["models:changed"]);
 */
export function useIpcQuery<T>(
  fetcher: () => Promise<T>,
  refetchOn: readonly EventName[] = []
): IpcQuery<T> {
  const [data, setData] = useState<T>();
  const [error, setError] = useState<IpcError>();
  const [loading, setLoading] = useState(true);
  const request = useRef(0);

  const refetch = useCallback(async () => {
    request.current += 1;
    const id = request.current;
    setLoading(true);
    try {
      const next = await fetcher();
      if (id === request.current) {
        setData(next);
        setError(undefined);
      }
    } catch (caught) {
      if (id === request.current) {
        setError(IpcError.from(caught));
      }
    } finally {
      if (id === request.current) {
        setLoading(false);
      }
    }
  }, [fetcher]);

  useEffect(() => {
    refetch();
  }, [refetch]);

  const eventsKey = refetchOn.join(" ");
  useEffect(() => {
    if (!eventsKey) {
      return;
    }
    const unsubscribes = (eventsKey.split(" ") as EventName[]).map((event) =>
      on(event, () => {
        refetch();
      })
    );
    return () => {
      for (const unsubscribe of unsubscribes) {
        unsubscribe();
      }
    };
  }, [eventsKey, refetch]);

  return { data, error, loading, refetch };
}

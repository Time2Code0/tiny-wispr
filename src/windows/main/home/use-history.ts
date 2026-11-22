import type { HistoryEntry } from "@/lib/ipc";

import { useCallback, useEffect, useRef, useState } from "react";

import { listHistory } from "@/lib/ipc";

export interface HistoryList {
  entries: HistoryEntry[] | undefined;
  error: boolean;
  loadingMore: boolean;
  loadMore: () => void;
  more: boolean;
}

/**
 * History matching `query`, a page at a time. Starts over when the query
 * changes or `history:changed` fires (`version`).
 */
export function useHistory(query: string, version: number): HistoryList {
  const [entries, setEntries] = useState<HistoryEntry[]>();
  const [cursor, setCursor] = useState<string | null>(null);
  const [error, setError] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const request = useRef(0);

  // biome-ignore lint/correctness/useExhaustiveDependencies: `version` reloads after history:changed
  useEffect(() => {
    request.current += 1;
    const id = request.current;
    listHistory(query.trim() || null, null)
      .then((page) => {
        if (id === request.current) {
          setEntries(page.entries);
          setCursor(page.nextCursor);
          setError(false);
        }
      })
      .catch(() => {
        if (id === request.current) {
          setError(true);
        }
      });
  }, [query, version]);

  const loadMore = useCallback(() => {
    if (!cursor) {
      return;
    }
    const id = request.current;
    setLoadingMore(true);
    listHistory(query.trim() || null, cursor)
      .then((page) => {
        if (id === request.current) {
          setEntries((previous) => [...(previous ?? []), ...page.entries]);
          setCursor(page.nextCursor);
        }
      })
      .catch(() => {
        if (id === request.current) {
          setError(true);
        }
      })
      .finally(() => setLoadingMore(false));
  }, [cursor, query]);

  return { entries, error, loadingMore, loadMore, more: cursor !== null };
}

/** `value`, once it has stopped changing for `ms`. */
export function useDebounced<T>(value: T, ms: number): T {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setSettled(value), ms);
    return () => clearTimeout(timer);
  }, [value, ms]);
  return settled;
}

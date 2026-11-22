import { useCallback, useSyncExternalStore } from "react";

function subscribe(onChange: () => void) {
  window.addEventListener("hashchange", onChange);
  return () => window.removeEventListener("hashchange", onChange);
}

const getHash = () => window.location.hash;

const HASH_PREFIX = /^#\/?/;

/**
 * A tiny hash router: `#/settings` → `"settings"`. Unknown hashes resolve to
 * `fallback`.
 */
export function useHashRoute<T extends string>(
  routes: readonly T[],
  fallback: T
): [route: T, navigate: (route: T) => void] {
  const hash = useSyncExternalStore(subscribe, getHash);
  const name = hash.replace(HASH_PREFIX, "");
  const route = routes.find((r) => r === name) ?? fallback;
  const navigate = useCallback((next: T) => {
    window.location.hash = `/${next}`;
  }, []);
  return [route, navigate];
}

import type { MainRoute } from "@/lib/ipc";

import { useCallback, useSyncExternalStore } from "react";

import { MAIN_ROUTES } from "@/lib/ipc/types";

export const SETTINGS_SECTIONS = [
  "general",
  "system",
  "privacy",
  "about",
] as const;
export type SettingsSection = (typeof SETTINGS_SECTIONS)[number];

export interface Location {
  route: MainRoute;
  /** Only set on the settings page. */
  section: SettingsSection | null;
}

const HASH_PREFIX = /^#\/?/;

/** `#/settings/privacy` → settings, privacy. Anything unknown is Home. */
export function parseHash(hash: string): Location {
  const [first, second] = hash.replace(HASH_PREFIX, "").split("/");
  const route = MAIN_ROUTES.find((r) => r === first) ?? "home";
  if (route !== "settings") {
    return { route, section: null };
  }
  const section = SETTINGS_SECTIONS.find((s) => s === second) ?? "general";
  return { route, section };
}

export function hashFor(route: MainRoute, section?: SettingsSection): string {
  if (route === "settings" && section) {
    return `#/settings/${section}`;
  }
  return `#/${route}`;
}

export const HOME_TABS = ["history", "activity"] as const;
export type HomeTab = (typeof HOME_TABS)[number];

/** `#/home/activity` → activity. Anything else on Home is the history. */
export function homeTabFromHash(hash: string): HomeTab {
  const [first, second] = hash.replace(HASH_PREFIX, "").split("/");
  return first === "home" && second === "activity" ? "activity" : "history";
}

export function homeHash(tab: HomeTab): string {
  return tab === "history" ? "#/home" : `#/home/${tab}`;
}

function subscribe(onChange: () => void) {
  window.addEventListener("hashchange", onChange);
  return () => window.removeEventListener("hashchange", onChange);
}

const getHash = () => window.location.hash;

/**
 * The page in the hash, so back and forward, reloads and deep links like
 * `#/settings/privacy` all work.
 */
export function useLocation(): [
  location: Location,
  navigate: (route: MainRoute, section?: SettingsSection) => void,
] {
  const location = parseHash(useSyncExternalStore(subscribe, getHash));
  const navigate = useCallback(
    (route: MainRoute, section?: SettingsSection) => {
      window.location.hash = hashFor(route, section);
    },
    []
  );
  return [location, navigate];
}

/** Home's tab, kept in the hash so reloads and back and forward keep it. */
export function useHomeTab(): [tab: HomeTab, select: (tab: HomeTab) => void] {
  const tab = homeTabFromHash(useSyncExternalStore(subscribe, getHash));
  const select = useCallback((next: HomeTab) => {
    window.location.hash = homeHash(next);
  }, []);
  return [tab, select];
}

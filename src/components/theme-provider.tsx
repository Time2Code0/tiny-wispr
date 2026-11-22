import type { Theme } from "@/lib/ipc";

import {
  createContext,
  use,
  useCallback,
  useLayoutEffect,
  useMemo,
} from "react";

import { useMediaQuery } from "@/hooks/use-media-query";
import { useSettings } from "@/hooks/use-settings";

type ResolvedTheme = "light" | "dark";

interface ThemeContextValue {
  resolvedTheme: ResolvedTheme;
  setTheme: (theme: Theme) => void;
  theme: Theme;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

/**
 * Applies the `theme` setting (system | light | dark) as the `.dark` class on
 * `<html>`, following `prefers-color-scheme` live when set to system.
 * `forcedTheme` pins a window to one theme (the pill is always dark).
 */
export function ThemeProvider({
  children,
  forcedTheme,
}: {
  children: React.ReactNode;
  forcedTheme?: ResolvedTheme;
}) {
  const { settings, update } = useSettings();
  const systemDark = useMediaQuery("(prefers-color-scheme: dark)");
  const theme: Theme = forcedTheme ?? settings?.theme ?? "system";
  const systemTheme: ResolvedTheme = systemDark ? "dark" : "light";
  const resolvedTheme: ResolvedTheme = theme === "system" ? systemTheme : theme;

  useLayoutEffect(() => {
    const root = document.documentElement;
    root.classList.toggle("dark", resolvedTheme === "dark");
    if (!forcedTheme) {
      root.style.colorScheme = resolvedTheme;
    }
  }, [resolvedTheme, forcedTheme]);

  const setTheme = useCallback(
    (next: Theme) => {
      update({ theme: next }).catch((error: unknown) => {
        console.error("Could not change the theme", error);
      });
    },
    [update]
  );

  const value = useMemo(
    () => ({ resolvedTheme, setTheme, theme }),
    [resolvedTheme, setTheme, theme]
  );

  return <ThemeContext value={value}>{children}</ThemeContext>;
}

export function useTheme(): ThemeContextValue {
  const context = use(ThemeContext);
  if (!context) {
    throw new Error("useTheme must be used within a ThemeProvider.");
  }
  return context;
}

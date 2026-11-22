import { StrictMode } from "react";

import { createRoot } from "react-dom/client";

import { ThemeProvider } from "@/components/theme-provider";
import { ToastProvider } from "@/components/ui/toast";

/** Mounts a window's React tree with the providers every window shares. */
export function mountWindow(
  app: React.ReactNode,
  options: { forcedTheme?: "light" | "dark"; toasts?: boolean } = {}
): void {
  const container = document.getElementById("root");
  if (!container) {
    throw new Error("Missing #root element");
  }
  const { forcedTheme, toasts = true } = options;
  let tree = app;
  if (toasts) {
    tree = <ToastProvider>{app}</ToastProvider>;
  }
  createRoot(container).render(
    <StrictMode>
      <ThemeProvider forcedTheme={forcedTheme}>{tree}</ThemeProvider>
    </StrictMode>
  );
}

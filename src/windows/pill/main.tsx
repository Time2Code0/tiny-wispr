import "@/styles/load";

import { lazy, Suspense } from "react";

import { mountWindow } from "@/components/window-root";
import { isTauri } from "@/lib/ipc";

import { App } from "./app";

const Harness = lazy(() =>
  import("./harness").then((module) => ({ default: module.Harness }))
);

const withControls =
  !isTauri && new URLSearchParams(window.location.search).has("controls");

mountWindow(
  withControls ? (
    <Suspense>
      <Harness />
    </Suspense>
  ) : (
    <App />
  ),
  { forcedTheme: "dark", toasts: false }
);

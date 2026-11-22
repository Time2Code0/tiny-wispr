import type { NetworkRequest, Release, UpdateState } from "./types";

import { IpcError } from "./errors";

const CHECK_MS = 800;
const DOWNLOAD_TICK_MS = 150;
const DOWNLOAD_TICKS = 20;
const ARCHIVE_BYTES = 31_457_280;
const MANIFEST_BYTES = 812;
const RESTART_MS = 1500;
const MANIFEST_URL =
  "https://github.com/Time2Code0/tiny-wispr/releases/latest/download/latest.json";

/** What the next check finds, or which step fails. */
export type MockUpdateOutcome =
  | "available"
  | "upToDate"
  | "checkFails"
  | "downloadFails";

/** What the simulated updater needs from the rest of the mock. */
export interface MockUpdateHost {
  /** A dictation or file job is running: installing waits, as the app's does. */
  busy: () => boolean;
  currentVersion: string;
  emit: (state: UpdateState) => void;
  log: (request: NetworkRequest) => void;
  offline: () => boolean;
  /** The app restarts; the browser preview reloads the page. */
  restart: () => void;
}

export interface MockUpdates {
  check: () => void;
  download: () => void;
  install: () => void;
  /** Jumps to a state, e.g. `"ready"`, with a sample release. */
  jump: (state: UpdateState["state"]) => void;
  /** Sets what the next check finds. */
  outcome: (outcome: MockUpdateOutcome) => void;
  state: () => UpdateState;
}

/** The next patch version, e.g. `0.1.0` → `0.1.1`. */
export function nextVersion(version: string): string {
  const [major = 0, minor = 0, patch = 0] = version
    .split("-")[0]
    .split(".")
    .map(Number);
  return `${major}.${minor}.${patch + 1}`;
}

function sampleRelease(currentVersion: string): Release {
  return {
    notes: [
      "### Features",
      "",
      "- Show what changed before you update",
      "",
      "### Fixes",
      "",
      "- Keep the pill above full-screen apps",
    ].join("\n"),
    publishedAt: Date.now() - 2 * 60 * 60 * 1000,
    version: nextVersion(currentVersion),
  };
}

function offlineError(what: string): IpcError {
  return new IpcError(
    "offline",
    `Stay offline is on, so Tiny Wispr can't ${what}`
  );
}

/**
 * Simulates the updater: `check_for_updates`, `download_update` and
 * `install_update`, with the requests in the network log.
 */
export function createMockUpdates(host: MockUpdateHost): MockUpdates {
  let state: UpdateState = { state: "idle" };
  let next: MockUpdateOutcome = "available";
  let timer: ReturnType<typeof setTimeout> | null = null;
  const release = sampleRelease(host.currentVersion);

  function set(value: UpdateState) {
    state = value;
    host.emit(value);
  }

  function clear() {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
  }

  function log(url: string, bytes: number, startedAt: number, ok: boolean) {
    host.log({
      bytes,
      finishedAt: Date.now(),
      message: ok
        ? null
        : "Couldn't reach the update server. Check your internet connection.",
      result: ok ? "completed" : "failed",
      startedAt,
      url,
    });
  }

  function canCheck(): boolean {
    return ["idle", "upToDate", "available", "failed"].includes(state.state);
  }

  function downloadable(): Release | null {
    if (state.state === "available") {
      return state.release;
    }
    if (state.state === "failed") {
      return state.release;
    }
    return null;
  }

  return {
    check() {
      if (host.offline()) {
        throw offlineError("check for updates");
      }
      if (!canCheck()) {
        return;
      }
      set({ state: "checking" });
      const startedAt = Date.now();
      timer = setTimeout(() => {
        timer = null;
        if (next === "checkFails") {
          log(MANIFEST_URL, 0, startedAt, false);
          set({
            message:
              "Couldn't reach the update server. Check your internet connection.",
            release: null,
            state: "failed",
          });
          return;
        }
        log(MANIFEST_URL, MANIFEST_BYTES, startedAt, true);
        set(
          next === "upToDate"
            ? { checkedAt: Date.now(), state: "upToDate" }
            : { release, state: "available" }
        );
      }, CHECK_MS);
    },
    download() {
      if (host.offline()) {
        throw offlineError("download the update");
      }
      const found = downloadable();
      if (!found) {
        return;
      }
      const url = `https://github.com/Time2Code0/tiny-wispr/releases/download/v${found.version}/Tiny-Wispr_${found.version}_aarch64.app.tar.gz`;
      const startedAt = Date.now();
      let tick = 0;
      const step = () => {
        tick += 1;
        const received = Math.round((ARCHIVE_BYTES * tick) / DOWNLOAD_TICKS);
        if (next === "downloadFails" && tick === DOWNLOAD_TICKS / 2) {
          timer = null;
          log(url, received, startedAt, false);
          set({
            message:
              "Couldn't reach the update server. Check your internet connection.",
            release: found,
            state: "failed",
          });
          return;
        }
        if (tick >= DOWNLOAD_TICKS) {
          timer = null;
          log(url, ARCHIVE_BYTES, startedAt, true);
          set({ release: found, state: "ready" });
          return;
        }
        set({
          received,
          release: found,
          state: "downloading",
          total: ARCHIVE_BYTES,
        });
        timer = setTimeout(step, DOWNLOAD_TICK_MS);
      };
      set({ received: 0, release: found, state: "downloading", total: null });
      timer = setTimeout(step, DOWNLOAD_TICK_MS);
    },
    install() {
      if (host.busy()) {
        throw new IpcError(
          "invalidInput",
          "Finish the dictation first, then restart to update"
        );
      }
      if (state.state !== "ready") {
        throw new IpcError("invalidInput", "No update is ready to install");
      }
      set({ release: state.release, state: "installing" });
      timer = setTimeout(() => {
        timer = null;
        host.restart();
      }, RESTART_MS);
    },
    jump(target) {
      clear();
      switch (target) {
        case "idle":
        case "checking":
          set({ state: target });
          break;
        case "upToDate":
          set({ checkedAt: Date.now(), state: target });
          break;
        case "downloading":
          set({
            received: ARCHIVE_BYTES * 0.4,
            release,
            state: target,
            total: ARCHIVE_BYTES,
          });
          break;
        case "failed":
          set({
            message:
              "Couldn't reach the update server. Check your internet connection.",
            release,
            state: target,
          });
          break;
        default:
          set({ release, state: target });
      }
    },
    outcome(value) {
      next = value;
    },
    state: () => state,
  };
}

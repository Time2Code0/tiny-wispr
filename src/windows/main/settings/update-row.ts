import type { Release, UpdateState } from "@/lib/ipc";

export type Action = "check" | "download" | "install";

const TIME = new Intl.DateTimeFormat(undefined, {
  hour: "numeric",
  minute: "2-digit",
});
const DAY = new Intl.DateTimeFormat(undefined, {
  day: "numeric",
  month: "long",
  year: "numeric",
});

export interface Row {
  action: { kind: Action; label: string } | null;
  busy: boolean;
  description: React.ReactNode;
  label: string;
}

function published(release: Release): string | null {
  return release.publishedAt === null
    ? null
    : `Released ${DAY.format(release.publishedAt)}`;
}

/** What the row says and offers in each state. */
export function updateRow(update: UpdateState): Row {
  switch (update.state) {
    case "idle":
      return {
        action: { kind: "check", label: "Check for updates" },
        busy: false,
        description: "Not checked since Tiny Wispr started.",
        label: "Updates",
      };
    case "checking":
      return {
        action: { kind: "check", label: "Check for updates" },
        busy: true,
        description: "Checking…",
        label: "Updates",
      };
    case "upToDate":
      return {
        action: { kind: "check", label: "Check again" },
        busy: false,
        description: `Checked at ${TIME.format(update.checkedAt)}.`,
        label: "Tiny Wispr is up to date",
      };
    case "available":
      return {
        action: { kind: "download", label: "Download" },
        busy: false,
        description: published(update.release),
        label: `Version ${update.release.version} is available`,
      };
    case "downloading":
      return {
        action: null,
        busy: true,
        description: null,
        label: `Downloading version ${update.release.version}`,
      };
    case "ready":
      return {
        action: { kind: "install", label: "Restart to update" },
        busy: false,
        description: "Tiny Wispr restarts to finish the update.",
        label: `Version ${update.release.version} is ready`,
      };
    case "installing":
      return {
        action: { kind: "install", label: "Restart to update" },
        busy: true,
        description: "Tiny Wispr restarts in a moment.",
        label: `Installing version ${update.release.version}`,
      };
    case "failed":
      return {
        action: update.release
          ? { kind: "download", label: "Try again" }
          : { kind: "check", label: "Try again" },
        busy: false,
        description: update.message,
        label: update.release
          ? "The update didn't finish"
          : "Couldn't check for updates",
      };
    default:
      return update satisfies never;
  }
}

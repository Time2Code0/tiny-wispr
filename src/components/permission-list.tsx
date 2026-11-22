import type { PermissionKind, Permissions, Platform } from "@/lib/ipc";

import { useCallback, useEffect, useRef, useState } from "react";

import {
  AccessIcon,
  CheckmarkCircle02Icon,
  KeyboardIcon,
  Mic01Icon,
} from "@hugeicons/core-free-icons";
import { cn } from "cn";

import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { detectPlatform } from "@/lib/hotkey";
import {
  getPermissions,
  onPermissionsChanged,
  openPermissionSettings,
  requestPermission,
} from "@/lib/ipc";

const PERMISSION_ORDER: readonly PermissionKind[] = [
  "microphone",
  "accessibility",
  "inputMonitoring",
];

/** The permissions this platform asks for, in the order to grant them. */
export function requiredPermissions(
  permissions: Permissions
): PermissionKind[] {
  return PERMISSION_ORDER.filter(
    (kind) => permissions[kind] !== "notApplicable"
  );
}

const POLL_MS = 1000;

/**
 * The current permissions, kept live by `permissions:changed` and, with
 * `poll`, by asking every second while the window is visible (the OS doesn't
 * always tell us when the user flips a switch in System Settings).
 */
export function usePermissions({ poll = false }: { poll?: boolean } = {}): {
  permissions: Permissions | undefined;
  refresh: () => void;
} {
  const [permissions, setPermissions] = useState<Permissions>();

  const refresh = useCallback(() => {
    getPermissions()
      .then(setPermissions)
      .catch((error: unknown) => {
        console.error("Could not read permissions", error);
      });
  }, []);

  useEffect(() => {
    refresh();
    return onPermissionsChanged(setPermissions);
  }, [refresh]);

  useEffect(() => {
    if (!poll) {
      return;
    }
    const timer = setInterval(() => {
      if (document.visibilityState === "visible") {
        refresh();
      }
    }, POLL_MS);
    return () => clearInterval(timer);
  }, [poll, refresh]);

  return { permissions, refresh };
}

interface PermissionCopy {
  icon: typeof Mic01Icon;
  purpose: string;
  title: string;
}

const COPY: Record<PermissionKind, PermissionCopy> = {
  accessibility: {
    icon: AccessIcon,
    purpose:
      "Lets Tiny Wispr paste text into other apps and notice your shortcut while you work in them.",
    title: "Accessibility",
  },
  inputMonitoring: {
    icon: KeyboardIcon,
    purpose: "Lets your shortcut work in every app.",
    title: "Input Monitoring",
  },
  microphone: {
    icon: Mic01Icon,
    purpose: "Hears you while you dictate. Audio never leaves this computer.",
    title: "Microphone",
  },
};

function settingsLabel(platform: Platform): string {
  return platform === "macos" ? "Open System Settings" : "Open Settings";
}

function PermissionRow({
  kind,
  platform,
  status,
}: {
  kind: PermissionKind;
  platform: Platform;
  status: Permissions[PermissionKind];
}) {
  const [busy, setBusy] = useState(false);
  const copy = COPY[kind];
  const titleId = `permission-${kind}`;

  const rowRef = useRef<HTMLLIElement>(null);
  const actionFocused = useRef(false);

  useEffect(() => {
    if (status !== "notDetermined") {
      setBusy(false);
    }
    // The button that had focus is gone once the permission is granted.
    const row = rowRef.current;
    const focusLost =
      document.activeElement === null ||
      document.activeElement === document.body;
    if (status === "granted" && actionFocused.current && focusLost && row) {
      actionFocused.current = false;
      const next = row.parentElement?.querySelector<HTMLElement>("button");
      (next ?? row).focus();
    }
  }, [status]);

  const onActionFocus = useCallback(() => {
    actionFocused.current = true;
  }, []);
  const onActionBlur = useCallback((event: React.FocusEvent) => {
    // Removing the focused button blurs it with nowhere to go; that case is handled above.
    if (event.relatedTarget) {
      actionFocused.current = false;
    }
  }, []);

  const grant = useCallback(() => {
    setBusy(true);
    requestPermission(kind).catch((error: unknown) => {
      setBusy(false);
      console.error(`Could not request ${kind}`, error);
    });
  }, [kind]);
  const openSettings = useCallback(() => {
    openPermissionSettings(kind).catch((error: unknown) => {
      console.error(`Could not open settings for ${kind}`, error);
    });
  }, [kind]);

  return (
    <li
      aria-labelledby={titleId}
      className="flex items-center gap-4 rounded-xl border bg-background px-4 py-3"
      ref={rowRef}
      tabIndex={-1}
    >
      <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-background-muted">
        <Icon aria-hidden="true" className="size-4.5" icon={copy.icon} />
      </span>
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <p className="font-medium text-foreground text-sm" id={titleId}>
          {copy.title}
        </p>
        <p className="text-foreground-muted text-xs">{copy.purpose}</p>
        {status === "denied" ? (
          <p className="text-destructive-foreground text-xs">
            Turned off. Switch it on for Tiny Wispr in{" "}
            {platform === "macos" ? "System Settings" : "Settings"}.
          </p>
        ) : null}
      </div>
      <div className="shrink-0">
        {status === "granted" ? (
          <span className="flex items-center gap-1.5 font-medium text-sm text-success-foreground">
            <Icon
              aria-hidden="true"
              className="size-4"
              icon={CheckmarkCircle02Icon}
            />
            Granted
          </span>
        ) : null}
        {status === "notDetermined" ? (
          <Button
            loading={busy}
            onBlur={onActionBlur}
            onClick={grant}
            onFocus={onActionFocus}
            size="sm"
            type="button"
          >
            Grant
          </Button>
        ) : null}
        {status === "denied" ? (
          <Button
            onBlur={onActionBlur}
            onClick={openSettings}
            onFocus={onActionFocus}
            size="sm"
            type="button"
            variant="outline"
          >
            {settingsLabel(platform)}
          </Button>
        ) : null}
      </div>
    </li>
  );
}

/**
 * One row per permission the platform needs, with what it's for, a button
 * to grant it (or to open the system settings once it was turned down) and
 * a check once it's granted. Pair with `usePermissions({ poll: true })`.
 */
export function PermissionList({
  permissions,
  platform = detectPlatform(),
  className,
}: {
  permissions: Permissions;
  platform?: Platform;
  className?: string;
}) {
  return (
    <ul className={cn("flex flex-col gap-2", className)}>
      {requiredPermissions(permissions).map((kind) => (
        <PermissionRow
          key={kind}
          kind={kind}
          platform={platform}
          status={permissions[kind]}
        />
      ))}
    </ul>
  );
}

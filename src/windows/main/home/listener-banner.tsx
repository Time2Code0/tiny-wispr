import type { ListenerState, PermissionStatus } from "@/lib/ipc";

import { useCallback } from "react";

import { Alert02Icon } from "@hugeicons/core-free-icons";

import {
  Alert,
  AlertAction,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { useIpcQuery } from "@/hooks/use-ipc-query";
import {
  getListenerStatus,
  getPermissions,
  openPermissionSettings,
  requestPermission,
} from "@/lib/ipc";

const LISTENER_EVENTS = ["listener:status", "permissions:changed"] as const;
const PERMISSION_EVENTS = ["permissions:changed"] as const;

const COPY: Partial<
  Record<ListenerState, { description: string; title: string }>
> = {
  failed: {
    description: "The shortcut listener stopped. Quit and reopen Tiny Wispr.",
    title: "Tiny Wispr can't see your shortcut",
  },
  secureInput: {
    description:
      "A password field or another app turned on secure input. Shortcuts work again once you leave it.",
    title: "Shortcuts are paused",
  },
  unsupported: {
    description:
      "This desktop doesn't let apps listen for shortcuts. Click the pill to dictate.",
    title: "Shortcuts don't work here",
  },
  waitingForPermission: {
    description:
      "Grant Accessibility access so Tiny Wispr can hear your shortcut and paste text. If it's already switched on in System Settings, remove Tiny Wispr from the list and add it again.",
    title: "Tiny Wispr can't see your shortcut",
  },
};

function GrantAccessibility({ status }: { status: PermissionStatus }) {
  const denied = status === "denied";
  const grant = useCallback(() => {
    const action = denied
      ? openPermissionSettings("accessibility")
      : requestPermission("accessibility");
    action.catch((error: unknown) => {
      console.error("Could not ask for Accessibility access", error);
    });
  }, [denied]);
  return (
    <AlertAction>
      <Button onClick={grant} size="sm" type="button" variant="outline">
        {denied ? "Open System Settings" : "Grant access"}
      </Button>
    </AlertAction>
  );
}

/** Says why shortcuts don't work, when the listener isn't running. */
export function ListenerBanner() {
  const listener = useIpcQuery(getListenerStatus, LISTENER_EVENTS);
  const permissions = useIpcQuery(getPermissions, PERMISSION_EVENTS);
  const state = listener.data?.state;
  const copy = state ? COPY[state] : undefined;
  if (!copy) {
    return null;
  }
  return (
    <Alert variant="warning">
      <Icon aria-hidden="true" icon={Alert02Icon} />
      <AlertTitle>{copy.title}</AlertTitle>
      <AlertDescription>{copy.description}</AlertDescription>
      {state === "waitingForPermission" && permissions.data ? (
        <GrantAccessibility status={permissions.data.accessibility} />
      ) : null}
    </Alert>
  );
}

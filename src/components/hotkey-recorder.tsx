import type { Hotkey, HotkeyAction, Platform } from "@/lib/ipc";

import { useCallback, useEffect, useRef, useState } from "react";

import { cn } from "cn";

import { ShortcutKeys } from "@/components/shortcut-keys";
import { Button } from "@/components/ui/button";
import { detectPlatform, hotkeyKeyCount, hotkeysEqual } from "@/lib/hotkey";
import {
  onHotkeyCaptured,
  startHotkeyCapture,
  stopHotkeyCapture,
  validateHotkey,
} from "@/lib/ipc";
import { IpcError } from "@/lib/ipc/errors";

/** Escape on its own cancels recording, except when recording Cancel itself. */
export function isCancelPress(hotkey: Hotkey, action: HotkeyAction): boolean {
  return (
    action !== "cancel" &&
    hotkey.key === "Escape" &&
    hotkey.modifiers.length === 0
  );
}

type Announcement = { kind: "issue" | "saved"; text: string } | null;

/**
 * A shortcut binding: click it, press a combination, and it saves on release
 * once the backend accepts it; otherwise the reason shows inline and it keeps
 * listening. Escape, a second click or clicking away cancels. "Reset to
 * default" appears when the binding differs from the default.
 */
export function HotkeyRecorder({
  action,
  value,
  defaultValue,
  onChange,
  label,
  platform = detectPlatform(),
  className,
}: {
  action: HotkeyAction;
  value: Hotkey;
  defaultValue: Hotkey;
  /** Saves the accepted combination. A rejection shows its message. */
  onChange: (hotkey: Hotkey) => Promise<unknown>;
  /** What the shortcut does, e.g. "Push to talk". Names the button. */
  label: string;
  platform?: Platform;
  className?: string;
}) {
  const [recording, setRecording] = useState(false);
  const [live, setLive] = useState<Hotkey | null>(null);
  const [announcement, setAnnouncement] = useState<Announcement>(null);
  const capture = useRef({ active: false });

  const stop = useCallback(() => {
    // biome-ignore lint/suspicious/noUnnecessaryConditions: the ref changes between calls; Biome narrows it to its initial value
    if (!capture.current.active) {
      return;
    }
    capture.current.active = false;
    setRecording(false);
    setLive(null);
    setAnnouncement((current) => (current?.kind === "issue" ? null : current));
    stopHotkeyCapture().catch((error: unknown) => {
      console.error("Could not stop recording the shortcut", error);
    });
  }, []);

  const start = useCallback(() => {
    capture.current.active = true;
    setRecording(true);
    setLive(null);
    setAnnouncement(null);
    startHotkeyCapture().catch((error: unknown) => {
      capture.current.active = false;
      setRecording(false);
      setAnnouncement({ kind: "issue", text: IpcError.from(error).message });
    });
  }, []);

  const save = useCallback(
    async (hotkey: Hotkey): Promise<boolean> => {
      try {
        const issue = await validateHotkey(action, hotkey);
        if (issue) {
          setAnnouncement({ kind: "issue", text: issue.message });
          return false;
        }
        await onChange(hotkey);
        setAnnouncement({ kind: "saved", text: "Shortcut saved" });
        return true;
      } catch (error) {
        setAnnouncement({ kind: "issue", text: IpcError.from(error).message });
        return false;
      }
    },
    [action, onChange]
  );

  useEffect(() => {
    if (!recording) {
      return;
    }
    return onHotkeyCaptured(({ done, hotkey }) => {
      // biome-ignore lint/suspicious/noUnnecessaryConditions: the ref changes between calls; Biome narrows it to its initial value
      if (!capture.current.active) {
        return;
      }
      if (isCancelPress(hotkey, action)) {
        stop();
        return;
      }
      if (!done) {
        setLive(hotkeyKeyCount(hotkey) > 0 ? hotkey : null);
        return;
      }
      setLive(null);
      save(hotkey).then((saved) => {
        if (saved) {
          stop();
        }
      });
    });
  }, [action, recording, save, stop]);

  useEffect(() => stop, [stop]);

  const toggle = useCallback(
    (event: React.MouseEvent) => {
      // Keys pressed while recording belong to the new shortcut, not to this button.
      if (recording && event.detail === 0) {
        return;
      }
      if (recording) {
        stop();
      } else {
        start();
      }
    },
    [recording, start, stop]
  );
  const onKeyDown = useCallback(
    (event: React.KeyboardEvent) => {
      if (recording && event.key === "Escape" && action !== "cancel") {
        event.preventDefault();
        stop();
      }
    },
    [action, recording, stop]
  );
  const reset = useCallback(() => {
    stop();
    save(defaultValue);
  }, [defaultValue, save, stop]);

  const isDefault = hotkeysEqual(value, defaultValue);
  const shown = live ?? value;

  return (
    <div className={cn("flex flex-col", className)}>
      <div className="flex items-center gap-2">
        <button
          aria-label={
            recording
              ? `Recording a new shortcut for ${label}. Press Escape to cancel.`
              : `${label} shortcut. Click to change.`
          }
          className={cn(
            "flex h-9 min-w-40 items-center gap-2 rounded-lg border bg-background px-3 text-start text-sm outline-none transition-colors hover:bg-accent focus-visible:ring-2 focus-visible:ring-foreground focus-visible:ring-offset-1 focus-visible:ring-offset-background motion-reduce:transition-none",
            recording && "border-foreground/40 bg-background-muted"
          )}
          data-recording={recording}
          onBlur={stop}
          onClick={toggle}
          onKeyDown={onKeyDown}
          type="button"
        >
          {recording && !live ? (
            <span className="text-foreground-muted">Press a shortcut…</span>
          ) : (
            <ShortcutKeys hotkey={shown} platform={platform} />
          )}
          {recording ? null : (
            <span className="ms-auto ps-3 text-foreground-muted text-xs">
              Change
            </span>
          )}
        </button>
        {isDefault ? null : (
          <Button onClick={reset} size="sm" type="button" variant="ghost">
            Reset to default
          </Button>
        )}
      </div>
      <p
        aria-live="polite"
        className={cn(
          "text-xs",
          announcement && "mt-1.5",
          announcement?.kind === "issue"
            ? "text-destructive-foreground"
            : "text-foreground-muted"
        )}
      >
        {announcement?.text}
      </p>
    </div>
  );
}

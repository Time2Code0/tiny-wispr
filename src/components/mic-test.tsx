import type { MicrophoneInfo } from "@/lib/ipc";

import { useCallback, useEffect, useRef, useState } from "react";

import { Alert02Icon, CheckmarkCircle02Icon } from "@hugeicons/core-free-icons";
import { cn } from "cn";

import { LevelBars } from "@/components/level-bars";
import { MicrophoneSelect } from "@/components/microphone-select";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { useAudioLevels } from "@/hooks/use-audio-levels";
import { detectPlatform } from "@/lib/hotkey";
import {
  listMicrophones,
  onAudioLevel,
  openPermissionSettings,
  startMicTest,
  stopMicTest,
} from "@/lib/ipc";
import { IpcError } from "@/lib/ipc/errors";

/** Smoothed RMS above this counts as hearing a voice rather than room noise. */
export const SPEECH_LEVEL = 0.05;

/** Without a single level for this long, the input is probably dead. */
const SILENT_INPUT_MS = 4000;

export type MicProblem =
  | { kind: "noMicrophone" }
  | { kind: "inUse" }
  | { kind: "denied" }
  | { kind: "failed"; message: string };

/** What went wrong when the microphone test couldn't start. */
export function micProblem(error: unknown): MicProblem {
  const ipcError = IpcError.from(error);
  switch (ipcError.kind) {
    case "noMicrophone":
      return { kind: "noMicrophone" };
    case "microphoneBusy":
      return { kind: "inUse" };
    case "permissionDenied":
      return { kind: "denied" };
    default:
      return { kind: "failed", message: ipcError.message };
  }
}

function problemText(problem: MicProblem): string {
  switch (problem.kind) {
    case "noMicrophone":
      return "No microphone found";
    case "inUse":
      return "In use by another app";
    case "denied":
      return "Microphone access is off";
    default:
      return problem.message;
  }
}

function problemHint(problem: MicProblem): string {
  switch (problem.kind) {
    case "noMicrophone":
      return "Connect a microphone, then try again.";
    case "inUse":
      return "Close the app that's using it, or pick another microphone.";
    case "denied":
      return "Allow microphone access for Tiny Wispr, then try again.";
    default:
      return "Try again, or pick another microphone.";
  }
}

const BAR_COUNT = 23;

function openMicrophoneSettings() {
  openPermissionSettings("microphone").catch((error: unknown) => {
    console.error("Could not open the microphone settings", error);
  });
}

/**
 * Runs the microphone test while mounted: live level bars, a microphone
 * picker, "We can hear you" once speech-level input arrives, and the errors
 * with a way to retry. Changing the microphone restarts the test.
 */
export function MicTest({
  microphone,
  onMicrophoneChange,
  onHeard,
  className,
}: {
  /** Device id, or `null` for the system default. */
  microphone: string | null;
  onMicrophoneChange: (id: string | null) => void;
  /** Called once, the first time speech-level input arrives. */
  onHeard?: () => void;
  className?: string;
}) {
  const levels = useAudioLevels(BAR_COUNT);
  const [microphones, setMicrophones] = useState<MicrophoneInfo[]>();
  const [problem, setProblem] = useState<MicProblem>();
  const [heard, setHeard] = useState(false);
  const [silent, setSilent] = useState(false);
  const [attempt, setAttempt] = useState(0);

  const heardRef = useRef(false);
  const retry = useCallback(() => setAttempt((count) => count + 1), []);
  const onHeardRef = useRef(onHeard);
  onHeardRef.current = onHeard;

  // biome-ignore lint/correctness/useExhaustiveDependencies: `attempt` restarts the test on Retry
  useEffect(() => {
    let active = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const armSilence = () => {
      clearTimeout(timer);
      timer = setTimeout(() => setSilent(true), SILENT_INPUT_MS);
    };
    setProblem(undefined);
    setSilent(false);

    const unsubscribe = onAudioLevel(({ level }) => {
      setSilent(false);
      armSilence();
      if (level >= SPEECH_LEVEL && !heardRef.current) {
        heardRef.current = true;
        setHeard(true);
        onHeardRef.current?.();
      }
    });

    listMicrophones()
      .then(async (found) => {
        if (!active) {
          return;
        }
        setMicrophones(found);
        if (found.length === 0) {
          setProblem({ kind: "noMicrophone" });
          return;
        }
        await startMicTest();
        armSilence();
      })
      .catch((error: unknown) => {
        if (active) {
          setProblem(micProblem(error));
        }
      });

    return () => {
      active = false;
      clearTimeout(timer);
      unsubscribe();
      stopMicTest().catch((error: unknown) => {
        console.error("Could not stop the microphone test", error);
      });
    };
  }, [microphone, attempt]);

  const status = (() => {
    if (problem) {
      return (
        <div className="flex flex-col items-center gap-1 text-center">
          <p className="flex items-center gap-1.5 font-medium text-destructive-foreground text-sm">
            <Icon aria-hidden="true" className="size-4" icon={Alert02Icon} />
            {problemText(problem)}
          </p>
          <p className="text-foreground-muted text-xs">
            {problemHint(problem)}
          </p>
        </div>
      );
    }
    if (heard) {
      return (
        <p className="flex items-center gap-1.5 font-medium text-sm text-success-foreground">
          <Icon
            aria-hidden="true"
            className="size-4"
            icon={CheckmarkCircle02Icon}
          />
          We can hear you
        </p>
      );
    }
    if (silent) {
      return (
        <p className="text-center text-foreground-muted text-sm">
          No sound is coming in. Check the microphone or pick another one.
        </p>
      );
    }
    return <p className="text-foreground-muted text-sm">Listening…</p>;
  })();

  return (
    <div className={cn("flex flex-col gap-4", className)}>
      <div className="flex flex-col items-center gap-4 rounded-xl border bg-background-muted px-6 py-6">
        <LevelBars
          active={!problem}
          className="h-14 gap-1 *:w-1"
          count={BAR_COUNT}
          levels={levels}
        />
        <div aria-live="polite" className="flex min-h-10 items-center">
          {status}
        </div>
        {problem ? (
          <div className="flex gap-2">
            {problem.kind === "denied" ? (
              <Button
                onClick={openMicrophoneSettings}
                size="sm"
                type="button"
                variant="outline"
              >
                {detectPlatform() === "macos"
                  ? "Open System Settings"
                  : "Open Settings"}
              </Button>
            ) : null}
            <Button onClick={retry} size="sm" type="button" variant="outline">
              Retry
            </Button>
          </div>
        ) : null}
      </div>
      <MicrophoneSelect
        disabled={!microphones || microphones.length === 0}
        microphones={microphones ?? []}
        onValueChange={onMicrophoneChange}
        value={microphone}
      />
    </div>
  );
}

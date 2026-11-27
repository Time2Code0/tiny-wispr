import type { DictationState, Platform, Settings } from "@/lib/ipc";

import { useCallback, useEffect, useRef, useState } from "react";

import { LevelBars } from "@/components/level-bars";
import { ShortcutKeys } from "@/components/shortcut-keys";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/loading";
import { Textarea } from "@/components/ui/textarea";
import { useAudioLevels } from "@/hooks/use-audio-levels";
import { useDictationState } from "@/hooks/use-dictation-state";
import {
  listHistory,
  onHistoryChanged,
  startDictation,
  stopDictation,
} from "@/lib/ipc";

import { LaunchAtLogin } from "./launch-at-login";

/** How long the real paste gets to land before the text is added by hand. */
const PASTE_GRACE_MS = 300;

/**
 * The text to show after a dictation finished: the current text with the
 * transcript appended, unless the paste already put it there.
 */
export function withTranscript(current: string, transcript: string): string {
  if (!transcript || current.includes(transcript)) {
    return current;
  }
  if (!current.trim()) {
    return transcript;
  }
  return `${current.trimEnd()} ${transcript}`;
}

function Status({
  platform,
  settings,
  state,
}: {
  platform: Platform;
  settings: Settings;
  state: DictationState;
}) {
  const levels = useAudioLevels(9);
  if (state.phase === "listening") {
    return (
      <span className="flex items-center gap-2">
        <LevelBars className="h-4" levels={levels} />
        Listening
      </span>
    );
  }
  if (state.phase === "processing") {
    return (
      <span className="flex items-center gap-2">
        <Spinner className="size-3.5" />
        {state.stage === "polishing" ? "Polishing" : "Transcribing"}
      </span>
    );
  }
  if (state.phase === "notice" && state.notice) {
    return <span>{state.notice.text}</span>;
  }
  return (
    <div className="flex items-center gap-1.5">
      Hold
      <ShortcutKeys
        hotkey={settings.shortcuts.pushToTalk}
        platform={platform}
      />
      and say something
    </div>
  );
}

export function TryItStep({
  platform,
  settings,
  update,
}: {
  platform: Platform;
  settings: Settings;
  update: (patch: Partial<Settings>) => Promise<unknown>;
}) {
  const state = useDictationState();
  const [text, setText] = useState("");
  const [dictations, setDictations] = useState(0);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const textRef = useRef(text);
  textRef.current = text;

  useEffect(() => {
    textareaRef.current?.focus();
    const shownAt = Date.now();
    let timer: ReturnType<typeof setTimeout> | undefined;
    const unsubscribe = onHistoryChanged(() => {
      clearTimeout(timer);
      timer = setTimeout(async () => {
        try {
          const { entries } = await listHistory();
          const [latest] = entries;
          if (!latest || latest.createdAt < shownAt) {
            return;
          }
          setDictations((count) => count + 1);
          setText(withTranscript(textRef.current, latest.finalText));
        } catch (error) {
          console.error("Could not read the transcript", error);
        }
      }, PASTE_GRACE_MS);
    });
    return () => {
      clearTimeout(timer);
      unsubscribe();
    };
  }, []);

  const handsFree = state.phase === "listening" && state.mode === "handsFree";
  const idle = state.phase === "idle" || state.phase === "notice";
  const startByClick = useCallback(() => {
    textareaRef.current?.focus();
    startDictation("handsFree").catch((error: unknown) => {
      console.error("Could not start dictating", error);
    });
  }, []);
  const stopByClick = useCallback(() => {
    textareaRef.current?.focus();
    stopDictation().catch((error: unknown) => {
      console.error("Could not stop dictating", error);
    });
  }, []);
  const changeText = useCallback(
    (event: React.ChangeEvent<HTMLTextAreaElement>) =>
      setText(event.target.value),
    []
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      <Textarea
        aria-label="Practice area"
        className="min-h-0 flex-1 resize-none"
        onChange={changeText}
        placeholder="Your words appear here."
        ref={textareaRef}
        value={text}
      />
      <div className="flex min-h-8 items-center gap-3 text-foreground-muted text-sm">
        <div aria-live="polite" className="flex items-center">
          <Status platform={platform} settings={settings} state={state} />
        </div>
        <div className="ms-auto">
          {handsFree ? (
            <Button
              onClick={stopByClick}
              size="sm"
              type="button"
              variant="outline"
            >
              Stop
            </Button>
          ) : null}
          {idle ? (
            <Button
              onClick={startByClick}
              size="sm"
              type="button"
              variant="ghost"
            >
              Or click to start
            </Button>
          ) : null}
        </div>
      </div>
      {dictations > 0 ? (
        <div className="flex flex-wrap items-center gap-1.5 text-foreground-muted text-xs">
          Now try hands-free: press
          <ShortcutKeys
            hotkey={settings.shortcuts.handsFree}
            platform={platform}
          />
          and talk without holding. Press it again to stop.
        </div>
      ) : null}
      <LaunchAtLogin settings={settings} update={update} />
    </div>
  );
}

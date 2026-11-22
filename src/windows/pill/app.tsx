import type { Notice } from "@/lib/ipc";
import type { Callout } from "./callout";

import { useCallback, useEffect, useRef, useState } from "react";

import { AnimatePresence, MotionConfig } from "motion/react";

import { useDictationState } from "@/hooks/use-dictation-state";
import { useSettings } from "@/hooks/use-settings";
import {
  cancelDictation,
  copyLastTranscript,
  openMainWindow,
  pasteLastTranscript,
  startDictation,
  stopDictation,
  undoCancel,
} from "@/lib/ipc";

import { CalloutSlot } from "./callout";
import { Capsule } from "./capsule";
import {
  useDelayedFlag,
  useElapsedMs,
  useHiddenUntil,
  useHitRegions,
  useSuccessBeat,
} from "./hooks";
import {
  announcement,
  capsuleView,
  HINT_AFTER_MS,
  noticeCommand,
  PROCESSING_LABEL_AFTER_MS,
  stageLabel,
  TIMER_AFTER_MS,
} from "./model";
import { PillMenu } from "./pill-menu";

/** An open menu closes this long after the pointer leaves the window. */
const MENU_LEAVE_MS = 800;

function run(what: string, command: () => Promise<unknown>) {
  command().catch((error: unknown) => {
    console.error(`Could not ${what}`, error);
  });
}

const cancel = () => run("cancel the dictation", cancelDictation);
const startHandsFree = () =>
  run("start dictating", () => startDictation("handsFree"));
const stop = () => run("stop the dictation", stopDictation);

function runNoticeAction(notice: Notice) {
  const command = noticeCommand(notice);
  switch (command?.kind) {
    case "undo":
      run("undo the cancel", undoCancel);
      break;
    case "pasteLast":
      run("paste the last transcript", pasteLastTranscript);
      break;
    case "copyLast":
      run("copy the last transcript", copyLastTranscript);
      break;
    case "openMain": {
      const { route } = command;
      run("open the main window", () => openMainWindow(route));
      break;
    }
    default:
      break;
  }
}

/**
 * The pill window is transparent and larger than the capsule: the hint,
 * labels, notices and menu are drawn above it. Only those pieces take
 * pointer events; the rest of the window lets clicks through.
 */
export function App() {
  const state = useDictationState();
  const { settings } = useSettings();
  const hiddenForNow = useHiddenUntil(settings?.pillHiddenUntil ?? null);
  const success = useSuccessBeat(state.phase);
  const elapsedMs = useElapsedMs(state);
  const [hovered, setHovered] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const capsuleRef = useRef<HTMLDivElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const openMenu = useCallback(() => setMenuOpen(true), []);

  const view = capsuleView({
    hovered: hovered || menuOpen,
    showWhenIdle: (settings?.showPillAlways ?? true) && !hiddenForNow,
    state,
    success,
  });
  const listening = state.phase === "listening";
  const showHint = useDelayedFlag(view === "hover" && !menuOpen, HINT_AFTER_MS);
  const showLabel = useDelayedFlag(
    state.phase === "processing",
    PROCESSING_LABEL_AFTER_MS
  );

  let callout: Callout | null = null;
  if (state.notice) {
    callout = { kind: "notice", notice: state.notice };
  } else if (showLabel) {
    callout = { kind: "label", text: stageLabel(state) };
  } else if (showHint && settings) {
    callout = { kind: "hint", shortcut: settings.shortcuts.pushToTalk };
  }

  useHitRegions(menuOpen);

  useEffect(() => {
    if (!listening) {
      return;
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !event.defaultPrevented) {
        cancel();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [listening]);

  useEffect(() => {
    if (view !== "idle" && view !== "hover") {
      setMenuOpen(false);
    }
  }, [view]);

  useEffect(() => {
    if (!menuOpen) {
      return;
    }
    let timer: ReturnType<typeof setTimeout> | undefined;
    const onLeave = () => {
      timer = setTimeout(() => setMenuOpen(false), MENU_LEAVE_MS);
    };
    const onEnter = () => clearTimeout(timer);
    const root = document.documentElement;
    root.addEventListener("pointerleave", onLeave);
    root.addEventListener("pointerenter", onEnter);
    return () => {
      clearTimeout(timer);
      root.removeEventListener("pointerleave", onLeave);
      root.removeEventListener("pointerenter", onEnter);
    };
  }, [menuOpen]);

  return (
    <MotionConfig reducedMotion="user">
      <div
        className="pointer-events-none absolute inset-0 flex flex-col items-center justify-end gap-2 pb-2"
        data-slot="pill"
      >
        <CalloutSlot callout={callout} onAction={runNoticeAction} />
        <AnimatePresence>
          {view === "hidden" ? null : (
            <Capsule
              buttonRef={buttonRef}
              capsuleRef={capsuleRef}
              elapsedMs={elapsedMs}
              key="capsule"
              menuOpen={menuOpen}
              onCancel={cancel}
              onHoverChange={setHovered}
              onOpenMenu={openMenu}
              onStart={startHandsFree}
              onStop={stop}
              showTimer={elapsedMs >= TIMER_AFTER_MS}
              view={view}
            />
          )}
        </AnimatePresence>
        <PillMenu
          anchor={capsuleRef}
          finalFocus={buttonRef}
          onOpenChange={setMenuOpen}
          open={menuOpen}
        />
        <p aria-live="polite" className="sr-only" role="status">
          {announcement(state)}
        </p>
      </div>
    </MotionConfig>
  );
}

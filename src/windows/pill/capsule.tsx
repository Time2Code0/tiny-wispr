import type { CapsuleView } from "./model";

import { useCallback } from "react";

import { Cancel01Icon, Tick02Icon } from "@hugeicons/core-free-icons";
import { cn } from "cn";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";

import { BAR_REST, LevelBars } from "@/components/level-bars";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { useAudioLevels } from "@/hooks/use-audio-levels";

import { HIT_REGION } from "./hooks";
import { capsuleSize, formatElapsed } from "./model";

const BAR_COUNT = 9;

const MORPH = { bounce: 0.2, type: "spring", visualDuration: 0.35 } as const;
const FADE = { duration: 0.15, ease: "easeOut" } as const;

/** Buttons sit 1px inside the capsule's border, so their focus ring is drawn inwards. */
const INSET_FOCUS =
  "rounded-full focus-visible:ring-0 focus-visible:ring-offset-0 focus-visible:outline-2 focus-visible:outline-foreground focus-visible:-outline-offset-2";

export function Capsule({
  buttonRef,
  capsuleRef,
  elapsedMs,
  menuOpen,
  onCancel,
  onHoverChange,
  onOpenMenu,
  onStart,
  onStop,
  showTimer,
  view,
}: {
  buttonRef: React.Ref<HTMLButtonElement>;
  capsuleRef: React.Ref<HTMLDivElement>;
  elapsedMs: number;
  menuOpen: boolean;
  onCancel: () => void;
  onHoverChange: (hovered: boolean) => void;
  onOpenMenu: () => void;
  onStart: () => void;
  onStop: () => void;
  showTimer: boolean;
  view: Exclude<CapsuleView, "hidden">;
}) {
  const reduceMotion = useReducedMotion() ?? false;
  const size = capsuleSize(view, showTimer);
  const resting = view === "idle" || view === "hover";
  const onPointerEnter = useCallback(
    () => onHoverChange(true),
    [onHoverChange]
  );
  const onPointerLeave = useCallback(
    () => onHoverChange(false),
    [onHoverChange]
  );

  return (
    <motion.div
      animate={{ opacity: 1, ...size }}
      className={cn(
        "pointer-events-auto relative shrink-0 overflow-hidden rounded-full border transition-[background-color,border-color,box-shadow] duration-200",
        view === "idle" && "border-foreground/30 bg-background/80",
        view === "hover" && "border-foreground/40 bg-background",
        !resting && "border-foreground/15 bg-background shadow-lg",
        resting &&
          "has-focus-visible:ring-2 has-focus-visible:ring-foreground has-focus-visible:ring-offset-2 has-focus-visible:ring-offset-background"
      )}
      data-slot="pill-capsule"
      data-view={view}
      exit={{ opacity: 0, transition: FADE }}
      initial={{ opacity: 0, ...size }}
      onPointerEnter={onPointerEnter}
      onPointerLeave={onPointerLeave}
      ref={capsuleRef}
      transition={reduceMotion ? { duration: 0, opacity: FADE } : MORPH}
      {...HIT_REGION}
    >
      <AnimatePresence initial={false}>
        <motion.div
          animate={{ opacity: 1, scale: 1 }}
          className="absolute inset-0 flex items-center justify-center"
          exit={{ opacity: 0, scale: 0.92 }}
          initial={{ opacity: 0, scale: 0.92 }}
          key={resting ? "resting" : view}
          transition={FADE}
        >
          {resting ? (
            <StartButton
              buttonRef={buttonRef}
              menuOpen={menuOpen}
              onOpenMenu={onOpenMenu}
              onStart={onStart}
            />
          ) : null}
          {view === "hold" ? <LiveBars /> : null}
          {view === "handsFree" ? (
            <HandsFree
              elapsedMs={elapsedMs}
              onCancel={onCancel}
              onStop={onStop}
              showTimer={showTimer}
            />
          ) : null}
          {view === "processing" ? <ProcessingBars /> : null}
          {view === "success" ? (
            <Icon
              aria-hidden="true"
              className="size-3.5 text-foreground"
              icon={Tick02Icon}
              strokeWidth={2.5}
            />
          ) : null}
        </motion.div>
      </AnimatePresence>
    </motion.div>
  );
}

/** Fills the resting capsule: click starts hands-free, right-click opens the menu. */
function StartButton({
  buttonRef,
  menuOpen,
  onOpenMenu,
  onStart,
}: {
  buttonRef: React.Ref<HTMLButtonElement>;
  menuOpen: boolean;
  onOpenMenu: () => void;
  onStart: () => void;
}) {
  const onContextMenu = useCallback(
    (event: React.MouseEvent) => {
      event.preventDefault();
      onOpenMenu();
    },
    [onOpenMenu]
  );
  const onKeyDown = useCallback(
    (event: React.KeyboardEvent) => {
      if (
        event.key === "ContextMenu" ||
        (event.shiftKey && event.key === "F10")
      ) {
        event.preventDefault();
        onOpenMenu();
      }
    },
    [onOpenMenu]
  );
  return (
    <Button
      aria-expanded={menuOpen}
      aria-haspopup="menu"
      aria-keyshortcuts="Shift+F10"
      aria-label="Start dictating"
      className="absolute inset-0 h-auto cursor-default rounded-full border-0 p-0 focus-visible:ring-0 focus-visible:ring-offset-0"
      onClick={onStart}
      onContextMenu={onContextMenu}
      onKeyDown={onKeyDown}
      ref={buttonRef}
      type="button"
      variant="unstyled"
    />
  );
}

function LiveBars() {
  const levels = useAudioLevels(12);
  return <LevelBars count={BAR_COUNT} levels={levels} />;
}

function HandsFree({
  elapsedMs,
  onCancel,
  onStop,
  showTimer,
}: {
  elapsedMs: number;
  onCancel: () => void;
  onStop: () => void;
  showTimer: boolean;
}) {
  return (
    <div className="flex size-full items-center gap-1.5 px-px">
      <Button
        aria-label="Cancel dictation"
        className={cn(
          "bg-foreground/10 text-foreground-muted hover:bg-foreground/20 hover:text-foreground",
          INSET_FOCUS
        )}
        onClick={onCancel}
        shape="full"
        size="icon-xs"
        type="button"
        variant="ghost"
      >
        <Icon aria-hidden="true" className="size-3" icon={Cancel01Icon} />
      </Button>
      <div className="flex flex-1 items-center justify-center gap-1.5">
        <LiveBars />
        {showTimer ? (
          <span className="font-medium text-foreground-muted text-xs tabular-nums">
            {formatElapsed(elapsedMs)}
          </span>
        ) : null}
      </div>
      <Button
        aria-label="Stop and paste"
        className={cn(
          "bg-destructive-subtel hover:bg-destructive data-pressed:bg-destructive",
          INSET_FOCUS
        )}
        onClick={onStop}
        shape="full"
        size="icon-xs"
        type="button"
        variant="ghost"
      >
        <span aria-hidden="true" className="size-2 rounded-xs bg-foreground" />
      </Button>
    </div>
  );
}

/** The level bars at rest, with a soft brightness wave travelling across them. */
function ProcessingBars() {
  const reduceMotion = useReducedMotion() ?? false;
  return (
    <div
      aria-hidden="true"
      className="flex h-4 items-center gap-0.5"
      data-slot="pill-processing"
    >
      {Array.from({ length: BAR_COUNT }, (_, index) => (
        <motion.span
          animate={reduceMotion ? { opacity: 0.6 } : { opacity: [0.3, 1, 0.3] }}
          className="block w-0.5 shrink-0 rounded-full bg-foreground"
          initial={{ opacity: 0.3 }}
          // biome-ignore lint/suspicious/noArrayIndexKey: bars are positional and never reorder
          key={index}
          style={{ height: `${BAR_REST * 100}%` }}
          transition={
            reduceMotion
              ? FADE
              : {
                  delay: index * 0.09,
                  duration: 1.4,
                  ease: "easeInOut",
                  repeat: Number.POSITIVE_INFINITY,
                  repeatDelay: 0.2,
                }
          }
        />
      ))}
    </div>
  );
}

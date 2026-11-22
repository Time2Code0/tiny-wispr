import type { Hotkey, Notice } from "@/lib/ipc";

import { useCallback } from "react";

import { Alert02Icon, AlertCircleIcon } from "@hugeicons/core-free-icons";
import { cn } from "cn";
import { AnimatePresence, motion } from "motion/react";

import { ShortcutKeys } from "@/components/shortcut-keys";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";

import { HIT_REGION } from "./hooks";
import { noticeTone } from "./model";

/** What shows above the capsule; at most one at a time. */
export type Callout =
  | { kind: "hint"; shortcut: Hotkey }
  | { kind: "label"; text: string }
  | { kind: "notice"; notice: Notice };

const SPRING_IN = {
  bounce: 0.25,
  type: "spring",
  visualDuration: 0.3,
} as const;
const FADE_OUT = { duration: 0.12, ease: "easeIn" } as const;

const CHIP =
  "pointer-events-auto flex h-7 items-center gap-1.5 whitespace-nowrap rounded-full border bg-background px-3 font-medium text-xs shadow-md";

function calloutKey(callout: Callout): string {
  switch (callout.kind) {
    case "notice":
      return `notice:${callout.notice.kind}:${callout.notice.text}`;
    case "label":
      return `label:${callout.text}`;
    default:
      return "hint";
  }
}

/**
 * The hint, processing label or notice above the capsule. Old and new overlap
 * in one grid cell while they cross-fade, so the capsule never moves.
 */
export function CalloutSlot({
  callout,
  onAction,
}: {
  callout: Callout | null;
  onAction: (notice: Notice) => void;
}) {
  return (
    <div className="grid max-w-full items-end justify-items-center px-4">
      <AnimatePresence initial={false}>
        {callout ? (
          <motion.div
            animate={{ opacity: 1, scale: 1, transition: SPRING_IN, y: 0 }}
            className="col-start-1 row-start-1 flex max-w-full justify-center"
            exit={{ opacity: 0, scale: 0.98, transition: FADE_OUT, y: 2 }}
            initial={{ opacity: 0, scale: 0.96, y: 6 }}
            key={calloutKey(callout)}
          >
            <CalloutBody callout={callout} onAction={onAction} />
          </motion.div>
        ) : null}
      </AnimatePresence>
    </div>
  );
}

function CalloutBody({
  callout,
  onAction,
}: {
  callout: Callout;
  onAction: (notice: Notice) => void;
}) {
  switch (callout.kind) {
    case "hint":
      return (
        <div className={cn(CHIP, "text-foreground-muted")} {...HIT_REGION}>
          Click or hold
          <ShortcutKeys
            className="*:bg-foreground/10 *:text-foreground"
            hotkey={callout.shortcut}
          />
          to start dictating
        </div>
      );
    case "label":
      return (
        <p className={cn(CHIP, "text-foreground-muted")} {...HIT_REGION}>
          {callout.text}
        </p>
      );
    default:
      return <NoticeCard notice={callout.notice} onAction={onAction} />;
  }
}

function NoticeCard({
  notice,
  onAction,
}: {
  notice: Notice;
  onAction: (notice: Notice) => void;
}) {
  const tone = noticeTone(notice.kind);
  const onClick = useCallback(() => onAction(notice), [onAction, notice]);
  return (
    <div
      className={cn(
        "pointer-events-auto flex min-h-9 max-w-80 items-center gap-2 rounded-xl border bg-background py-1.5 ps-3 pe-1.5 font-medium text-foreground text-xs shadow-lg",
        tone === "warning" && "border-warning-subtel",
        tone === "error" && "border-destructive-subtel",
        !notice.action && "pe-3"
      )}
      data-slot="pill-notice"
      data-tone={tone}
      {...HIT_REGION}
    >
      {tone === "warning" ? (
        <Icon
          aria-hidden="true"
          className="size-3.5 shrink-0 text-warning-foreground"
          icon={Alert02Icon}
        />
      ) : null}
      {tone === "error" ? (
        <Icon
          aria-hidden="true"
          className="size-3.5 shrink-0 text-destructive-foreground"
          icon={AlertCircleIcon}
        />
      ) : null}
      <p className="min-w-0 text-pretty">{notice.text}</p>
      {notice.action ? (
        <Button
          className="ms-1"
          onClick={onClick}
          size="xs"
          type="button"
          variant="secondary"
        >
          {notice.action.label}
        </Button>
      ) : null}
    </div>
  );
}

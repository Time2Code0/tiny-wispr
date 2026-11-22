import type {
  DictationState,
  MainRoute,
  Notice,
  NoticeKind,
  PillRect,
} from "@/lib/ipc";

/** What the capsule shows. */
export type CapsuleView =
  | "hidden"
  | "idle"
  | "hover"
  | "hold"
  | "handsFree"
  | "processing"
  | "success";

export interface Size {
  height: number;
  width: number;
}

/**
 * The native window, in CSS pixels. It is larger than the capsule because the
 * hint, notices and menu are drawn above it; clicks outside them pass through.
 */
export const PILL_WINDOW: Size = { height: 280, width: 360 };

/** Visible capsule sizes in CSS pixels; the native window is sized from these. */
export const CAPSULE_SIZES: Record<Exclude<CapsuleView, "hidden">, Size> = {
  handsFree: { height: 28, width: 112 },
  hold: { height: 28, width: 88 },
  hover: { height: 14, width: 48 },
  idle: { height: 10, width: 40 },
  processing: { height: 28, width: 88 },
  success: { height: 28, width: 28 },
};

/** Extra width for the hands-free timer. */
const TIMER_WIDTH = 32;

/** Hands-free dictation shows its elapsed time after this long. */
export const TIMER_AFTER_MS = 30_000;

/** The processing label appears only when transcription takes this long. */
export const PROCESSING_LABEL_AFTER_MS = 1500;

/** The hover hint waits this long so passing over the pill doesn't flash it. */
export const HINT_AFTER_MS = 400;

/** How long the check mark shows after text was inserted. */
export const SUCCESS_MS = 600;

export function capsuleView({
  hovered,
  showWhenIdle,
  state,
  success,
}: {
  hovered: boolean;
  /** "Show the pill at all times", and not hidden for an hour. */
  showWhenIdle: boolean;
  state: DictationState;
  /** Text was just inserted. */
  success: boolean;
}): CapsuleView {
  switch (state.phase) {
    case "listening":
      return state.mode === "handsFree" ? "handsFree" : "hold";
    case "processing":
      return "processing";
    case "notice":
      return hovered ? "hover" : "idle";
    default:
      if (success) {
        return "success";
      }
      if (!showWhenIdle) {
        return "hidden";
      }
      return hovered ? "hover" : "idle";
  }
}

export function capsuleSize(view: CapsuleView, showTimer: boolean): Size {
  if (view === "hidden") {
    return { height: 0, width: 0 };
  }
  const size = CAPSULE_SIZES[view];
  if (view === "handsFree" && showTimer) {
    return { ...size, width: size.width + TIMER_WIDTH };
  }
  return size;
}

/** `0:42`, `19:00`, `1:02:03`. */
export function formatElapsed(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = String(total % 60).padStart(2, "0");
  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, "0")}:${seconds}`;
  }
  return `${minutes}:${seconds}`;
}

export type NoticeTone = "neutral" | "warning" | "error";

export function noticeTone(kind: NoticeKind): NoticeTone {
  switch (kind) {
    case "noMicrophone":
    case "error":
      return "error";
    case "pasteFailed":
    case "noModel":
    case "timeLimitWarning":
      return "warning";
    default:
      return "neutral";
  }
}

export type NoticeCommand =
  | { kind: "undo" }
  | { kind: "pasteLast" }
  | { kind: "copyLast" }
  | { kind: "openMain"; route: MainRoute };

/** What a notice's button does. */
export function noticeCommand(notice: Notice): NoticeCommand | null {
  switch (notice.action?.kind) {
    case "undo":
      return { kind: "undo" };
    case "pasteLast":
      return { kind: "pasteLast" };
    case "copyLast":
      return { kind: "copyLast" };
    case "openSettings":
      return {
        kind: "openMain",
        route: notice.kind === "noModel" ? "models" : "settings",
      };
    default:
      return null;
  }
}

/** What the live region says; it only speaks when this text changes. */
export function announcement(state: DictationState): string {
  switch (state.phase) {
    case "listening":
      return state.mode === "handsFree"
        ? "Listening hands-free. Press Escape to cancel"
        : "Listening";
    case "processing":
      return stageLabel(state);
    case "notice":
      return state.notice?.text ?? "";
    default:
      return "";
  }
}

export function stageLabel(state: DictationState): string {
  return state.stage === "polishing" ? "Polishing" : "Transcribing";
}

/** Extra margin around each interactive piece, so edges aren't fiddly. */
export const HIT_PADDING = 4;

interface RectLike {
  height: number;
  width: number;
  x: number;
  y: number;
}

/**
 * The window areas that take clicks: each rectangle padded, rounded outwards
 * to whole pixels, clipped to the window, with empty and covered ones dropped.
 */
export function hitRegions(
  rects: readonly RectLike[],
  bounds: Size,
  padding = HIT_PADDING
): PillRect[] {
  const clipped: PillRect[] = [];
  for (const rect of rects) {
    if (rect.width <= 0 || rect.height <= 0) {
      continue;
    }
    const left = Math.max(0, Math.floor(rect.x - padding));
    const top = Math.max(0, Math.floor(rect.y - padding));
    const right = Math.min(
      bounds.width,
      Math.ceil(rect.x + rect.width + padding)
    );
    const bottom = Math.min(
      bounds.height,
      Math.ceil(rect.y + rect.height + padding)
    );
    if (right > left && bottom > top) {
      clipped.push({
        height: bottom - top,
        width: right - left,
        x: left,
        y: top,
      });
    }
  }
  return clipped.filter(
    (rect, index) =>
      !clipped.some(
        (other, otherIndex) =>
          otherIndex !== index &&
          contains(other, rect) &&
          // Of two identical rectangles, keep the first.
          (!contains(rect, other) || otherIndex < index)
      )
  );
}

function contains(outer: PillRect, inner: PillRect): boolean {
  return (
    outer.x <= inner.x &&
    outer.y <= inner.y &&
    outer.x + outer.width >= inner.x + inner.width &&
    outer.y + outer.height >= inner.y + inner.height
  );
}

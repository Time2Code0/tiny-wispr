import type { DictationState, Notice } from "@/lib/ipc";

import { describe, expect, it } from "vitest";

import {
  announcement,
  CAPSULE_SIZES,
  capsuleSize,
  capsuleView,
  formatElapsed,
  hitRegions,
  noticeCommand,
  noticeTone,
} from "./model";

const IDLE: DictationState = {
  elapsedMs: 0,
  mode: null,
  notice: null,
  phase: "idle",
  stage: null,
};

const notice = (patch: Partial<Notice>): Notice => ({
  action: null,
  kind: "error",
  text: "Something went wrong",
  ...patch,
});

describe("capsuleView", () => {
  const view = (
    state: Partial<DictationState>,
    options: {
      hovered?: boolean;
      showWhenIdle?: boolean;
      success?: boolean;
    } = {}
  ) =>
    capsuleView({
      hovered: options.hovered ?? false,
      showWhenIdle: options.showWhenIdle ?? true,
      state: { ...IDLE, ...state },
      success: options.success ?? false,
    });

  it("follows the dictation phase and mode", () => {
    expect(view({})).toBe("idle");
    expect(view({}, { hovered: true })).toBe("hover");
    expect(view({ mode: "hold", phase: "listening" })).toBe("hold");
    expect(view({ mode: "handsFree", phase: "listening" })).toBe("handsFree");
    expect(view({ phase: "processing", stage: "polishing" })).toBe(
      "processing"
    );
  });

  it("hides when idle only if the pill isn't shown at all times", () => {
    expect(view({}, { showWhenIdle: false })).toBe("hidden");
    expect(view({}, { hovered: true, showWhenIdle: false })).toBe("hidden");
    expect(
      view({ mode: "hold", phase: "listening" }, { showWhenIdle: false })
    ).toBe("hold");
    expect(
      view({ notice: notice({}), phase: "notice" }, { showWhenIdle: false })
    ).toBe("idle");
  });

  it("shows the check after insertion, even when hidden while idle", () => {
    expect(view({}, { showWhenIdle: false, success: true })).toBe("success");
    expect(view({ mode: "hold", phase: "listening" }, { success: true })).toBe(
      "hold"
    );
  });
});

describe("capsuleSize", () => {
  it("widens hands-free for the timer only", () => {
    const plain = capsuleSize("handsFree", false);
    expect(plain).toEqual(CAPSULE_SIZES.handsFree);
    expect(capsuleSize("handsFree", true).width).toBeGreaterThan(plain.width);
    expect(capsuleSize("hold", true)).toEqual(CAPSULE_SIZES.hold);
    expect(capsuleSize("hidden", false)).toEqual({ height: 0, width: 0 });
  });
});

describe("formatElapsed", () => {
  it("formats minutes and seconds", () => {
    expect(formatElapsed(0)).toBe("0:00");
    expect(formatElapsed(42_999)).toBe("0:42");
    expect(formatElapsed(65_000)).toBe("1:05");
    expect(formatElapsed(19 * 60_000)).toBe("19:00");
  });

  it("adds hours when needed and never goes negative", () => {
    expect(formatElapsed(3_723_000)).toBe("1:02:03");
    expect(formatElapsed(-500)).toBe("0:00");
  });
});

describe("noticeTone", () => {
  it("maps each kind to a tone", () => {
    expect(noticeTone("cancelled")).toBe("neutral");
    expect(noticeTone("noSpeech")).toBe("neutral");
    expect(noticeTone("stillProcessing")).toBe("neutral");
    expect(noticeTone("pasteFailed")).toBe("warning");
    expect(noticeTone("noModel")).toBe("warning");
    expect(noticeTone("timeLimitWarning")).toBe("warning");
    expect(noticeTone("noMicrophone")).toBe("error");
    expect(noticeTone("error")).toBe("error");
  });
});

describe("noticeCommand", () => {
  it("maps actions to commands", () => {
    expect(noticeCommand(notice({}))).toBeNull();
    expect(
      noticeCommand(notice({ action: { kind: "undo", label: "Undo" } }))
    ).toEqual({ kind: "undo" });
    expect(
      noticeCommand(notice({ action: { kind: "copyLast", label: "Copy" } }))
    ).toEqual({ kind: "copyLast" });
    expect(
      noticeCommand(notice({ action: { kind: "pasteLast", label: "Paste" } }))
    ).toEqual({ kind: "pasteLast" });
  });

  it("opens models for a missing model and settings otherwise", () => {
    const action = { kind: "openSettings", label: "Open settings" } as const;
    expect(noticeCommand(notice({ action, kind: "noModel" }))).toEqual({
      kind: "openMain",
      route: "models",
    });
    expect(noticeCommand(notice({ action, kind: "noMicrophone" }))).toEqual({
      kind: "openMain",
      route: "settings",
    });
  });
});

describe("announcement", () => {
  it("names the phase without the elapsed time", () => {
    expect(announcement(IDLE)).toBe("");
    expect(
      announcement({
        ...IDLE,
        elapsedMs: 1000,
        mode: "hold",
        phase: "listening",
      })
    ).toBe(
      announcement({
        ...IDLE,
        elapsedMs: 9000,
        mode: "hold",
        phase: "listening",
      })
    );
    expect(
      announcement({ ...IDLE, phase: "processing", stage: "polishing" })
    ).toBe("Polishing");
    expect(
      announcement({
        ...IDLE,
        notice: notice({ text: "Didn't catch that" }),
        phase: "notice",
      })
    ).toBe("Didn't catch that");
  });
});

describe("hitRegions", () => {
  const bounds = { height: 132, width: 360 };

  it("pads and rounds outwards", () => {
    expect(
      hitRegions([{ height: 10, width: 40, x: 160.4, y: 113.6 }], bounds, 4)
    ).toEqual([{ height: 19, width: 49, x: 156, y: 109 }]);
  });

  it("clips to the window and drops empty rectangles", () => {
    expect(
      hitRegions(
        [
          { height: 20, width: 20, x: 350, y: 120 },
          { height: 0, width: 40, x: 10, y: 10 },
          { height: 10, width: 10, x: 400, y: 10 },
        ],
        bounds,
        4
      )
    ).toEqual([{ height: 16, width: 14, x: 346, y: 116 }]);
  });

  it("drops rectangles covered by another, keeping one of duplicates", () => {
    const card = { height: 40, width: 200, x: 80, y: 40 };
    const button = { height: 24, width: 50, x: 220, y: 48 };
    expect(hitRegions([button, card, card], bounds, 0)).toEqual([card]);
  });
});

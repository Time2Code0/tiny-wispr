import { describe, expect, it } from "vitest";

import {
  defaultShortcuts,
  formatHotkey,
  hotkeyFromKeyboardEvent,
  hotkeyParts,
  hotkeysEqual,
} from "./hotkey";

describe("formatHotkey", () => {
  it("formats macOS shortcuts like the Rust side", () => {
    expect(formatHotkey({ key: null, modifiers: ["fn"] }, "macos")).toBe("fn");
    expect(formatHotkey({ key: "Space", modifiers: ["fn"] }, "macos")).toBe(
      "fn Space"
    );
    expect(
      formatHotkey({ key: "KeyV", modifiers: ["meta", "ctrl"] }, "macos")
    ).toBe("⌃⌘V");
    expect(
      formatHotkey({ key: null, modifiers: ["alt", "ctrl"] }, "macos")
    ).toBe("⌃⌥");
    expect(formatHotkey({ key: "Escape", modifiers: [] }, "macos")).toBe("esc");
  });

  it("joins with + on Windows and Linux", () => {
    expect(
      formatHotkey({ key: null, modifiers: ["meta", "ctrl"] }, "windows")
    ).toBe("Ctrl+Win");
    expect(
      formatHotkey({ key: "KeyZ", modifiers: ["shift", "alt"] }, "linux")
    ).toBe("Alt+Shift+Z");
    expect(
      formatHotkey({ key: "Space", modifiers: ["ctrl", "meta"] }, "linux")
    ).toBe("Ctrl+Super+Space");
  });

  it("matches the documented defaults", () => {
    const mac = defaultShortcuts("macos");
    expect(formatHotkey(mac.pushToTalk, "macos")).toBe("fn");
    expect(formatHotkey(mac.handsFree, "macos")).toBe("fn Space");
    expect(formatHotkey(mac.pasteLast, "macos")).toBe("⌃⌘V");
    const win = defaultShortcuts("windows");
    expect(formatHotkey(win.pushToTalk, "windows")).toBe("Ctrl+Win");
    expect(formatHotkey(win.handsFree, "windows")).toBe("Ctrl+Win+Space");
    expect(formatHotkey(win.pasteLast, "windows")).toBe("Alt+Shift+Z");
  });
});

describe("hotkey helpers", () => {
  it("splits into parts for <Kbd>", () => {
    expect(
      hotkeyParts({ key: "Digit1", modifiers: ["shift"] }, "macos")
    ).toEqual(["⇧", "1"]);
  });

  it("reads a keyboard event", () => {
    const fromKey = hotkeyFromKeyboardEvent({
      altKey: false,
      code: "KeyV",
      ctrlKey: true,
      metaKey: true,
      shiftKey: false,
    });
    expect(fromKey).toEqual({ key: "KeyV", modifiers: ["ctrl", "meta"] });
    const modifierOnly = hotkeyFromKeyboardEvent({
      altKey: true,
      code: "ControlLeft",
      ctrlKey: true,
      metaKey: false,
      shiftKey: false,
    });
    expect(modifierOnly).toEqual({ key: null, modifiers: ["ctrl", "alt"] });
  });

  it("compares regardless of modifier order", () => {
    expect(
      hotkeysEqual(
        { key: null, modifiers: ["meta", "ctrl"] },
        { key: null, modifiers: ["ctrl", "meta"] }
      )
    ).toBe(true);
  });
});

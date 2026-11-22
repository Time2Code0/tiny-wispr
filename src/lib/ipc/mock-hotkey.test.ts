import type { Hotkey, Modifier } from "./types";

import { describe, expect, it } from "vitest";

import { defaultShortcuts } from "@/lib/hotkey";

import { validateHotkey } from "./mock-hotkey";

const hk = (modifiers: Modifier[], key: string | null = null): Hotkey => ({
  key,
  modifiers,
});

const mac = defaultShortcuts("macos");
const win = defaultShortcuts("windows");

describe("mock validate_hotkey", () => {
  it.each([
    ["empty", hk([]), "Press a shortcut"],
    ["too many keys", hk(["ctrl", "alt", "shift"], "KeyD"), "Use up to 3 keys"],
    ["bare letter", hk([], "KeyD"), "Add a modifier key such as Ctrl or Fn"],
    [
      "escape outside cancel",
      hk([], "Escape"),
      "Escape alone only works for Cancel",
    ],
    ["reserved", hk(["meta"], "KeyC"), "That shortcut is reserved by macOS"],
    ["duplicate", mac.handsFree, "Already used for Hands-free"],
  ])("rejects %s with the app's message", (_, hotkey, message) => {
    expect(validateHotkey("pushToTalk", hotkey, "macos", mac)?.message).toBe(
      message
    );
  });

  it("allows what the app allows", () => {
    expect(validateHotkey("cancel", hk([], "Escape"), "macos", mac)).toBeNull();
    expect(
      validateHotkey("pushToTalk", hk([], "F13"), "macos", mac)
    ).toBeNull();
    expect(
      validateHotkey("pushToTalk", hk(["ctrl", "alt"], "KeyD"), "macos", mac)
    ).toBeNull();
    expect(
      validateHotkey("pushToTalk", mac.pushToTalk, "macos", mac)
    ).toBeNull();
  });

  it("names the platform and the conflicting action", () => {
    expect(
      validateHotkey("pushToTalk", hk(["fn"], "KeyD"), "windows", win)
    ).toEqual({
      conflictsWith: null,
      kind: "reserved",
      message: "Fn can't be used on Windows",
    });
    expect(
      validateHotkey("pushToTalk", win.pasteLast, "windows", win)?.conflictsWith
    ).toBe("pasteLast");
    expect(
      validateHotkey("pushToTalk", hk(["meta"]), "windows", win)?.kind
    ).toBe("reserved");
  });
});

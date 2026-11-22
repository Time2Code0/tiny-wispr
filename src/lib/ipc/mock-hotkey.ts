import type {
  Hotkey,
  HotkeyAction,
  HotkeyIssue,
  Modifier,
  Platform,
  Shortcuts,
} from "./types";

import { hotkeyKeyCount, hotkeysEqual, MAX_HOTKEY_KEYS } from "@/lib/hotkey";

/**
 * The common cases of `tw_core::hotkey::validate`, with the same messages, so
 * the browser mock rejects what the app rejects.
 */

const ACTION_LABELS: Record<HotkeyAction, string> = {
  cancel: "Cancel",
  commandMode: "Command mode",
  handsFree: "Hands-free",
  pasteLast: "Paste last transcript",
  pushToTalk: "Push to talk",
};

const PLATFORM_NAMES: Record<Platform, string> = {
  linux: "Linux",
  macos: "macOS",
  windows: "Windows",
};

const ARROWS = ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"];

/** A modifier set and the keys reserved with exactly those modifiers. */
type Reserved = [Modifier[], string[]];

const RESERVED: Record<Platform, Reserved[]> = {
  linux: [
    [["ctrl"], ["KeyA", "KeyC", "KeyS", "KeyV", "KeyX", "KeyZ"]],
    [["alt"], ["Tab", "F2", "F4"]],
    [
      ["ctrl", "alt"],
      ["Delete", "KeyT", "KeyL", "Backspace", ...ARROWS],
    ],
    [["meta"], ["KeyA", "KeyD", "KeyL", "KeyV", "Tab", "Space", ...ARROWS]],
    [[], ["PrintScreen", "F11"]],
  ],
  macos: [
    [
      ["meta"],
      ["KeyA", "KeyC", "KeyH", "KeyM", "KeyQ", "KeyS", "KeyV", "KeyW", "KeyX"],
    ],
    [["meta"], ["KeyZ", "Tab", "Space", "Comma", "Backquote"]],
    [
      ["shift", "meta"],
      ["KeyZ", "KeyQ", "Digit3", "Digit4", "Digit5"],
    ],
    [
      ["alt", "meta"],
      ["Escape", "KeyD", "KeyH"],
    ],
    [
      ["ctrl", "meta"],
      ["KeyQ", "KeyF", "Space"],
    ],
    [["ctrl"], [...ARROWS, "Space"]],
    [["fn"], ARROWS],
    [[], ["F11"]],
  ],
  windows: [
    [["ctrl"], ["KeyA", "KeyC", "KeyS", "KeyV", "KeyX", "KeyY", "KeyZ"]],
    [["alt"], ["Tab", "F4", "Escape", "Space"]],
    [["ctrl", "alt"], ["Delete"]],
    [["meta"], ["KeyD", "KeyE", "KeyL", "KeyR", "KeyV", "Tab", ...ARROWS]],
    [[], ["PrintScreen", "F5", "F11"]],
  ],
};

const FUNCTION_KEY = /^F([1-9]|1\d|2[0-4])$/;

const issue = (
  kind: HotkeyIssue["kind"],
  message: string,
  conflictsWith: HotkeyAction | null = null
): HotkeyIssue => ({ conflictsWith, kind, message });

function isReserved(hotkey: Hotkey, platform: Platform): boolean {
  if (!hotkey.key) {
    // The Windows key and Super alone open the start menu.
    return (
      platform !== "macos" &&
      hotkeysEqual(hotkey, { key: null, modifiers: ["meta"] })
    );
  }
  const { key } = hotkey;
  return RESERVED[platform].some(
    ([modifiers, keys]) =>
      hotkeysEqual(hotkey, { key, modifiers }) && keys.includes(key)
  );
}

function checkBareKey(
  action: HotkeyAction,
  key: string,
  platform: Platform
): HotkeyIssue | null {
  if (key === "Escape") {
    return action === "cancel"
      ? null
      : issue("escapeOnlyForCancel", "Escape alone only works for Cancel");
  }
  if (key === "CapsLock") {
    return action === "pushToTalk" || action === "handsFree"
      ? null
      : issue(
          "needsModifier",
          "Caps Lock alone only works for push to talk and hands-free"
        );
  }
  if (FUNCTION_KEY.test(key)) {
    return null;
  }
  return issue(
    "needsModifier",
    platform === "macos"
      ? "Add a modifier key such as Ctrl or Fn"
      : "Add a modifier key such as Ctrl or Alt"
  );
}

export function validateHotkey(
  action: HotkeyAction,
  hotkey: Hotkey,
  platform: Platform,
  current: Shortcuts
): HotkeyIssue | null {
  const count = hotkeyKeyCount(hotkey);
  if (count === 0) {
    return issue("empty", "Press a shortcut");
  }
  if (count > MAX_HOTKEY_KEYS) {
    return issue("tooManyKeys", "Use up to 3 keys");
  }
  if (platform !== "macos" && hotkey.modifiers.includes("fn")) {
    return issue("reserved", `Fn can't be used on ${PLATFORM_NAMES[platform]}`);
  }
  if (isReserved(hotkey, platform)) {
    return issue(
      "reserved",
      `That shortcut is reserved by ${PLATFORM_NAMES[platform]}`
    );
  }
  if (hotkey.modifiers.length === 0) {
    const bare = checkBareKey(action, hotkey.key ?? "", platform);
    if (bare) {
      return bare;
    }
  }
  const other = (Object.keys(current) as HotkeyAction[]).find(
    (a) => a !== action && hotkeysEqual(current[a], hotkey)
  );
  if (other) {
    return issue(
      "duplicate",
      `Already used for ${ACTION_LABELS[other]}`,
      other
    );
  }
  return null;
}

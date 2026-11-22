import type {
  Hotkey,
  KeyCode,
  Modifier,
  Platform,
  Shortcuts,
} from "@/lib/ipc/types";

import { MODIFIERS } from "@/lib/ipc/types";

/** Mirrors `tw_core::hotkey::MAX_KEYS`. */
export const MAX_HOTKEY_KEYS = 3;

const MAC_MODIFIER_LABELS: Record<Modifier, string> = {
  alt: "⌥",
  ctrl: "⌃",
  fn: "fn",
  meta: "⌘",
  shift: "⇧",
};

const SYMBOL_KEY_LABELS: Record<string, string> = {
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  ArrowUp: "↑",
  Backquote: "`",
  Backslash: "\\",
  BracketLeft: "[",
  BracketRight: "]",
  Comma: ",",
  Equal: "=",
  Minus: "-",
  Period: ".",
  Quote: "'",
  Semicolon: ";",
  Slash: "/",
};

const MAC_KEY_LABELS: Record<string, string> = {
  Backspace: "⌫",
  CapsLock: "⇪",
  Enter: "↩",
  Escape: "esc",
  Tab: "⇥",
};

const OTHER_KEY_LABELS: Record<string, string> = {
  CapsLock: "Caps Lock",
  Escape: "Esc",
};

const APPLE_AGENT = /Mac|iPhone|iPad/;
const WINDOWS_AGENT = /Win/;

export function detectPlatform(): Platform {
  if (typeof navigator === "undefined") {
    return "macos";
  }
  const agent = navigator.userAgent;
  if (APPLE_AGENT.test(agent)) {
    return "macos";
  }
  if (WINDOWS_AGENT.test(agent)) {
    return "windows";
  }
  return "linux";
}

export function modifierLabel(modifier: Modifier, platform: Platform): string {
  if (platform === "macos") {
    return MAC_MODIFIER_LABELS[modifier];
  }
  switch (modifier) {
    case "fn":
      return "Fn";
    case "ctrl":
      return "Ctrl";
    case "alt":
      return "Alt";
    case "shift":
      return "Shift";
    case "meta":
      return platform === "windows" ? "Win" : "Super";
    default:
      return modifier;
  }
}

export function keyLabel(code: KeyCode, platform: Platform): string {
  if (code.startsWith("Key")) {
    return code.slice(3);
  }
  if (code.startsWith("Digit")) {
    return code.slice(5);
  }
  const platformLabels =
    platform === "macos" ? MAC_KEY_LABELS : OTHER_KEY_LABELS;
  return platformLabels[code] ?? SYMBOL_KEY_LABELS[code] ?? code;
}

export function sortModifiers(modifiers: Iterable<Modifier>): Modifier[] {
  const set = new Set(modifiers);
  return MODIFIERS.filter((m) => set.has(m));
}

/** Labels of each key in display order, e.g. `["⌃", "⌘", "V"]`. */
export function hotkeyParts(hotkey: Hotkey, platform: Platform): string[] {
  const parts = sortModifiers(hotkey.modifiers).map((m) =>
    modifierLabel(m, platform)
  );
  if (hotkey.key) {
    parts.push(keyLabel(hotkey.key, platform));
  }
  return parts;
}

/**
 * `"fn"`, `"⌃⌘V"`, `"fn Space"` on macOS; `"Ctrl+Win"`, `"Alt+Shift+Z"`
 * elsewhere. Mirrors `Hotkey::display` in `tw_core::hotkey`.
 */
export function formatHotkey(hotkey: Hotkey, platform: Platform): string {
  const parts = hotkeyParts(hotkey, platform);
  if (platform !== "macos") {
    return parts.join("+");
  }
  let out = "";
  for (const [i, part] of parts.entries()) {
    const previous = parts[i - 1];
    if (
      previous !== undefined &&
      ([...previous].length > 1 || [...part].length > 1)
    ) {
      out += " ";
    }
    out += part;
  }
  return out;
}

export function hotkeyKeyCount(hotkey: Hotkey): number {
  return hotkey.modifiers.length + (hotkey.key ? 1 : 0);
}

export function hotkeysEqual(a: Hotkey, b: Hotkey): boolean {
  return (
    a.key === b.key &&
    sortModifiers(a.modifiers).join() === sortModifiers(b.modifiers).join()
  );
}

const combo = (modifiers: Modifier[], key: KeyCode | null = null): Hotkey => ({
  key,
  modifiers: sortModifiers(modifiers),
});

/** Mirrors `Shortcuts::defaults` (spec §2.1). */
export function defaultShortcuts(platform: Platform): Shortcuts {
  if (platform === "macos") {
    return {
      cancel: combo([], "Escape"),
      commandMode: combo(["fn", "ctrl"]),
      handsFree: combo(["fn"], "Space"),
      pasteLast: combo(["ctrl", "meta"], "KeyV"),
      pushToTalk: combo(["fn"]),
    };
  }
  return {
    cancel: combo([], "Escape"),
    commandMode: combo(["ctrl", "meta", "alt"]),
    handsFree: combo(["ctrl", "meta"], "Space"),
    pasteLast: combo(["alt", "shift"], "KeyZ"),
    pushToTalk: combo(["ctrl", "meta"]),
  };
}

const MODIFIER_CODES: Record<string, Modifier> = {
  AltLeft: "alt",
  AltRight: "alt",
  ControlLeft: "ctrl",
  ControlRight: "ctrl",
  Fn: "fn",
  MetaLeft: "meta",
  MetaRight: "meta",
  ShiftLeft: "shift",
  ShiftRight: "shift",
};

/**
 * The combination held during a browser keyboard event. Browsers never see
 * Fn, so the real recorder lives in Rust (`hotkey:captured`); this is for the
 * mock backend and for in-page shortcuts.
 */
export function hotkeyFromKeyboardEvent(
  event: Pick<
    KeyboardEvent,
    "altKey" | "code" | "ctrlKey" | "metaKey" | "shiftKey"
  >
): Hotkey {
  const modifiers: Modifier[] = [];
  if (event.ctrlKey) {
    modifiers.push("ctrl");
  }
  if (event.altKey) {
    modifiers.push("alt");
  }
  if (event.shiftKey) {
    modifiers.push("shift");
  }
  if (event.metaKey) {
    modifiers.push("meta");
  }
  const modifierFromCode = MODIFIER_CODES[event.code];
  if (modifierFromCode) {
    modifiers.push(modifierFromCode);
    return combo(modifiers);
  }
  return combo(modifiers, event.code || null);
}

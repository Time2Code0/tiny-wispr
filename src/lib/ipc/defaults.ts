import type { Platform, Settings, SettingsPatch } from "./types";

import { defaultShortcuts } from "@/lib/hotkey";

/** Mirrors `impl Default for Settings` in `tw_core::settings`. */
export function defaultSettings(platform: Platform): Settings {
  return {
    cleanupLevel: "light",
    dictationMode: "hold",
    historyRetention: "forever",
    insertionMethod: "paste",
    languages: { autoDetect: true, selected: ["en"] },
    launchAtLogin: false,
    llmModel: null,
    microphone: null,
    offline: false,
    onboardingCompleted: false,
    onboardingStep: "welcome",
    pillHiddenUntil: null,
    restoreClipboard: true,
    shortcuts: defaultShortcuts(platform),
    showInDock: true,
    showPillAlways: true,
    sounds: true,
    sttModel: null,
    styles: {
      email: "formal",
      other: "formal",
      personal: "formal",
      work: "formal",
    },
    theme: "system",
  };
}

/** Mirrors `Settings::apply`: absent keys are left alone. */
export function applyPatch(settings: Settings, patch: SettingsPatch): Settings {
  const next = { ...settings };
  for (const key of Object.keys(patch) as (keyof Settings)[]) {
    const value = patch[key];
    if (value !== undefined) {
      Object.assign(next, { [key]: value });
    }
  }
  return next;
}

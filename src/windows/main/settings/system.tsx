import type { InsertionMethod, Theme } from "@/lib/ipc";

import { useCallback } from "react";

import { cn } from "cn";

import {
  Radio,
  RadioGroup,
  RadioGroupPrimitive,
  RadioPrimitive,
} from "@/components/ui/radio-group";
import {
  SegmentedControl,
  SegmentedControlItem,
} from "@/components/ui/segmented-control";
import { useIpcQuery } from "@/hooks/use-ipc-query";
import { getHardwareInfo } from "@/lib/ipc";

import { Section, SettingGroup, SettingRow } from "../page";
import { useSavedSettings } from "../use-saved-settings";
import { SwitchRow } from "./switch-row";

const INSERTION: {
  description: string;
  label: string;
  value: InsertionMethod;
}[] = [
  {
    description:
      "Fast. Puts the text on the clipboard for a moment and pastes it.",
    label: "Paste",
    value: "paste",
  },
  {
    description:
      "Slower. Types the text key by key, for apps that block pasting.",
    label: "Type it out",
    value: "type",
  },
];

const THEMES: { label: string; value: Theme }[] = [
  { label: "System", value: "system" },
  { label: "Light", value: "light" },
  { label: "Dark", value: "dark" },
];

const isInsertion = (value: unknown): value is InsertionMethod =>
  value === "paste" || value === "type";
const isTheme = (value: unknown): value is Theme =>
  value === "system" || value === "light" || value === "dark";

export function SystemSettings() {
  const { settings, save } = useSavedSettings();
  const hardware = useIpcQuery(getHardwareInfo);
  const mac = hardware.data?.platform === "macos";

  const setLaunch = useCallback(
    (launchAtLogin: boolean) => save({ launchAtLogin }),
    [save]
  );
  const setPill = useCallback(
    (showPillAlways: boolean) => save({ showPillAlways }),
    [save]
  );
  const setSounds = useCallback((sounds: boolean) => save({ sounds }), [save]);
  const setDock = useCallback(
    (showInDock: boolean) => save({ showInDock }),
    [save]
  );
  const setRestore = useCallback(
    (restoreClipboard: boolean) => save({ restoreClipboard }),
    [save]
  );
  const setInsertion = useCallback(
    (value: unknown) => {
      if (isInsertion(value)) {
        save({ insertionMethod: value });
      }
    },
    [save]
  );
  const setTheme = useCallback(
    (value: unknown) => {
      if (isTheme(value)) {
        save({ theme: value });
      }
    },
    [save]
  );

  if (!settings) {
    return null;
  }

  return (
    <>
      <Section title="App">
        <SettingGroup>
          <SwitchRow
            checked={settings.launchAtLogin}
            description="Start Tiny Wispr when you log in, so it's ready when you are."
            label="Launch at login"
            onCheckedChange={setLaunch}
          />
          <SwitchRow
            checked={settings.showPillAlways}
            description="When off, the pill only appears while you dictate."
            label="Show the pill at all times"
            onCheckedChange={setPill}
          />
          <SwitchRow
            checked={settings.sounds}
            description="A soft sound when the microphone starts and stops."
            label="Sounds"
            onCheckedChange={setSounds}
          />
          {mac ? (
            <SwitchRow
              checked={settings.showInDock}
              description="When off, Tiny Wispr lives in the menu bar only."
              label="Show in Dock"
              onCheckedChange={setDock}
            />
          ) : null}
        </SettingGroup>
      </Section>

      <Section
        description="How your words get into the app you're using."
        title="Inserting text"
      >
        <RadioGroup
          aria-label="Insertion method"
          className="grid grid-cols-2 gap-2"
          onValueChange={setInsertion}
          value={settings.insertionMethod}
        >
          {INSERTION.map((option) => (
            // biome-ignore lint/a11y/noLabelWithoutControl: the Radio inside is the control
            <label
              className={cn(
                "flex cursor-pointer flex-col gap-1 rounded-xl border bg-background px-4 py-3 transition-colors hover:bg-accent has-data-checked:border-foreground/40 has-data-checked:bg-background-muted motion-reduce:transition-none"
              )}
              key={option.value}
            >
              <span className="flex items-center gap-2 font-medium text-foreground text-sm">
                <Radio value={option.value} />
                {option.label}
              </span>
              <span className="text-pretty text-foreground-muted text-xs">
                {option.description}
              </span>
            </label>
          ))}
        </RadioGroup>
        <SettingGroup>
          <SwitchRow
            checked={settings.restoreClipboard}
            description={
              settings.insertionMethod === "paste"
                ? "Put back whatever you had copied before."
                : "Only applies when pasting."
            }
            disabled={settings.insertionMethod !== "paste"}
            label="Restore my clipboard after pasting"
            onCheckedChange={setRestore}
          />
        </SettingGroup>
      </Section>

      <Section title="Appearance">
        <SettingGroup>
          <SettingRow
            control={
              <SegmentedControl
                aria-label="Theme"
                render={
                  <RadioGroupPrimitive
                    onValueChange={setTheme}
                    value={settings.theme}
                  />
                }
                size="sm"
              >
                {THEMES.map((theme) => (
                  <SegmentedControlItem
                    key={theme.value}
                    render={<RadioPrimitive.Root value={theme.value} />}
                  >
                    {theme.label}
                  </SegmentedControlItem>
                ))}
              </SegmentedControl>
            }
            description="System follows your computer's light or dark setting."
            label="Theme"
          />
        </SettingGroup>
      </Section>
    </>
  );
}

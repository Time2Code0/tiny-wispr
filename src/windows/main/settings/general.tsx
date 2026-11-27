import type { Hotkey, HotkeyAction, Settings } from "@/lib/ipc";
import type { Language } from "./languages";

import { useCallback, useMemo, useState } from "react";

import { HotkeyRecorder } from "@/components/hotkey-recorder";
import { MicTest } from "@/components/mic-test";
import { MicrophoneSelect } from "@/components/microphone-select";
import { Button } from "@/components/ui/button";
import {
  Combobox,
  ComboboxChip,
  ComboboxChips,
  ComboboxChipsInput,
  ComboboxEmpty,
  ComboboxItem,
  ComboboxList,
  ComboboxPopup,
  ComboboxValue,
} from "@/components/ui/combobox";
import { toastManager } from "@/components/ui/toast";
import { useIpcQuery } from "@/hooks/use-ipc-query";
import { useSettings } from "@/hooks/use-settings";
import { defaultShortcuts, detectPlatform } from "@/lib/hotkey";
import {
  getDefaultShortcuts,
  getHardwareInfo,
  listMicrophones,
  listModels,
  resetShortcuts,
} from "@/lib/ipc";

import { Section, SettingGroup } from "../page";
import { hashFor } from "../route";
import { useSavedSettings } from "../use-saved-settings";
import { WHISPER_LANGUAGES } from "./languages";
import { SwitchRow } from "./switch-row";

const SHORTCUTS: {
  action: HotkeyAction;
  description: string;
  label: string;
}[] = [
  {
    action: "pushToTalk",
    description: "Hold to dictate, release to paste. Tap twice for hands-free.",
    label: "Push to talk",
  },
  {
    action: "handsFree",
    description: "Press to start, press again to stop and paste.",
    label: "Hands-free",
  },
  {
    action: "cancel",
    description: "Stop and throw away what you said.",
    label: "Cancel",
  },
  {
    action: "pasteLast",
    description: "Paste your last dictation again.",
    label: "Paste last transcript",
  },
];

const MODELS_EVENTS = ["models:changed"] as const;

function ShortcutRow({
  action,
  description,
  label,
  settings,
}: {
  action: HotkeyAction;
  description: string;
  label: string;
  settings: Settings;
}) {
  const { update } = useSettings();
  const hardware = useIpcQuery(getHardwareInfo);
  const defaults = useIpcQuery(getDefaultShortcuts);
  const platform = hardware.data?.platform ?? detectPlatform();
  const change = useCallback(
    (hotkey: Hotkey) =>
      update({ shortcuts: { ...settings.shortcuts, [action]: hotkey } }),
    [action, settings.shortcuts, update]
  );
  return (
    <div className="flex items-start justify-between gap-6 px-4 py-3.5">
      <div className="flex min-w-0 flex-col gap-0.5">
        <span className="text-foreground text-sm">{label}</span>
        <span className="text-pretty text-foreground-muted text-xs">
          {description}
        </span>
      </div>
      <HotkeyRecorder
        action={action}
        className="shrink-0 items-end"
        defaultValue={(defaults.data ?? defaultShortcuts(platform))[action]}
        label={label}
        onChange={change}
        platform={platform}
        value={settings.shortcuts[action]}
      />
    </div>
  );
}

function Shortcuts({ settings }: { settings: Settings }) {
  const [resetting, setResetting] = useState(false);
  const reset = useCallback(async () => {
    setResetting(true);
    try {
      await resetShortcuts();
      toastManager.add({ title: "Shortcuts reset", type: "success" });
    } catch {
      toastManager.add({
        title: "Couldn't reset the shortcuts",
        type: "error",
      });
    } finally {
      setResetting(false);
    }
  }, []);

  return (
    <Section
      action={
        <Button
          loading={resetting}
          onClick={reset}
          size="sm"
          type="button"
          variant="ghost"
        >
          Reset to default
        </Button>
      }
      description="Click a shortcut, then press the keys you want."
      title="Shortcuts"
    >
      <SettingGroup>
        {SHORTCUTS.map((shortcut) => (
          <ShortcutRow
            key={shortcut.action}
            settings={settings}
            {...shortcut}
          />
        ))}
      </SettingGroup>
    </Section>
  );
}

function Microphone({
  microphone,
  onChange,
}: {
  microphone: string | null;
  onChange: (id: string | null) => void;
}) {
  const microphones = useIpcQuery(listMicrophones);
  const [testing, setTesting] = useState(false);
  const toggle = useCallback(() => setTesting((open) => !open), []);

  return (
    <Section
      action={
        <Button
          aria-expanded={testing}
          onClick={toggle}
          size="sm"
          type="button"
          variant="outline"
        >
          {testing ? "Done" : "Test"}
        </Button>
      }
      description="System default follows the input you pick in your computer's sound settings."
      title="Microphone"
    >
      {testing ? (
        <MicTest
          className="rounded-xl border px-4 py-4"
          microphone={microphone}
          onMicrophoneChange={onChange}
        />
      ) : (
        <MicrophoneSelect
          className="max-w-sm"
          disabled={!microphones.data}
          label="Input"
          microphones={microphones.data ?? []}
          onValueChange={onChange}
          value={microphone}
        />
      )}
    </Section>
  );
}

const languageLabel = (language: Language) => language.name;
const languageValue = (language: Language) => language.code;

function Languages({
  settings,
  save,
}: {
  save: (patch: Partial<Settings>) => void;
  settings: Settings;
}) {
  const models = useIpcQuery(listModels, MODELS_EVENTS);
  const voice = models.data?.find((m) => m.id === settings.sttModel);
  const englishOnly = voice?.englishOnly === true;
  const { autoDetect, selected } = settings.languages;
  const [issue, setIssue] = useState<string | null>(null);

  const value = useMemo(
    () => WHISPER_LANGUAGES.filter((l) => selected.includes(l.code)),
    [selected]
  );

  const setAutoDetect = useCallback(
    (on: boolean) => {
      setIssue(null);
      save({
        languages: {
          autoDetect: on,
          selected: on || selected.length > 0 ? selected : ["en"],
        },
      });
    },
    [save, selected]
  );

  const choose = useCallback(
    (next: Language[]) => {
      if (next.length === 0) {
        setIssue(
          "Keep at least one language, or turn on Detect automatically."
        );
        return;
      }
      setIssue(null);
      save({
        languages: { autoDetect: false, selected: next.map((l) => l.code) },
      });
    },
    [save]
  );

  return (
    <Section
      description="Which languages you speak. Picking them helps when you switch between a few."
      title="Languages"
    >
      {englishOnly ? (
        <p className="text-pretty rounded-xl border bg-background-muted px-4 py-3 text-foreground-muted text-sm">
          {voice.label} only understands English. To dictate in other languages,{" "}
          <a href={hashFor("models")}>choose a multilingual model</a>.
        </p>
      ) : (
        <SettingGroup>
          <SwitchRow
            checked={autoDetect}
            description="Tiny Wispr works out the language each time you speak."
            label="Detect automatically"
            onCheckedChange={setAutoDetect}
          />
          {autoDetect ? null : (
            <div className="flex flex-col gap-2 px-4 py-3.5">
              <Combobox
                items={WHISPER_LANGUAGES}
                itemToStringLabel={languageLabel}
                itemToStringValue={languageValue}
                multiple
                onValueChange={choose}
                value={value}
              >
                <ComboboxChips>
                  <ComboboxValue>
                    {(chosen: Language[]) => (
                      <>
                        {chosen.map((language) => (
                          <ComboboxChip
                            aria-label={language.name}
                            key={language.code}
                            removeProps={{
                              "aria-label": `Remove ${language.name}`,
                            }}
                          >
                            {language.name}
                          </ComboboxChip>
                        ))}
                        <ComboboxChipsInput
                          aria-label="Add a language"
                          placeholder={
                            chosen.length > 0 ? "Add a language" : "Search"
                          }
                        />
                      </>
                    )}
                  </ComboboxValue>
                </ComboboxChips>
                <ComboboxPopup>
                  <ComboboxEmpty>No language by that name.</ComboboxEmpty>
                  <ComboboxList>
                    {(language: Language) => (
                      <ComboboxItem key={language.code} value={language}>
                        {language.name}
                      </ComboboxItem>
                    )}
                  </ComboboxList>
                </ComboboxPopup>
              </Combobox>
              {issue ? (
                <p className="text-destructive-foreground text-xs" role="alert">
                  {issue}
                </p>
              ) : null}
            </div>
          )}
        </SettingGroup>
      )}
    </Section>
  );
}

export function GeneralSettings() {
  const { settings, save } = useSavedSettings();
  const setMicrophone = useCallback(
    (microphone: string | null) => save({ microphone }),
    [save]
  );
  if (!settings) {
    return null;
  }
  return (
    <>
      <Shortcuts settings={settings} />
      <Microphone microphone={settings.microphone} onChange={setMicrophone} />
      <Languages save={save} settings={settings} />
    </>
  );
}

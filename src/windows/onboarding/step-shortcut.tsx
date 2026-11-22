import type { Hotkey, HotkeyAction, Platform, Settings } from "@/lib/ipc";

import { HotkeyRecorder } from "@/components/hotkey-recorder";
import { ShortcutKeys } from "@/components/shortcut-keys";
import { defaultShortcuts, hotkeysEqual } from "@/lib/hotkey";

const FN_ONLY: Hotkey = { key: null, modifiers: ["fn"] };

export function ShortcutStep({
  platform,
  settings,
  update,
}: {
  platform: Platform;
  settings: Settings;
  update: (patch: Partial<Settings>) => Promise<unknown>;
}) {
  const defaults = defaultShortcuts(platform);
  const save = (action: HotkeyAction) => (hotkey: Hotkey) =>
    update({ shortcuts: { ...settings.shortcuts, [action]: hotkey } });
  const { pushToTalk } = settings.shortcuts;

  return (
    <div className="flex flex-col gap-6">
      <div className="flex items-center justify-center rounded-xl border bg-background-muted py-8">
        <ShortcutKeys
          className="gap-2 [&_kbd]:h-12 [&_kbd]:min-w-12 [&_kbd]:rounded-lg [&_kbd]:border [&_kbd]:bg-background [&_kbd]:px-3 [&_kbd]:text-foreground [&_kbd]:text-xl"
          hotkey={pushToTalk}
          platform={platform}
        />
      </div>
      <div className="flex flex-col gap-3">
        <Row label="Push to talk">
          <HotkeyRecorder
            action="pushToTalk"
            defaultValue={defaults.pushToTalk}
            label="Push to talk"
            onChange={save("pushToTalk")}
            platform={platform}
            value={pushToTalk}
          />
        </Row>
        <Row label="Hands-free">
          <HotkeyRecorder
            action="handsFree"
            defaultValue={defaults.handsFree}
            label="Hands-free"
            onChange={save("handsFree")}
            platform={platform}
            value={settings.shortcuts.handsFree}
          />
        </Row>
      </div>
      {platform === "macos" && hotkeysEqual(pushToTalk, FN_ONLY) ? (
        <p className="text-foreground-muted text-xs">
          If fn opens the emoji picker, go to System Settings › Keyboard and set
          “Press fn key to” to “Do nothing”.
        </p>
      ) : null}
    </div>
  );
}

function Row({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="grid grid-cols-[8rem_1fr] items-start gap-4">
      <p className="pt-2 text-foreground text-sm">{label}</p>
      {children}
    </div>
  );
}

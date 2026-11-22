import type { Hotkey, Platform } from "@/lib/ipc";

import { Kbd, KbdGroup } from "@/components/ui/kbd";
import { detectPlatform, formatHotkey, hotkeyParts } from "@/lib/hotkey";

/** A shortcut as key caps, with the full combination as its accessible name. */
export function ShortcutKeys({
  hotkey,
  platform = detectPlatform(),
  className,
}: {
  className?: string;
  hotkey: Hotkey;
  platform?: Platform;
}) {
  return (
    <KbdGroup aria-label={formatHotkey(hotkey, platform)} className={className}>
      {hotkeyParts(hotkey, platform).map((part) => (
        <Kbd aria-hidden="true" key={part}>
          {part}
        </Kbd>
      ))}
    </KbdGroup>
  );
}

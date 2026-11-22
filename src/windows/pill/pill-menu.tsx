import { useCallback } from "react";

import { Settings01Icon, ViewOffSlashIcon } from "@hugeicons/core-free-icons";

import { Icon } from "@/components/ui/icon";
import {
  Menu,
  MenuGroup,
  MenuGroupLabel,
  MenuItem,
  MenuPopup,
  MenuRadioGroup,
  MenuRadioItem,
  MenuSeparator,
} from "@/components/ui/menu";
import { useIpcQuery } from "@/hooks/use-ipc-query";
import { useSettings } from "@/hooks/use-settings";
import { hidePillFor, listMicrophones, openMainWindow } from "@/lib/ipc";

const HIDE_MINUTES = 60;

/** Radio value for "follow the system default"; device ids are strings. */
const SYSTEM_DEFAULT = Symbol("system default");

function report(what: string) {
  return (error: unknown) => {
    console.error(`Could not ${what}`, error);
  };
}

const hideForAnHour = () => {
  hidePillFor(HIDE_MINUTES).catch(report("hide the pill"));
};

const openSettings = () => {
  openMainWindow("settings").catch(report("open settings"));
};

/**
 * The capsule's menu: right-click or Shift+F10 on the idle capsule. One column
 * with the microphones inline, because a submenu beside it wouldn't fit the
 * pill's window.
 */
export function PillMenu({
  anchor,
  finalFocus,
  onOpenChange,
  open,
}: {
  anchor: React.RefObject<HTMLElement | null>;
  finalFocus: React.RefObject<HTMLElement | null>;
  onOpenChange: (open: boolean) => void;
  open: boolean;
}) {
  return (
    <Menu onOpenChange={onOpenChange} open={open}>
      <MenuPopup
        anchor={anchor}
        className="w-56"
        finalFocus={finalFocus}
        side="top"
        sideOffset={6}
      >
        <MenuItem onClick={hideForAnHour}>
          <Icon aria-hidden="true" icon={ViewOffSlashIcon} />
          Hide for 1 hour
        </MenuItem>
        <MenuSeparator />
        <Microphones />
        <MenuSeparator />
        <MenuItem onClick={openSettings}>
          <Icon aria-hidden="true" icon={Settings01Icon} />
          Settings
        </MenuItem>
      </MenuPopup>
    </Menu>
  );
}

/** Mounted each time the menu opens, so the list is fresh. */
function Microphones() {
  const { settings, update } = useSettings();
  const microphones = useIpcQuery(listMicrophones);
  const current = settings?.microphone ?? SYSTEM_DEFAULT;
  const onValueChange = useCallback(
    (value: unknown) => {
      const microphone = typeof value === "string" ? value : null;
      update({ microphone }).catch(report("change the microphone"));
    },
    [update]
  );

  return (
    <MenuGroup>
      <MenuGroupLabel>Microphone</MenuGroupLabel>
      <MenuRadioGroup onValueChange={onValueChange} value={current}>
        <MenuRadioItem closeOnClick value={SYSTEM_DEFAULT}>
          System default
        </MenuRadioItem>
        {microphones.data?.map((microphone) => (
          <MenuRadioItem closeOnClick key={microphone.id} value={microphone.id}>
            <span className="block truncate">{microphone.name}</span>
          </MenuRadioItem>
        ))}
      </MenuRadioGroup>
      {microphones.error ? (
        <MenuItem disabled>Couldn't list microphones</MenuItem>
      ) : null}
      {microphones.data?.length === 0 ? (
        <MenuItem disabled>No microphone found</MenuItem>
      ) : null}
    </MenuGroup>
  );
}

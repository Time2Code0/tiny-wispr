import type { HistoryRetention, Platform } from "@/lib/ipc";

import { useCallback, useState } from "react";

import { LockIcon } from "@hugeicons/core-free-icons";

import { PermissionList, usePermissions } from "@/components/permission-list";
import {
  AlertDialog,
  AlertDialogClose,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogPopup,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Radio, RadioGroup } from "@/components/ui/radio-group";
import { toastManager } from "@/components/ui/toast";
import { useIpcQuery } from "@/hooks/use-ipc-query";
import { clearHistory, getHardwareInfo, getStats } from "@/lib/ipc";

import { Section, SettingGroup, SettingRow } from "../page";
import { useSavedSettings } from "../use-saved-settings";
import { NetworkActivity } from "./network-activity";
import { SwitchRow } from "./switch-row";

const RETENTION: {
  description: string;
  label: string;
  value: HistoryRetention;
}[] = [
  {
    description: "Every dictation stays until you delete it.",
    label: "Keep forever",
    value: "forever",
  },
  {
    description: "Dictations older than 30 days are deleted.",
    label: "30 days",
    value: "thirtyDays",
  },
  {
    description: "Dictations older than a day are deleted.",
    label: "24 hours",
    value: "oneDay",
  },
  {
    description: "Nothing is saved. Home shows no history.",
    label: "Never",
    value: "never",
  },
];

const DATA_DIR: Record<Platform, string> = {
  linux: "~/.local/share/dev.time2code.tinywispr",
  macos: "~/Library/Application Support/dev.time2code.tinywispr",
  windows: "%APPDATA%\\dev.time2code.tinywispr",
};

const STATS_EVENTS = ["history:changed"] as const;

const isRetention = (value: unknown): value is HistoryRetention =>
  RETENTION.some((option) => option.value === value);

function DeleteAllHistory() {
  const stats = useIpcQuery(getStats, STATS_EVENTS);
  const count = stats.data?.totalDictations ?? 0;
  const [open, setOpen] = useState(false);
  const [pending, setPending] = useState(false);

  const confirm = useCallback(async () => {
    setPending(true);
    try {
      await clearHistory();
      setOpen(false);
      toastManager.add({ title: "History deleted", type: "success" });
    } catch {
      toastManager.add({
        title: "Couldn't delete your history",
        type: "error",
      });
    } finally {
      setPending(false);
    }
  }, []);

  const label = `${count.toLocaleString()} ${count === 1 ? "dictation" : "dictations"}`;

  return (
    <SettingRow
      control={
        <AlertDialog onOpenChange={setOpen} open={open}>
          <AlertDialogTrigger
            disabled={count === 0}
            render={<Button size="sm" type="button" variant="outline" />}
          >
            Delete all history
          </AlertDialogTrigger>
          <AlertDialogPopup>
            <AlertDialogHeader>
              <AlertDialogTitle>Delete all history?</AlertDialogTitle>
              <AlertDialogDescription>
                This deletes {label} from this computer. Your stats start over.
                You can't undo this.
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogClose
                render={<Button type="button" variant="ghost" />}
              >
                Cancel
              </AlertDialogClose>
              <Button
                loading={pending}
                onClick={confirm}
                type="button"
                variant="destructive"
              >
                Delete {label}
              </Button>
            </AlertDialogFooter>
          </AlertDialogPopup>
        </AlertDialog>
      }
      description={
        count === 0 ? "There's nothing to delete." : `${label} kept.`
      }
      label="Delete all history"
    />
  );
}

export function PrivacySettings() {
  const { settings, save } = useSavedSettings();
  const hardware = useIpcQuery(getHardwareInfo);
  const { permissions } = usePermissions({ poll: true });

  const setOffline = useCallback(
    (offline: boolean) => save({ offline }),
    [save]
  );
  const setRetention = useCallback(
    (value: unknown) => {
      if (isRetention(value)) {
        save({ historyRetention: value });
      }
    },
    [save]
  );

  if (!settings) {
    return null;
  }

  return (
    <>
      <div className="-mt-2 flex gap-3 rounded-xl border bg-background-muted px-4 py-3.5">
        <Icon
          aria-hidden="true"
          className="mt-0.5 size-4 shrink-0 text-foreground-muted"
          icon={LockIcon}
        />
        <p className="text-pretty text-foreground text-sm">
          Tiny Wispr never sends your voice or text anywhere. The only network
          requests it makes are the model downloads you start.
        </p>
      </div>

      <Section
        description="Dictations are kept on this computer so you can find and copy them later."
        title="History"
      >
        <RadioGroup
          aria-label="Keep history"
          className="flex flex-col divide-y rounded-xl border"
          onValueChange={setRetention}
          value={settings.historyRetention}
        >
          {RETENTION.map((option) => (
            // biome-ignore lint/a11y/noLabelWithoutControl: the Radio inside is the control
            <label
              className="flex cursor-pointer items-start gap-3 px-4 py-3 transition-colors first:rounded-t-xl last:rounded-b-xl hover:bg-accent motion-reduce:transition-none"
              key={option.value}
            >
              <Radio className="mt-0.5" value={option.value} />
              <span className="flex flex-col gap-0.5">
                <span className="text-foreground text-sm">{option.label}</span>
                <span className="text-pretty text-foreground-muted text-xs">
                  {option.description}
                </span>
              </span>
            </label>
          ))}
        </RadioGroup>
        <SettingGroup>
          <DeleteAllHistory />
        </SettingGroup>
      </Section>

      {permissions ? (
        <Section
          description="Tiny Wispr needs these to hear you and to type into other apps. If you turned one off, turn it back on here."
          title="Permissions"
        >
          <PermissionList
            permissions={permissions}
            platform={hardware.data?.platform}
          />
        </Section>
      ) : null}

      <Section title="Where your data lives">
        <div className="flex flex-col gap-2 text-foreground-muted text-sm">
          <p className="text-pretty">
            Settings, history, your dictionary and snippets, and downloaded
            models are kept in one folder. Audio is never saved.
          </p>
          {hardware.data ? (
            <code className="self-start rounded-md border bg-background-muted px-2 py-1 font-mono text-foreground text-xs">
              {DATA_DIR[hardware.data.platform]}
            </code>
          ) : null}
        </div>
      </Section>

      <NetworkActivity>
        <SettingGroup>
          <SwitchRow
            checked={settings.offline}
            description="Block all network use. Model downloads won't start until you turn this off."
            label="Stay offline"
            onCheckedChange={setOffline}
          />
        </SettingGroup>
      </NetworkActivity>
    </>
  );
}

import type { Release, UpdateState } from "@/lib/ipc";

import { useCallback, useState } from "react";

import { formatProgress, progressPercent } from "@/components/model-row";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { toastManager } from "@/components/ui/toast";
import { useUpdateState } from "@/hooks/use-update-state";
import { checkForUpdates, downloadUpdate, installUpdate } from "@/lib/ipc";

import { Section, SettingGroup, SettingRow } from "../page";
import { useSavedSettings } from "../use-saved-settings";
import { noteBlocks } from "./release-notes";
import { SwitchRow } from "./switch-row";
import { type Action, updateRow } from "./update-row";

const RUN: Record<Action, () => Promise<null>> = {
  check: checkForUpdates,
  download: downloadUpdate,
  install: installUpdate,
};

const FAILED: Record<Action, string> = {
  check: "Couldn't check for updates",
  download: "Couldn't download the update",
  install: "Couldn't restart to update",
};

function releaseOf(update: UpdateState): Release | null {
  return "release" in update ? update.release : null;
}

function ReleaseNotes({ release }: { release: Release }) {
  const blocks = release.notes ? noteBlocks(release.notes) : [];
  if (blocks.length === 0) {
    return null;
  }
  return (
    <div className="flex flex-col gap-2 rounded-xl border bg-background-muted px-4 py-3.5 text-sm">
      <p className="font-medium text-foreground">
        What's new in {release.version}
      </p>
      {blocks.map((block, index) => {
        const key = `${block.kind}-${index}`;
        if (block.kind === "heading") {
          return (
            <p className="text-foreground-muted text-xs" key={key}>
              {block.text}
            </p>
          );
        }
        if (block.kind === "list") {
          return (
            <ul className="flex list-disc flex-col gap-1 ps-4" key={key}>
              {block.items.map((item) => (
                <li className="text-pretty text-foreground" key={item}>
                  {item}
                </li>
              ))}
            </ul>
          );
        }
        return (
          <p className="text-pretty text-foreground" key={key}>
            {block.text}
          </p>
        );
      })}
    </div>
  );
}

function DownloadRow({
  update,
}: {
  update: Extract<UpdateState, { state: "downloading" }>;
}) {
  const { received, release, total } = update;
  return (
    <div className="flex flex-col gap-2 px-4 py-3.5">
      <span className="text-foreground text-sm">
        Downloading version {release.version}
      </span>
      <div className="flex items-center gap-3">
        <Progress
          aria-label={`Downloading version ${release.version}`}
          className="flex-1"
          value={total === null ? null : progressPercent(received, total)}
        />
        {total === null ? null : (
          <span className="shrink-0 font-mono text-foreground-muted text-xs tabular-nums">
            {formatProgress(received, total)}
          </span>
        )}
      </div>
    </div>
  );
}

export function UpdatesSection() {
  const update = useUpdateState();
  const { settings, save } = useSavedSettings();
  const [pending, setPending] = useState<Action | null>(null);
  const row = updateRow(update);
  const actionKind = row.action?.kind ?? null;

  const run = useCallback(async () => {
    if (!actionKind) {
      return;
    }
    setPending(actionKind);
    try {
      await RUN[actionKind]();
    } catch (error) {
      toastManager.add({
        description: error instanceof Error ? error.message : undefined,
        title: FAILED[actionKind],
        type: "error",
      });
    } finally {
      setPending(null);
    }
  }, [actionKind]);

  const setAutomatic = useCallback(
    (enabled: boolean) => save({ checkForUpdates: enabled }),
    [save]
  );

  if (!settings) {
    return null;
  }

  const { offline } = settings;
  const release = releaseOf(update);

  return (
    <Section
      description="Tiny Wispr asks GitHub whether a newer version is out. Nothing about you or your dictations is sent."
      title="Updates"
    >
      <SettingGroup>
        {update.state === "downloading" ? (
          <DownloadRow update={update} />
        ) : (
          <SettingRow
            control={
              row.action ? (
                <Button
                  disabled={offline || row.busy}
                  loading={row.busy || pending === row.action.kind}
                  onClick={run}
                  size="sm"
                  type="button"
                  variant={row.action.kind === "check" ? "outline" : "default"}
                >
                  {row.action.label}
                </Button>
              ) : null
            }
            description={
              offline
                ? "Stay offline is on. Turn it off in Privacy to check for updates."
                : row.description
            }
            label={row.label}
          />
        )}
        <SwitchRow
          checked={settings.checkForUpdates}
          description="Once a day while Tiny Wispr is open. Updates are only downloaded when you choose to."
          label="Check for updates automatically"
          onCheckedChange={setAutomatic}
        />
      </SettingGroup>
      {release && update.state !== "failed" ? (
        <ReleaseNotes release={release} />
      ) : null}
    </Section>
  );
}

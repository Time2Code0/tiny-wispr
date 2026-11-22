import type { CleanupLevel, HardwareInfo, Settings } from "@/lib/ipc";

import { useCallback } from "react";

import { CleanupLevelCards } from "@/components/cleanup-level-cards";
import { ModelList } from "@/components/model-list";
import { OfflineNotice } from "@/components/offline-notice";
import { Button } from "@/components/ui/button";
import { useIpcQuery } from "@/hooks/use-ipc-query";
import { getHardwareInfo, getRecommendation } from "@/lib/ipc";

const GIB = 1024 ** 3;

/** "Apple M3 Max · 36 GB of memory", or cores and architecture without a name. */
export function machineSummary(hardware: HardwareInfo): string {
  const processor = hardware.cpuName ?? [
    hardware.appleSilicon ? "Apple silicon" : hardware.arch,
    `${hardware.cpuCores} cores`,
  ];
  return [
    processor,
    `${Math.round(hardware.totalMemoryBytes / GIB)} GB of memory`,
  ]
    .flat()
    .join(" · ");
}

type Update = (patch: Partial<Settings>) => Promise<unknown>;

const logError = (error: unknown) => {
  console.error("Could not save the model choice", error);
};

function GoOnline({ update }: { update: Update }) {
  const goOnline = useCallback(() => {
    update({ offline: false }).catch(logError);
  }, [update]);
  return (
    <OfflineNotice
      action={
        <Button onClick={goOnline} size="sm" type="button" variant="outline">
          Turn it off
        </Button>
      }
    />
  );
}

export function VoiceModelStep({
  settings,
  update,
}: {
  settings: Settings;
  update: Update;
}) {
  const hardware = useIpcQuery(getHardwareInfo);
  const recommendation = useIpcQuery(getRecommendation);
  const reason = recommendation.data?.voice?.reason;
  const select = useCallback(
    (id: string) => {
      update({ sttModel: id }).catch(logError);
    },
    [update]
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      {hardware.data ? (
        <p className="text-foreground-muted text-xs">
          <span className="font-medium text-foreground">
            {machineSummary(hardware.data)}
          </span>
          {reason ? ` · ${reason}` : null}
        </p>
      ) : null}
      {settings.offline ? <GoOnline update={update} /> : null}
      <ModelList
        className="min-h-0 flex-1"
        kind="voice"
        languages={settings.languages}
        offline={settings.offline}
        onSelect={select}
        selectedId={settings.sttModel}
      />
    </div>
  );
}

export function PolishStep({
  settings,
  update,
}: {
  settings: Settings;
  update: Update;
}) {
  const select = useCallback(
    (id: string) => {
      update({ llmModel: id }).catch(logError);
    },
    [update]
  );
  const changeLevel = useCallback(
    (cleanupLevel: CleanupLevel) => {
      update({ cleanupLevel }).catch(logError);
    },
    [update]
  );
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      {settings.offline ? <GoOnline update={update} /> : null}
      <ModelList
        className="min-h-0 flex-1"
        kind="language"
        offline={settings.offline}
        onSelect={select}
        selectedId={settings.llmModel}
      />
      <CleanupLevelCards
        onValueChange={changeLevel}
        value={settings.cleanupLevel}
      />
    </div>
  );
}

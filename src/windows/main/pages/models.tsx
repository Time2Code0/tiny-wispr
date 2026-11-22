import type { HardwareInfo, ModelInfo, ModelKind } from "@/lib/ipc";

import { useCallback, useRef, useState } from "react";

import { CheckmarkCircle02Icon } from "@hugeicons/core-free-icons";

import { ModelList } from "@/components/model-list";
import { formatBytes } from "@/components/model-row";
import { OfflineNotice } from "@/components/offline-notice";
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
import { toastManager } from "@/components/ui/toast";
import { useIpcQuery } from "@/hooks/use-ipc-query";
import {
  deleteModel,
  getHardwareInfo,
  getRecommendation,
  importModel,
  isTauri,
  listModels,
  pickModelFile,
} from "@/lib/ipc";
import { IpcError } from "@/lib/ipc/errors";

import { focusAfterRemoval } from "../focus-after-removal";
import { Page, Section } from "../page";
import { hashFor } from "../route";
import { useSavedSettings } from "../use-saved-settings";

const GIB = 1024 ** 3;
const MODELS_EVENTS = ["models:changed"] as const;

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

/** Adds a model file from disk. Hidden in the browser, which has no picker. */
function ImportModel({ kind }: { kind: ModelKind }) {
  const [error, setError] = useState<string>();
  const [pending, setPending] = useState(false);
  const pick = useCallback(async () => {
    setError(undefined);
    try {
      const path = await pickModelFile(kind);
      if (!path) {
        return;
      }
      setPending(true);
      const model = await importModel(path, kind);
      toastManager.add({ title: `${model.label} added`, type: "success" });
    } catch (caught) {
      setError(IpcError.from(caught).message);
    } finally {
      setPending(false);
    }
  }, [kind]);

  if (!isTauri) {
    return null;
  }
  return (
    <div className="flex flex-col items-start gap-1.5">
      <Button
        loading={pending}
        onClick={pick}
        size="sm"
        type="button"
        variant="outline"
      >
        Import a model file…
      </Button>
      {error ? (
        <p className="text-destructive-foreground text-xs" role="alert">
          {error}
        </p>
      ) : null}
    </div>
  );
}

function InUse() {
  return (
    <span className="flex h-7 items-center gap-1.5 font-medium text-sm text-success-foreground">
      <Icon
        aria-hidden="true"
        className="size-4"
        icon={CheckmarkCircle02Icon}
      />
      In use
    </span>
  );
}

function NoLanguageModel({
  active,
  onUse,
}: {
  active: boolean;
  onUse: () => void;
}) {
  return (
    <div className="flex items-start justify-between gap-4 rounded-xl border bg-background px-4 py-3.5">
      <div className="flex min-w-0 flex-col gap-0.5">
        <span className="font-medium text-foreground text-sm">None</span>
        <span className="text-pretty text-foreground-muted text-xs">
          No polish. Light cleanup still removes fillers and fixes punctuation
          with built-in rules.
        </span>
      </div>
      {active ? (
        <InUse />
      ) : (
        <Button onClick={onUse} size="sm" type="button" variant="outline">
          Use
        </Button>
      )}
    </div>
  );
}

function DeleteModel({ model, inUse }: { inUse: boolean; model: ModelInfo }) {
  const [open, setOpen] = useState(false);
  const [pending, setPending] = useState(false);
  const trigger = useRef<HTMLButtonElement>(null);
  const confirm = useCallback(async () => {
    setPending(true);
    try {
      focusAfterRemoval(trigger.current?.closest("li"), trigger.current);
      await deleteModel(model.id);
      setOpen(false);
      toastManager.add({ title: `${model.label} deleted`, type: "success" });
    } catch {
      toastManager.add({
        title: `Couldn't delete ${model.label}`,
        type: "error",
      });
    } finally {
      setPending(false);
    }
  }, [model.id, model.label]);

  let consequence = "";
  if (inUse) {
    consequence =
      model.kind === "voice"
        ? " You'll need another voice model to dictate."
        : " Polish turns off until you pick another language model.";
  }

  return (
    <AlertDialog onOpenChange={setOpen} open={open}>
      <AlertDialogTrigger
        aria-label={`Delete ${model.label}`}
        ref={trigger}
        render={<Button size="sm" type="button" variant="ghost" />}
      >
        Delete
      </AlertDialogTrigger>
      <AlertDialogPopup>
        <AlertDialogHeader>
          <AlertDialogTitle>Delete {model.label}?</AlertDialogTitle>
          <AlertDialogDescription>
            This frees {formatBytes(model.sizeBytes)}. You can download it again
            later.{consequence}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogClose render={<Button type="button" variant="ghost" />}>
            Cancel
          </AlertDialogClose>
          <Button
            loading={pending}
            onClick={confirm}
            type="button"
            variant="destructive"
          >
            Delete
          </Button>
        </AlertDialogFooter>
      </AlertDialogPopup>
    </AlertDialog>
  );
}

function Downloaded({ inUse }: { inUse: (model: ModelInfo) => boolean }) {
  const models = useIpcQuery(listModels, MODELS_EVENTS);
  const downloaded = (models.data ?? []).filter(
    (m) => m.status.state === "downloaded"
  );
  if (downloaded.length === 0) {
    return null;
  }
  const total = downloaded.reduce((sum, m) => sum + m.sizeBytes, 0);
  return (
    <Section
      description={`${formatBytes(total)} in all. Delete a model to free the space.`}
      title="On this computer"
    >
      <ul className="flex flex-col divide-y rounded-xl border">
        {downloaded.map((model) => (
          <li
            className="flex items-center justify-between gap-4 px-4 py-2.5"
            key={model.id}
          >
            <div className="flex min-w-0 flex-col">
              <span className="truncate text-foreground text-sm">
                {model.label}
              </span>
              <span className="text-foreground-muted text-xs">
                {model.kind === "voice" ? "Voice" : "Language"} ·{" "}
                <span className="font-mono">
                  {formatBytes(model.sizeBytes)}
                </span>
                {inUse(model) ? " · In use" : null}
              </span>
            </div>
            <DeleteModel inUse={inUse(model)} model={model} />
          </li>
        ))}
      </ul>
    </Section>
  );
}

export function ModelsPage() {
  const { settings, save } = useSavedSettings();
  const hardware = useIpcQuery(getHardwareInfo);
  const recommendation = useIpcQuery(getRecommendation);

  const useVoice = useCallback((id: string) => save({ sttModel: id }), [save]);
  const useLanguage = useCallback(
    (id: string) => save({ llmModel: id }),
    [save]
  );
  const useNone = useCallback(() => save({ llmModel: null }), [save]);
  const inUse = useCallback(
    (model: ModelInfo) =>
      model.id ===
      (model.kind === "voice" ? settings?.sttModel : settings?.llmModel),
    [settings?.llmModel, settings?.sttModel]
  );

  const voiceReason = recommendation.data?.voice?.reason;
  const languageReason = recommendation.data?.language?.reason;

  return (
    <Page
      description="Models run on this computer. Downloading one is the only time Tiny Wispr uses the internet."
      title="Models"
    >
      {hardware.data ? (
        <p className="-mt-4 text-pretty text-foreground-muted text-sm">
          <span className="text-foreground">
            {machineSummary(hardware.data)}
          </span>
        </p>
      ) : null}

      {settings?.offline ? (
        <OfflineNotice
          action={
            <Button
              nativeButton={false}
              render={<a href={hashFor("settings", "privacy")} />}
              size="sm"
              variant="outline"
            >
              Open Privacy
            </Button>
          }
        />
      ) : null}

      <Section
        description={
          voiceReason
            ? `Turn your speech into text. ${voiceReason}`
            : "Turn your speech into text."
        }
        title="Voice models"
      >
        {settings ? (
          <ModelList
            kind="voice"
            languages={settings.languages}
            offline={settings.offline}
            onSelect={useVoice}
            selectedId={settings.sttModel}
          />
        ) : null}
        <ImportModel kind="voice" />
      </Section>

      <Section
        description={
          languageReason
            ? `Optional. Polishes what you said into clean writing. ${languageReason}`
            : "Optional. Polishes what you said into clean writing."
        }
        title="Language models"
      >
        {settings ? (
          <>
            <NoLanguageModel
              active={settings.llmModel === null}
              onUse={useNone}
            />
            <ModelList
              kind="language"
              offline={settings.offline}
              onSelect={useLanguage}
              selectedId={settings.llmModel}
            />
          </>
        ) : null}
        <ImportModel kind="language" />
      </Section>

      <Downloaded inUse={inUse} />
    </Page>
  );
}

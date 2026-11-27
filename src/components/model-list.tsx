import type { Languages, ModelInfo, ModelKind, ModelProgress } from "@/lib/ipc";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { cn } from "cn";

import { ModelRow, ModelTableHeader } from "@/components/model-row";
import { Spinner } from "@/components/ui/loading";
import {
  RadioGroupPrimitive,
  RadioPrimitive,
} from "@/components/ui/radio-group";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  SegmentedControl,
  SegmentedControlItem,
} from "@/components/ui/segmented-control";
import { useIpcQuery } from "@/hooks/use-ipc-query";
import {
  cancelDownload,
  downloadModel,
  listModels,
  onModelsProgress,
} from "@/lib/ipc";

/** Which voice models to list: English-only ones, or multilingual ones. */
export type SpokenLanguages = "english" | "any";

/** A full-size model and its compressed copies, shown as one row. */
export interface ModelGroup {
  id: string;
  /** The full-size model first, then the copies from largest to smallest. */
  variants: ModelInfo[];
}

export interface ModelSection {
  groups: ModelGroup[];
  title: string | null;
}

/** Each full-size model with its compressed copies, smallest model first. */
export function groupVariants(models: readonly ModelInfo[]): ModelGroup[] {
  const ids = new Set(models.map((model) => model.id));
  const groups = new Map<string, ModelInfo[]>();
  for (const model of models) {
    const key =
      model.variantOf && ids.has(model.variantOf) ? model.variantOf : model.id;
    groups.set(key, [...(groups.get(key) ?? []), model]);
  }
  return [...groups.entries()]
    .map(([id, members]) => ({
      id,
      variants: members.sort((a, b) => {
        if (a.id === id || b.id === id) {
          return a.id === id ? -1 : 1;
        }
        return b.sizeBytes - a.sizeBytes;
      }),
    }))
    .sort(
      (a, b) =>
        (a.variants[0]?.sizeBytes ?? 0) - (b.variants[0]?.sizeBytes ?? 0)
    );
}

/**
 * Whisper names its English-only models after the multilingual ones
 * (`small` and `small.en`). The largest sizes have no English-only version,
 * so English speakers see the multilingual ones there.
 */
function hasEnglishVersion(
  model: ModelInfo,
  models: readonly ModelInfo[]
): boolean {
  const original = model.variantOf ?? model.id;
  return models.some(
    (other) => other.englishOnly && other.id === `${original}.en`
  );
}

export function showsFor(
  languages: SpokenLanguages,
  model: ModelInfo,
  models: readonly ModelInfo[]
): boolean {
  if (model.custom || model.kind !== "voice") {
    return true;
  }
  if (languages === "any") {
    return !model.englishOnly;
  }
  return model.englishOnly || !hasEnglishVersion(model, models);
}

/**
 * Catalog models in size order, split so the odd ones out don't interleave
 * with the rest: reasoning language models, then imported files.
 */
export function modelSections(
  models: readonly ModelInfo[],
  languages: SpokenLanguages
): ModelSection[] {
  const shown = models.filter((model) => showsFor(languages, model, models));
  const sections: ModelSection[] = [
    {
      groups: groupVariants(shown.filter((m) => !(m.custom || m.reasoning))),
      title: null,
    },
    {
      groups: groupVariants(shown.filter((m) => m.reasoning && !m.custom)),
      title: "Slower models that think before answering",
    },
    { groups: groupVariants(shown.filter((m) => m.custom)), title: "Imported" },
  ];
  return sections.filter((section) => section.groups.length > 0);
}

const LOCAL_STATE_ORDER = ["downloading", "paused", "failed", "downloaded"];

/**
 * The file a row describes until the person picks another: the one in use,
 * then one that is downloading or on disk, then the recommended one, then
 * the full-size model.
 */
export function defaultVariant(
  group: ModelGroup,
  selectedId: string | null
): ModelInfo | undefined {
  const { variants } = group;
  const inUse = variants.find((variant) => variant.id === selectedId);
  if (inUse) {
    return inUse;
  }
  for (const state of LOCAL_STATE_ORDER) {
    const local = variants.find((variant) => variant.status.state === state);
    if (local) {
      return local;
    }
  }
  return variants.find((variant) => variant.recommended) ?? variants[0];
}

function speaksOnlyEnglish(languages: Languages | undefined): boolean {
  return (
    languages !== undefined &&
    !languages.autoDetect &&
    languages.selected.length === 1 &&
    languages.selected[0] === "en"
  );
}

/**
 * English when the model in use is English-only, or when dictation is set to
 * English alone and the model in use (if any) is listed under English.
 */
export function initialLanguages(
  models: readonly ModelInfo[],
  selectedId: string | null,
  languages: Languages | undefined
): SpokenLanguages {
  const selected = models.find((model) => model.id === selectedId);
  if (selected?.englishOnly) {
    return "english";
  }
  if (!speaksOnlyEnglish(languages)) {
    return "any";
  }
  return !selected || showsFor("english", selected, models) ? "english" : "any";
}

/** The model as the latest progress event describes it. */
export function withProgress(
  model: ModelInfo,
  progress: ModelProgress | undefined
): ModelInfo {
  if (progress?.state !== "downloading") {
    return model;
  }
  return {
    ...model,
    status: {
      received: progress.received,
      state: "downloading",
      total: progress.total,
    },
  };
}

function LanguageChoice({
  onValueChange,
  value,
}: {
  onValueChange: (value: SpokenLanguages) => void;
  value: SpokenLanguages;
}) {
  const change = useCallback(
    (next: unknown) => {
      if (next === "english" || next === "any") {
        onValueChange(next);
      }
    },
    [onValueChange]
  );
  return (
    <div className="flex items-center gap-3">
      <span className="text-foreground-muted text-xs" id="model-languages">
        You'll dictate in
      </span>
      <SegmentedControl
        aria-labelledby="model-languages"
        render={<RadioGroupPrimitive onValueChange={change} value={value} />}
        size="sm"
      >
        <SegmentedControlItem render={<RadioPrimitive.Root value="english" />}>
          English
        </SegmentedControlItem>
        <SegmentedControlItem render={<RadioPrimitive.Root value="any" />}>
          Other languages too
        </SegmentedControlItem>
      </SegmentedControl>
    </div>
  );
}

/**
 * The models of one kind as a table, smallest first, one row per model with
 * its compressed copies chosen in the download column. Voice models are
 * filtered by the languages the person dictates in. When nothing is
 * selected yet, the first download to finish is selected. Scrolls inside its
 * own area when given a height.
 */
export function ModelList({
  kind,
  selectedId,
  onSelect,
  languages: dictationLanguages,
  className,
}: {
  kind: ModelKind;
  /** The model in use for this kind (`settings.sttModel` or `llmModel`). */
  selectedId: string | null;
  onSelect: (id: string) => void;
  /** `settings.languages`, which picks the voice models shown first. */
  languages?: Languages;
  className?: string;
}) {
  const { data, error, refetch } = useIpcQuery(listModels, ["models:changed"]);
  const [progress, setProgress] = useState<Record<string, ModelProgress>>({});
  const [failures, setFailures] = useState<Record<string, string>>({});
  const [languages, setLanguages] = useState<SpokenLanguages | null>(null);
  const [chosen, setChosen] = useState<Record<string, string>>({});

  const latest = useRef({ data, onSelect, selectedId });
  latest.current = { data, onSelect, selectedId };

  useEffect(
    () =>
      onModelsProgress((event) => {
        setProgress((previous) => ({ ...previous, [event.id]: event }));
        if (event.state === "downloading") {
          return;
        }
        refetch();
        const { current } = latest;
        const model = current.data?.find((m) => m.id === event.id);
        if (
          event.state === "done" &&
          model?.kind === kind &&
          current.selectedId === null
        ) {
          current.onSelect(event.id);
        }
      }),
    [kind, refetch]
  );

  const models = useMemo(
    () =>
      (data ?? [])
        .filter((model) => model.kind === kind)
        .map((model) => {
          const failure = failures[model.id];
          if (failure && model.status.state === "notDownloaded") {
            return {
              ...model,
              status: { message: failure, state: "failed" as const },
            };
          }
          return withProgress(model, progress[model.id]);
        }),
    [data, failures, kind, progress]
  );
  const spoken =
    languages ?? initialLanguages(models, selectedId, dictationLanguages);
  const sections = useMemo(
    () => modelSections(models, kind === "voice" ? spoken : "any"),
    [kind, models, spoken]
  );

  const download = useCallback((id: string) => {
    setFailures(({ [id]: _, ...others }) => others);
    downloadModel(id).catch((caught: unknown) => {
      const message =
        caught instanceof Error ? caught.message : "The download failed.";
      setFailures((previous) => ({ ...previous, [id]: message }));
    });
  }, []);
  const pause = useCallback((id: string) => {
    cancelDownload(id).catch((caught: unknown) => {
      console.error("Could not pause the download", caught);
    });
  }, []);
  const choose = useCallback(
    (id: string) => {
      const model = models.find((m) => m.id === id);
      setChosen((previous) => ({
        ...previous,
        [model?.variantOf ?? id]: id,
      }));
    },
    [models]
  );

  if (!data) {
    return (
      <div
        className={cn(
          "flex items-center justify-center text-foreground-muted text-sm",
          className
        )}
      >
        {error ? "Couldn't load the models." : <Spinner />}
      </div>
    );
  }

  const row = (group: ModelGroup) => {
    const picked =
      group.variants.find((variant) => variant.id === chosen[group.id]) ??
      defaultVariant(group, selectedId);
    if (!picked) {
      return null;
    }
    return (
      <ModelRow
        chosen={picked}
        emphasis={picked.recommended && selectedId === null}
        key={group.id}
        onChoose={choose}
        onDownload={download}
        onPause={pause}
        onUse={onSelect}
        selectedId={selectedId}
        variants={group.variants}
      />
    );
  };

  return (
    <div className={cn("flex min-h-0 flex-col gap-3", className)}>
      {kind === "voice" ? (
        <LanguageChoice onValueChange={setLanguages} value={spoken} />
      ) : null}
      <ScrollArea className="min-h-0 flex-1" scrollbarGutter scrollFade>
        <div className="flex flex-col gap-4 pb-1">
          {sections.map((section, index) => (
            <section
              aria-label={section.title ?? "Models"}
              className="flex flex-col gap-2"
              key={section.title ?? "models"}
            >
              {section.title ? (
                <h3 className="ps-1 font-medium text-foreground-muted text-xs">
                  {section.title}
                </h3>
              ) : null}
              <div className="rounded-xl border bg-background">
                {index === 0 ? <ModelTableHeader /> : null}
                <ul className="flex flex-col">{section.groups.map(row)}</ul>
              </div>
            </section>
          ))}
        </div>
      </ScrollArea>
    </div>
  );
}

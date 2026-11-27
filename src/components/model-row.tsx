import type { Compression, ModelInfo, ModelStatus } from "@/lib/ipc";

import { useCallback } from "react";

import {
  Alert02Icon,
  CheckmarkCircle02Icon,
  PauseIcon,
} from "@hugeicons/core-free-icons";
import { cn } from "cn";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Progress } from "@/components/ui/progress";
import {
  Select,
  SelectItem,
  SelectPopup,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

const MB = 1_000_000;
const GB = 1_000_000_000;

/** Download sizes in decimal units, as the model hosts list them: "574 MB", "1.6 GB". */
export function formatBytes(bytes: number): string {
  if (bytes >= GB) {
    return `${(bytes / GB).toFixed(1)} GB`;
  }
  return `${Math.max(1, Math.round(bytes / MB))} MB`;
}

/** "312 MB of 1.6 GB". */
export function formatProgress(received: number, total: number): string {
  return `${formatBytes(received)} of ${formatBytes(total)}`;
}

export function progressPercent(received: number, total: number): number {
  if (total <= 0) {
    return 0;
  }
  return Math.min(100, Math.round((received / total) * 100));
}

const SCALE = [1, 2, 3, 4, 5] as const;

function Scale({ label, value }: { label: string; value: number }) {
  if (value === 0) {
    return <span className="text-foreground-muted text-xs">Unknown</span>;
  }
  return (
    <span
      aria-label={`${label} ${value} of 5`}
      className="inline-flex items-center"
      role="img"
    >
      <span aria-hidden="true" className="inline-flex gap-0.5">
        {SCALE.map((step) => (
          <span
            className={cn(
              "size-1.5 rounded-full",
              step <= value ? "bg-foreground-muted" : "bg-accent"
            )}
            key={step}
          />
        ))}
      </span>
    </span>
  );
}

const COMPRESSION_LABELS: Record<Compression, string> = {
  light: "Lightly compressed",
  none: "Full size",
  strong: "Compressed",
};

/**
 * Name, download, speed, accuracy and action columns, shared by the rows and
 * the header so they line up.
 */
export const MODEL_ROW_GRID =
  "grid grid-cols-[minmax(0,1fr)_5.5rem_3.25rem_3.25rem_5.5rem] items-center gap-x-2";

export function ModelTableHeader() {
  return (
    <div
      aria-hidden="true"
      className={cn(
        MODEL_ROW_GRID,
        "border-b px-3 py-2 text-foreground-muted text-xs"
      )}
    >
      <span>Model</span>
      <span className="ps-2.5">Download</span>
      <span>Speed</span>
      <span>Accuracy</span>
    </div>
  );
}

function variantNote(model: ModelInfo, inUse: boolean): string | null {
  if (inUse) {
    return "In use";
  }
  switch (model.status.state) {
    case "downloaded":
      return "Downloaded";
    case "downloading":
      return "Downloading";
    case "paused":
      return "Paused";
    default:
      return model.recommended ? "Recommended" : null;
  }
}

/** The download size, or a choice between the full and compressed files. */
function SizeChoice({
  chosen,
  label,
  onChoose,
  selectedId,
  variants,
}: {
  chosen: ModelInfo;
  label: string;
  onChoose: (id: string) => void;
  selectedId: string | null;
  variants: readonly ModelInfo[];
}) {
  const change = useCallback(
    (next: unknown) => {
      if (typeof next === "string") {
        onChoose(next);
      }
    },
    [onChoose]
  );
  if (variants.length === 1) {
    return (
      <span className="ps-2.5 font-mono text-foreground-muted text-xs tabular-nums">
        {formatBytes(chosen.sizeBytes)}
      </span>
    );
  }
  const items = variants.map((variant) => ({
    label: formatBytes(variant.sizeBytes),
    value: variant.id,
  }));
  return (
    <Select items={items} onValueChange={change} value={chosen.id}>
      <SelectTrigger
        aria-label={`Download size of ${label}`}
        className="min-w-0 font-mono text-xs tabular-nums"
        size="sm"
      >
        <SelectValue />
      </SelectTrigger>
      <SelectPopup>
        {variants.map((variant) => {
          const note = variantNote(variant, variant.id === selectedId);
          return (
            <SelectItem key={variant.id} value={variant.id}>
              <span className="flex items-baseline gap-2">
                <span className="font-mono tabular-nums">
                  {formatBytes(variant.sizeBytes)}
                </span>
                <span className="text-foreground-muted">
                  {COMPRESSION_LABELS[variant.compression]}
                  {note ? ` · ${note}` : null}
                </span>
              </span>
            </SelectItem>
          );
        })}
      </SelectPopup>
    </Select>
  );
}

function Downloading({
  label,
  onPause,
  received,
  total,
}: {
  label: string;
  onPause: () => void;
  received: number;
  total: number;
}) {
  return (
    <div className="flex items-center gap-3">
      <Progress
        aria-label={`Downloading ${label}`}
        className="flex-1"
        value={progressPercent(received, total)}
      />
      <span className="shrink-0 font-mono text-foreground-muted text-xs tabular-nums">
        {formatProgress(received, total)}
      </span>
      <Button
        aria-label={`Pause download of ${label}`}
        onClick={onPause}
        size="icon-xs"
        type="button"
        variant="ghost"
      >
        <Icon aria-hidden="true" icon={PauseIcon} />
      </Button>
    </div>
  );
}

function Action({
  emphasis,
  inUse,
  label,
  onDownload,
  onUse,
  status,
}: {
  emphasis: boolean;
  inUse: boolean;
  label: string;
  onDownload: () => void;
  onUse: () => void;
  status: ModelStatus;
}) {
  switch (status.state) {
    case "notDownloaded":
      return (
        <Button
          aria-label={`Download ${label}`}
          onClick={onDownload}
          size="sm"
          type="button"
          variant={emphasis ? "default" : "outline"}
        >
          Download
        </Button>
      );
    case "paused":
      return (
        <Button
          aria-label={`Resume download of ${label}`}
          onClick={onDownload}
          size="sm"
          type="button"
          variant={emphasis ? "default" : "outline"}
        >
          Resume
        </Button>
      );
    case "failed":
      return (
        <Button
          aria-label={`Retry download of ${label}`}
          onClick={onDownload}
          size="sm"
          type="button"
          variant="outline"
        >
          Retry
        </Button>
      );
    case "downloaded":
      return inUse ? (
        <span className="flex h-7 items-center gap-1.5 font-medium text-sm text-success-foreground">
          <Icon
            aria-hidden="true"
            className="size-4"
            icon={CheckmarkCircle02Icon}
          />
          In use
        </span>
      ) : (
        <Button
          aria-label={`Use ${label}`}
          onClick={onUse}
          size="sm"
          type="button"
          variant={emphasis ? "default" : "outline"}
        >
          Use
        </Button>
      );
    default:
      return null;
  }
}

/**
 * One model size: its name and description, the download size (a choice when
 * compressed copies exist), speed, accuracy, and the one action that fits
 * the chosen file's state: Download, a progress bar with Pause, Resume,
 * Retry, Use, or "In use".
 */
export function ModelRow({
  variants,
  chosen,
  selectedId,
  emphasis = false,
  onChoose,
  onDownload,
  onPause,
  onUse,
}: {
  /** The full-size model first, then its compressed copies. */
  variants: readonly ModelInfo[];
  /** The variant the columns and the action describe. */
  chosen: ModelInfo;
  /** The model the app uses for this kind. */
  selectedId: string | null;
  /** Give the action the primary style, e.g. for the recommended model. */
  emphasis?: boolean;
  onChoose: (id: string) => void;
  onDownload: (id: string) => void;
  onPause: (id: string) => void;
  onUse: (id: string) => void;
}) {
  const [original] = variants;
  const { id, status } = chosen;
  const inUse = id === selectedId;
  const label = original?.label ?? chosen.label;
  const download = useCallback(() => onDownload(id), [id, onDownload]);
  const pause = useCallback(() => onPause(id), [id, onPause]);
  const use = useCallback(() => onUse(id), [id, onUse]);
  const titleId = `model-${id}`;
  const recommended = variants.some((variant) => variant.recommended);

  return (
    <li
      aria-labelledby={titleId}
      className={cn(
        MODEL_ROW_GRID,
        "gap-y-0.5 border-b px-3 py-2 last:border-b-0",
        inUse && "bg-background-muted"
      )}
      data-state={status.state}
    >
      <p
        className="truncate font-medium text-foreground text-sm"
        id={titleId}
        title={label}
      >
        {label}
      </p>
      <SizeChoice
        chosen={chosen}
        label={label}
        onChoose={onChoose}
        selectedId={selectedId}
        variants={variants}
      />
      <Scale label="Speed" value={chosen.speed} />
      <Scale label="Accuracy" value={chosen.accuracy} />
      <div className="flex justify-end">
        {status.state === "downloading" ? null : (
          <Action
            emphasis={emphasis}
            inUse={inUse}
            label={chosen.label}
            onDownload={download}
            onUse={use}
            status={status}
          />
        )}
      </div>
      {status.state === "downloading" ? (
        <div className="col-span-full">
          <Downloading
            label={chosen.label}
            onPause={pause}
            received={status.received}
            total={status.total || chosen.sizeBytes}
          />
        </div>
      ) : null}
      {status.state !== "downloading" &&
      (recommended || original?.description) ? (
        <p className="col-span-4 text-pretty text-foreground-muted text-xs">
          {recommended ? (
            <Badge className="me-2" size="sm" variant="outline">
              Recommended
            </Badge>
          ) : null}
          {original?.description}
        </p>
      ) : null}
      {status.state === "paused" ? (
        <p className="col-span-full font-mono text-foreground-muted text-xs tabular-nums">
          Paused at {formatProgress(status.received, status.total)}
        </p>
      ) : null}
      {status.state === "failed" ? (
        <p className="col-span-full flex items-center gap-1.5 text-destructive-foreground text-xs">
          <Icon aria-hidden="true" className="size-3.5" icon={Alert02Icon} />
          {status.message}
        </p>
      ) : null}
    </li>
  );
}

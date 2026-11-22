import type { FileProgress } from "@/lib/ipc";

import { useCallback, useEffect, useState } from "react";

import {
  AudioWave01Icon,
  Cancel01Icon,
  Copy01Icon,
  Download04Icon,
  Tick02Icon,
} from "@hugeicons/core-free-icons";

import { Button, buttonVariants } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Link } from "@/components/ui/link";
import {
  Progress,
  ProgressIndicator,
  ProgressTrack,
} from "@/components/ui/progress";
import { toastManager } from "@/components/ui/toast";
import { saveFileTranscript } from "@/lib/ipc";

import { hashFor } from "../route";

const COPIED_MS = 1500;

function stageLabel(job: FileProgress): string {
  switch (job.stage) {
    case "decoding":
      return "Reading the file";
    case "transcribing":
      return job.fraction === null
        ? "Transcribing"
        : `Transcribing, ${Math.round(job.fraction * 100)}%`;
    case "polishing":
      return "Polishing the text";
    default:
      return "";
  }
}

/** What a screen reader hears: stage changes and every quarter, not every tick. */
export function announcementFor(job: FileProgress | null): string {
  if (!job) {
    return "";
  }
  switch (job.stage) {
    case "decoding":
      return `Transcribing ${job.fileName}`;
    case "transcribing": {
      const quarter = Math.floor((job.fraction ?? 0) * 4) * 25;
      return quarter > 0 ? `${quarter}% transcribed` : "Transcribing";
    }
    case "polishing":
      return "Polishing the text";
    case "done":
      return `Done. ${words(job.wordCount ?? 0)}`;
    case "failed":
      return job.error?.message ?? "Couldn't transcribe the file";
    case "cancelled":
      return "Cancelled";
    default:
      return "";
  }
}

const words = (n: number) =>
  `${n.toLocaleString()} ${n === 1 ? "word" : "words"}`;

/** `272000` → "4:32", `3723000` → "1:02:03". */
export function clock(ms: number): string {
  const total = Math.round(ms / 1000);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = String(total % 60).padStart(2, "0");
  return hours > 0
    ? `${hours}:${String(minutes).padStart(2, "0")}:${seconds}`
    : `${minutes}:${seconds}`;
}

function Header({ job, onClose }: { job: FileProgress; onClose?: () => void }) {
  return (
    <div className="flex items-center gap-3">
      <Icon
        aria-hidden="true"
        className="size-4 shrink-0 text-foreground-muted"
        icon={AudioWave01Icon}
      />
      <h3 className="min-w-0 flex-1 truncate font-medium text-foreground text-sm">
        {job.fileName}
      </h3>
      {onClose ? (
        <Button
          aria-label="Close"
          onClick={onClose}
          size="icon-sm"
          type="button"
          variant="ghost"
        >
          <Icon aria-hidden="true" icon={Cancel01Icon} />
        </Button>
      ) : null}
    </div>
  );
}

function Running({
  job,
  onCancel,
}: {
  job: FileProgress;
  onCancel: () => void;
}) {
  return (
    <>
      <Header job={job} />
      <Progress
        aria-label={`Transcribing ${job.fileName}`}
        value={job.fraction === null ? null : job.fraction * 100}
      >
        <ProgressTrack>
          <ProgressIndicator className="motion-reduce:transition-none" />
        </ProgressTrack>
      </Progress>
      <div className="flex items-center justify-between gap-3">
        <span className="text-foreground-muted text-sm tabular-nums">
          {stageLabel(job)}
        </span>
        <Button onClick={onCancel} size="sm" type="button" variant="ghost">
          Cancel
        </Button>
      </div>
    </>
  );
}

function Result({ job, onClose }: { job: FileProgress; onClose: () => void }) {
  const text = job.text ?? "";
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!copied) {
      return;
    }
    const timer = setTimeout(() => setCopied(false), COPIED_MS);
    return () => clearTimeout(timer);
  }, [copied]);

  const copy = useCallback(() => {
    navigator.clipboard
      .writeText(text)
      .then(() => setCopied(true))
      .catch(() => {
        toastManager.add({ title: "Couldn't copy the text", type: "error" });
      });
  }, [text]);

  const save = useCallback(() => {
    saveFileTranscript(text, job.fileName)
      .then((saved) => {
        if (saved) {
          toastManager.add({ title: "Transcript saved", type: "success" });
        }
      })
      .catch(() => {
        toastManager.add({
          title: "Couldn't save the transcript",
          type: "error",
        });
      });
  }, [job.fileName, text]);

  const meta = [
    words(job.wordCount ?? 0),
    job.durationMs === null ? null : clock(job.durationMs),
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <>
      <Header job={job} onClose={onClose} />
      <section
        aria-label="Transcript"
        className="max-h-64 select-text overflow-y-auto whitespace-pre-wrap rounded-lg bg-background-muted px-3 py-2 text-foreground text-sm leading-relaxed outline-none focus-visible:ring-2 focus-visible:ring-foreground"
        // biome-ignore lint/a11y/noNoninteractiveTabindex: a scrolling region must be reachable by keyboard
        tabIndex={0}
      >
        {text}
      </section>
      <div className="flex flex-wrap items-center gap-2">
        <span className="me-auto text-foreground-muted text-sm tabular-nums">
          {meta}
        </span>
        <Button onClick={copy} size="sm" type="button" variant="outline">
          <Icon aria-hidden="true" icon={copied ? Tick02Icon : Copy01Icon} />
          {copied ? "Copied" : "Copy"}
        </Button>
        <Button onClick={save} size="sm" type="button" variant="outline">
          <Icon aria-hidden="true" icon={Download04Icon} />
          Save as text…
        </Button>
      </div>
    </>
  );
}

function Failed({ job, onClose }: { job: FileProgress; onClose: () => void }) {
  const noModel = job.error?.kind === "noModel";
  return (
    <>
      <Header job={job} onClose={onClose} />
      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="text-destructive-foreground text-sm">
          {job.error?.message ?? "Couldn't transcribe this file"}
        </p>
        {noModel ? (
          <Link
            className={buttonVariants({ size: "sm", variant: "outline" })}
            href={hashFor("models")}
          >
            Go to Models
          </Link>
        ) : null}
      </div>
    </>
  );
}

/** The file job at the top of the history: progress, then the result or the error. */
export function FileCard({
  job,
  onCancel,
  onClose,
}: {
  job: FileProgress;
  onCancel: () => void;
  onClose: () => void;
}) {
  return (
    <section
      aria-label="File transcription"
      className="flex flex-col gap-3 rounded-xl border bg-background p-4"
    >
      {job.stage === "done" ? <Result job={job} onClose={onClose} /> : null}
      {job.stage === "failed" ? <Failed job={job} onClose={onClose} /> : null}
      {job.stage === "done" || job.stage === "failed" ? null : (
        <Running job={job} onCancel={onCancel} />
      )}
    </section>
  );
}

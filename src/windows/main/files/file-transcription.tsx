import { useCallback } from "react";

import { FileUploadIcon } from "@hugeicons/core-free-icons";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";

import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { toastManager } from "@/components/ui/toast";
import { isTauri, pickAudioFile } from "@/lib/ipc";
import { SAMPLE_FILE_PATH } from "@/lib/ipc/mock-files";

import { announcementFor, FileCard } from "./file-card";
import { useFileDrop } from "./use-file-drop";
import {
  isRunning,
  isSupportedFile,
  SUPPORTED_FILES_HINT,
  useFileTranscription,
} from "./use-file-transcription";

const EASE_OUT = [0.23, 1, 0.32, 1] as const;

function DropHint({ show }: { show: boolean }) {
  const calm = useReducedMotion() ?? false;
  return (
    <AnimatePresence>
      {show ? (
        <motion.div
          animate={{ opacity: 1 }}
          aria-hidden="true"
          className="pointer-events-none fixed inset-0 z-50 flex items-center justify-center bg-background/90 p-6"
          exit={{ opacity: 0 }}
          initial={{ opacity: calm ? 1 : 0 }}
          transition={{ duration: calm ? 0 : 0.15, ease: EASE_OUT }}
        >
          <div className="flex size-full flex-col items-center justify-center gap-2 rounded-2xl border-2 border-dashed text-center">
            <Icon
              className="size-6 text-foreground-muted"
              icon={FileUploadIcon}
            />
            <p className="font-medium text-foreground">Drop to transcribe</p>
            <p className="max-w-sm text-balance text-foreground-muted text-sm">
              {SUPPORTED_FILES_HINT}
            </p>
          </div>
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}

/**
 * Turning an audio file into text on Home: the picker button, the window-wide
 * drop target, and the job's card above the history.
 */
export function FileTranscription() {
  const { cancel, dismiss, job, start } = useFileTranscription();
  const calm = useReducedMotion() ?? false;
  const busy = isRunning(job);

  const dropped = useCallback(
    (paths: string[]) => {
      const path = paths.find(isSupportedFile);
      if (!path) {
        toastManager.add({
          description: SUPPORTED_FILES_HINT,
          title: "This file type isn't supported",
          type: "error",
        });
        return;
      }
      if (busy) {
        toastManager.add({
          title: "A file is already being transcribed",
          type: "error",
        });
        return;
      }
      start(path);
    },
    [busy, start]
  );
  const dragging = useFileDrop(dropped);

  const pick = useCallback(() => {
    if (!isTauri) {
      start(SAMPLE_FILE_PATH);
      return;
    }
    pickAudioFile()
      .then((path) => {
        if (path) {
          start(path);
        }
      })
      .catch(() => {
        toastManager.add({
          title: "Couldn't open the file picker",
          type: "error",
        });
      });
  }, [start]);

  return (
    <div className="flex flex-col gap-3">
      <div className="flex justify-end">
        <Button
          disabled={busy}
          onClick={pick}
          size="sm"
          type="button"
          variant="ghost"
        >
          <Icon aria-hidden="true" icon={FileUploadIcon} />
          {isTauri ? "Transcribe a file…" : "Try a sample file"}
        </Button>
      </div>
      <p aria-live="polite" className="sr-only">
        {announcementFor(job)}
      </p>
      <AnimatePresence initial={false}>
        {job && job.stage !== "cancelled" ? (
          <motion.div
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: calm ? 0 : -4 }}
            initial={{ opacity: 0, y: calm ? 0 : -4 }}
            key={job.jobId}
            transition={{ duration: calm ? 0 : 0.2, ease: EASE_OUT }}
          >
            <FileCard job={job} onCancel={cancel} onClose={dismiss} />
          </motion.div>
        ) : null}
      </AnimatePresence>
      <DropHint show={dragging} />
    </div>
  );
}

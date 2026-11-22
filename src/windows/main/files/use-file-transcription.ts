import type { FileProgress } from "@/lib/ipc";

import { useCallback, useEffect, useState } from "react";

import { toastManager } from "@/components/ui/toast";
import { useIpcEvent } from "@/hooks/use-ipc-event";
import {
  AUDIO_FILE_EXTENSIONS,
  cancelFileTranscription,
  getFileTranscription,
  transcribeFile,
} from "@/lib/ipc";
import { IpcError } from "@/lib/ipc/errors";

const PATH_SEPARATOR = /[\\/]/;

export const SUPPORTED_FILES_HINT =
  "Tiny Wispr can transcribe WAV, MP3, M4A, AAC, FLAC and Ogg files, and the sound of MP4 and MOV videos.";

export function fileNameOf(path: string): string {
  return path.split(PATH_SEPARATOR).pop() ?? path;
}

export function isSupportedFile(path: string): boolean {
  const extension = path.split(".").pop()?.toLowerCase() ?? "";
  return (AUDIO_FILE_EXTENSIONS as readonly string[]).includes(extension);
}

export function isRunning(job: FileProgress | null): boolean {
  return (
    job?.stage === "decoding" ||
    job?.stage === "transcribing" ||
    job?.stage === "polishing"
  );
}

/**
 * The file job shown on Home: the running one (also after the window was
 * closed and reopened), then its result or error until dismissed.
 */
export function useFileTranscription() {
  const [job, setJob] = useState<FileProgress | null>(null);

  useEffect(() => {
    let live = true;
    getFileTranscription()
      .then((current) => {
        if (live && current) {
          setJob(current);
        }
      })
      .catch(() => {
        // Nothing running is the same as not knowing: the card stays away.
      });
    return () => {
      live = false;
    };
  }, []);

  useIpcEvent("files:progress", setJob);

  const start = useCallback((path: string) => {
    if (!isSupportedFile(path)) {
      toastManager.add({
        description: SUPPORTED_FILES_HINT,
        title: "This file type isn't supported",
        type: "error",
      });
      return;
    }
    transcribeFile(path)
      .then((jobId) => {
        // The first event may arrive before the job id does.
        setJob((current) =>
          current?.jobId === jobId
            ? current
            : {
                durationMs: null,
                error: null,
                fileName: fileNameOf(path),
                fraction: null,
                jobId,
                stage: "decoding",
                text: null,
                wordCount: null,
              }
        );
      })
      .catch((error: unknown) => {
        toastManager.add({
          title: IpcError.from(error).message,
          type: "error",
        });
      });
  }, []);

  const cancel = useCallback(() => {
    if (job && isRunning(job)) {
      cancelFileTranscription(job.jobId).catch(() => {
        toastManager.add({ title: "Couldn't cancel", type: "error" });
      });
    }
  }, [job]);

  const dismiss = useCallback(() => setJob(null), []);

  return { cancel, dismiss, job, start };
}

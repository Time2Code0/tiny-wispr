import type {
  FileErrorKind,
  FileProgress,
  FileStage,
  HistoryEntry,
} from "./types";

import { IpcError } from "./errors";

const PATH_SEPARATOR = /[\\/]/;
const WHITESPACE = /\s+/;
const TICK_MS = 500;
/** The whole simulated job takes about 20 s. */
const DECODE_TICKS = 3;
const TRANSCRIBE_TICKS = 30;
const POLISH_TICKS = 7;
const SAMPLE_DURATION_MS = 272_000;
const EXTENSIONS = new Set([
  "wav",
  "mp3",
  "m4a",
  "aac",
  "mp4",
  "mov",
  "flac",
  "ogg",
  "oga",
]);

/** Shown by the browser preview in place of the native file picker. */
export const SAMPLE_FILE_PATH = "/Users/you/Recordings/Planning call.m4a";

const SAMPLE_TEXT = [
  "Good morning, everyone. Thanks for joining the weekly planning call. The garden project is on schedule, and the new benches arrive on Tuesday.",
  "The library wants volunteers for the reading hour on Saturday mornings. Please send me your name if you can help for an hour or two.",
  "The budget for the summer festival is almost final. We still need quotes for the stage, the lights and the sound system, and Maria will collect those by the end of the month.",
  "That's everything for today. See you all next week.",
].join("\n\n");

const ERROR_MESSAGES: Record<FileErrorKind, string> = {
  corrupt: "This file is damaged or isn't an audio file",
  empty: "This file is empty",
  failed: "Transcription failed",
  noAudioTrack: "This file has no audio track",
  noModel: "Download a voice model first",
  noSpeech: "No speech found in this file",
  tooLong: "This file is longer than 3 hours",
  unreadable: "Couldn't open this file",
  unsupported: "This file type isn't supported",
};

/** What the simulated job needs from the rest of the mock. */
export interface MockFileHost {
  /** A dictation is running: the job waits, as the app's does. */
  dictationBusy: () => boolean;
  emit: (progress: FileProgress) => void;
  hasVoiceModel: () => boolean;
  polishes: () => boolean;
  /** Saves to history and counts in the stats, respecting retention. */
  save: (entry: HistoryEntry) => void;
}

export interface MockFileJobs {
  cancel: (jobId: string) => void;
  current: () => FileProgress | null;
  /** Makes the next job fail with this error, or `null` to succeed. */
  failNext: (kind: FileErrorKind | null) => void;
  transcribe: (path: string) => string;
}

/**
 * Simulates `transcribe_file`: decoding, transcribing and (with a language
 * model) polishing over about 20 s, then a history entry named after the
 * file.
 */
export function createMockFileJobs(host: MockFileHost): MockFileJobs {
  let running: {
    progress: FileProgress;
    timer: ReturnType<typeof setInterval>;
  } | null = null;
  let failNext: FileErrorKind | null = null;

  function report(progress: FileProgress) {
    if (running) {
      running.progress = progress;
    }
    host.emit(progress);
  }

  function end(progress: FileProgress) {
    if (running) {
      clearInterval(running.timer);
      running = null;
    }
    host.emit(progress);
  }

  function transcribe(path: string): string {
    const extension = path.split(".").pop()?.toLowerCase() ?? "";
    if (!EXTENSIONS.has(extension)) {
      throw new IpcError(
        "invalidInput",
        "Tiny Wispr can transcribe WAV, MP3, M4A, AAC, FLAC and Ogg files, and the sound of MP4 and MOV videos"
      );
    }
    if (running) {
      throw new IpcError("invalidInput", "A file is already being transcribed");
    }
    const jobId = crypto.randomUUID();
    const fileName = path.split(PATH_SEPARATOR).pop() ?? path;
    const base: FileProgress = {
      durationMs: null,
      error: null,
      fileName,
      fraction: null,
      jobId,
      stage: "decoding",
      text: null,
      wordCount: null,
    };
    const fail = (kind: FileErrorKind) =>
      end({
        ...base,
        error: { kind, message: ERROR_MESSAGES[kind] },
        stage: "failed",
      });
    const polish = host.polishes();
    let tick = 0;
    const at = (stage: FileStage, fraction: number | null) => ({
      ...base,
      durationMs: SAMPLE_DURATION_MS,
      fraction,
      stage,
    });
    const step = () => {
      if (host.dictationBusy()) {
        return;
      }
      tick += 1;
      if (tick === DECODE_TICKS && failNext) {
        const kind = failNext;
        failNext = null;
        fail(kind);
      } else if (tick <= DECODE_TICKS) {
        report(base);
      } else if (tick <= DECODE_TICKS + TRANSCRIBE_TICKS) {
        report(at("transcribing", (tick - DECODE_TICKS) / TRANSCRIBE_TICKS));
      } else if (
        polish &&
        tick <= DECODE_TICKS + TRANSCRIBE_TICKS + POLISH_TICKS
      ) {
        const done = tick - DECODE_TICKS - TRANSCRIBE_TICKS - 1;
        report(at("polishing", done / POLISH_TICKS));
      } else {
        complete();
      }
    };
    const complete = () => {
      const wordCount = SAMPLE_TEXT.split(WHITESPACE).length;
      host.save({
        app: { category: "other", id: null, name: `File: ${fileName}` },
        createdAt: Date.now(),
        durationMs: SAMPLE_DURATION_MS,
        finalText: SAMPLE_TEXT,
        id: crypto.randomUUID(),
        language: null,
        rawText: SAMPLE_TEXT,
        wordCount,
      });
      end({ ...at("done", 1), text: SAMPLE_TEXT, wordCount });
    };

    if (!host.hasVoiceModel()) {
      setTimeout(() => fail("noModel"), 0);
      return jobId;
    }
    running = { progress: base, timer: setInterval(step, TICK_MS) };
    setTimeout(() => report(base), 0);
    return jobId;
  }

  return {
    cancel(jobId) {
      if (running?.progress.jobId === jobId) {
        const { progress } = running;
        end({ ...progress, fraction: null, stage: "cancelled" });
      }
    },
    current: () => running?.progress ?? null,
    failNext(kind) {
      failNext = kind;
    },
    transcribe,
  };
}

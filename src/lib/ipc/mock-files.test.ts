import type { FileProgress, HistoryEntry } from "./types";

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { createMockFileJobs, SAMPLE_FILE_PATH } from "./mock-files";

function setup(options: { busy?: boolean; model?: boolean } = {}) {
  const events: FileProgress[] = [];
  const saved: HistoryEntry[] = [];
  const host = {
    busy: options.busy ?? false,
  };
  const jobs = createMockFileJobs({
    dictationBusy: () => host.busy,
    emit: (progress) => events.push(progress),
    hasVoiceModel: () => options.model ?? true,
    polishes: () => false,
    save: (entry) => saved.push(entry),
  });
  return { events, host, jobs, saved };
}

describe("mock file jobs", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("runs through the stages in about 20 s and saves to history", () => {
    const { events, jobs, saved } = setup();
    const id = jobs.transcribe(SAMPLE_FILE_PATH);
    vi.advanceTimersByTime(21_000);
    const stages = [...new Set(events.map((e) => e.stage))];
    expect(stages).toEqual(["decoding", "transcribing", "done"]);
    expect(events.at(-1)?.text).toBeTruthy();
    expect(events.every((e) => e.jobId === id)).toBe(true);
    expect(saved[0]?.app?.name).toBe("File: Planning call.m4a");
    expect(jobs.current()).toBeNull();
  });

  it("cancels, waits for dictation, and refuses a second job", () => {
    const { events, host, jobs, saved } = setup({ busy: true });
    const id = jobs.transcribe("memo.mp3");
    expect(() => jobs.transcribe("other.mp3")).toThrow("already");
    vi.advanceTimersByTime(5000);
    expect(events.map((e) => e.stage)).toEqual(["decoding"]);
    host.busy = false;
    vi.advanceTimersByTime(5000);
    expect(jobs.current()?.stage).toBe("transcribing");
    jobs.cancel(id);
    expect(events.at(-1)?.stage).toBe("cancelled");
    vi.advanceTimersByTime(30_000);
    expect(saved).toHaveLength(0);
  });

  it("fails without a voice model and rejects unsupported files", () => {
    const { events, jobs } = setup({ model: false });
    expect(() => jobs.transcribe("notes.txt")).toThrow("can transcribe");
    jobs.transcribe("memo.wav");
    vi.advanceTimersByTime(10);
    expect(events.at(-1)?.error?.kind).toBe("noModel");
  });
});

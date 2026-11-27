import type { NetworkRequest, UpdateState } from "./types";

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { createMockUpdates, nextVersion } from "./mock-updates";

const MANIFEST = /latest\.json$/;

function setup(options: { busy?: boolean } = {}) {
  const states: UpdateState[] = [];
  const log: NetworkRequest[] = [];
  const restart = vi.fn();
  const updates = createMockUpdates({
    busy: () => options.busy ?? false,
    currentVersion: "0.1.0",
    emit: (state) => states.push(state),
    log: (request) => log.push(request),
    restart,
  });
  return { log, restart, states, updates };
}

describe("mock updates", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("finds the next version, downloads it and restarts", () => {
    const { log, restart, states, updates } = setup();
    updates.check();
    expect(updates.state().state).toBe("checking");
    vi.advanceTimersByTime(1000);
    const available = updates.state();
    expect(available.state).toBe("available");
    expect(available.state === "available" && available.release.version).toBe(
      "0.1.1"
    );

    updates.download();
    vi.advanceTimersByTime(5000);
    expect(updates.state().state).toBe("ready");
    expect(states.some((s) => s.state === "downloading")).toBe(true);

    updates.install();
    expect(updates.state().state).toBe("installing");
    vi.advanceTimersByTime(2000);
    expect(restart).toHaveBeenCalledOnce();
    expect(log.map((r) => r.result)).toEqual(["completed", "completed"]);
    expect(log[0]?.url).toMatch(MANIFEST);
  });

  it("says when nothing newer is out", () => {
    const { updates } = setup();
    updates.outcome("upToDate");
    updates.check();
    vi.advanceTimersByTime(1000);
    expect(updates.state().state).toBe("upToDate");
  });

  it("keeps the release after a failed download so it can be retried", () => {
    const { log, updates } = setup();
    updates.outcome("downloadFails");
    updates.check();
    vi.advanceTimersByTime(1000);
    updates.download();
    vi.advanceTimersByTime(5000);
    const failed = updates.state();
    expect(failed.state === "failed" && failed.release?.version).toBe("0.1.1");
    expect(log.at(-1)?.result).toBe("failed");
  });

  it("won't restart during a dictation", () => {
    const { updates } = setup({ busy: true });
    updates.jump("ready");
    expect(() => updates.install()).toThrow(
      "Finish the dictation first, then restart to update"
    );
  });

  it("counts the next patch version", () => {
    expect(nextVersion("0.1.0")).toBe("0.1.1");
    expect(nextVersion("1.4.9-rc.1")).toBe("1.4.10");
  });
});

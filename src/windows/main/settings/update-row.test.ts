import type { Release } from "@/lib/ipc";

import { describe, expect, it } from "vitest";

import { updateRow } from "./update-row";

const release: Release = {
  notes: "- Faster start",
  publishedAt: null,
  version: "0.2.1",
};

describe("update row", () => {
  it("offers the next step in each state", () => {
    expect(updateRow({ state: "idle" }).action?.kind).toBe("check");
    expect(updateRow({ release, state: "available" }).action?.kind).toBe(
      "download"
    );
    expect(updateRow({ release, state: "ready" }).action).toEqual({
      kind: "install",
      label: "Restart to update",
    });
    expect(
      updateRow({ received: 1, release, state: "downloading", total: 2 }).action
    ).toBeNull();
  });

  it("retries the step that failed", () => {
    expect(
      updateRow({ message: "x", release, state: "failed" }).action?.kind
    ).toBe("download");
    expect(
      updateRow({ message: "x", release: null, state: "failed" }).action?.kind
    ).toBe("check");
  });

  it("names the version", () => {
    expect(updateRow({ release, state: "available" }).label).toBe(
      "Version 0.2.1 is available"
    );
  });
});

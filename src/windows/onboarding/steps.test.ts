import type { ModelInfo, Permissions } from "@/lib/ipc";

import { describe, expect, it, vi } from "vitest";

import { isCancelPress } from "@/components/hotkey-recorder";
import { micProblem } from "@/components/mic-test";
import { requiredPermissions } from "@/components/permission-list";
import { IpcError } from "@/lib/ipc/errors";

import { machineSummary } from "./step-models";
import { withTranscript } from "./step-try-it";
import { adjacentStep, fill, type StepContext, stepGate } from "./steps";

vi.mock("@/lib/ipc", () => ({ getDictationState: () => undefined }));
vi.mock("@/lib/ipc/store", () => ({ createIpcStore: () => ({}) }));

const MAC_PENDING: Permissions = {
  accessibility: "notDetermined",
  inputMonitoring: "notApplicable",
  microphone: "granted",
};
const MAC_GRANTED: Permissions = {
  accessibility: "granted",
  inputMonitoring: "granted",
  microphone: "granted",
};
const LINUX: Permissions = {
  accessibility: "notApplicable",
  inputMonitoring: "notApplicable",
  microphone: "notApplicable",
};

function voice(id: string, state: "downloaded" | "notDownloaded"): ModelInfo {
  return {
    accuracy: 3,
    compression: "none",
    contextLength: null,
    custom: false,
    description: "",
    englishOnly: false,
    family: "whisper",
    file: id,
    id,
    kind: "voice",
    label: id,
    ramMb: 0,
    reasoning: false,
    recommended: false,
    sha256: null,
    sizeBytes: 1,
    speed: 3,
    status: { state },
    url: null,
    variantOf: null,
  };
}

const context = (patch: Partial<StepContext> = {}): StepContext => ({
  heardSpeech: false,
  permissions: MAC_PENDING,
  sttModel: null,
  voiceModels: [],
  ...patch,
});

describe("stepGate", () => {
  it("lets welcome, shortcut, polish and try it through", () => {
    for (const step of ["welcome", "shortcut", "polish", "tryIt"] as const) {
      expect(stepGate(step, context()).met).toBe(true);
    }
  });

  it("names the first permission still missing", () => {
    expect(stepGate("permissions", context())).toEqual({
      met: false,
      reason: "Allow Accessibility to continue",
    });
    expect(
      stepGate(
        "permissions",
        context({ permissions: { ...MAC_PENDING, microphone: "denied" } })
      )
    ).toEqual({ met: false, reason: "Allow microphone access to continue" });
    expect(
      stepGate("permissions", context({ permissions: MAC_GRANTED })).met
    ).toBe(true);
    expect(stepGate("permissions", context({ permissions: LINUX })).met).toBe(
      true
    );
  });

  it("waits for the microphone to hear speech", () => {
    expect(stepGate("microphone", context()).met).toBe(false);
    expect(stepGate("microphone", context({ heardSpeech: true })).met).toBe(
      true
    );
  });

  it("needs a voice model that is selected and downloaded", () => {
    const notDownloaded = voice("small", "notDownloaded");
    const downloaded = voice("base", "downloaded");
    expect(stepGate("voiceModel", context())).toEqual({
      met: false,
      reason: "Download a voice model to continue",
    });
    expect(
      stepGate(
        "voiceModel",
        context({ sttModel: "small", voiceModels: [notDownloaded, downloaded] })
      )
    ).toEqual({ met: false, reason: "Choose a downloaded model to continue" });
    expect(
      stepGate(
        "voiceModel",
        context({ sttModel: "base", voiceModels: [notDownloaded, downloaded] })
      ).met
    ).toBe(true);
  });
});

describe("adjacentStep", () => {
  it("walks the steps in order", () => {
    expect(adjacentStep("welcome", 1, context())).toBe("permissions");
    expect(adjacentStep("polish", 1, context())).toBe("tryIt");
    expect(adjacentStep("tryIt", 1, context())).toBeUndefined();
    expect(adjacentStep("welcome", -1, context())).toBeUndefined();
  });

  it("skips permissions that are already granted, both ways", () => {
    const granted = context({ permissions: MAC_GRANTED });
    expect(adjacentStep("welcome", 1, granted)).toBe("microphone");
    expect(adjacentStep("microphone", -1, granted)).toBe("welcome");
    expect(adjacentStep("welcome", 1, context({ permissions: LINUX }))).toBe(
      "microphone"
    );
  });
});

describe("helpers", () => {
  it("fills placeholders and leaves unknown ones", () => {
    expect(
      fill("Hold {key} on this {place}. {other}", { key: "fn", place: "Mac" })
    ).toBe("Hold fn on this Mac. {other}");
  });

  it("lists only the permissions the platform asks for", () => {
    expect(requiredPermissions(MAC_PENDING)).toEqual([
      "microphone",
      "accessibility",
    ]);
    expect(requiredPermissions(LINUX)).toEqual([]);
  });

  it("treats Escape alone as cancel unless recording Cancel", () => {
    const escapeAlone = { key: "Escape", modifiers: [] };
    expect(isCancelPress(escapeAlone, "pushToTalk")).toBe(true);
    expect(isCancelPress(escapeAlone, "cancel")).toBe(false);
    expect(
      isCancelPress({ key: "Escape", modifiers: ["ctrl"] }, "pushToTalk")
    ).toBe(false);
  });

  it("explains why the microphone test failed", () => {
    expect(micProblem(new IpcError("noMicrophone", "No microphone"))).toEqual({
      kind: "noMicrophone",
    });
    expect(
      micProblem({ kind: "microphoneBusy", message: "Another app is using it" })
    ).toEqual({ kind: "inUse" });
    expect(micProblem(new IpcError("permissionDenied", "Off"))).toEqual({
      kind: "denied",
    });
    expect(micProblem("boom")).toEqual({ kind: "failed", message: "boom" });
  });

  it("appends a transcript unless the paste already put it there", () => {
    expect(withTranscript("", "Hello there.")).toBe("Hello there.");
    expect(withTranscript("Hi. ", "Hello there.")).toBe("Hi. Hello there.");
    expect(withTranscript("Hi. Hello there.", "Hello there.")).toBe(
      "Hi. Hello there."
    );
    expect(withTranscript("Hi.", "")).toBe("Hi.");
  });

  it("summarises the machine", () => {
    const hardware = {
      appleSilicon: true,
      arch: "aarch64",
      cpuCores: 14,
      cpuName: "Apple M3 Max",
      platform: "macos",
      totalMemoryBytes: 36 * 1024 ** 3,
    } as const;
    expect(machineSummary(hardware)).toBe("Apple M3 Max · 36 GB of memory");
    expect(machineSummary({ ...hardware, cpuName: null })).toBe(
      "Apple silicon · 14 cores · 36 GB of memory"
    );
  });
});

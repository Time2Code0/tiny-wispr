import type { ModelInfo } from "@/lib/ipc";

import { describe, expect, it, vi } from "vitest";

import {
  defaultVariant,
  groupVariants,
  initialLanguages,
  modelSections,
  withProgress,
} from "@/components/model-list";
import {
  formatBytes,
  formatProgress,
  progressPercent,
} from "@/components/model-row";

vi.mock("@/lib/ipc", () => ({}));

const MB = 1_000_000;

function model(
  id: string,
  sizeMb: number,
  speed: number,
  accuracy: number,
  extra: Partial<ModelInfo> = {}
): ModelInfo {
  return {
    accuracy,
    compression: "none",
    contextLength: null,
    custom: false,
    description: "",
    englishOnly: false,
    family: "whisper",
    file: `${id}.bin`,
    id,
    kind: "voice",
    label: id,
    ramMb: 0,
    reasoning: false,
    recommended: false,
    sha256: null,
    sizeBytes: sizeMb * MB,
    speed,
    status: { state: "notDownloaded" },
    url: null,
    variantOf: null,
    ...extra,
  };
}

/** The real whisper catalog's shape (tw-core), sizes in MB. */
const VOICE: ModelInfo[] = [
  model("tiny", 78, 5, 1),
  model("tiny-q5_1", 32, 5, 1, { compression: "strong", variantOf: "tiny" }),
  model("tiny.en", 78, 5, 1, { englishOnly: true }),
  model("base", 148, 5, 2),
  model("base-q5_1", 60, 5, 2, { compression: "strong", variantOf: "base" }),
  model("base.en", 148, 5, 2, { englishOnly: true }),
  model("small", 488, 4, 3),
  model("small-q5_1", 190, 4, 3, { compression: "strong", variantOf: "small" }),
  model("small.en", 488, 4, 3, { englishOnly: true }),
  model("medium", 1534, 2, 4),
  model("medium-q5_0", 539, 2, 4, {
    compression: "strong",
    variantOf: "medium",
  }),
  model("medium.en", 1534, 2, 4, { englishOnly: true }),
  model("large-v3-turbo", 1625, 3, 5, { recommended: true }),
  model("large-v3-turbo-q8_0", 874, 3, 5, {
    compression: "light",
    variantOf: "large-v3-turbo",
  }),
  model("large-v3-turbo-q5_0", 574, 3, 4, {
    compression: "strong",
    variantOf: "large-v3-turbo",
  }),
  model("large-v3", 3095, 1, 5),
  model("large-v3-q5_0", 1081, 1, 5, {
    compression: "strong",
    variantOf: "large-v3",
  }),
];

const LANGUAGE: ModelInfo[] = [
  model("llama-1b", 808, 5, 2, { family: "llama", kind: "language" }),
  model("llama-3b", 2019, 3, 3, { family: "llama", kind: "language" }),
  model("qwen-2b", 1281, 4, 3, { family: "qwen", kind: "language" }),
  model("qwen-4b", 2497, 3, 4, { family: "qwen", kind: "language" }),
  model("ministral-3b", 2147, 3, 4, { family: "mistral", kind: "language" }),
  model("ministral-8b", 5199, 2, 5, {
    family: "mistral",
    kind: "language",
    recommended: true,
  }),
  model("r1-1.5b", 1117, 3, 1, {
    family: "deepseek",
    kind: "language",
    reasoning: true,
  }),
  model("r1-8b", 5028, 1, 4, {
    family: "deepseek",
    kind: "language",
    reasoning: true,
  }),
];

const ids = (models: ModelInfo[]) => models.map((m) => m.id);

const rowIds = (sections: ReturnType<typeof modelSections>) =>
  sections.map((section) => section.groups.map((group) => group.id));

describe("groupVariants", () => {
  it("puts compressed copies under their original, largest first", () => {
    const turbo = groupVariants(VOICE).find((g) => g.id === "large-v3-turbo");
    expect(ids(turbo?.variants ?? [])).toEqual([
      "large-v3-turbo",
      "large-v3-turbo-q8_0",
      "large-v3-turbo-q5_0",
    ]);
  });

  it("orders rows by the original's size, smallest first", () => {
    expect(groupVariants(LANGUAGE).map((g) => g.id)).toEqual([
      "llama-1b",
      "r1-1.5b",
      "qwen-2b",
      "llama-3b",
      "ministral-3b",
      "qwen-4b",
      "r1-8b",
      "ministral-8b",
    ]);
  });

  it("shows a copy on its own when its original is missing", () => {
    const copy = VOICE.find((m) => m.id === "base-q5_1");
    expect(groupVariants(copy ? [copy] : []).map((g) => g.id)).toEqual([
      "base-q5_1",
    ]);
  });
});

describe("modelSections", () => {
  it("lists one multilingual row per size for other languages", () => {
    expect(rowIds(modelSections(VOICE, "any"))).toEqual([
      ["tiny", "base", "small", "medium", "large-v3-turbo", "large-v3"],
    ]);
  });

  it("lists English-only models, and multilingual ones where no English version exists", () => {
    expect(rowIds(modelSections(VOICE, "english"))).toEqual([
      [
        "tiny.en",
        "base.en",
        "small.en",
        "medium.en",
        "large-v3-turbo",
        "large-v3",
      ],
    ]);
  });

  it("puts reasoning language models and imported files in their own sections", () => {
    const mine = model("mine", 1, 0, 0, {
      custom: true,
      family: "custom",
      kind: "language",
    });
    const sections = modelSections([mine, ...LANGUAGE], "any");
    expect(sections.map((section) => section.title)).toEqual([
      null,
      "Slower models that think before answering",
      "Imported",
    ]);
    expect(rowIds(sections)[1]).toEqual(["r1-1.5b", "r1-8b"]);
  });
});

describe("defaultVariant", () => {
  const [turbo] = groupVariants(
    VOICE.filter((m) => m.id.startsWith("large-v3-turbo"))
  );

  it("describes the recommended file, or the full-size one", () => {
    expect(turbo && defaultVariant(turbo, null)?.id).toBe("large-v3-turbo");
    const [plain] = groupVariants(
      VOICE.filter((m) => m.id.startsWith("medium"))
    );
    expect(plain && defaultVariant(plain, null)?.id).toBe("medium");
  });

  it("prefers the file in use, then one on disk", () => {
    const group = turbo && {
      ...turbo,
      variants: turbo.variants.map((m) =>
        m.id === "large-v3-turbo-q5_0"
          ? { ...m, status: { state: "downloaded" as const } }
          : m
      ),
    };
    expect(group && defaultVariant(group, null)?.id).toBe(
      "large-v3-turbo-q5_0"
    );
    expect(group && defaultVariant(group, "large-v3-turbo-q8_0")?.id).toBe(
      "large-v3-turbo-q8_0"
    );
  });
});

describe("initialLanguages", () => {
  const english = { autoDetect: false, selected: ["en"] };
  const detect = { autoDetect: true, selected: ["en"] };

  it("follows an English-only model in use", () => {
    expect(initialLanguages(VOICE, "base.en", detect)).toBe("english");
  });

  it("uses English when dictation is set to English alone", () => {
    expect(initialLanguages(VOICE, null, english)).toBe("english");
    expect(initialLanguages(VOICE, "large-v3", english)).toBe("english");
    expect(initialLanguages(VOICE, null, detect)).toBe("any");
  });

  it("never hides the model in use", () => {
    expect(initialLanguages(VOICE, "small", english)).toBe("any");
  });
});

describe("withProgress", () => {
  const base = model("base", 148, 5, 2);

  it("shows the latest received bytes while downloading", () => {
    const progress = {
      id: "base",
      received: 50,
      state: "downloading",
      total: 148,
    } as const;
    expect(withProgress(base, progress).status).toEqual({
      received: 50,
      state: "downloading",
      total: 148,
    });
  });

  it("leaves the model alone once the download stopped", () => {
    const progress = {
      id: "base",
      received: 148,
      state: "done",
      total: 148,
    } as const;
    expect(withProgress(base, progress)).toBe(base);
    expect(withProgress(base, undefined)).toBe(base);
  });
});

describe("sizes", () => {
  it("formats downloads in MB below a gigabyte and GB above", () => {
    expect(formatBytes(32_152_673)).toBe("32 MB");
    expect(formatBytes(574_041_195)).toBe("574 MB");
    expect(formatBytes(1_624_555_275)).toBe("1.6 GB");
    expect(formatBytes(3_095_033_483)).toBe("3.1 GB");
    expect(formatBytes(0)).toBe("1 MB");
  });

  it("formats progress and clamps the percentage", () => {
    expect(formatProgress(312 * MB, 1_624_555_275)).toBe("312 MB of 1.6 GB");
    expect(progressPercent(50, 200)).toBe(25);
    expect(progressPercent(300, 200)).toBe(100);
    expect(progressPercent(10, 0)).toBe(0);
  });
});

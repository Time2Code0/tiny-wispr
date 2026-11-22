import type { HistoryEntry } from "@/lib/ipc";

import { describe, expect, it } from "vitest";

import {
  buildHeatmap,
  dayLabel,
  greeting,
  groupByDay,
  heatLevel,
  wasEdited,
} from "./home/format";
import { hashFor, homeHash, homeTabFromHash, parseHash } from "./route";
import { CATEGORIES, styleSample } from "./style/styles";
import {
  expansionIssue,
  isDuplicate,
  matchesSearch,
  phraseIssue,
} from "./validation";

const entry = (id: string, at: Date, raw = "a", final = "a"): HistoryEntry => ({
  app: null,
  createdAt: at.getTime(),
  durationMs: 1000,
  finalText: final,
  id,
  language: "en",
  rawText: raw,
  wordCount: 1,
});

describe("routes", () => {
  it("reads deep links", () => {
    expect(parseHash("#/settings/privacy")).toEqual({
      route: "settings",
      section: "privacy",
    });
    expect(parseHash("#/settings")).toEqual({
      route: "settings",
      section: "general",
    });
    expect(parseHash("#/dictionary")).toEqual({
      route: "dictionary",
      section: null,
    });
    expect(parseHash("#/nope")).toEqual({ route: "home", section: null });
    expect(parseHash("")).toEqual({ route: "home", section: null });
  });

  it("writes hashes that read back", () => {
    expect(parseHash(hashFor("settings", "about"))).toEqual({
      route: "settings",
      section: "about",
    });
    expect(hashFor("models")).toBe("#/models");
  });

  it("keeps Home's tab in the hash", () => {
    expect(homeTabFromHash("#/home/activity")).toBe("activity");
    expect(homeTabFromHash("#/home")).toBe("history");
    expect(homeTabFromHash("")).toBe("history");
    expect(homeTabFromHash("#/settings/activity")).toBe("history");
    expect(parseHash(homeHash("activity"))).toEqual({
      route: "home",
      section: null,
    });
    expect(homeTabFromHash(homeHash("activity"))).toBe("activity");
  });
});

describe("greeting", () => {
  it("follows the local hour", () => {
    expect(greeting(3)).toBe("Good evening");
    expect(greeting(5)).toBe("Good morning");
    expect(greeting(11)).toBe("Good morning");
    expect(greeting(12)).toBe("Good afternoon");
    expect(greeting(17)).toBe("Good afternoon");
    expect(greeting(18)).toBe("Good evening");
  });
});

describe("history days", () => {
  const now = new Date(2026, 10, 15, 10, 0);

  it("labels today, yesterday and older days", () => {
    expect(dayLabel("2026-11-15", now, "en-US")).toBe("Today");
    expect(dayLabel("2026-11-14", now, "en-US")).toBe("Yesterday");
    expect(dayLabel("2026-11-10", now, "en-US")).toBe("Tuesday, November 10");
    expect(dayLabel("2025-12-31", now, "en-US")).toBe(
      "Wednesday, December 31, 2025"
    );
  });

  it("groups by local day and keeps the order", () => {
    const groups = groupByDay(
      [
        entry("a", new Date(2026, 10, 15, 9)),
        entry("b", new Date(2026, 10, 15, 0, 5)),
        entry("c", new Date(2026, 10, 14, 23, 55)),
        entry("d", new Date(2026, 10, 2, 12)),
      ],
      now,
      "en-US"
    );
    expect(groups.map((g) => [g.label, g.entries.map((e) => e.id)])).toEqual([
      ["Today", ["a", "b"]],
      ["Yesterday", ["c"]],
      ["Monday, November 2", ["d"]],
    ]);
  });

  it("knows when cleanup changed the text", () => {
    expect(wasEdited(entry("a", now, "um hi", "Hi."))).toBe(true);
    expect(wasEdited(entry("a", now, "Hi.", "Hi. "))).toBe(false);
  });
});

describe("heatmap", () => {
  it("bands words into five shades", () => {
    expect(heatLevel(0, 100)).toBe(0);
    expect(heatLevel(1, 100)).toBe(1);
    expect(heatLevel(25, 100)).toBe(1);
    expect(heatLevel(26, 100)).toBe(2);
    expect(heatLevel(75, 100)).toBe(3);
    expect(heatLevel(100, 100)).toBe(4);
  });

  it("ends on today's week and leaves the future empty", () => {
    // Sunday 15 November 2026: the last column holds only today.
    const today = new Date(2026, 10, 15, 18);
    const map = buildHeatmap(
      [
        { date: "2026-11-15", words: 40 },
        { date: "2026-11-14", words: 10 },
        { date: "2026-05-01", words: 999 },
      ],
      today
    );
    expect(map.weeks).toHaveLength(26);
    expect(map.weeks.every((w) => w.length === 7)).toBe(true);
    const last = map.weeks.at(-1) ?? [];
    expect(last[0]).toEqual({ date: "2026-11-15", level: 4, words: 40 });
    expect(last.slice(1).every((cell) => cell === null)).toBe(true);
    expect(map.weeks[0]?.[0]?.date).toBe("2026-05-24");
    expect(map.weeks.at(-2)?.[6]).toEqual({
      date: "2026-11-14",
      level: 1,
      words: 10,
    });
    expect(map.totalWords).toBe(50);
    expect(map.activeDays).toBe(2);
  });
});

describe("validation", () => {
  it("checks phrases", () => {
    expect(phraseIssue("Word", "  ")).toBe("Word can't be empty");
    expect(phraseIssue("Word", "x".repeat(61))).toBe(
      "Word can be up to 60 characters"
    );
    expect(phraseIssue("Word", "x".repeat(60))).toBeNull();
  });

  it("finds duplicates ignoring case and the item itself", () => {
    const items = [{ id: "1", word: "Tauri" }];
    expect(isDuplicate(items, (i) => i.word, " tauri ", null)).toBe(true);
    expect(isDuplicate(items, (i) => i.word, "tauri", "1")).toBe(false);
    expect(isDuplicate(items, (i) => i.word, "Rust", null)).toBe(false);
  });

  it("checks expansions", () => {
    expect(expansionIssue("")).toBe("Expansion can't be empty");
    expect(expansionIssue("x".repeat(4001))).toBe(
      "Expansion can be up to 4,000 characters"
    );
    expect(expansionIssue("x".repeat(4000))).toBeNull();
  });

  it("searches any field", () => {
    expect(matchesSearch("", "a")).toBe(true);
    expect(matchesSearch("GRE", null, "Greg")).toBe(true);
    expect(matchesSearch("x", null, "Greg")).toBe(false);
  });
});

describe("style samples", () => {
  it("drops the final period only in messaging apps", () => {
    expect(styleSample("casual", "personal")).toBe(
      "Sounds good, let's meet at noon"
    );
    expect(styleSample("veryCasual", "personal")).toBe(
      "sounds good, let's meet at noon"
    );
    expect(styleSample("casual", "email")).toBe(
      "Sounds good, let's meet at noon."
    );
    expect(styleSample("formal", "work")).toBe(
      "Sounds good. Let's meet at noon."
    );
    expect(styleSample("excited", "other")).toBe(
      "Sounds good, let's meet at noon!"
    );
  });

  it("offers very casual only for personal messages", () => {
    for (const [category, { styles }] of Object.entries(CATEGORIES)) {
      expect(styles.includes("veryCasual")).toBe(category === "personal");
      expect(styles.includes("excited")).toBe(category !== "personal");
    }
  });
});

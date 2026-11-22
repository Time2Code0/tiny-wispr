import { describe, expect, it } from "vitest";

import { approximatePolish } from "./mock-text";

describe("mock test_polish", () => {
  it.each([
    ["formal", "other", "Thanks for the update."],
    ["casual", "personal", "Thanks for the update"],
    ["casual", "email", "Thanks for the update."],
    ["veryCasual", "work", "thanks for the update"],
    ["excited", "other", "Thanks for the update!"],
  ] as const)("%s in %s", (style, category, expected) => {
    expect(
      approximatePolish("  thanks for  the update ", "medium", style, category)
    ).toBe(expected);
  });

  it("leaves text alone without cleanup", () => {
    expect(approximatePolish("um hi", "none", "formal", "other")).toBe("um hi");
  });
});

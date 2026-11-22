import { describe, expect, it } from "vitest";

import { noteBlocks } from "./release-notes";

describe("release notes", () => {
  it("keeps headings, lists and paragraphs", () => {
    expect(
      noteBlocks(
        "## 0.2.0\n\n### Features\n\n- Check for **updates**\n- Show `notes`\n\nThanks for testing."
      )
    ).toEqual([
      { kind: "heading", text: "0.2.0" },
      { kind: "heading", text: "Features" },
      { items: ["Check for updates", "Show notes"], kind: "list" },
      { kind: "paragraph", text: "Thanks for testing." },
    ]);
  });

  it("shows a link's text and never its address", () => {
    expect(noteBlocks("- Fix [the pill](https://example.com/1)")).toEqual([
      { items: ["Fix the pill"], kind: "list" },
    ]);
  });
});

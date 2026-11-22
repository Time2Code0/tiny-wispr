import { describe, expect, it } from "vitest";

import { BAR_REST, barHeights } from "./level-bars";

describe("barHeights", () => {
  it("rests at the minimum height in silence", () => {
    expect(barHeights([0, 0, 0], 5)).toEqual(
      Array.from({ length: 5 }, () => BAR_REST)
    );
    expect(barHeights([], 5)).toEqual(
      Array.from({ length: 5 }, () => BAR_REST)
    );
  });

  it("puts the newest level in the centre and older ones outwards", () => {
    const heights = barHeights([0, 0, 1], 5);
    expect(heights[2]).toBe(1);
    expect(heights[1]).toBe(BAR_REST);
    expect(heights[3]).toBe(BAR_REST);
  });

  it("is symmetric and stays within rest..1", () => {
    const heights = barHeights([0.2, 0.9, 0.4, 1, 0.6], 9);
    expect(heights).toEqual([...heights].reverse());
    for (const height of heights) {
      expect(height).toBeGreaterThanOrEqual(BAR_REST);
      expect(height).toBeLessThanOrEqual(1);
    }
  });

  it("makes the centre respond more than the edges", () => {
    const heights = barHeights(
      Array.from({ length: 9 }, () => 1),
      9
    );
    expect(heights[4]).toBe(1);
    expect(heights[0]).toBeLessThan(heights[2] ?? 0);
    expect(heights[2]).toBeLessThan(heights[4] ?? 0);
  });

  it("clamps levels outside 0..1", () => {
    expect(barHeights([5], 1)).toEqual([1]);
    expect(barHeights([-1], 1)).toEqual([BAR_REST]);
  });

  it("follows one averaged level when calm, so nothing ripples", () => {
    const rippling = barHeights([0, 1, 0, 1], 5);
    const calm = barHeights([0, 1, 0, 1], 5, true);
    expect(calm[1]).toBe(calm[3]);
    expect(calm[2]).toBeCloseTo(BAR_REST + (1 - BAR_REST) * Math.sqrt(0.5));
    expect(calm).not.toEqual(rippling);
  });
});

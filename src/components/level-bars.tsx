import { cn } from "cn";
import { useReducedMotion } from "motion/react";

/** Fraction of the full height a bar keeps in silence, so it still reads as live. */
export const BAR_REST = 0.2;

/** How much the outermost bars respond compared with the centre ones. */
const EDGE_RESPONSE = 0.4;

/** Levels averaged into the single value shown under reduced motion. */
const CALM_WINDOW = 8;

const clamp01 = (value: number) => Math.min(1, Math.max(0, value));

/**
 * RMS of speech sits low on a linear scale; the square root spreads it over
 * the bar height the way a VU meter does.
 */
const loudness = (level: number) => Math.sqrt(clamp01(level));

/**
 * Bar heights (BAR_REST..1) for `count` bars from recent levels, newest last.
 * The newest level sits in the centre and older ones ripple outwards; the
 * centre bars respond more than the edges. With `calm`, every bar follows one
 * averaged level instead, so nothing ripples.
 */
export function barHeights(
  levels: readonly number[],
  count: number,
  calm = false
): number[] {
  const centre = (count - 1) / 2;
  const averaged = calm ? average(levels.slice(-CALM_WINDOW)) : 0;
  return Array.from({ length: count }, (_, index) => {
    const distance = Math.abs(index - centre);
    const spread = centre === 0 ? 0 : distance / centre;
    const response = 1 - (1 - EDGE_RESPONSE) * spread * spread;
    const level = calm ? averaged : (levels.at(-1 - Math.round(distance)) ?? 0);
    return BAR_REST + (1 - BAR_REST) * response * loudness(level);
  });
}

function average(values: readonly number[]): number {
  if (values.length === 0) {
    return 0;
  }
  return values.reduce((sum, value) => sum + value, 0) / values.length;
}

export type LevelBarsTone = "default" | "muted";

/**
 * A row of rounded vertical bars driven by recent input levels (0..1, newest
 * last), e.g. from `useAudioLevels`. The bars fill the height of the row: size
 * it with `className` (`h-4`), and widen the bars with a child selector
 * (`*:w-1`). Bars are decorative; give the surrounding control a label.
 */
export function LevelBars({
  levels,
  count = 9,
  active = true,
  tone = "default",
  className,
}: {
  /** Recent levels, oldest first. Shorter than `count` is fine. */
  levels: readonly number[];
  /** Number of bars; odd counts keep a single centre bar. */
  count?: number;
  /** When false, the bars rest at their minimum height. */
  active?: boolean;
  tone?: LevelBarsTone;
  className?: string;
}) {
  const calm = useReducedMotion() ?? false;
  const heights = active
    ? barHeights(levels, count, calm)
    : Array.from({ length: count }, () => BAR_REST);

  return (
    <div
      aria-hidden="true"
      className={cn("flex h-4 items-center gap-0.5", className)}
      data-active={active}
      data-slot="level-bars"
    >
      {heights.map((height, index) => (
        <span
          className={cn(
            "block w-0.5 shrink-0 rounded-full transition-[height,background-color] duration-150 ease-out motion-reduce:transition-none",
            tone === "default" && "bg-foreground",
            tone === "muted" && "bg-foreground-muted",
            !active && "opacity-60"
          )}
          // biome-ignore lint/suspicious/noArrayIndexKey: bars are positional and never reorder
          key={index}
          style={{ height: `${(height * 100).toFixed(1)}%` }}
        />
      ))}
    </div>
  );
}

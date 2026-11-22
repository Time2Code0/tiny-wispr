import type { DailyWords } from "@/lib/ipc";
import type { HeatCell } from "./format";

import { useCallback, useId, useMemo, useState } from "react";

import { cn } from "cn";

import {
  Tooltip,
  TooltipCreateHandle,
  TooltipPopup,
  TooltipTrigger,
} from "@/components/ui/tooltip";

import { buildHeatmap, HEATMAP_WEEKS } from "./format";

const SHADES = [
  "bg-foreground/5",
  "bg-primary/30",
  "bg-primary/55",
  "bg-primary/80",
  "bg-primary",
] as const;

const tooltip = TooltipCreateHandle<HeatCell>();

const words = (n: number) =>
  `${n.toLocaleString()} ${n === 1 ? "word" : "words"}`;

function cellLabel(cell: HeatCell): string {
  const [year = 0, month = 1, day = 1] = cell.date.split("-").map(Number);
  const date = new Date(year, month - 1, day).toLocaleDateString(undefined, {
    day: "numeric",
    month: "long",
    weekday: "long",
  });
  return cell.words > 0 ? `${date}: ${words(cell.words)}` : `${date}: none`;
}

const MOVES: Record<string, number> = {
  ArrowDown: 1,
  ArrowLeft: -7,
  ArrowRight: 7,
  ArrowUp: -1,
};

/**
 * Words per day for the last half year, one column per week. A single tab
 * stop; the arrow keys move between days and each day names itself.
 */
export function ActivityHeatmap({
  daily,
  today,
}: {
  daily: readonly DailyWords[];
  today: Date;
}) {
  const summaryId = useId();
  const map = useMemo(() => buildHeatmap(daily, today), [daily, today]);
  const days = useMemo(
    () => map.weeks.flat().filter((c): c is HeatCell => c !== null),
    [map]
  );
  const [focused, setFocused] = useState(days.length - 1);

  const onKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLElement>) => {
      const move = MOVES[event.key];
      if (move === undefined) {
        return;
      }
      event.preventDefault();
      const next = Math.min(days.length - 1, Math.max(0, focused + move));
      setFocused(next);
      event.currentTarget
        .querySelector<HTMLElement>(`[data-day="${next}"]`)
        ?.focus();
    },
    [days.length, focused]
  );
  const onFocus = useCallback((event: React.FocusEvent<HTMLElement>) => {
    const day = Number(event.target.dataset.day);
    if (Number.isInteger(day)) {
      setFocused(day);
    }
  }, []);

  const months = map.weeks.map((week) => {
    const start = week.find((cell) => cell?.date.endsWith("-01"));
    if (!start) {
      return null;
    }
    const [year = 0, month = 1] = start.date.split("-").map(Number);
    return new Date(year, month - 1, 1).toLocaleDateString(undefined, {
      month: "short",
    });
  });

  let index = -1;
  return (
    <figure className="flex flex-col gap-2">
      <div aria-hidden="true" className="grid auto-cols-fr grid-flow-col gap-1">
        {months.map((month, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: one label slot per week column
          <span className="text-foreground-muted text-xs" key={i}>
            {month}
          </span>
        ))}
      </div>
      {/* biome-ignore lint/a11y/noNoninteractiveElementInteractions: arrow keys move focus between the day cells inside */}
      {/* biome-ignore lint/a11y/useSemanticElements: a fieldset would imply form controls; this groups labelled day cells */}
      <div
        aria-describedby={summaryId}
        aria-label="Words per day"
        className="grid auto-cols-fr grid-flow-col grid-rows-7 gap-1"
        onFocus={onFocus}
        onKeyDown={onKeyDown}
        role="group"
      >
        {map.weeks.flat().map((cell, i) => {
          if (!cell) {
            // biome-ignore lint/suspicious/noArrayIndexKey: future days have no date to key on
            return <span aria-hidden="true" key={`future-${i}`} />;
          }
          index += 1;
          const position = index;
          return (
            <TooltipTrigger
              aria-label={cellLabel(cell)}
              className={cn(
                "aspect-square rounded-sm outline-none focus-visible:ring-2 focus-visible:ring-foreground",
                SHADES[cell.level]
              )}
              delay={0}
              handle={tooltip}
              key={cell.date}
              payload={cell}
              render={
                <div
                  data-day={position}
                  role="img"
                  tabIndex={position === focused ? 0 : -1}
                />
              }
            />
          );
        })}
      </div>
      <figcaption
        className="flex items-center justify-between gap-4 text-foreground-muted text-xs"
        id={summaryId}
      >
        <span>
          {map.activeDays > 0
            ? `${words(map.totalWords)} on ${map.activeDays} ${map.activeDays === 1 ? "day" : "days"} in the last ${HEATMAP_WEEKS} weeks.`
            : `No dictations in the last ${HEATMAP_WEEKS} weeks yet.`}
        </span>
        <span aria-hidden="true" className="flex items-center gap-1">
          Less
          {SHADES.map((shade) => (
            <span className={cn("size-2.5 rounded-sm", shade)} key={shade} />
          ))}
          More
        </span>
      </figcaption>
      <Tooltip handle={tooltip}>
        {({ payload }) => (
          <TooltipPopup>{payload ? cellLabel(payload) : null}</TooltipPopup>
        )}
      </Tooltip>
    </figure>
  );
}

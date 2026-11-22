import type { CleanupLevel } from "@/lib/ipc";

import { useCallback, useId } from "react";

import { cn } from "cn";

import { Radio, RadioGroup } from "@/components/ui/radio-group";

const SPOKEN = "um so we should uh ship it friday, actually thursday";

interface LevelCopy {
  example: string;
  label: string;
  /** What it needs or does, in a few words. */
  note: string;
}

const LEVELS: Record<CleanupLevel, LevelCopy> = {
  light: {
    example: "So we should ship it Friday, actually Thursday.",
    label: "Light",
    note: "No model needed",
  },
  medium: {
    example: "Let's ship it Thursday.",
    label: "Medium",
    note: "Needs a model",
  },
  none: {
    example: SPOKEN,
    label: "None",
    note: "As heard",
  },
};

const ORDER: readonly CleanupLevel[] = ["none", "light", "medium"];

function isCleanupLevel(value: unknown): value is CleanupLevel {
  return typeof value === "string" && value in LEVELS;
}

/**
 * None / Light / Medium as cards, each with a short note and the same
 * spoken sentence as it would come out.
 */
export function CleanupLevelCards({
  value,
  onValueChange,
  className,
}: {
  value: CleanupLevel;
  onValueChange: (level: CleanupLevel) => void;
  className?: string;
}) {
  const spokenId = useId();
  const change = useCallback(
    (next: unknown) => {
      if (isCleanupLevel(next)) {
        onValueChange(next);
      }
    },
    [onValueChange]
  );
  return (
    <div className={cn("flex flex-col gap-2", className)}>
      <p className="text-foreground-muted text-xs" id={spokenId}>
        You say “{SPOKEN}”
      </p>
      <RadioGroup
        aria-describedby={spokenId}
        aria-label="Cleanup level"
        className="grid grid-cols-3 gap-2"
        onValueChange={change}
        value={value}
      >
        {ORDER.map((level) => {
          const copy = LEVELS[level];
          return (
            // biome-ignore lint/a11y/noLabelWithoutControl: the Radio inside is the control
            <label
              className={cn(
                "flex cursor-pointer flex-col gap-1 rounded-xl border bg-background px-3 py-2.5 transition-colors hover:bg-accent has-data-checked:border-foreground/40 has-data-checked:bg-background-muted motion-reduce:transition-none"
              )}
              key={level}
            >
              <span className="flex items-center gap-2 font-medium text-foreground text-sm">
                <Radio value={level} />
                {copy.label}
              </span>
              <span className="text-foreground-muted text-xs">{copy.note}</span>
              <span className="text-foreground text-xs italic">
                “{copy.example}”
              </span>
            </label>
          );
        })}
      </RadioGroup>
    </div>
  );
}

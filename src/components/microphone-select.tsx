import type { MicrophoneInfo } from "@/lib/ipc";

import { useCallback } from "react";

import { cn } from "cn";

import {
  Select,
  SelectItem,
  SelectLabel,
  SelectPopup,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

/** Stands for `null` (follow the system default), which a select can't hold. */
const SYSTEM_DEFAULT = "system-default";

/** Picks the input device; "System default" follows the OS setting. */
export function MicrophoneSelect({
  microphones,
  value,
  onValueChange,
  label = "Change microphone",
  disabled = false,
  className,
}: {
  microphones: readonly MicrophoneInfo[];
  /** Device id, or `null` for the system default. */
  value: string | null;
  onValueChange: (id: string | null) => void;
  label?: string;
  disabled?: boolean;
  className?: string;
}) {
  const items = [
    { label: "System default", value: SYSTEM_DEFAULT },
    ...microphones.map((mic) => ({ label: mic.name, value: mic.id })),
  ];
  const change = useCallback(
    (next: unknown) => {
      if (typeof next === "string") {
        onValueChange(next === SYSTEM_DEFAULT ? null : next);
      }
    },
    [onValueChange]
  );
  const known = value !== null && microphones.some((mic) => mic.id === value);

  return (
    <div className={cn("flex flex-col", className)}>
      <Select
        disabled={disabled}
        items={items}
        onValueChange={change}
        value={known ? value : SYSTEM_DEFAULT}
      >
        <SelectLabel>{label}</SelectLabel>
        <SelectTrigger>
          <SelectValue />
        </SelectTrigger>
        <SelectPopup>
          {items.map((item) => (
            <SelectItem key={item.value} value={item.value}>
              {item.label}
            </SelectItem>
          ))}
        </SelectPopup>
      </Select>
    </div>
  );
}

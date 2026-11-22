import { useId } from "react";

import { Switch } from "@/components/ui/switch";

import { descriptionId, SettingRow } from "../page";

/** A setting that is on or off. */
export function SwitchRow({
  label,
  description,
  checked,
  onCheckedChange,
  disabled,
}: {
  checked: boolean;
  description?: React.ReactNode;
  disabled?: boolean;
  label: React.ReactNode;
  onCheckedChange: (checked: boolean) => void;
}) {
  const id = useId();
  return (
    <SettingRow
      control={
        <Switch
          aria-describedby={description ? descriptionId(id) : undefined}
          checked={checked}
          disabled={disabled}
          id={id}
          onCheckedChange={onCheckedChange}
        />
      }
      description={description}
      htmlFor={id}
      label={label}
    />
  );
}

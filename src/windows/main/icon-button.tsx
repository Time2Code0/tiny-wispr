import type { IconSvgElement } from "@hugeicons/react";

import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { Tooltip, TooltipPopup, TooltipTrigger } from "@/components/ui/tooltip";

/** An icon-only button, named for screen readers and explained on hover. */
export function IconButton({
  label,
  icon,
  onClick,
  className,
  disabled,
}: {
  className?: string;
  disabled?: boolean;
  icon: IconSvgElement;
  label: string;
  onClick: (event: React.MouseEvent<HTMLButtonElement>) => void;
}) {
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            aria-label={label}
            className={className}
            disabled={disabled}
            onClick={onClick}
            size="icon-sm"
            type="button"
            variant="ghost"
          />
        }
      >
        <Icon aria-hidden="true" icon={icon} />
      </TooltipTrigger>
      <TooltipPopup>{label}</TooltipPopup>
    </Tooltip>
  );
}

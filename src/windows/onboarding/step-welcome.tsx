import {
  CpuIcon,
  UserAccountIcon,
  WifiOff01Icon,
} from "@hugeicons/core-free-icons";

import { Icon } from "@/components/ui/icon";

const POINTS = [
  {
    icon: UserAccountIcon,
    text: "Open it and start talking.",
    title: "No account",
  },
  {
    icon: WifiOff01Icon,
    text: "Once a model is downloaded, no internet is needed.",
    title: "Works offline",
  },
  {
    icon: CpuIcon,
    text: "Choose the models. Delete them any time.",
    title: "Your models",
  },
] as const;

export function WelcomeStep() {
  return (
    <ul className="mt-auto grid grid-cols-3 gap-3 pb-2">
      {POINTS.map((point) => (
        <li
          className="flex flex-col gap-2 rounded-xl border bg-background-muted p-4"
          key={point.title}
        >
          <Icon
            aria-hidden="true"
            className="size-5 text-foreground-muted"
            icon={point.icon}
          />
          <p className="font-medium text-foreground text-sm">{point.title}</p>
          <p className="text-foreground-muted text-xs">{point.text}</p>
        </li>
      ))}
    </ul>
  );
}

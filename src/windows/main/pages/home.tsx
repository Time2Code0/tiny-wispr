import type { Stats } from "@/lib/ipc";

import { useMemo } from "react";

import {
  DashboardSpeed01Icon,
  FireIcon,
  MagicWand01Icon,
  TextFontIcon,
} from "@hugeicons/core-free-icons";
import { cn } from "cn";

import { ShortcutKeys } from "@/components/shortcut-keys";
import { Icon } from "@/components/ui/icon";
import { Skeleton } from "@/components/ui/skeleton";
import { Tooltip, TooltipPopup, TooltipTrigger } from "@/components/ui/tooltip";
import { useIpcQuery } from "@/hooks/use-ipc-query";
import { useSettings } from "@/hooks/use-settings";
import { getStats } from "@/lib/ipc";

import { FileTranscription } from "../files/file-transcription";
import { greeting } from "../home/format";
import { ListenerBanner } from "../home/listener-banner";
import { HomeTabs } from "../home/tabs";
import { Page } from "../page";

const STATS_EVENTS = ["history:changed"] as const;

const days = (n: number) => `${n.toLocaleString()} ${n === 1 ? "day" : "days"}`;

interface Chip {
  hint?: string;
  icon: typeof FireIcon;
  label: string;
  value: string;
}

function ChipContent({ chip }: { chip: Chip }) {
  return (
    <>
      <Icon
        aria-hidden="true"
        className="size-4 text-foreground-muted"
        icon={chip.icon}
      />
      <span className="font-medium text-foreground tabular-nums">
        {chip.value}
      </span>
      <span className="text-foreground-muted">{chip.label}</span>
      {chip.hint ? <span className="sr-only">. {chip.hint}</span> : null}
    </>
  );
}

function StatChips({ stats }: { stats: Stats | undefined }) {
  if (!stats) {
    return (
      <div aria-hidden="true" className="flex gap-2">
        <Skeleton className="h-8 w-28 rounded-full" />
        <Skeleton className="h-8 w-28 rounded-full" />
        <Skeleton className="h-8 w-28 rounded-full" />
      </div>
    );
  }
  const chips: Chip[] = [
    {
      hint: `Longest streak: ${days(stats.longestStreakDays)}`,
      icon: FireIcon,
      label: "day streak",
      value: stats.streakDays.toLocaleString(),
    },
    {
      icon: TextFontIcon,
      label: stats.totalWords === 1 ? "word" : "words",
      value: stats.totalWords.toLocaleString(),
    },
    {
      icon: DashboardSpeed01Icon,
      label: "words per minute",
      value: stats.wordsPerMinute.toLocaleString(),
    },
  ];
  if (stats.fixesMade > 0) {
    chips.push({
      icon: MagicWand01Icon,
      label: stats.fixesMade === 1 ? "fix made" : "fixes made",
      value: stats.fixesMade.toLocaleString(),
    });
  }
  const chipClass =
    "flex items-center gap-2 rounded-full border bg-background px-3 py-1.5 text-sm";
  return (
    <ul aria-label="Your stats" className="flex flex-wrap gap-2">
      {chips.map((chip) =>
        chip.hint ? (
          <li className="flex" key={chip.label}>
            <Tooltip>
              <TooltipTrigger
                className={cn(
                  chipClass,
                  "outline-none focus-visible:ring-2 focus-visible:ring-foreground"
                )}
                render={<span />}
                tabIndex={0}
              >
                <ChipContent chip={chip} />
              </TooltipTrigger>
              <TooltipPopup>{chip.hint}</TooltipPopup>
            </Tooltip>
          </li>
        ) : (
          <li className={chipClass} key={chip.label}>
            <ChipContent chip={chip} />
          </li>
        )
      )}
    </ul>
  );
}

export function HomePage() {
  const { settings } = useSettings();
  const stats = useIpcQuery(getStats, STATS_EVENTS);
  const pushToTalk = settings?.shortcuts.pushToTalk;
  const today = useMemo(() => new Date(), []);

  return (
    <Page title={greeting(today.getHours())}>
      <div className="-mt-6 flex flex-col gap-5">
        <ListenerBanner />
        <StatChips stats={stats.data} />
        {pushToTalk ? (
          <div className="flex flex-wrap items-center gap-1.5 text-foreground-muted text-sm">
            Hold <ShortcutKeys hotkey={pushToTalk} /> in any app and speak.
            Release to paste. Tap it twice to keep listening hands-free.
          </div>
        ) : null}
      </div>
      <FileTranscription />
      <HomeTabs
        pushToTalk={pushToTalk}
        retention={settings?.historyRetention}
        stats={stats.data}
        today={today}
      />
    </Page>
  );
}

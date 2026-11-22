import type { HistoryRetention, Hotkey, Stats } from "@/lib/ipc";

import { useCallback, useId } from "react";

import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyTitle,
} from "@/components/ui/empty";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsList, TabsPanel, TabsTab } from "@/components/ui/tabs";

import { HOME_TABS, useHomeTab } from "../route";
import { ActivityHeatmap } from "./heatmap";
import { HistorySection } from "./history";

const count = (n: number, one: string, many: string) =>
  `${n.toLocaleString()} ${n === 1 ? one : many}`;

function Figures({ stats }: { stats: Stats }) {
  const figures = [
    {
      label: "Longest streak",
      value: count(stats.longestStreakDays, "day", "days"),
    },
    { label: "Dictations", value: stats.totalDictations.toLocaleString() },
    { label: "Fixes made", value: stats.fixesMade.toLocaleString() },
  ];
  return (
    <dl className="grid grid-cols-3 gap-2">
      {figures.map((figure) => (
        <div
          className="flex flex-col gap-0.5 rounded-xl border px-4 py-3"
          key={figure.label}
        >
          <dt className="text-foreground-muted text-xs">{figure.label}</dt>
          <dd className="font-medium text-foreground text-lg tabular-nums">
            {figure.value}
          </dd>
        </div>
      ))}
    </dl>
  );
}

function TopApps({ apps }: { apps: Stats["topApps"] }) {
  const titleId = useId();
  return (
    <section aria-labelledby={titleId} className="flex flex-col gap-2">
      <h3 className="font-medium text-foreground-muted text-xs" id={titleId}>
        Top apps
      </h3>
      <ol className="flex flex-col divide-y">
        {apps.map((app, rank) => (
          <li className="flex items-baseline gap-3 py-2 text-sm" key={app.name}>
            <span className="w-4 text-foreground-muted tabular-nums">
              {rank + 1}
            </span>
            <span className="min-w-0 flex-1 truncate text-foreground">
              {app.name}
            </span>
            <span className="text-foreground-muted tabular-nums">
              {count(app.words, "word", "words")}
            </span>
          </li>
        ))}
      </ol>
    </section>
  );
}

function Activity({ stats, today }: { stats: Stats | undefined; today: Date }) {
  if (!stats) {
    return <Skeleton aria-hidden="true" className="h-44 w-full rounded-xl" />;
  }
  if (stats.totalDictations === 0) {
    return (
      <Empty className="rounded-xl border md:py-12">
        <EmptyHeader className="gap-2">
          <EmptyTitle>No activity yet</EmptyTitle>
          <EmptyDescription>
            Your streaks, busiest days and top apps show up here after your
            first dictation.
          </EmptyDescription>
        </EmptyHeader>
      </Empty>
    );
  }
  return (
    <div className="flex flex-col gap-6">
      <Figures stats={stats} />
      <ActivityHeatmap daily={stats.daily} today={today} />
      {stats.topApps.length > 0 ? <TopApps apps={stats.topApps} /> : null}
    </div>
  );
}

/** Home below the stat chips: the history, and the activity on its own tab. */
export function HomeTabs({
  stats,
  today,
  pushToTalk,
  retention,
}: {
  pushToTalk: Hotkey | undefined;
  retention: HistoryRetention | undefined;
  stats: Stats | undefined;
  today: Date;
}) {
  const [tab, select] = useHomeTab();
  const change = useCallback(
    (value: unknown) => {
      const next = HOME_TABS.find((t) => t === value);
      if (next) {
        select(next);
      }
    },
    [select]
  );

  return (
    <Tabs className="gap-6" onValueChange={change} value={tab}>
      <TabsList>
        <TabsTab value="history">History</TabsTab>
        <TabsTab value="activity">Activity</TabsTab>
      </TabsList>
      <TabsPanel keepMounted value="history">
        <HistorySection pushToTalk={pushToTalk} retention={retention} />
      </TabsPanel>
      <TabsPanel value="activity">
        <Activity stats={stats} today={today} />
      </TabsPanel>
    </Tabs>
  );
}

import type { DailyWords, HistoryEntry } from "@/lib/ipc";

const MORNING_ENDS = 12;
const AFTERNOON_ENDS = 18;
const NIGHT_ENDS = 5;

export function greeting(hour: number): string {
  if (hour < NIGHT_ENDS || hour >= AFTERNOON_ENDS) {
    return "Good evening";
  }
  if (hour < MORNING_ENDS) {
    return "Good morning";
  }
  return "Good afternoon";
}

const pad = (n: number) => String(n).padStart(2, "0");

/** The local calendar date of `date`, as `YYYY-MM-DD`. */
export function dayKey(date: Date): string {
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/** Midnight at the start of the local day `days` after `date`. */
export function addDays(date: Date, days: number): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + days);
}

function parseDayKey(key: string): Date {
  const [year = 0, month = 1, day = 1] = key.split("-").map(Number);
  return new Date(year, month - 1, day);
}

/** "Today", "Yesterday", or the weekday and date (with the year when it isn't this one). */
export function dayLabel(key: string, now: Date, locale?: string): string {
  if (key === dayKey(now)) {
    return "Today";
  }
  if (key === dayKey(addDays(now, -1))) {
    return "Yesterday";
  }
  const date = parseDayKey(key);
  return date.toLocaleDateString(locale, {
    day: "numeric",
    month: "long",
    weekday: "long",
    ...(date.getFullYear() === now.getFullYear() ? {} : { year: "numeric" }),
  });
}

export interface DayGroup {
  entries: HistoryEntry[];
  key: string;
  label: string;
}

/** History (newest first) split into days, keeping the order it came in. */
export function groupByDay(
  entries: readonly HistoryEntry[],
  now: Date,
  locale?: string
): DayGroup[] {
  const groups: DayGroup[] = [];
  for (const entry of entries) {
    const key = dayKey(new Date(entry.createdAt));
    const last = groups.at(-1);
    if (last?.key === key) {
      last.entries.push(entry);
    } else {
      groups.push({ entries: [entry], key, label: dayLabel(key, now, locale) });
    }
  }
  return groups;
}

/** Cleanup changed what was said, so the row can offer the raw transcript. */
export function wasEdited(entry: HistoryEntry): boolean {
  return entry.rawText.trim() !== entry.finalText.trim();
}

// ── activity heatmap ─────────────────────────────────────────────────────

export const HEATMAP_WEEKS = 26;
export const HEAT_LEVELS = 5;

export interface HeatCell {
  date: string;
  /** 0 (none) to {@link HEAT_LEVELS} - 1 (busiest). */
  level: number;
  words: number;
}

export interface Heatmap {
  activeDays: number;
  totalWords: number;
  /** Columns of seven days, Sunday first; days after today are `null`. */
  weeks: (HeatCell | null)[][];
}

/** Splits 1..max into four equal bands so one huge day doesn't wash out the rest. */
export function heatLevel(words: number, max: number): number {
  if (words <= 0 || max <= 0) {
    return 0;
  }
  const top = HEAT_LEVELS - 1;
  return Math.min(top, Math.max(1, Math.ceil((words / max) * top)));
}

/** The last `weeks` weeks of words per day, ending with the week of `today`. */
export function buildHeatmap(
  daily: readonly DailyWords[],
  today: Date,
  weeks = HEATMAP_WEEKS
): Heatmap {
  const start = addDays(today, -today.getDay() - (weeks - 1) * 7);
  const todayKey = dayKey(today);
  const byDay = new Map(daily.map((d) => [d.date, d.words]));

  const days: { date: string; words: number }[] = [];
  for (let i = 0; i < weeks * 7; i += 1) {
    const date = dayKey(addDays(start, i));
    if (date > todayKey) {
      break;
    }
    days.push({ date, words: byDay.get(date) ?? 0 });
  }

  const max = Math.max(0, ...days.map((d) => d.words));
  const columns: (HeatCell | null)[][] = [];
  for (let w = 0; w < weeks; w += 1) {
    const column: (HeatCell | null)[] = [];
    for (let d = 0; d < 7; d += 1) {
      const day = days[w * 7 + d];
      column.push(day ? { ...day, level: heatLevel(day.words, max) } : null);
    }
    columns.push(column);
  }

  const active = days.filter((d) => d.words > 0);
  return {
    activeDays: active.length,
    totalWords: active.reduce((sum, d) => sum + d.words, 0),
    weeks: columns,
  };
}

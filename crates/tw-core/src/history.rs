//! Dictation history and the stats derived from it.
//!
//! Stats come from a [`StatsLedger`] kept apart from the entries, so deleting
//! history, or never keeping it, leaves the numbers on the Home page intact.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::Millis;
use crate::text::AppInfo;

/// Entries per `list_history` page.
pub const PAGE_SIZE: usize = 50;
/// Days in the words-per-day heatmap: 26 weeks.
pub const HEATMAP_DAYS: usize = 26 * 7;
/// Apps listed in [`Stats::top_apps`].
pub const TOP_APPS: usize = 5;
const FILE_VERSION: u32 = 1;
const MINUTE_MS: Millis = 60 * 1_000;
const QUARTER_HOUR_MS: Millis = 15 * MINUTE_MS;
const DAY_MS: Millis = 24 * 60 * MINUTE_MS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub created_at: Millis,
    /// Whisper's output before any cleanup.
    pub raw_text: String,
    /// What was inserted.
    pub final_text: String,
    pub app: Option<AppInfo>,
    pub duration_ms: u64,
    pub word_count: u32,
    pub language: Option<String>,
}

impl HistoryEntry {
    /// Cleanup changed the text, so "Undo AI edit" is offered.
    pub fn was_edited(&self) -> bool {
        self.raw_text != self.final_text
    }

    fn matches(&self, query: &str) -> bool {
        self.final_text.to_lowercase().contains(query)
            || self.raw_text.to_lowercase().contains(query)
            || self
                .app
                .as_ref()
                .is_some_and(|app| app.name.to_lowercase().contains(query))
    }

    /// Newest first; the id breaks ties so the order is total.
    fn order_key(&self) -> (std::cmp::Reverse<Millis>, std::cmp::Reverse<&str>) {
        (
            std::cmp::Reverse(self.created_at),
            std::cmp::Reverse(self.id.as_str()),
        )
    }

    fn cursor(&self) -> String {
        format!("{}:{}", self.created_at, self.id)
    }
}

/// One page of `list_history`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub entries: Vec<HistoryEntry>,
    /// Pass back as `cursor` for the next page; `None` at the end.
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HistoryRetention {
    #[default]
    Forever,
    ThirtyDays,
    OneDay,
    Never,
}

impl HistoryRetention {
    pub fn max_age_ms(self) -> Option<Millis> {
        match self {
            Self::Forever => None,
            Self::ThirtyDays => Some(30 * DAY_MS),
            Self::OneDay => Some(DAY_MS),
            Self::Never => Some(0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyWords {
    /// Local date, `YYYY-MM-DD`.
    pub date: String,
    pub words: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUsage {
    pub name: String,
    pub dictations: u64,
    pub words: u64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    /// Consecutive local days with a dictation, ending at the last active
    /// day. A streak that ended yesterday still counts until today is over.
    pub streak_days: u32,
    pub longest_streak_days: u32,
    pub total_words: u64,
    /// Words over the time spent speaking.
    pub words_per_minute: f64,
    pub total_dictations: u64,
    /// Fillers removed and dictionary corrections.
    pub fixes_made: u64,
    /// Words per local day for the last [`HEATMAP_DAYS`] days, oldest first,
    /// ending today. Days without dictation are included with 0.
    pub daily: Vec<DailyWords>,
    /// Most used apps by dictation count, at most [`TOP_APPS`].
    pub top_apps: Vec<AppUsage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Activity {
    dictations: u64,
    words: u64,
    duration_ms: u64,
    fixes: u64,
}

impl Activity {
    fn add(&mut self, other: Self) {
        self.dictations += other.dictations;
        self.words += other.words;
        self.duration_ms += other.duration_ms;
        self.fixes += other.fixes;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct AppTotals {
    dictations: u64,
    words: u64,
}

/// The numbers behind [`Stats`], without any dictated text.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StatsLedger {
    /// Activity per UTC quarter hour since the epoch. Every UTC offset in use
    /// is a whole number of quarter hours, so local days come out exact for
    /// whatever offset the stats are read with.
    slots: BTreeMap<i64, Activity>,
    apps: BTreeMap<String, AppTotals>,
}

impl StatsLedger {
    pub fn from_entries(entries: &[HistoryEntry]) -> Self {
        let mut ledger = Self::default();
        for entry in entries {
            ledger.record(entry, 0);
        }
        ledger
    }

    /// Counts one dictation; `fixes` is what the rule-based pipeline fixed.
    pub fn record(&mut self, entry: &HistoryEntry, fixes: u32) {
        let words = u64::from(entry.word_count);
        self.slots
            .entry(entry.created_at.div_euclid(QUARTER_HOUR_MS))
            .or_default()
            .add(Activity {
                dictations: 1,
                words,
                duration_ms: entry.duration_ms,
                fixes: u64::from(fixes),
            });
        if let Some(app) = entry.app.as_ref().filter(|app| !app.name.trim().is_empty()) {
            let totals = self.apps.entry(app.name.clone()).or_default();
            totals.dictations += 1;
            totals.words += words;
        }
    }

    /// Stats as of `now`, with days in local time `utc_offset_minutes` east
    /// of UTC.
    pub fn stats(&self, now: Millis, utc_offset_minutes: i32) -> Stats {
        let offset_ms = Millis::from(utc_offset_minutes) * MINUTE_MS;
        let local_day = |ms: Millis| (ms + offset_ms).div_euclid(DAY_MS);
        let mut total = Activity::default();
        let mut days: BTreeMap<i64, u64> = BTreeMap::new();
        for (&slot, &activity) in &self.slots {
            total.add(activity);
            if activity.dictations > 0 {
                *days.entry(local_day(slot * QUARTER_HOUR_MS)).or_default() += activity.words;
            }
        }
        let today = local_day(now);
        let active: BTreeSet<i64> = days.keys().copied().collect();
        let (streak_days, longest_streak_days) = streaks(&active, today);
        let daily = (0..HEATMAP_DAYS as i64)
            .rev()
            .map(|ago| {
                let day = today - ago;
                DailyWords {
                    date: civil_date(day),
                    words: u32::try_from(days.get(&day).copied().unwrap_or(0)).unwrap_or(u32::MAX),
                }
            })
            .collect();
        let mut top_apps: Vec<AppUsage> = self
            .apps
            .iter()
            .map(|(name, totals)| AppUsage {
                name: name.clone(),
                dictations: totals.dictations,
                words: totals.words,
            })
            .collect();
        top_apps.sort_by(|a, b| {
            b.dictations
                .cmp(&a.dictations)
                .then(b.words.cmp(&a.words))
                .then(a.name.cmp(&b.name))
        });
        top_apps.truncate(TOP_APPS);
        let words_per_minute = if total.duration_ms == 0 {
            0.0
        } else {
            total.words as f64 * MINUTE_MS as f64 / total.duration_ms as f64
        };
        Stats {
            streak_days,
            longest_streak_days,
            total_words: total.words,
            words_per_minute,
            total_dictations: total.dictations,
            fixes_made: total.fixes,
            daily,
            top_apps,
        }
    }
}

/// The current streak (ending today, or yesterday while today is still
/// open) and the longest one.
fn streaks(active: &BTreeSet<i64>, today: i64) -> (u32, u32) {
    let mut longest = 0;
    let mut run = 0;
    let mut prev: Option<i64> = None;
    for &day in active {
        run = if prev == Some(day - 1) { run + 1 } else { 1 };
        longest = longest.max(run);
        prev = Some(day);
    }
    let end = if active.contains(&today) {
        today
    } else {
        today - 1
    };
    let current = (0..)
        .take_while(|&back| active.contains(&(end - back)))
        .count();
    (u32::try_from(current).unwrap_or(u32::MAX), longest)
}

/// Days since 1970-01-01 as `YYYY-MM-DD` (proleptic Gregorian).
fn civil_date(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Why a history file was not read completely. What could be read is still
/// returned.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HistoryLoadError {
    #[error("the history file is unreadable, starting with an empty history: {0}")]
    Corrupt(String),
    #[error("skipped {0} unreadable history entries")]
    SkippedEntries(usize),
    #[error("the saved stats are unreadable and were rebuilt from the history")]
    StatsRebuilt,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileOut<'a> {
    version: u32,
    entries: &'a [HistoryEntry],
    stats: &'a StatsLedger,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileIn {
    #[serde(default)]
    entries: Vec<serde_json::Value>,
    stats: Option<serde_json::Value>,
}

/// The history, newest first, and the stats ledger.
#[derive(Debug, Clone, Default)]
pub struct HistoryStore {
    entries: Vec<HistoryEntry>,
    ledger: StatsLedger,
}

impl HistoryStore {
    /// A store whose stats are rebuilt from `entries`. Use
    /// [`Self::with_ledger`] to keep stats for deleted or unsaved entries.
    pub fn new(entries: Vec<HistoryEntry>) -> Self {
        let ledger = StatsLedger::from_entries(&entries);
        Self::with_ledger(entries, ledger)
    }

    pub fn with_ledger(mut entries: Vec<HistoryEntry>, ledger: StatsLedger) -> Self {
        entries.sort_by(|a, b| a.order_key().cmp(&b.order_key()));
        entries.dedup_by(|a, b| a.id == b.id);
        Self { entries, ledger }
    }

    /// Newest first.
    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    pub fn ledger(&self) -> &StatsLedger {
        &self.ledger
    }

    /// Up to [`PAGE_SIZE`] entries, newest first, whose final text, raw text
    /// or app name contains `query` (any case). `cursor` is the previous
    /// page's `next_cursor`; an unknown cursor gives an empty page.
    pub fn list(&self, query: Option<&str>, cursor: Option<&str>) -> HistoryPage {
        let query = query.map(|q| q.trim().to_lowercase()).unwrap_or_default();
        let start = match cursor {
            None => 0,
            Some(cursor) => {
                let Some((created_at, id)) = cursor
                    .split_once(':')
                    .and_then(|(at, id)| Some((at.parse::<Millis>().ok()?, id)))
                else {
                    return HistoryPage::default();
                };
                let key = (std::cmp::Reverse(created_at), std::cmp::Reverse(id));
                self.entries.partition_point(|e| e.order_key() <= key)
            }
        };
        let mut matching = self.entries[start..]
            .iter()
            .filter(|e| query.is_empty() || e.matches(&query));
        let entries: Vec<HistoryEntry> = matching.by_ref().take(PAGE_SIZE).cloned().collect();
        let next_cursor = match (entries.last(), matching.next()) {
            (Some(last), Some(_)) => Some(last.cursor()),
            _ => None,
        };
        HistoryPage {
            entries,
            next_cursor,
        }
    }

    pub fn push(&mut self, entry: HistoryEntry) {
        self.push_with_fixes(entry, 0);
    }

    /// Adds a dictation and counts it in the stats. An entry with an id
    /// already present replaces it.
    pub fn push_with_fixes(&mut self, entry: HistoryEntry, fixes: u32) {
        self.ledger.record(&entry, fixes);
        self.entries.retain(|e| e.id != entry.id);
        let at = self
            .entries
            .partition_point(|e| e.order_key() < entry.order_key());
        self.entries.insert(at, entry);
    }

    /// Deletes an entry; its words still count in the stats.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        self.entries.len() != before
    }

    /// Deletes every entry; the stats stay.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Drops entries older than `retention` allows. Stats are kept.
    pub fn apply_retention(&mut self, retention: HistoryRetention, now: Millis) {
        match retention.max_age_ms() {
            None => {}
            Some(0) => self.entries.clear(),
            Some(max_age) => self.entries.retain(|e| e.created_at > now - max_age),
        }
    }

    /// Stats with days in UTC. Prefer [`Self::stats_with_offset`].
    pub fn stats(&self, now: Millis) -> Stats {
        self.stats_with_offset(now, 0)
    }

    pub fn stats_with_offset(&self, now: Millis, utc_offset_minutes: i32) -> Stats {
        self.ledger.stats(now, utc_offset_minutes)
    }

    /// The history file: `{"version", "entries", "stats"}`.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(&FileOut {
            version: FILE_VERSION,
            entries: &self.entries,
            stats: &self.ledger,
        })
    }

    /// Reads a history file, also the older plain list of entries. Unknown
    /// fields are ignored. Whatever can't be read is left out and reported;
    /// a file that isn't a history at all gives an empty store.
    pub fn from_json(json: &str) -> (Self, Option<HistoryLoadError>) {
        let value: serde_json::Value = match serde_json::from_str(json) {
            Ok(value) => value,
            Err(err) => {
                return (
                    Self::default(),
                    Some(HistoryLoadError::Corrupt(err.to_string())),
                );
            }
        };
        let file = if value.is_array() {
            serde_json::from_value::<Vec<serde_json::Value>>(value).map(|entries| FileIn {
                entries,
                stats: None,
            })
        } else {
            serde_json::from_value::<FileIn>(value)
        };
        let file = match file {
            Ok(file) => file,
            Err(err) => {
                return (
                    Self::default(),
                    Some(HistoryLoadError::Corrupt(err.to_string())),
                );
            }
        };
        let total = file.entries.len();
        let entries: Vec<HistoryEntry> = file
            .entries
            .into_iter()
            .filter_map(|e| serde_json::from_value(e).ok())
            .collect();
        let skipped = total - entries.len();
        let mut problem = (skipped > 0).then_some(HistoryLoadError::SkippedEntries(skipped));
        let ledger = match file.stats.map(serde_json::from_value::<StatsLedger>) {
            Some(Ok(ledger)) => ledger,
            None => StatsLedger::from_entries(&entries),
            Some(Err(_)) => {
                problem.get_or_insert(HistoryLoadError::StatsRebuilt);
                StatsLedger::from_entries(&entries)
            }
        };
        (Self::with_ledger(entries, ledger), problem)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::AppCategory;

    /// 2026-11-15 12:00 UTC.
    const NOW: Millis = 1_794_744_000_000;
    const HOUR: Millis = 60 * MINUTE_MS;

    fn entry(id: &str, created_at: Millis, text: &str) -> HistoryEntry {
        HistoryEntry {
            id: id.into(),
            created_at,
            raw_text: format!("um {text}"),
            final_text: text.into(),
            app: None,
            duration_ms: 1_000,
            word_count: crate::text::count_words(text),
            language: Some("en".into()),
        }
    }

    fn timed(id: &str, created_at: Millis, words: u32, duration_ms: u64) -> HistoryEntry {
        HistoryEntry {
            word_count: words,
            duration_ms,
            ..entry(id, created_at, "x")
        }
    }

    fn in_app(mut e: HistoryEntry, name: &str) -> HistoryEntry {
        e.app = Some(AppInfo {
            name: name.into(),
            id: None,
            category: AppCategory::Other,
        });
        e
    }

    fn ids(page: &HistoryPage) -> Vec<&str> {
        page.entries.iter().map(|e| e.id.as_str()).collect()
    }

    #[test]
    fn newest_first_with_stable_ids() {
        let mut store = HistoryStore::new(vec![entry("a", 1, "one"), entry("c", 3, "three")]);
        store.push(entry("b", 2, "two"));
        store.push(entry("d", 3, "tie"));
        store.push(entry("b", 2, "two again"));
        let order: Vec<&str> = store.entries().iter().map(|e| e.id.as_str()).collect();
        assert_eq!(order, ["d", "c", "b", "a"]);
        assert_eq!(store.entries()[2].final_text, "two again");
        assert!(store.remove("a"));
        assert!(!store.remove("a"));
        assert_eq!(store.entries().len(), 3);
        store.clear();
        assert!(store.entries().is_empty());
    }

    #[test]
    fn search_is_case_insensitive_over_text_and_app() {
        let store = HistoryStore::new(vec![
            in_app(entry("1", 1, "Grüße an Anna"), "Slack"),
            in_app(entry("2", 2, "Lunch at noon?"), "Messages"),
            entry("3", 3, "ÄPFEL kaufen"),
        ]);
        let cases: &[(Option<&str>, &[&str])] = &[
            (None, &["3", "2", "1"]),
            (Some(""), &["3", "2", "1"]),
            (Some("  "), &["3", "2", "1"]),
            (Some("anna"), &["1"]),
            (Some("GRÜSSE"), &[]),
            (Some("grüße"), &["1"]),
            (Some("äpfel"), &["3"]),
            (Some("slack"), &["1"]),
            (Some(" um "), &["3", "2", "1"]),
            (Some("noon?"), &["2"]),
            (Some("dinner"), &[]),
        ];
        for &(query, want) in cases {
            let page = store.list(query, None);
            assert_eq!(ids(&page), want, "{query:?}");
            assert_eq!(page.next_cursor, None);
        }
    }

    #[test]
    fn pages_follow_the_cursor() {
        let entries = (0..120)
            .map(|i| entry(&format!("e{i:03}"), Millis::from(i / 2), "hello"))
            .collect();
        let mut store = HistoryStore::new(entries);
        let first = store.list(None, None);
        assert_eq!(first.entries.len(), PAGE_SIZE);
        assert_eq!(first.entries[0].id, "e119");
        let cursor = first.next_cursor.clone().unwrap_or_default();
        // Deleting what was already shown doesn't shift the next page.
        assert!(store.remove("e119"));
        assert!(store.remove(&first.entries[PAGE_SIZE - 1].id));
        let second = store.list(None, Some(&cursor));
        assert_eq!(second.entries.len(), PAGE_SIZE);
        assert_eq!(second.entries[0].id, "e069");
        let third = store.list(None, second.next_cursor.as_deref());
        assert_eq!(third.entries.len(), 20);
        assert_eq!(third.entries[19].id, "e000");
        assert_eq!(third.next_cursor, None);
        let mut seen: Vec<&str> = [&first, &second, &third]
            .iter()
            .flat_map(|p| ids(p))
            .collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), 120);
        assert_eq!(store.list(None, Some("garbage")), HistoryPage::default());
        // A full last page has no next cursor.
        let exactly = HistoryStore::new((0..50).map(|i| entry(&i.to_string(), i, "x")).collect());
        assert_eq!(exactly.list(None, None).next_cursor, None);
        // Search pages too.
        let page = store.list(Some("HELLO"), Some(&cursor));
        assert_eq!(ids(&page), ids(&second));
    }

    #[test]
    fn retention() {
        let entries = || {
            vec![
                entry("now", NOW, "a"),
                entry("hours", NOW - 5 * HOUR, "b"),
                entry("days", NOW - 3 * DAY_MS, "c"),
                entry("months", NOW - 40 * DAY_MS, "d"),
            ]
        };
        let cases: &[(HistoryRetention, &[&str])] = &[
            (
                HistoryRetention::Forever,
                &["now", "hours", "days", "months"],
            ),
            (HistoryRetention::ThirtyDays, &["now", "hours", "days"]),
            (HistoryRetention::OneDay, &["now", "hours"]),
            (HistoryRetention::Never, &[]),
        ];
        for &(retention, want) in cases {
            let mut store = HistoryStore::new(entries());
            store.apply_retention(retention, NOW);
            let kept: Vec<&str> = store.entries().iter().map(|e| e.id.as_str()).collect();
            assert_eq!(kept, want, "{retention:?}");
            assert_eq!(store.stats(NOW).total_dictations, 4, "{retention:?}");
        }
    }

    #[test]
    fn stats_survive_deletion_and_never_storing() {
        let mut store = HistoryStore::default();
        store.push_with_fixes(entry("a", NOW, "one two three"), 2);
        store.apply_retention(HistoryRetention::Never, NOW);
        store.push_with_fixes(entry("b", NOW, "four five"), 1);
        assert!(store.remove("b"));
        store.push(entry("c", NOW, "six"));
        store.clear();
        let stats = store.stats(NOW);
        assert!(store.entries().is_empty());
        assert_eq!(stats.total_words, 6);
        assert_eq!(stats.total_dictations, 3);
        assert_eq!(stats.fixes_made, 3);
        assert_eq!(stats.streak_days, 1);
        // Rebuilding from the (empty) entries would zero everything.
        assert_eq!(HistoryStore::new(Vec::new()).stats(NOW).total_words, 0);
    }

    #[test]
    fn words_per_minute_over_speech_time() {
        let cases: &[(&[(u32, u64)], f64)] = &[
            (&[], 0.0),
            (&[(10, 0)], 0.0),
            (&[(120, 60_000)], 120.0),
            (&[(30, 15_000), (90, 45_000)], 120.0),
            (&[(50, 20_000)], 150.0),
            (&[(1, 90_000)], 1.0 / 1.5),
        ];
        for (dictations, want) in cases {
            let entries = dictations
                .iter()
                .enumerate()
                .map(|(i, &(words, ms))| timed(&i.to_string(), NOW, words, ms))
                .collect();
            let stats = HistoryStore::new(entries).stats(NOW);
            assert!(
                (stats.words_per_minute - want).abs() < 1e-9,
                "{dictations:?}"
            );
        }
    }

    #[test]
    fn streaks_in_local_days() {
        let day = |n: i64| NOW - n * DAY_MS;
        let cases: &[(&str, &[Millis], i32, u32, u32)] = &[
            ("nothing", &[], 0, 0, 0),
            ("today", &[NOW], 0, 1, 1),
            ("three days", &[day(0), day(1), day(2)], 0, 3, 3),
            ("today not yet", &[day(1), day(2)], 0, 2, 2),
            ("lapsed", &[day(2), day(3), day(4)], 0, 0, 3),
            ("gap day", &[day(0), day(2), day(3), day(4)], 0, 1, 3),
            (
                "many per day",
                &[NOW, NOW - HOUR, day(1), day(1) + HOUR],
                0,
                2,
                2,
            ),
            // 23:30 and 00:30 UTC straddle midnight in UTC ...
            (
                "utc midnight",
                &[
                    NOW - 12 * HOUR - 30 * MINUTE_MS,
                    NOW - 11 * HOUR - 30 * MINUTE_MS,
                ],
                0,
                2,
                2,
            ),
            // ... but both fall on 15 Nov at UTC+2 ...
            (
                "east of utc",
                &[
                    NOW - 12 * HOUR - 30 * MINUTE_MS,
                    NOW - 11 * HOUR - 30 * MINUTE_MS,
                ],
                120,
                1,
                1,
            ),
            // ... and both on 14 Nov at UTC−1, which still counts today.
            (
                "west of utc",
                &[
                    NOW - 12 * HOUR - 30 * MINUTE_MS,
                    NOW - 11 * HOUR - 30 * MINUTE_MS,
                ],
                -60,
                1,
                1,
            ),
            // It's 01:00 on 15 Nov at UTC−11; both were on 14 Nov there.
            (
                "late west",
                &[NOW - 13 * HOUR + 30 * MINUTE_MS, NOW - 2 * HOUR],
                -660,
                1,
                1,
            ),
            ("india", &[NOW - 6 * HOUR], 330, 1, 1),
        ];
        for &(name, times, offset, current, longest) in cases {
            let entries = times
                .iter()
                .enumerate()
                .map(|(i, &at)| entry(&i.to_string(), at, "hi"))
                .collect();
            let stats = HistoryStore::new(entries).stats_with_offset(NOW, offset);
            assert_eq!(
                (stats.streak_days, stats.longest_streak_days),
                (current, longest),
                "{name}"
            );
        }
    }

    #[test]
    fn words_per_day_for_26_weeks() {
        let store = HistoryStore::new(vec![
            timed("a", NOW, 10, 1),
            timed("b", NOW - HOUR, 5, 1),
            timed("c", NOW - DAY_MS, 7, 1),
            timed("d", NOW - 181 * DAY_MS, 3, 1),
            timed("e", NOW - 182 * DAY_MS, 100, 1),
            // 23:00 local on 14 Nov at UTC−2, 01:00 UTC on 15 Nov.
            timed("f", NOW - 11 * HOUR, 1, 1),
        ]);
        let stats = store.stats_with_offset(NOW, -120);
        assert_eq!(stats.daily.len(), HEATMAP_DAYS);
        let last = &stats.daily[HEATMAP_DAYS - 1];
        assert_eq!((last.date.as_str(), last.words), ("2026-11-15", 15));
        let yesterday = &stats.daily[HEATMAP_DAYS - 2];
        assert_eq!(
            (yesterday.date.as_str(), yesterday.words),
            ("2026-11-14", 8)
        );
        assert_eq!(
            (stats.daily[0].date.as_str(), stats.daily[0].words),
            ("2026-05-18", 3)
        );
        assert_eq!(stats.daily.iter().map(|d| d.words).sum::<u32>(), 26);
        assert_eq!(stats.total_words, 126);
    }

    #[test]
    fn civil_dates() {
        let cases = [
            (0, "1970-01-01"),
            (-1, "1969-12-31"),
            (11_016, "2000-02-29"),
            (20_772, "2026-11-15"),
        ];
        for (days, want) in cases {
            assert_eq!(civil_date(days), want);
        }
    }

    #[test]
    fn top_apps_by_dictations() {
        let mut entries: Vec<HistoryEntry> = Vec::new();
        for (app, count) in [
            ("Slack", 3),
            ("Mail", 1),
            ("Notes", 3),
            ("A", 1),
            ("B", 1),
            ("C", 1),
            (" ", 9),
        ] {
            for i in 0..count {
                entries.push(in_app(timed(&format!("{app}{i}"), NOW, 2, 1), app));
            }
        }
        entries.push(in_app(timed("extra", NOW, 10, 1), "Notes"));
        entries.push(timed("none", NOW, 10, 1));
        let top = HistoryStore::new(entries).stats(NOW).top_apps;
        let names: Vec<&str> = top.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Notes", "Slack", "A", "B", "C"]);
        assert_eq!((top[0].dictations, top[0].words), (4, 16));
    }

    #[test]
    fn json_round_trip() {
        let mut store = HistoryStore::default();
        store.push_with_fixes(in_app(entry("a", NOW, "Grüße, Anna"), "Slack"), 2);
        store.push(entry("b", NOW - DAY_MS, "hello"));
        store.remove("b");
        let json = store.to_json().unwrap_or_default();
        assert!(json.starts_with("{\"version\":1,"), "{json}");
        let (loaded, problem) = HistoryStore::from_json(&json);
        assert_eq!(problem, None);
        assert_eq!(loaded.entries(), store.entries());
        assert_eq!(loaded.ledger(), store.ledger());
        assert_eq!(loaded.stats(NOW), store.stats(NOW));
        assert_eq!(loaded.stats(NOW).total_dictations, 2);
    }

    #[test]
    fn json_tolerates_old_and_unknown_shapes() {
        let good = serde_json::to_value(entry("a", NOW, "hi")).unwrap_or_default();
        let mut extra = good.clone();
        if let Some(map) = extra.as_object_mut() {
            map.insert("mood".into(), "happy".into());
            map.insert("id".into(), "b".into());
        }
        let file = serde_json::json!({
            "version": 7,
            "future": true,
            "entries": [good, extra, {"id": "broken"}, 42],
        });
        let (store, problem) = HistoryStore::from_json(&file.to_string());
        assert_eq!(store.entries().len(), 2);
        assert_eq!(problem, Some(HistoryLoadError::SkippedEntries(2)));
        assert_eq!(store.stats(NOW).total_dictations, 2);

        let legacy = serde_json::json!([entry("a", NOW, "one two")]).to_string();
        let (store, problem) = HistoryStore::from_json(&legacy);
        assert_eq!(problem, None);
        assert_eq!(store.stats(NOW).total_words, 2);

        let bad_stats = serde_json::json!({"version": 1, "entries": [entry("a", NOW, "one")], "stats": {"slots": "nope"}});
        let (store, problem) = HistoryStore::from_json(&bad_stats.to_string());
        assert_eq!(problem, Some(HistoryLoadError::StatsRebuilt));
        assert_eq!(store.stats(NOW).total_words, 1);

        let (store, problem) =
            HistoryStore::from_json(r#"{"version":1,"stats":{"slots":{},"later":1}}"#);
        assert!(store.entries().is_empty());
        assert_eq!(problem, None);
    }

    #[test]
    fn corrupt_files_start_empty() {
        for json in ["", "{", "not json", "42", "\"text\"", r#"{"entries": 5}"#] {
            let (store, problem) = HistoryStore::from_json(json);
            assert!(store.entries().is_empty(), "{json:?}");
            assert_eq!(store.stats(NOW).total_dictations, 0);
            assert!(
                matches!(problem, Some(HistoryLoadError::Corrupt(_))),
                "{json:?}"
            );
        }
    }

    #[test]
    fn entry_serialises_in_camel_case() {
        let e = entry("x", 0, "Hello");
        assert!(e.was_edited());
        let json = serde_json::to_string(&e).unwrap_or_default();
        assert!(json.contains("\"rawText\""));
        assert_eq!(serde_json::from_str::<HistoryEntry>(&json).ok(), Some(e));
        let stats = serde_json::to_string(&Stats::default()).unwrap_or_default();
        for key in [
            "streakDays",
            "longestStreakDays",
            "fixesMade",
            "topApps",
            "wordsPerMinute",
        ] {
            assert!(stats.contains(key), "{key}");
        }
    }
}

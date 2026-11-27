import type {
  AppUsage,
  Backend,
  CommandArgs,
  CommandName,
  CommandResult,
  DictationMode,
  DictationPhase,
  DictationState,
  DictionaryEntry,
  EventName,
  Events,
  FileErrorKind,
  HistoryEntry,
  HistoryRetention,
  Hotkey,
  ListenerState,
  ListenerStatus,
  MainRoute,
  MicrophoneInfo,
  ModelInfo,
  ModelKind,
  NetworkRequest,
  Notice,
  NoticeKind,
  Permissions,
  PillRect,
  Platform,
  ProcessingStage,
  RecommendationPick,
  Settings,
  SettingsPatch,
  Snippet,
  Stats,
  UpdateState,
} from "./types";

import {
  detectPlatform,
  formatHotkey,
  hotkeyFromKeyboardEvent,
  hotkeyKeyCount,
} from "@/lib/hotkey";

import { version } from "../../../package.json";
import { applyPatch, defaultSettings } from "./defaults";
import { IpcError } from "./errors";
import { createMockFileJobs } from "./mock-files";
import { validateHotkey } from "./mock-hotkey";
import { approximatePolish } from "./mock-text";
import { createMockUpdates, type MockUpdateOutcome } from "./mock-updates";

/**
 * In-memory backend for `pnpm dev` in a plain browser. Each window (tab) has
 * its own copy. Drive it from the console through `window.__tinyWisprMock`.
 */

const DAY_MS = 86_400_000;
const HEATMAP_DAYS = 26 * 7;
const LATENCY_MS = 30;
const LEVEL_INTERVAL_MS = 33;
const PERMISSION_GRANT_MS = 1000;
const DOWNLOAD_TICK_MS = 200;
const DOWNLOAD_TICKS = 25;
const NOTICE_MS = 5000;
const TRANSCRIBE_MS = 900;
const POLISH_MS = 700;
const MAX_PHRASE_CHARS = 60;
const MAX_EXPANSION_CHARS = 4000;
const PAGE_SIZE = 50;
const WHITESPACE = /\s+/;
const SETTINGS_KEY = "tinyWispr.mock.settings";
const MIC_TEST_ERRORS: Record<MicTestFailure, string> = {
  microphoneBusy: "Another app is using the microphone",
  noMicrophone: "No microphone found",
  permissionDenied: "Microphone access is turned off",
};
const PATH_SEPARATOR = /[\\/]/;

type Handlers = {
  [C in CommandName]: (
    args: CommandArgs<C>
  ) => CommandResult<C> | Promise<CommandResult<C>>;
};

/** How `start_mic_test` can fail. */
export type MicTestFailure =
  | "noMicrophone"
  | "microphoneBusy"
  | "permissionDenied";

export interface MockControls {
  /** Makes the next file job fail with this error, or `null` to succeed. */
  fileFails: (kind: FileErrorKind | null) => void;
  /** The rectangles last sent with `set_pill_hit_regions`. */
  readonly hitRegions: readonly PillRect[];
  /** Fixes the simulated input level (0..1), or `null` for simulated speech. */
  level: (value: number | null) => void;
  /**
   * Forces the shortcut listener's state, e.g. `"secureInput"`, or `null` to
   * follow the permissions again (macOS waits for Accessibility).
   */
  listener: (state: ListenerState | null) => void;
  /** Makes `start_mic_test` reject with this error, or `null` to succeed. */
  micTestFails: (kind: MicTestFailure | null) => void;
  /** Sends `window:navigate`, as the tray and the pill do. */
  navigate: (route: MainRoute) => void;
  /**
   * Shows a pill notice, as the backend would. The time limit warning keeps
   * a running dictation going.
   */
  notice: (kind: NoticeKind) => void;
  /**
   * Jumps to a dictation phase. `elapsedMs` backdates the start of a
   * dictation, e.g. to see the hands-free timer.
   */
  setPhase: (
    phase: DictationPhase,
    options?: {
      elapsedMs?: number;
      mode?: DictationMode;
      stage?: ProcessingStage;
    }
  ) => void;
  /** Changes settings as `update_settings` would. */
  settings: (patch: SettingsPatch) => void;
  /** Current mock state, for inspection. */
  readonly state: unknown;
  /** Jumps the updater to a state, e.g. `"ready"`, with a sample release. */
  update: (state: UpdateState["state"]) => void;
  /** What the next update check finds, or which step fails. */
  updateOutcome: (outcome: MockUpdateOutcome) => void;
}

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
    __tinyWisprMock?: MockControls;
  }
}

const delay = (ms: number) =>
  new Promise<void>((resolve) => {
    setTimeout(resolve, ms);
  });

const countWords = (text: string) =>
  text.trim().split(WHITESPACE).filter(Boolean).length;

function localDate(ms: number): string {
  const d = new Date(ms);
  const month = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${d.getFullYear()}-${month}-${day}`;
}

/** The real catalog (tw-core), so the lists look as they do in the app. */
function seedModels(): ModelInfo[] {
  const base = {
    compression: "none",
    contextLength: null,
    custom: false,
    englishOnly: false,
    reasoning: false,
    recommended: false,
    sha256: null,
    status: { state: "notDownloaded" },
    variantOf: null,
  } as const;
  const hf = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";
  const voice = (
    id: string,
    label: string,
    sizeBytes: number,
    ramMb: number,
    speed: number,
    accuracy: number,
    description: string,
    extra: Partial<ModelInfo>
  ): ModelInfo => ({
    ...base,
    accuracy,
    description,
    family: "whisper",
    file: `ggml-${id}.bin`,
    id,
    kind: "voice",
    label,
    ramMb,
    sizeBytes,
    speed,
    url: `${hf}/ggml-${id}.bin`,
    ...extra,
  });
  const language = (
    id: string,
    family: string,
    label: string,
    sizeBytes: number,
    ramMb: number,
    speed: number,
    accuracy: number,
    description: string,
    extra: Partial<ModelInfo>
  ): ModelInfo => ({
    ...base,
    accuracy,
    description,
    family,
    file: `${id}.gguf`,
    id,
    kind: "language",
    label,
    ramMb,
    sizeBytes,
    speed,
    url: null,
    ...extra,
  });
  return [
    voice(
      "tiny",
      "Tiny",
      77_691_713,
      280,
      5,
      1,
      "Smallest and fastest. Rough with accents and background noise.",
      {}
    ),
    voice(
      "tiny-q5_1",
      "Tiny (compressed)",
      32_152_673,
      200,
      5,
      1,
      "The smallest download. Rough with accents and background noise.",
      { compression: "strong", variantOf: "tiny" }
    ),
    voice(
      "tiny.en",
      "Tiny (English)",
      77_704_715,
      280,
      5,
      1,
      "Fastest. Fine for short, clear English.",
      { englishOnly: true }
    ),
    voice(
      "tiny.en-q5_1",
      "Tiny (English, compressed)",
      32_166_155,
      200,
      5,
      1,
      "The smallest download. Fine for short, clear English.",
      { compression: "strong", englishOnly: true, variantOf: "tiny.en" }
    ),
    voice(
      "base",
      "Base",
      147_951_465,
      390,
      5,
      2,
      "Fast and small. Good for clear speech in most languages.",
      {}
    ),
    voice(
      "base-q5_1",
      "Base (compressed)",
      59_707_625,
      300,
      5,
      2,
      "A smaller download of Base with nearly the same accuracy.",
      { compression: "strong", variantOf: "base" }
    ),
    voice(
      "base.en",
      "Base (English)",
      147_964_211,
      390,
      5,
      2,
      "Fast and small. Good for clear English.",
      { englishOnly: true, status: { state: "downloaded" } }
    ),
    voice(
      "base.en-q5_1",
      "Base (English, compressed)",
      59_721_011,
      300,
      5,
      2,
      "A smaller download of Base (English) with nearly the same accuracy.",
      { compression: "strong", englishOnly: true, variantOf: "base.en" }
    ),
    voice(
      "small",
      "Small",
      487_601_967,
      860,
      4,
      3,
      "A good balance of speed and accuracy in many languages.",
      {}
    ),
    voice(
      "small-q5_1",
      "Small (compressed)",
      190_085_487,
      560,
      4,
      3,
      "A smaller download of Small with nearly the same accuracy.",
      { compression: "strong", variantOf: "small" }
    ),
    voice(
      "small.en",
      "Small (English)",
      487_614_201,
      860,
      4,
      3,
      "A good balance of speed and accuracy for English.",
      { englishOnly: true }
    ),
    voice(
      "small.en-q5_1",
      "Small (English, compressed)",
      190_098_681,
      560,
      4,
      3,
      "A smaller download of Small (English) with nearly the same accuracy.",
      { compression: "strong", englishOnly: true, variantOf: "small.en" }
    ),
    voice(
      "medium",
      "Medium",
      1_533_763_059,
      2100,
      2,
      4,
      "Accurate in many languages, but slow without a fast GPU.",
      {}
    ),
    voice(
      "medium-q5_0",
      "Medium (compressed)",
      539_212_467,
      1100,
      2,
      4,
      "A smaller download of Medium with nearly the same accuracy.",
      { compression: "strong", variantOf: "medium" }
    ),
    voice(
      "medium.en",
      "Medium (English)",
      1_533_774_781,
      2100,
      2,
      4,
      "Accurate English, but slow without a fast GPU.",
      { englishOnly: true }
    ),
    voice(
      "medium.en-q5_0",
      "Medium (English, compressed)",
      539_225_533,
      1100,
      2,
      4,
      "A smaller download of Medium (English) with nearly the same accuracy.",
      { compression: "strong", englishOnly: true, variantOf: "medium.en" }
    ),
    voice(
      "large-v3-turbo",
      "Large v3 Turbo",
      1_624_555_275,
      2000,
      3,
      5,
      "Close to the most accurate, and much faster than Large v3.",
      {}
    ),
    voice(
      "large-v3-turbo-q8_0",
      "Large v3 Turbo (lightly compressed)",
      874_188_075,
      1250,
      3,
      5,
      "Half the download of Large v3 Turbo with the same accuracy in practice.",
      { compression: "light", variantOf: "large-v3-turbo" }
    ),
    voice(
      "large-v3-turbo-q5_0",
      "Large v3 Turbo (compressed)",
      574_041_195,
      950,
      3,
      4,
      "Accurate in many languages at a third of the download.",
      { compression: "strong", recommended: true, variantOf: "large-v3-turbo" }
    ),
    voice(
      "large-v3",
      "Large v3",
      3_095_033_483,
      3900,
      1,
      5,
      "The most accurate. Slow, and needs a powerful computer.",
      {}
    ),
    voice(
      "large-v3-q5_0",
      "Large v3 (compressed)",
      1_081_140_203,
      1900,
      1,
      5,
      "A smaller download of Large v3 with nearly the same accuracy. Slow.",
      { compression: "strong", variantOf: "large-v3" }
    ),
    language(
      "llama-3.2-1b-q4_k_m",
      "llama",
      "Llama 3.2 1B",
      807_690_688,
      1300,
      5,
      2,
      "The fastest cleanup. Fixes fillers and punctuation, may miss subtler mistakes.",
      { contextLength: 131_072 }
    ),
    language(
      "llama-3.2-3b-q4_k_m",
      "llama",
      "Llama 3.2 3B",
      2_019_377_440,
      2700,
      3,
      3,
      "Reliable cleanup for English at a moderate size.",
      { contextLength: 131_072 }
    ),
    language(
      "qwen3.5-2b-q4_k_m",
      "qwen",
      "Qwen 3.5 2B",
      1_280_835_840,
      1900,
      4,
      3,
      "Fast, small and good in many languages.",
      { contextLength: 262_144, status: { state: "downloaded" } }
    ),
    language(
      "qwen3-4b-instruct-2507-q4_k_m",
      "qwen",
      "Qwen 3 4B Instruct",
      2_497_281_120,
      3300,
      3,
      4,
      "Careful, accurate cleanup in many languages. A good default.",
      { contextLength: 262_144, recommended: true }
    ),
    language(
      "ministral-3-3b-q4_k_m",
      "mistral",
      "Ministral 3 3B",
      2_147_023_008,
      2900,
      3,
      4,
      "Balanced cleanup, strong in European languages.",
      { contextLength: 262_144 }
    ),
    language(
      "ministral-3-8b-q4_k_m",
      "mistral",
      "Ministral 3 8B",
      5_198_911_904,
      6300,
      2,
      5,
      "The most polished cleanup. Needs 16 GB of memory or more.",
      { contextLength: 262_144 }
    ),
    language(
      "deepseek-r1-distill-qwen-1.5b-q4_k_m",
      "deepseek",
      "DeepSeek R1 Distill 1.5B",
      1_117_321_312,
      1700,
      3,
      1,
      "Small, but it often answers or rewrites instead of cleaning up. Not a good first choice.",
      { contextLength: 131_072, reasoning: true }
    ),
    language(
      "deepseek-r1-0528-qwen3-8b-q4_k_m",
      "deepseek",
      "DeepSeek R1 0528 8B",
      5_027_785_216,
      6100,
      1,
      4,
      "Thinks before answering. Thorough, but slow for everyday dictation.",
      { contextLength: 131_072, reasoning: true }
    ),
  ];
}

const SAMPLE_APPS = [
  { category: "workMessaging", id: "com.tinyspeck.slackmacgap", name: "Slack" },
  {
    category: "personalMessaging",
    id: "com.apple.MobileSMS",
    name: "Messages",
  },
  { category: "email", id: "com.apple.mail", name: "Mail" },
  { category: "documents", id: "com.apple.Notes", name: "Notes" },
  { category: "code", id: "com.microsoft.VSCode", name: "Visual Studio Code" },
] as const;

const SAMPLE_TRANSCRIPTS: [raw: string, final: string][] = [
  [
    "um can you send me the notes from this morning's call",
    "Can you send me the notes from this morning's call?",
  ],
  [
    "running ten minutes late see you at the cafe",
    "Running ten minutes late, see you at the cafe",
  ],
  [
    "hi team uh quick update the release is moving to thursday so we have time for one more round of testing",
    "Hi team, quick update: the release is moving to Thursday, so we have time for one more round of testing.",
  ],
  [
    "remember to buy oat milk and coffee beans",
    "Remember to buy oat milk and coffee beans.",
  ],
  [
    "this function should return early when the list is empty",
    "This function should return early when the list is empty.",
  ],
  [
    "thanks so much for the quick reply that works for me",
    "Thanks so much for the quick reply, that works for me.",
  ],
];

const SEED_DAY_OFFSETS = [0, 0, 0, 1, 1, 2, 3, 5, 8, 12, 15];

function seedHistory(now: number): HistoryEntry[] {
  return SEED_DAY_OFFSETS.map((daysAgo, i) => {
    const [raw, final] = SAMPLE_TRANSCRIPTS[i % SAMPLE_TRANSCRIPTS.length] ??
      SAMPLE_TRANSCRIPTS[0] ?? ["", ""];
    const app = SAMPLE_APPS[i % SAMPLE_APPS.length] ?? SAMPLE_APPS[0];
    const words = countWords(final);
    return {
      app: { ...app },
      createdAt: now - daysAgo * DAY_MS - (i + 1) * 47 * 60_000,
      durationMs: words * 420,
      finalText: final,
      id: `seed-${i}`,
      language: "en",
      rawText: raw,
      wordCount: words,
    };
  });
}

function initialPermissions(platform: Platform): Permissions {
  if (platform === "macos") {
    return {
      accessibility: "notDetermined",
      inputMonitoring: "notDetermined",
      microphone: "notDetermined",
    };
  }
  return {
    accessibility: "notApplicable",
    inputMonitoring: "notApplicable",
    microphone: platform === "windows" ? "notDetermined" : "notApplicable",
  };
}

const TOP_APPS = 5;

/** A dictation as the stats count it; deleting history doesn't remove it. */
interface Counted {
  entry: HistoryEntry;
  fixes: number;
}

function computeStats(counted: Counted[], now: number): Stats {
  const totalWords = counted.reduce((sum, c) => sum + c.entry.wordCount, 0);
  const totalMs = counted.reduce((sum, c) => sum + c.entry.durationMs, 0);
  const byDay = new Map<string, number>();
  const apps = new Map<string, AppUsage>();
  for (const { entry } of counted) {
    const date = localDate(entry.createdAt);
    byDay.set(date, (byDay.get(date) ?? 0) + entry.wordCount);
    if (entry.app) {
      const usage = apps.get(entry.app.name) ?? {
        dictations: 0,
        name: entry.app.name,
        words: 0,
      };
      usage.dictations += 1;
      usage.words += entry.wordCount;
      apps.set(entry.app.name, usage);
    }
  }
  let streakDays = 0;
  // A streak that ended yesterday still counts until today is over.
  const streakEnd = byDay.has(localDate(now)) ? now : now - DAY_MS;
  while (byDay.has(localDate(streakEnd - streakDays * DAY_MS))) {
    streakDays += 1;
  }
  let longestStreakDays = 0;
  for (const date of byDay.keys()) {
    const start = new Date(`${date}T12:00:00`).getTime();
    if (byDay.has(localDate(start - DAY_MS))) {
      continue;
    }
    let length = 1;
    while (byDay.has(localDate(start + length * DAY_MS))) {
      length += 1;
    }
    longestStreakDays = Math.max(longestStreakDays, length);
  }
  return {
    daily: Array.from({ length: HEATMAP_DAYS }, (_, i) => {
      const date = localDate(now - (HEATMAP_DAYS - 1 - i) * DAY_MS);
      return { date, words: byDay.get(date) ?? 0 };
    }),
    fixesMade: counted.reduce((sum, c) => sum + c.fixes, 0),
    longestStreakDays,
    streakDays,
    topApps: [...apps.values()]
      .sort(
        (a, b) =>
          b.dictations - a.dictations ||
          b.words - a.words ||
          a.name.localeCompare(b.name)
      )
      .slice(0, TOP_APPS),
    totalDictations: counted.length,
    totalWords,
    wordsPerMinute:
      totalMs > 0 ? Math.round(totalWords / (totalMs / 60_000)) : 0,
  };
}

function keptByRetention(
  history: HistoryEntry[],
  retention: HistoryRetention,
  now: number
): HistoryEntry[] {
  const maxAge = {
    forever: Number.POSITIVE_INFINITY,
    never: 0,
    oneDay: DAY_MS,
    thirtyDays: 30 * DAY_MS,
  }[retention];
  return history.filter((e) => now - e.createdAt < maxAge);
}

function checkPhrase(label: string, value: string) {
  if (!value.trim()) {
    throw new IpcError("invalidInput", `${label} can't be empty`);
  }
  if (value.length > MAX_PHRASE_CHARS) {
    throw new IpcError(
      "invalidInput",
      `${label} can be up to ${MAX_PHRASE_CHARS} characters`
    );
  }
}

/** Settings survive a reload of the tab, like the app's settings file. */
function loadSettings(): Settings | null {
  try {
    const saved = sessionStorage.getItem(SETTINGS_KEY);
    return saved ? (JSON.parse(saved) as Settings) : null;
  } catch {
    return null;
  }
}

function saveSettings(settings: Settings) {
  try {
    sessionStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
  } catch {
    // Storage is off (private window): settings last until the reload.
  }
}

export function createMockBackend(): Backend {
  const platform = detectPlatform();
  const now = Date.now();
  const listeners = new Map<EventName, Set<(payload: never) => void>>();

  const state = {
    dictation: {
      elapsedMs: 0,
      mode: null,
      notice: null,
      phase: "idle",
      stage: null,
    } as DictationState,
    dictionary: [
      {
        createdAt: now - 3 * DAY_MS,
        id: "dict-1",
        misspelling: "whisper flow",
        word: "Wispr Flow",
      },
      {
        createdAt: now - 2 * DAY_MS,
        id: "dict-2",
        misspelling: null,
        word: "Tauri",
      },
      {
        createdAt: now - DAY_MS,
        id: "dict-3",
        misspelling: "post gress",
        word: "Postgres",
      },
    ] as DictionaryEntry[],
    fixedLevel: null as number | null,
    history: seedHistory(now),
    hitRegions: [] as PillRect[],
    listenerOverride: null as ListenerState | null,
    microphones: [
      { id: "builtin", isDefault: true, name: "MacBook Pro Microphone" },
      { id: "usb-headset", isDefault: false, name: "USB Headset" },
      { id: "studio", isDefault: false, name: "Studio Display Microphone" },
    ] as MicrophoneInfo[],
    micTestFails: null as MicTestFailure | null,
    models: seedModels(),
    networkLog: [] as NetworkRequest[],
    permissions: initialPermissions(platform),
    settings: loadSettings() ?? {
      ...defaultSettings(platform),
      sttModel: "base.en",
    },
    snippets: [
      {
        createdAt: now - 4 * DAY_MS,
        expansion: "221B Baker Street, London NW1 6XE",
        id: "snip-1",
        trigger: "my address",
      },
      {
        createdAt: now - DAY_MS,
        expansion:
          "You can book a time that suits you here: https://cal.example.com/intro",
        id: "snip-2",
        trigger: "calendar link",
      },
    ] as Snippet[],
  };
  const counted: Counted[] = state.history.map((entry) => ({
    entry,
    fixes: entry.rawText === entry.finalText ? 0 : 1,
  }));

  /** Where the running download started, for the network log. */
  const downloadStarts = new Map<
    string,
    { resumedAt: number; startedAt: number }
  >();

  const timers = {
    capture: null as null | (() => void),
    downloads: new Map<string, ReturnType<typeof setInterval>>(),
    level: null as ReturnType<typeof setInterval> | null,
    micTest: false,
    notice: null as ReturnType<typeof setTimeout> | null,
    phase: null as ReturnType<typeof setTimeout> | null,
    startedAt: 0,
  };

  function emit<E extends EventName>(event: E, payload: Events[E]) {
    for (const handler of listeners.get(event) ?? []) {
      (handler as (payload: Events[E]) => void)(payload);
    }
  }

  function setSettings(next: Settings) {
    state.settings = next;
    saveSettings(next);
    state.history = keptByRetention(
      state.history,
      next.historyRetention,
      Date.now()
    );
    emit("settings:changed", next);
    return next;
  }

  // ── audio levels ───────────────────────────────────────────────────────
  function startLevels() {
    if (timers.level) {
      return;
    }
    let level = 0;
    let t = 0;
    timers.level = setInterval(() => {
      t += 1;
      const syllable = Math.abs(Math.sin(t / 3)) * (0.5 + Math.random() * 0.5);
      const pause = Math.sin(t / 40) > 0.7 ? 0.1 : 1;
      const target = state.fixedLevel ?? syllable * pause;
      level = level * 0.6 + target * 0.4;
      emit("audio:level", { level: Math.min(1, Math.max(0, level)) });
      if (state.dictation.phase === "listening") {
        state.dictation = {
          ...state.dictation,
          elapsedMs: Date.now() - timers.startedAt,
        };
      }
    }, LEVEL_INTERVAL_MS);
  }

  function stopLevels() {
    if (timers.level && !timers.micTest) {
      clearInterval(timers.level);
      timers.level = null;
    }
  }

  // ── dictation ──────────────────────────────────────────────────────────
  function setDictation(next: Partial<DictationState>) {
    state.dictation = {
      elapsedMs: 0,
      mode: null,
      notice: null,
      phase: "idle",
      stage: null,
      ...next,
    };
    emit("dictation:state", state.dictation);
  }

  function clearPhaseTimers() {
    for (const key of ["phase", "notice"] as const) {
      const timer = timers[key];
      if (timer) {
        clearTimeout(timer);
        timers[key] = null;
      }
    }
  }

  function makeNotice(kind: NoticeKind): Notice {
    const pasteLast = formatHotkey(
      state.settings.shortcuts.pasteLast,
      platform
    );
    switch (kind) {
      case "cancelled":
        return {
          action: { kind: "undo", label: "Undo" },
          kind,
          text: "Dictation cancelled",
        };
      case "pasteFailed":
        return {
          action: { kind: "copyLast", label: "Copy" },
          kind,
          text: `Couldn't paste. Press ${pasteLast} to paste it`,
        };
      case "noSpeech":
        return { action: null, kind, text: "Didn't catch that" };
      case "noMicrophone":
        return {
          action: { kind: "openSettings", label: "Open settings" },
          kind,
          text: "No microphone found",
        };
      case "stillProcessing":
        return {
          action: null,
          kind,
          text: "Still processing your last dictation",
        };
      case "noModel":
        return {
          action: { kind: "openSettings", label: "Open settings" },
          kind,
          text: "No voice model yet",
        };
      case "timeLimitWarning":
        return { action: null, kind, text: "Dictation stops in 1 minute" };
      default:
        return { action: null, kind, text: "Something went wrong" };
    }
  }

  function showNotice(kind: NoticeKind) {
    if (kind === "timeLimitWarning" && state.dictation.phase === "listening") {
      const listening = { ...state.dictation, notice: null };
      setDictation({ ...listening, notice: makeNotice(kind) });
      timers.notice = setTimeout(() => {
        if (state.dictation.phase === "listening") {
          setDictation({
            ...listening,
            elapsedMs: Date.now() - timers.startedAt,
          });
        }
      }, NOTICE_MS);
      return;
    }
    clearPhaseTimers();
    stopLevels();
    setDictation({ notice: makeNotice(kind), phase: "notice" });
    timers.notice = setTimeout(() => setDictation({}), NOTICE_MS);
  }

  function startListening(mode: DictationMode, elapsedMs = 0) {
    if (state.dictation.phase === "processing") {
      showNotice("stillProcessing");
      return;
    }
    if (!state.settings.sttModel) {
      showNotice("noModel");
      return;
    }
    clearPhaseTimers();
    timers.startedAt = Date.now() - elapsedMs;
    setDictation({ elapsedMs, mode, phase: "listening" });
    startLevels();
  }

  function finish(mode: DictationMode | null) {
    clearPhaseTimers();
    stopLevels();
    const durationMs = Math.max(1000, Date.now() - timers.startedAt);
    setDictation({ mode, phase: "processing", stage: "transcribing" });
    const polish = Boolean(state.settings.llmModel);
    const complete = () => {
      const index = state.history.length % SAMPLE_TRANSCRIPTS.length;
      const [raw, final] = SAMPLE_TRANSCRIPTS[index] ?? ["", ""];
      const text = state.settings.cleanupLevel === "none" ? raw : final;
      const app = SAMPLE_APPS[index % SAMPLE_APPS.length] ?? SAMPLE_APPS[0];
      const entry: HistoryEntry = {
        app: { ...app },
        createdAt: Date.now(),
        durationMs,
        finalText: text,
        id: crypto.randomUUID(),
        language: "en",
        rawText: raw,
        wordCount: countWords(text),
      };
      addHistory(entry, text === raw ? 0 : 1);
      setDictation({});
    };
    timers.phase = setTimeout(() => {
      if (!polish) {
        complete();
        return;
      }
      setDictation({ mode, phase: "processing", stage: "polishing" });
      timers.phase = setTimeout(complete, POLISH_MS);
    }, TRANSCRIBE_MS);
  }

  // ── hotkey capture ─────────────────────────────────────────────────────
  function startCapture() {
    timers.capture?.();
    let best: Hotkey = { key: null, modifiers: [] };
    const onDown = (event: KeyboardEvent) => {
      event.preventDefault();
      const current = hotkeyFromKeyboardEvent(event);
      if (hotkeyKeyCount(current) >= hotkeyKeyCount(best)) {
        best = current;
      }
      emit("hotkey:captured", { done: false, hotkey: current });
    };
    const onUp = (event: KeyboardEvent) => {
      event.preventDefault();
      if (event.ctrlKey || event.altKey || event.shiftKey || event.metaKey) {
        return;
      }
      emit("hotkey:captured", { done: true, hotkey: best });
      best = { key: null, modifiers: [] };
    };
    window.addEventListener("keydown", onDown, true);
    window.addEventListener("keyup", onUp, true);
    timers.capture = () => {
      window.removeEventListener("keydown", onDown, true);
      window.removeEventListener("keyup", onUp, true);
      timers.capture = null;
    };
  }

  // ── models ─────────────────────────────────────────────────────────────
  function recommendationFor(kind: ModelKind): RecommendationPick | null {
    const model = state.models.find((m) => m.kind === kind && m.recommended);
    if (!model) {
      return null;
    }
    const reason =
      kind === "voice"
        ? "Accurate and still quick with 36 GB of memory and Apple silicon."
        : "Careful edits that fit comfortably in 36 GB of memory.";
    return { id: model.id, reason };
  }

  function findModel(id: string): ModelInfo {
    const model = state.models.find((m) => m.id === id);
    if (!model) {
      throw new IpcError("notFound", `No model with id ${id}`);
    }
    return model;
  }

  function setModel(id: string, patch: Partial<ModelInfo>) {
    state.models = state.models.map((m) =>
      m.id === id ? { ...m, ...patch } : m
    );
  }

  function logRequest(
    model: ModelInfo,
    startedAt: number,
    bytes: number,
    result: NetworkRequest["result"]
  ) {
    state.networkLog.push({
      bytes,
      finishedAt: Date.now(),
      message: null,
      result,
      startedAt,
      url: model.url ?? `https://huggingface.co/${model.file}`,
    });
  }

  function listenerStatus(): ListenerStatus {
    if (state.listenerOverride) {
      return {
        canSwallow: state.listenerOverride === "running",
        state: state.listenerOverride,
      };
    }
    if (platform !== "macos" || state.permissions.accessibility === "granted") {
      return { canSwallow: true, state: "running" };
    }
    if (state.permissions.inputMonitoring === "granted") {
      return { canSwallow: false, state: "running" };
    }
    return { canSwallow: false, state: "waitingForPermission" };
  }

  function startDownload(model: ModelInfo) {
    const total = model.sizeBytes;
    let received = model.status.state === "paused" ? model.status.received : 0;
    const resumedAt = received;
    const startedAt = Date.now();
    downloadStarts.set(model.id, { resumedAt, startedAt });
    setModel(model.id, { status: { received, state: "downloading", total } });
    emit("models:changed", null);
    const timer = setInterval(() => {
      received = Math.min(total, received + Math.ceil(total / DOWNLOAD_TICKS));
      if (received < total) {
        setModel(model.id, {
          status: { received, state: "downloading", total },
        });
        emit("models:progress", {
          id: model.id,
          received,
          state: "downloading",
          total,
        });
        return;
      }
      clearInterval(timer);
      timers.downloads.delete(model.id);
      downloadStarts.delete(model.id);
      logRequest(model, startedAt, total - resumedAt, "completed");
      setModel(model.id, { status: { state: "downloaded" } });
      emit("models:progress", { id: model.id, received, state: "done", total });
      emit("models:changed", null);
    }, DOWNLOAD_TICK_MS);
    timers.downloads.set(model.id, timer);
  }

  // ── commands ───────────────────────────────────────────────────────────
  /** Counts `entry` in the stats; keeps it only if retention allows. */
  function addHistory(entry: HistoryEntry, fixes: number) {
    counted.push({ entry, fixes });
    state.history = keptByRetention(
      [entry, ...state.history],
      state.settings.historyRetention,
      entry.createdAt
    );
    emit("history:changed", null);
  }

  const files = createMockFileJobs({
    dictationBusy: () =>
      state.dictation.phase === "listening" ||
      state.dictation.phase === "processing",
    emit: (progress) => emit("files:progress", progress),
    hasVoiceModel: () => Boolean(state.settings.sttModel),
    polishes: () =>
      Boolean(state.settings.llmModel) &&
      state.settings.cleanupLevel !== "none",
    save: (entry) => addHistory(entry, 0),
  });

  const updates = createMockUpdates({
    busy: () =>
      state.dictation.phase === "listening" ||
      state.dictation.phase === "processing" ||
      files.current() !== null,
    currentVersion: version,
    emit: (update) => emit("updates:state", update),
    log: (request) => state.networkLog.push(request),
    restart: () => window.location.reload(),
  });

  const handlers: Handlers = {
    cancel_dictation: () => {
      if (state.dictation.phase === "listening") {
        showNotice("cancelled");
      }
      return null;
    },
    cancel_download: ({ id }) => {
      const timer = timers.downloads.get(id);
      const model = findModel(id);
      if (timer) {
        clearInterval(timer);
        timers.downloads.delete(id);
      }
      if (model.status.state === "downloading") {
        const { received, total } = model.status;
        const start = downloadStarts.get(id);
        if (start) {
          downloadStarts.delete(id);
          logRequest(
            model,
            start.startedAt,
            received - start.resumedAt,
            "cancelled"
          );
        }
        setModel(id, { status: { received, state: "paused", total } });
        emit("models:progress", { id, received, state: "paused", total });
        emit("models:changed", null);
      }
      return null;
    },
    cancel_file_transcription: ({ jobId }) => {
      files.cancel(jobId);
      return null;
    },
    check_for_updates: () => {
      updates.check();
      return null;
    },
    clear_history: () => {
      state.history = [];
      emit("history:changed", null);
      return null;
    },
    complete_onboarding: () => {
      const next = setSettings(
        applyPatch(state.settings, { onboardingCompleted: true })
      );
      if (!isMainPage()) {
        setTimeout(() => window.location.assign("/index.html#/home"), 0);
      }
      return next;
    },
    copy_last_transcript: async () => {
      const [last] = state.history;
      if (!last) {
        throw new IpcError("notFound", "Nothing to copy yet");
      }
      await navigator.clipboard?.writeText(last.finalText).catch(() => {
        // The page may not have focus; the real backend writes the clipboard itself.
      });
      return null;
    },
    delete_dictionary_entry: ({ id }) => {
      state.dictionary = state.dictionary.filter((e) => e.id !== id);
      return null;
    },
    delete_history_entry: ({ id }) => {
      state.history = state.history.filter((e) => e.id !== id);
      emit("history:changed", null);
      return null;
    },
    delete_model: ({ id }) => {
      const model = findModel(id);
      setModel(id, { status: { state: "notDownloaded" } });
      const key = model.kind === "voice" ? "sttModel" : "llmModel";
      if (state.settings[key] === id) {
        setSettings(applyPatch(state.settings, { [key]: null }));
      }
      emit("models:changed", null);
      return null;
    },
    delete_snippet: ({ id }) => {
      state.snippets = state.snippets.filter((s) => s.id !== id);
      return null;
    },
    download_model: ({ id }) => {
      const model = findModel(id);
      if (model.status.state === "downloaded" || timers.downloads.has(id)) {
        return null;
      }
      startDownload(model);
      return null;
    },
    download_update: () => {
      updates.download();
      return null;
    },
    get_default_shortcuts: () => defaultSettings(platform).shortcuts,
    get_dictation_state: () => state.dictation,
    get_file_transcription: () => files.current(),
    get_hardware_info: () => ({
      appleSilicon: platform === "macos",
      arch: "aarch64",
      cpuCores: 14,
      cpuName: platform === "macos" ? "Apple M3 Max" : "AMD Ryzen 9 7940HS",
      platform,
      totalMemoryBytes: 36 * 1024 ** 3,
    }),
    get_listener_status: () => listenerStatus(),
    get_permissions: () => state.permissions,
    get_recommendation: () => ({
      language: recommendationFor("language"),
      voice: recommendationFor("voice"),
    }),
    get_settings: () => state.settings,
    get_stats: () => computeStats(counted, Date.now()),
    get_update_state: () => updates.state(),
    hide_pill_for: ({ minutes }) =>
      setSettings(
        applyPatch(state.settings, {
          pillHiddenUntil: Date.now() + minutes * 60_000,
        })
      ),
    import_model: ({ kind, path }) => {
      const file = path.split(PATH_SEPARATOR).pop() ?? path;
      const model: ModelInfo = {
        accuracy: 3,
        compression: "none",
        contextLength: kind === "language" ? 4096 : null,
        custom: true,
        description: "",
        englishOnly: false,
        family: "custom",
        file,
        id: `custom-${crypto.randomUUID()}`,
        kind,
        label: file,
        ramMb: 0,
        reasoning: false,
        recommended: false,
        sha256: null,
        sizeBytes: 0,
        speed: 3,
        status: { state: "downloaded" },
        url: null,
        variantOf: null,
      };
      state.models = [...state.models, model];
      emit("models:changed", null);
      return model;
    },
    install_update: () => {
      updates.install();
      return null;
    },
    list_dictionary: () => state.dictionary,
    list_history: ({ cursor, query }) => {
      const needle = query?.trim().toLowerCase();
      const matches = state.history
        .filter(
          (e) =>
            !needle ||
            e.finalText.toLowerCase().includes(needle) ||
            e.rawText.toLowerCase().includes(needle)
        )
        .sort((a, b) => b.createdAt - a.createdAt);
      const start = cursor ? Number(cursor) : 0;
      const end = start + PAGE_SIZE;
      return {
        entries: matches.slice(start, end),
        nextCursor: end < matches.length ? String(end) : null,
      };
    },
    list_microphones: () => state.microphones,
    list_models: () => state.models,
    list_network_log: () => state.networkLog,
    list_snippets: () => state.snippets,
    open_main_window: ({ route }) => {
      if (isMainPage()) {
        emit("window:navigate", { route: route ?? "home" });
      } else {
        window.location.assign(`/index.html#/${route ?? "home"}`);
      }
      return null;
    },
    open_permission_settings: () => null,
    paste_last_transcript: () => {
      if (state.history.length === 0) {
        throw new IpcError("notFound", "Nothing to paste yet");
      }
      return null;
    },
    request_permission: ({ kind }) => {
      setTimeout(() => {
        state.permissions = { ...state.permissions, [kind]: "granted" };
        emit("permissions:changed", state.permissions);
        emit("listener:status", listenerStatus());
      }, PERMISSION_GRANT_MS);
      return null;
    },
    reset_shortcuts: () =>
      setSettings(
        applyPatch(state.settings, {
          shortcuts: defaultSettings(platform).shortcuts,
        })
      ),
    save_file_transcript: () => true,
    set_pill_hit_regions: ({ rects }) => {
      state.hitRegions = rects;
      return null;
    },
    start_dictation: ({ mode }) => {
      startListening(mode);
      return null;
    },
    start_hotkey_capture: () => {
      startCapture();
      return null;
    },
    start_mic_test: () => {
      if (state.micTestFails) {
        throw new IpcError(
          state.micTestFails,
          MIC_TEST_ERRORS[state.micTestFails]
        );
      }
      timers.micTest = true;
      startLevels();
      return null;
    },
    stop_dictation: () => {
      if (state.dictation.phase === "listening") {
        finish(state.dictation.mode);
      }
      return null;
    },
    stop_hotkey_capture: () => {
      timers.capture?.();
      return null;
    },
    stop_mic_test: () => {
      timers.micTest = false;
      if (state.dictation.phase !== "listening") {
        stopLevels();
      }
      return null;
    },
    test_polish: ({ category, text }) =>
      approximatePolish(
        text,
        state.settings.cleanupLevel,
        state.settings.styles[category ?? "other"],
        category ?? "other"
      ),
    transcribe_file: ({ path }) => files.transcribe(path),
    undo_cancel: () => {
      if (state.dictation.notice?.kind === "cancelled") {
        finish(null);
      }
      return null;
    },
    update_settings: ({ patch }) =>
      setSettings(applyPatch(state.settings, patch)),
    upsert_dictionary_entry: ({ entry }) => {
      checkPhrase("Word", entry.word);
      if (entry.misspelling) {
        checkPhrase("Misspelling", entry.misspelling);
      }
      const duplicate = state.dictionary.some(
        (e) =>
          e.id !== entry.id &&
          e.word.toLowerCase() === entry.word.trim().toLowerCase()
      );
      if (duplicate) {
        throw new IpcError(
          "invalidInput",
          "This word is already in your dictionary"
        );
      }
      const saved: DictionaryEntry = {
        createdAt:
          state.dictionary.find((e) => e.id === entry.id)?.createdAt ??
          Date.now(),
        id: entry.id ?? crypto.randomUUID(),
        misspelling: entry.misspelling?.trim() || null,
        word: entry.word.trim(),
      };
      state.dictionary = [
        saved,
        ...state.dictionary.filter((e) => e.id !== saved.id),
      ];
      return saved;
    },
    upsert_snippet: ({ snippet }) => {
      checkPhrase("Trigger", snippet.trigger);
      if (!snippet.expansion.trim()) {
        throw new IpcError("invalidInput", "Expansion can't be empty");
      }
      if (snippet.expansion.length > MAX_EXPANSION_CHARS) {
        throw new IpcError(
          "invalidInput",
          `Expansion can be up to ${MAX_EXPANSION_CHARS.toLocaleString("en")} characters`
        );
      }
      const duplicate = state.snippets.some(
        (s) =>
          s.id !== snippet.id &&
          s.trigger.toLowerCase() === snippet.trigger.trim().toLowerCase()
      );
      if (duplicate) {
        throw new IpcError(
          "invalidInput",
          "A snippet with this trigger already exists"
        );
      }
      const saved: Snippet = {
        createdAt:
          state.snippets.find((s) => s.id === snippet.id)?.createdAt ??
          Date.now(),
        expansion: snippet.expansion,
        id: snippet.id ?? crypto.randomUUID(),
        trigger: snippet.trigger.trim(),
      };
      state.snippets = [
        saved,
        ...state.snippets.filter((s) => s.id !== saved.id),
      ];
      return saved;
    },
    validate_hotkey: ({ action, hotkey }) =>
      validateHotkey(action, hotkey, platform, state.settings.shortcuts),
  };

  const controls: MockControls = {
    fileFails(kind) {
      files.failNext(kind);
    },
    get hitRegions() {
      return state.hitRegions;
    },
    level(value) {
      state.fixedLevel =
        value === null ? null : Math.min(1, Math.max(0, value));
    },
    listener(listenerState) {
      state.listenerOverride = listenerState;
      emit("listener:status", listenerStatus());
    },
    micTestFails(kind) {
      state.micTestFails = kind;
    },
    navigate(route) {
      emit("window:navigate", { route });
    },
    notice(kind) {
      showNotice(kind);
    },
    setPhase(phase, options = {}) {
      clearPhaseTimers();
      if (phase === "listening") {
        startListening(options.mode ?? "hold", options.elapsedMs);
      } else if (phase === "processing") {
        stopLevels();
        setDictation({
          mode: options.mode ?? "hold",
          phase,
          stage: options.stage ?? "transcribing",
        });
      } else if (phase === "notice") {
        showNotice("cancelled");
      } else {
        stopLevels();
        setDictation({});
      }
    },
    settings(patch) {
      setSettings(applyPatch(state.settings, patch));
    },
    get state() {
      return state;
    },
    update(target) {
      updates.jump(target);
    },
    updateOutcome(outcome) {
      updates.outcome(outcome);
    },
  };
  window.__tinyWisprMock = controls;

  return {
    async invoke<C extends CommandName>(
      command: C,
      args: CommandArgs<C>
    ): Promise<CommandResult<C>> {
      await delay(LATENCY_MS);
      const handler: Handlers[C] = handlers[command];
      return structuredClone(await handler(args));
    },
    kind: "mock",
    listen(event, handler) {
      let set = listeners.get(event);
      if (!set) {
        set = new Set();
        listeners.set(event, set);
      }
      const entry = handler as (payload: never) => void;
      set.add(entry);
      return () => {
        set.delete(entry);
      };
    },
  };
}

function isMainPage(): boolean {
  const path = window.location.pathname;
  return path === "/" || path.endsWith("/index.html");
}

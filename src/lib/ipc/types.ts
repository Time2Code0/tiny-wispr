/**
 * The IPC contract (spec §4), mirrored by hand from the Rust types:
 * `crates/tw-core/src/**` and `src-tauri/src/{error,events,windows}.rs`.
 * Change both sides together.
 */

/** Milliseconds since the Unix epoch. */
export type Millis = number;

// ── hotkey (tw_core::hotkey) ─────────────────────────────────────────────

export type Platform = "macos" | "windows" | "linux";

/** Declaration order is the display order. */
export const MODIFIERS = ["fn", "ctrl", "alt", "shift", "meta"] as const;
export type Modifier = (typeof MODIFIERS)[number];

/** W3C `KeyboardEvent.code`, e.g. `"Space"`, `"KeyV"`, `"Escape"`. */
export type KeyCode = string;

export interface Hotkey {
  key: KeyCode | null;
  /** Sorted in {@link MODIFIERS} order, no duplicates. */
  modifiers: Modifier[];
}

export const HOTKEY_ACTIONS = [
  "pushToTalk",
  "handsFree",
  "cancel",
  "pasteLast",
  "commandMode",
] as const;
export type HotkeyAction = (typeof HOTKEY_ACTIONS)[number];

export type Shortcuts = Record<HotkeyAction, Hotkey>;

export type HotkeyIssueKind =
  | "empty"
  | "tooManyKeys"
  | "needsModifier"
  | "escapeOnlyForCancel"
  | "reserved"
  | "duplicate";

export interface HotkeyIssue {
  conflictsWith: HotkeyAction | null;
  kind: HotkeyIssueKind;
  message: string;
}

// ── text (tw_core::text) ─────────────────────────────────────────────────

export type CleanupLevel = "none" | "light" | "medium";

export const STYLE_CATEGORIES = ["personal", "work", "email", "other"] as const;
export type StyleCategory = (typeof STYLE_CATEGORIES)[number];

export type WritingStyle = "formal" | "casual" | "veryCasual" | "excited";

export type Styles = Record<StyleCategory, WritingStyle>;

export type AppCategory =
  | "personalMessaging"
  | "workMessaging"
  | "email"
  | "code"
  | "documents"
  | "browser"
  | "other";

export interface AppInfo {
  category: AppCategory;
  /** macOS bundle id, Windows exe name, or Linux WM_CLASS. */
  id: string | null;
  name: string;
}

export interface DictionaryEntry {
  createdAt: Millis;
  id: string;
  misspelling: string | null;
  word: string;
}

export interface DictionaryEntryInput {
  id: string | null;
  misspelling: string | null;
  word: string;
}

export interface Snippet {
  createdAt: Millis;
  expansion: string;
  id: string;
  trigger: string;
}

export interface SnippetInput {
  expansion: string;
  id: string | null;
  trigger: string;
}

// ── settings (tw_core::settings) ─────────────────────────────────────────

export type DictationMode = "hold" | "handsFree";
export type InsertionMethod = "paste" | "type";
export type Theme = "system" | "light" | "dark";
export type HistoryRetention = "forever" | "thirtyDays" | "oneDay" | "never";

export const ONBOARDING_STEPS = [
  "welcome",
  "permissions",
  "microphone",
  "shortcut",
  "voiceModel",
  "polish",
  "tryIt",
] as const;
export type OnboardingStep = (typeof ONBOARDING_STEPS)[number];

export interface Languages {
  autoDetect: boolean;
  /** ISO 639-1 codes. */
  selected: string[];
}

export interface Settings {
  /** Look for a new version once a day. Off by default. */
  checkForUpdates: boolean;
  cleanupLevel: CleanupLevel;
  dictationMode: DictationMode;
  historyRetention: HistoryRetention;
  insertionMethod: InsertionMethod;
  languages: Languages;
  launchAtLogin: boolean;
  llmModel: string | null;
  /** Input device id; `null` follows the system default. */
  microphone: string | null;
  onboardingCompleted: boolean;
  onboardingStep: OnboardingStep;
  pillHiddenUntil: Millis | null;
  restoreClipboard: boolean;
  shortcuts: Shortcuts;
  showInDock: boolean;
  showPillAlways: boolean;
  sounds: boolean;
  sttModel: string | null;
  styles: Styles;
  theme: Theme;
}

/** Absent keys are left alone; `null` clears a nullable setting. */
export type SettingsPatch = Partial<Settings>;

// ── dictation (tw_core::dictation) ───────────────────────────────────────

export type DictationPhase = "idle" | "listening" | "processing" | "notice";
export type ProcessingStage = "transcribing" | "polishing";

export type NoticeKind =
  | "cancelled"
  | "pasteFailed"
  | "noSpeech"
  | "noMicrophone"
  | "stillProcessing"
  | "noModel"
  | "timeLimitWarning"
  | "error";

export type NoticeActionKind =
  | "undo"
  | "pasteLast"
  | "copyLast"
  | "openSettings";

export interface NoticeAction {
  kind: NoticeActionKind;
  label: string;
}

export interface Notice {
  action: NoticeAction | null;
  kind: NoticeKind;
  text: string;
}

export interface DictationState {
  elapsedMs: number;
  mode: DictationMode | null;
  notice: Notice | null;
  phase: DictationPhase;
  stage: ProcessingStage | null;
}

// ── models (tw_core::models) ─────────────────────────────────────────────

export type ModelKind = "voice" | "language";

export type ModelStatus =
  | { state: "notDownloaded" }
  | { state: "downloading"; received: number; total: number }
  | { state: "paused"; received: number; total: number }
  | { state: "downloaded" }
  | { state: "failed"; message: string };

/** How far a model file was compressed from the original. */
export type Compression = "none" | "light" | "strong";

export interface ModelInfo {
  /** 1 (rough) to 5 (accurate). */
  accuracy: number;
  compression: Compression;
  contextLength: number | null;
  custom: boolean;
  /** One plain sentence for people who don't know the model. */
  description: string;
  englishOnly: boolean;
  family: string;
  file: string;
  id: string;
  kind: ModelKind;
  label: string;
  ramMb: number;
  reasoning: boolean;
  recommended: boolean;
  sha256: string | null;
  sizeBytes: number;
  /** 1 (slow) to 5 (fast). */
  speed: number;
  status: ModelStatus;
  url: string | null;
  /** The model this is a compressed copy of. */
  variantOf: string | null;
}

export type DownloadState =
  | "downloading"
  | "paused"
  | "done"
  | "failed"
  | "cancelled";

export interface ModelProgress {
  id: string;
  received: number;
  state: DownloadState;
  total: number;
}

export interface HardwareInfo {
  appleSilicon: boolean;
  arch: string;
  cpuCores: number;
  /** For example "Apple M3 Max"; null when the OS doesn't say. */
  cpuName: string | null;
  platform: Platform;
  totalMemoryBytes: number;
}

/** One recommended model and why, in words the UI can show as is. */
export interface RecommendationPick {
  id: string;
  reason: string;
}

/** The models picked for this computer (`tw_core::models::Recommendation`). */
export interface Recommendation {
  language: RecommendationPick | null;
  voice: RecommendationPick | null;
}

// ── history (tw_core::history) ───────────────────────────────────────────

export interface HistoryEntry {
  app: AppInfo | null;
  createdAt: Millis;
  durationMs: number;
  finalText: string;
  id: string;
  language: string | null;
  rawText: string;
  wordCount: number;
}

export interface HistoryPage {
  entries: HistoryEntry[];
  nextCursor: string | null;
}

export interface DailyWords {
  /** Local date, `YYYY-MM-DD`. */
  date: string;
  words: number;
}

export interface AppUsage {
  dictations: number;
  name: string;
  words: number;
}

export interface Stats {
  /** Words per local day for the last 182 days, oldest first, ending today. */
  daily: DailyWords[];
  /** Fillers removed and dictionary corrections. */
  fixesMade: number;
  longestStreakDays: number;
  streakDays: number;
  /** Most used apps by dictation count, at most 5. */
  topApps: AppUsage[];
  totalDictations: number;
  totalWords: number;
  wordsPerMinute: number;
}

// ── permissions (tw_core::permissions) ───────────────────────────────────

export type PermissionKind = "microphone" | "accessibility" | "inputMonitoring";

export type PermissionStatus =
  | "granted"
  | "denied"
  | "notDetermined"
  | "notApplicable";

export type Permissions = Record<PermissionKind, PermissionStatus>;

export interface MicrophoneInfo {
  id: string;
  isDefault: boolean;
  name: string;
}

// ── app (src-tauri) ──────────────────────────────────────────────────────

export const MAIN_ROUTES = [
  "home",
  "dictionary",
  "snippets",
  "style",
  "models",
  "settings",
] as const;
export type MainRoute = (typeof MAIN_ROUTES)[number];

export type ErrorKind =
  | "notImplemented"
  | "invalidInput"
  | "notFound"
  | "permissionMissing"
  /** `start_mic_test`: no input device. */
  | "noMicrophone"
  /** `start_mic_test`: another app holds the microphone. */
  | "microphoneBusy"
  /** `start_mic_test`: microphone access is turned off. */
  | "permissionDenied"
  | "io"
  | "internal";

/** What every failing command rejects with. */
export interface CommandError {
  kind: ErrorKind;
  message: string;
}

export interface AudioLevel {
  /** Smoothed RMS, 0..1. */
  level: number;
}

export interface HotkeyCapture {
  /** All keys were released: this is the final combination. */
  done: boolean;
  hotkey: Hotkey;
}

export interface Navigate {
  route: MainRoute;
}

export type ListenerState =
  | "starting"
  /** macOS: Accessibility (or Input Monitoring) is not granted yet. */
  | "waitingForPermission"
  | "running"
  /** macOS Secure Event Input is on: shortcuts don't arrive meanwhile. */
  | "secureInput"
  /** No global shortcuts here (Wayland). */
  | "unsupported"
  | "failed";

/** Payload of `listener:status` and result of `get_listener_status`. */
export interface ListenerStatus {
  /**
   * False while the listener runs on Input Monitoring alone: shortcuts with a
   * key still reach the focused app.
   */
  canSwallow: boolean;
  state: ListenerState;
}

export type RequestResult = "completed" | "cancelled" | "restarted" | "failed";

/** One request the app made: a model download or an update check. */
export interface NetworkRequest {
  bytes: number;
  finishedAt: number;
  /** Set when the request failed. */
  message: string | null;
  result: RequestResult;
  startedAt: number;
  url: string;
}

/** A rectangle in CSS pixels, relative to the window's top-left corner. */
export interface PillRect {
  height: number;
  width: number;
  x: number;
  y: number;
}

// ── file transcription (src-tauri/src/files.rs) ──────────────────────────

export type FileStage =
  | "decoding"
  | "transcribing"
  | "polishing"
  | "done"
  | "failed"
  | "cancelled";

export type FileErrorKind =
  | "noModel"
  | "noAudioTrack"
  | "noSpeech"
  | "unsupported"
  | "corrupt"
  | "empty"
  | "tooLong"
  | "unreadable"
  | "failed";

/** `message` is written to be shown as is. */
export interface FileError {
  kind: FileErrorKind;
  message: string;
}

/** Payload of `files:progress`, and the running job's latest state. */
export interface FileProgress {
  /** Length of the audio, once decoded. */
  durationMs: number | null;
  error: FileError | null;
  fileName: string;
  /** 0..1 while transcribing or polishing. */
  fraction: number | null;
  jobId: string;
  stage: FileStage;
  /** The transcript, when done. */
  text: string | null;
  wordCount: number | null;
}

// ── updates (tw_core::updates) ───────────────────────────────────────────

/** A newer version on the update server. */
export interface Release {
  /** What changed, as Markdown. */
  notes: string | null;
  publishedAt: Millis | null;
  version: string;
}

/** Payload of `updates:state` and result of `get_update_state`. */
export type UpdateState =
  /** Nothing checked since launch. */
  | { state: "idle" }
  | { state: "checking" }
  | { checkedAt: Millis; state: "upToDate" }
  | { release: Release; state: "available" }
  | {
      received: number;
      release: Release;
      state: "downloading";
      total: number | null;
    }
  /** Downloaded and verified. Installed when the app restarts. */
  | { release: Release; state: "ready" }
  /** Replacing the app, then restarting. */
  | { release: Release; state: "installing" }
  /** `release` is set when a download failed, so it can be retried. */
  | { message: string; release: Release | null; state: "failed" };

// ── command and event maps ───────────────────────────────────────────────

/** Every command: its arguments (camelCase, as Tauri expects) and result. */
export interface Commands {
  cancel_dictation: { args: undefined; result: null };
  cancel_download: { args: { id: string }; result: null };
  cancel_file_transcription: { args: { jobId: string }; result: null };
  /** Starts a check; the result arrives as `updates:state`. */
  check_for_updates: { args: undefined; result: null };
  clear_history: { args: undefined; result: null };
  complete_onboarding: { args: undefined; result: Settings };
  copy_last_transcript: { args: undefined; result: null };
  delete_dictionary_entry: { args: { id: string }; result: null };
  delete_history_entry: { args: { id: string }; result: null };
  delete_model: { args: { id: string }; result: null };
  delete_snippet: { args: { id: string }; result: null };
  download_model: { args: { id: string }; result: null };
  /** Downloads the release the last check found. */
  download_update: { args: undefined; result: null };
  /** What reset_shortcuts restores; development builds have their own. */
  get_default_shortcuts: { args: undefined; result: Shortcuts };
  get_dictation_state: { args: undefined; result: DictationState };
  /** The running file job's latest progress; null when none runs. */
  get_file_transcription: { args: undefined; result: FileProgress | null };
  get_hardware_info: { args: undefined; result: HardwareInfo };
  get_listener_status: { args: undefined; result: ListenerStatus };
  get_permissions: { args: undefined; result: Permissions };
  get_recommendation: { args: undefined; result: Recommendation };
  get_settings: { args: undefined; result: Settings };
  get_stats: { args: undefined; result: Stats };
  get_update_state: { args: undefined; result: UpdateState };
  hide_pill_for: { args: { minutes: number }; result: Settings };
  import_model: {
    args: { kind: ModelKind; path: string };
    result: ModelInfo;
  };
  /** Installs the downloaded update and restarts the app. */
  install_update: { args: undefined; result: null };
  list_dictionary: { args: undefined; result: DictionaryEntry[] };
  list_history: {
    args: { cursor: string | null; query: string | null };
    result: HistoryPage;
  };
  list_microphones: { args: undefined; result: MicrophoneInfo[] };
  list_models: { args: undefined; result: ModelInfo[] };
  /** Oldest first, as recorded. */
  list_network_log: { args: undefined; result: NetworkRequest[] };
  list_snippets: { args: undefined; result: Snippet[] };
  open_main_window: { args: { route: MainRoute | null }; result: null };
  open_permission_settings: {
    args: { kind: PermissionKind };
    result: null;
  };
  paste_last_transcript: { args: undefined; result: null };
  request_permission: { args: { kind: PermissionKind }; result: null };
  reset_shortcuts: { args: undefined; result: Settings };
  /** Opens a save dialog; false when the person cancels it. */
  save_file_transcript: {
    args: { fileName: string; text: string };
    result: boolean;
  };
  /** The pill's interactive areas; clicks anywhere else pass through. */
  set_pill_hit_regions: { args: { rects: PillRect[] }; result: null };
  start_dictation: { args: { mode: DictationMode }; result: null };
  start_hotkey_capture: { args: undefined; result: null };
  start_mic_test: { args: undefined; result: null };
  stop_dictation: { args: undefined; result: null };
  stop_hotkey_capture: { args: undefined; result: null };
  stop_mic_test: { args: undefined; result: null };
  test_polish: {
    args: { category: StyleCategory | null; text: string };
    result: string;
  };
  /** Starts a file job; progress arrives as `files:progress`. */
  transcribe_file: { args: { path: string }; result: string };
  undo_cancel: { args: undefined; result: null };
  update_settings: { args: { patch: SettingsPatch }; result: Settings };
  upsert_dictionary_entry: {
    args: { entry: DictionaryEntryInput };
    result: DictionaryEntry;
  };
  upsert_snippet: { args: { snippet: SnippetInput }; result: Snippet };
  validate_hotkey: {
    args: { action: HotkeyAction; hotkey: Hotkey };
    result: HotkeyIssue | null;
  };
}

export type CommandName = keyof Commands;
export type CommandArgs<C extends CommandName> = Commands[C]["args"];
export type CommandResult<C extends CommandName> = Commands[C]["result"];

/** Every event and its payload. */
export interface Events {
  "audio:level": AudioLevel;
  "dictation:state": DictationState;
  "files:progress": FileProgress;
  "history:changed": null;
  "hotkey:captured": HotkeyCapture;
  "listener:status": ListenerStatus;
  "models:changed": null;
  "models:progress": ModelProgress;
  "permissions:changed": Permissions;
  "settings:changed": Settings;
  "updates:state": UpdateState;
  /** Main window only. */
  "window:navigate": Navigate;
}

export type EventName = keyof Events;

export type Unsubscribe = () => void;

/** What `tauri.ts` and `mock.ts` both implement. */
export interface Backend {
  invoke: <C extends CommandName>(
    command: C,
    args: CommandArgs<C>
  ) => Promise<CommandResult<C>>;
  readonly kind: "tauri" | "mock";
  listen: <E extends EventName>(
    event: E,
    handler: (payload: Events[E]) => void
  ) => Unsubscribe;
}

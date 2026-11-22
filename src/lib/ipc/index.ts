import type {
  Backend,
  DictationMode,
  DictionaryEntryInput,
  EventName,
  Events,
  Hotkey,
  HotkeyAction,
  MainRoute,
  ModelKind,
  PermissionKind,
  PillRect,
  SettingsPatch,
  SnippetInput,
  StyleCategory,
  Unsubscribe,
} from "./types";

import { createMockBackend } from "./mock";
import { createTauriBackend } from "./tauri";

export type * from "./types";

/** True inside the Tauri app; false in a plain browser (`pnpm dev`). */
export const isTauri =
  typeof window !== "undefined" && window.__TAURI_INTERNALS__ !== undefined;

export const backend: Backend = isTauri
  ? createTauriBackend()
  : createMockBackend();

// ── commands ─────────────────────────────────────────────────────────────

// settings
export const getSettings = () => backend.invoke("get_settings", undefined);
export const updateSettings = (patch: SettingsPatch) =>
  backend.invoke("update_settings", { patch });
export const resetShortcuts = () =>
  backend.invoke("reset_shortcuts", undefined);

// hotkeys
export const validateHotkey = (action: HotkeyAction, hotkey: Hotkey) =>
  backend.invoke("validate_hotkey", { action, hotkey });
export const startHotkeyCapture = () =>
  backend.invoke("start_hotkey_capture", undefined);
export const stopHotkeyCapture = () =>
  backend.invoke("stop_hotkey_capture", undefined);
/** Whether the global shortcut listener runs, and why not. */
export const getListenerStatus = () =>
  backend.invoke("get_listener_status", undefined);

// permissions
export const getPermissions = () =>
  backend.invoke("get_permissions", undefined);
export const requestPermission = (kind: PermissionKind) =>
  backend.invoke("request_permission", { kind });
export const openPermissionSettings = (kind: PermissionKind) =>
  backend.invoke("open_permission_settings", { kind });

// microphone
export const listMicrophones = () =>
  backend.invoke("list_microphones", undefined);
export const startMicTest = () => backend.invoke("start_mic_test", undefined);
export const stopMicTest = () => backend.invoke("stop_mic_test", undefined);

// models
export const listModels = () => backend.invoke("list_models", undefined);
export const downloadModel = (id: string) =>
  backend.invoke("download_model", { id });
export const cancelDownload = (id: string) =>
  backend.invoke("cancel_download", { id });
export const deleteModel = (id: string) =>
  backend.invoke("delete_model", { id });
export const importModel = (path: string, kind: ModelKind) =>
  backend.invoke("import_model", { kind, path });
/**
 * Opens the system file picker for a model file of `kind`; `null` when the
 * person cancels. Only inside the app: the browser mock has no file picker.
 */
export async function pickModelFile(kind: ModelKind): Promise<string | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  return open({
    directory: false,
    filters: [
      kind === "voice"
        ? { extensions: ["bin"], name: "Voice model" }
        : { extensions: ["gguf"], name: "Language model" },
    ],
    multiple: false,
    title: "Import a model file",
  });
}
export const getHardwareInfo = () =>
  backend.invoke("get_hardware_info", undefined);
export const getRecommendation = () =>
  backend.invoke("get_recommendation", undefined);

// dictation
export const startDictation = (mode: DictationMode) =>
  backend.invoke("start_dictation", { mode });
export const stopDictation = () => backend.invoke("stop_dictation", undefined);
export const cancelDictation = () =>
  backend.invoke("cancel_dictation", undefined);
export const undoCancel = () => backend.invoke("undo_cancel", undefined);
export const pasteLastTranscript = () =>
  backend.invoke("paste_last_transcript", undefined);
export const copyLastTranscript = () =>
  backend.invoke("copy_last_transcript", undefined);
export const getDictationState = () =>
  backend.invoke("get_dictation_state", undefined);

// file transcription
/** What `transcribe_file` accepts (`tw_audio::decode::SUPPORTED_EXTENSIONS`). */
export const AUDIO_FILE_EXTENSIONS = [
  "wav",
  "mp3",
  "m4a",
  "aac",
  "mp4",
  "mov",
  "flac",
  "ogg",
  "oga",
] as const;
export const transcribeFile = (path: string) =>
  backend.invoke("transcribe_file", { path });
export const cancelFileTranscription = (jobId: string) =>
  backend.invoke("cancel_file_transcription", { jobId });
export const getFileTranscription = () =>
  backend.invoke("get_file_transcription", undefined);
export const saveFileTranscript = (text: string, fileName: string) =>
  backend.invoke("save_file_transcript", { fileName, text });
/**
 * Opens the system file picker for an audio or video file; `null` when the
 * person cancels. Only inside the app: the browser mock has no file picker.
 */
export async function pickAudioFile(): Promise<string | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  return open({
    directory: false,
    filters: [{ extensions: [...AUDIO_FILE_EXTENSIONS], name: "Audio" }],
    multiple: false,
    title: "Transcribe a file",
  });
}

// history
export const listHistory = (
  query: string | null = null,
  cursor: string | null = null
) => backend.invoke("list_history", { cursor, query });
export const deleteHistoryEntry = (id: string) =>
  backend.invoke("delete_history_entry", { id });
export const clearHistory = () => backend.invoke("clear_history", undefined);
export const getStats = () => backend.invoke("get_stats", undefined);

// dictionary and snippets
export const listDictionary = () =>
  backend.invoke("list_dictionary", undefined);
export const upsertDictionaryEntry = (entry: DictionaryEntryInput) =>
  backend.invoke("upsert_dictionary_entry", { entry });
export const deleteDictionaryEntry = (id: string) =>
  backend.invoke("delete_dictionary_entry", { id });
export const listSnippets = () => backend.invoke("list_snippets", undefined);
export const upsertSnippet = (snippet: SnippetInput) =>
  backend.invoke("upsert_snippet", { snippet });
export const deleteSnippet = (id: string) =>
  backend.invoke("delete_snippet", { id });
export const testPolish = (
  text: string,
  category: StyleCategory | null = null
) => backend.invoke("test_polish", { category, text });

// app and windows
export const completeOnboarding = () =>
  backend.invoke("complete_onboarding", undefined);
export const openMainWindow = (route: MainRoute | null = null) =>
  backend.invoke("open_main_window", { route });
export const hidePillFor = (minutes: number) =>
  backend.invoke("hide_pill_for", { minutes });
export const setPillHitRegions = (rects: PillRect[]) =>
  backend.invoke("set_pill_hit_regions", { rects });
/** Every request the app has made, oldest first. */
export const listNetworkLog = () =>
  backend.invoke("list_network_log", undefined);

// ── events ───────────────────────────────────────────────────────────────

/** Subscribes to an event; returns the unsubscribe function. */
export function on<E extends EventName>(
  event: E,
  handler: (payload: Events[E]) => void
): Unsubscribe {
  return backend.listen(event, handler);
}

type Handler<E extends EventName> = (payload: Events[E]) => void;

export const onDictationState = (handler: Handler<"dictation:state">) =>
  on("dictation:state", handler);
export const onAudioLevel = (handler: Handler<"audio:level">) =>
  on("audio:level", handler);
export const onModelsProgress = (handler: Handler<"models:progress">) =>
  on("models:progress", handler);
export const onModelsChanged = (handler: Handler<"models:changed">) =>
  on("models:changed", handler);
export const onSettingsChanged = (handler: Handler<"settings:changed">) =>
  on("settings:changed", handler);
export const onPermissionsChanged = (handler: Handler<"permissions:changed">) =>
  on("permissions:changed", handler);
export const onHistoryChanged = (handler: Handler<"history:changed">) =>
  on("history:changed", handler);
export const onFilesProgress = (handler: Handler<"files:progress">) =>
  on("files:progress", handler);
export const onHotkeyCaptured = (handler: Handler<"hotkey:captured">) =>
  on("hotkey:captured", handler);
export const onListenerStatus = (handler: Handler<"listener:status">) =>
  on("listener:status", handler);
export const onNavigate = (handler: Handler<"window:navigate">) =>
  on("window:navigate", handler);

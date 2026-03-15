import { invoke } from "@tauri-apps/api/core";
import type { Session, SessionDetail, ModelConfig, CalendarEvent } from "../types";

export const ipc = {
  startSession: (title?: string, attendees?: string[]): Promise<Session> =>
    invoke("start_session", { title: title ?? null, attendees: attendees ?? null }),

  stopSession: (id: string): Promise<Session> =>
    invoke("stop_session", { id }),

  listSessions: (): Promise<Session[]> =>
    invoke("list_sessions"),

  getSession: (id: string): Promise<SessionDetail> =>
    invoke("get_session", { id }),

  deleteSession: (id: string): Promise<void> =>
    invoke("delete_session", { id }),

  exportSession: (id: string, destDir: string): Promise<string> =>
    invoke("export_session", { id, destDir }),

  updateTranscriptChunk: (id: string, text: string): Promise<void> =>
    invoke("update_transcript_chunk", { id, text }),

  updateSessionNotes: (id: string, notes: string): Promise<void> =>
    invoke("update_session_notes", { id, notes }),

  getModelConfig: (): Promise<ModelConfig> =>
    invoke("get_model_config"),

  setModelConfig: (config: ModelConfig): Promise<void> =>
    invoke("set_model_config", { config }),

  renameSession: (id: string, title: string): Promise<void> =>
    invoke("rename_session", { id, title }),

  /** Open a native file picker filtered to .bin files. Returns the selected path or null. */
  pickModelFile: (): Promise<string | null> =>
    invoke("pick_model_file"),

  checkModelDownloaded: (modelId: string): Promise<boolean> =>
    invoke("check_model_downloaded", { modelId }),

  /** Download a Whisper model file from HuggingFace into the app cache dir.
   *  Emits `model:download_progress { pct: number }` events during download.
   *  Resolves with the local file path when done. */
  downloadModel: (modelId: string): Promise<string> =>
    invoke("download_model", { modelId }),

  listUpcomingEvents: (): Promise<CalendarEvent[]> =>
    invoke("list_upcoming_events"),

  openSystemSettings: (pane: string): Promise<void> =>
    invoke("open_system_settings", { pane }),

  testProviderConnection: (): Promise<string> =>
    invoke("test_provider_connection"),

  getRtIntervalSecs: (): Promise<number> =>
    invoke("get_rt_interval_secs"),
};

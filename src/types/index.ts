export type SessionStatus = "recording" | "processing" | "done" | "error";

export interface Session {
  id: string;
  title: string;
  startedAt: string;
  endedAt: string | null;
  status: SessionStatus;
  durationSecs: number | null;
  errorMessage: string | null;
}

export interface TranscriptChunk {
  id: string;
  sessionId: string;
  speakerLabel: string | null;
  timestampSecs: number;
  text: string;
}

export interface SessionDetail extends Session {
  transcript: TranscriptChunk[];
  notes: string | null;
}

export interface CalendarEvent {
  title: string;
  startDate: string;
  endDate: string;
  meetingUrl: string | null;
  attendees: string[];
  calendarName: string;
}

export type ModelProviderKind =
  | "local_file"
  | "hf_download"
  | "ollama"
  | "openai"
  | "anthropic"
  | "gemini";

export type CaptureMode = "mic_only" | "mic_and_system";

export interface ModelConfig {
  captureMode: CaptureMode;
  transcription: TranscriptionConfig;
  summarization: SummarizationConfig;
  /** Tauri shortcut string, e.g. "CmdOrCtrl+Shift+R". Null = default. */
  hotkeyToggle: string | null;
}

export interface TranscriptionConfig {
  provider: "local_file" | "hf_download" | "whisper_api";
  modelPath: string | null;
  modelId: string | null;
  apiKey: string | null;
  /** ISO-639-1 code ("en", "ja", "zh", …) or null for auto-detect */
  language: string | null;
}

/** Languages supported by Whisper, shown in the Settings dropdown */
export const WHISPER_LANGUAGES = [
  { code: "auto", label: "Auto-detect" },
  { code: "en", label: "English" },
  { code: "ja", label: "Japanese" },
  { code: "zh", label: "Chinese" },
  { code: "ko", label: "Korean" },
  { code: "es", label: "Spanish" },
  { code: "fr", label: "French" },
  { code: "de", label: "German" },
  { code: "pt", label: "Portuguese" },
  { code: "it", label: "Italian" },
  { code: "ru", label: "Russian" },
  { code: "ar", label: "Arabic" },
  { code: "hi", label: "Hindi" },
  { code: "nl", label: "Dutch" },
  { code: "sv", label: "Swedish" },
  { code: "pl", label: "Polish" },
  { code: "th", label: "Thai" },
  { code: "vi", label: "Vietnamese" },
  { code: "id", label: "Indonesian" },
  { code: "tr", label: "Turkish" },
] as const;

export interface SummarizationConfig {
  enabled: boolean;
  provider: ModelProviderKind;
  baseUrl: string | null;
  apiKey: string | null;
  model: string | null;
  prompt: string | null;
}

// Known Whisper models available on HuggingFace (ggerganov/whisper.cpp)
// Multilingual models support Chinese, Japanese, Spanish, French, etc.
// English-only (.en) models are ~10% faster for English but useless for other languages.
export const WHISPER_MODELS = [
  { id: "ggml-base.bin",      label: "Base · Multilingual — 142 MB (recommended)", size: 142  },
  { id: "ggml-small.bin",     label: "Small · Multilingual — 466 MB",               size: 466  },
  { id: "ggml-medium.bin",    label: "Medium · Multilingual — 1.5 GB",              size: 1500 },
  { id: "ggml-large-v3.bin",  label: "Large v3 · Multilingual — 2.9 GB (best)",    size: 2900 },
  { id: "ggml-tiny.bin",      label: "Tiny · Multilingual — 75 MB (fastest)",       size: 75   },
  { id: "ggml-base.en.bin",   label: "Base · English-only — 142 MB",                size: 142  },
  { id: "ggml-small.en.bin",  label: "Small · English-only — 466 MB",               size: 466  },
  { id: "ggml-medium.en.bin", label: "Medium · English-only — 1.5 GB",              size: 1500 },
  { id: "ggml-tiny.en.bin",   label: "Tiny · English-only — 75 MB",                 size: 75   },
] as const;

// Preset model names per summarization provider
export const PROVIDER_MODELS: Record<string, string[]> = {
  ollama:    ["llama3.2", "llama3.1", "llama3.3", "mistral", "qwen2.5", "gemma3", "phi4"],
  openai:    ["gpt-4o", "gpt-4o-mini", "gpt-4-turbo", "o1-mini", "o3-mini"],
  anthropic: ["claude-opus-4-6", "claude-sonnet-4-6", "claude-haiku-4-5-20251001"],
  gemini:    ["gemini-2.0-flash", "gemini-1.5-pro", "gemini-1.5-flash"],
  local_file:[],
};

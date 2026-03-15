# Aura — Project Context

## Project Overview & Architecture
Privacy-first, botless meeting/audio assistant. Captures audio, transcribes locally (whisper.cpp), and summarizes (pluggable LLMs). See `ROADMAP.md` for upcoming features.
- **Monolithic Tauri**: React + TS frontend (narrow 380×600 tray-window), Rust backend.
- **Strict IPC**: UI ↔ backend communication *only* via Tauri `invoke()` (`src/lib/ipc.ts`). Never bypass.

## Directory Layout
```
src/                  # React UI. Constants must sync with Rust.
  lib/ipc.ts          # Tauri invoke wrappers.
  test/setup.ts       # Global Tauri API mocks.
src-tauri/src/        # Rust backend.
  audio/              # cpal/ScreenCaptureKit + VAD.
  transcribe/         # whisper-rs orchestration.
  models/             # Provider plugin architectures (LLMs, Transcribers).
  session/            # SQLite operations + types.
  commands/           # Tauri invoke handlers (pub async fn).
```

## Key Decisions & Development Rules
- **Testing is the Gatekeeper**: Run `npm run test:all` before ANY build/ship. 
  - `npm run test` (Frontend Vitest) | `npm run test:rust` (Rust cargo test).
  - Tauri APIs are strictly mocked in frontend tests (`vi.mocked(invoke)`).
- **No panics**: Rust `commands` must return `Result<T, String>`. Never `unwrap()`. Use `friendly_error` mapping.
- **Schema Migrations**: SQLite at `~/Library/Application Support/app.aura.aura/aura.db`. Versioned via `schema_version`. Add new `migrate_vN()` in `storage/mod.rs` for schema changes.
- **Constants source of truth**: `RT_INTERVAL_SECS` lives only in `src-tauri/src/commands/recorder.rs`; frontend reads it at runtime via `ipc.getRtIntervalSecs()`. `src/constants.ts` is intentionally empty.
- **Event Bus**: Backend emits `session:updated`, `model:download_progress`, `processing:step`.
- **RT Pipeline**: Two parallel tasks per session — `spawn_vad_drain` (polls audio every 100 ms, emits sentences via channel) + `spawn_rt_loop` (receives sentences, transcribes). No fixed sleep timer.
- **WhisperContext caching**: Model is loaded once per process and reused across all RT passes (`WHISPER_CACHE` in `models/local.rs`). Changing the model path triggers a reload.

## Workflow Cheatsheets

### Adding an LLM Provider
1. Create `src-tauri/src/models/<name>.rs` impl `SummarizationProvider`.
2. Add to `models/mod.rs` (pub use, enum variant, `build_summarization_provider` match arm).
3. Add to `<Settings />` (frontend) & `ModelProviderKind` (`src/types/index.ts`).

### Adding a Backend Command
1. Add handler in `commands/mod.rs` (`Result<T, String>`).
2. Register in `lib.rs -> invoke_handler![]`.
3. Add TS wrapper in `src/lib/ipc.ts`.
4. Add corresponding Rust unit test; run `npm run test:all`.

# Aura

Privacy-first meeting assistant for macOS. Lives in your menu bar, captures mic and system audio, transcribes locally via Whisper, and summarizes with the LLM of your choice — no bots, no cloud uploads, no subscription.

## Features

- **No-bot capture** — records directly from mic and system audio loopback; nothing joins your call
- **Local transcription** — runs Whisper on-device via whisper.cpp; audio never leaves your machine
- **Pluggable LLMs** — summarize with Ollama (local), OpenAI, Anthropic, or Gemini
- **Real-time transcript** — sentence-boundary triggered, typically appears within 1–2 s of finishing a sentence
- **Menu bar app** — lives in the tray, pops up on click, hides on blur

## Requirements

- macOS 12.3+ (Sonoma recommended)
- For system audio capture: Screen Recording permission (macOS requirement for any loopback)
- A transcription source — one of:
  - OpenAI Whisper API key (easiest setup)
  - Downloaded local model (Base multilingual is 142 MB, works offline)
  - Custom `.bin` / `.gguf` model file

## Installation

Download the latest `Aura_*.dmg` (Universal Binary) from [Releases](../../releases), open it, drag Aura to Applications.

**First launch:** right-click Aura → Open → Open to bypass the Gatekeeper prompt (one-time).

On first launch, click the tray icon → **Settings** to configure a transcription model.

## Building from source

**Prerequisites:** Rust, Node.js 18+, CMake (required by whisper.cpp)

```bash
# Install CMake if needed
brew install cmake

# Install JS dependencies
npm install

# Dev mode (hot-reload frontend, Rust rebuilt on change)
npm run dev

# Production build → outputs .app + .dmg in src-tauri/target/release/bundle/
npm run build
```

## Development

```bash
npm run test          # Frontend tests (Vitest)
npm run test:rust     # Rust unit tests
npm run test:all      # Both — run before every build

npm run test:watch    # Frontend tests in watch mode
```

See [`CLAUDE.md`](CLAUDE.md) for architecture details, adding new providers, and coding conventions.

## How it works

```
Mic (cpal) ──┐
             ├── mix to 16kHz mono ── Whisper ── transcript chunks ── SQLite
Loopback ───┘                                                              │
(ScreenCaptureKit)                                               LLM summary (Ollama / API)
```

A VAD (voice activity detection) loop detects sentence boundaries and dispatches each sentence to Whisper immediately (~1–2 s latency). On stop, remaining audio is transcribed and the full transcript is sent to the configured LLM for a summary. Raw audio is never stored.

## Transcription models

| Model | Size | Languages | Speed |
|---|---|---|---|
| Base (recommended) | 142 MB | Multilingual | Fast |
| Small | 466 MB | Multilingual | Good |
| Medium | 1.5 GB | Multilingual | Better |
| Large v3 | 2.9 GB | Multilingual | Best |
| Tiny | 75 MB | Multilingual | Fastest |

English-only variants (`.en`) are ~10% faster but cannot transcribe other languages.

## Summarization providers

| Provider | Setup | Notes |
|---|---|---|
| Ollama | Install [Ollama](https://ollama.ai), pull a model | Fully local, free |
| OpenAI | API key | gpt-4o recommended |
| Anthropic | API key | claude-sonnet-4-6 recommended |
| Gemini | API key | gemini-2.0-flash recommended |
| Custom (OpenAI-compat) | Base URL + optional key | Any OpenAI-compatible endpoint |

## Privacy

- No account required
- No telemetry
- Audio is processed in-memory and discarded immediately after transcription
- Transcripts and notes are stored locally in SQLite at `~/Library/Application Support/app.aura.aura/`
- System audio capture requires Screen Recording permission — macOS mandates this for any app reading the audio loopback; no screen content is ever captured or stored

## License

MIT

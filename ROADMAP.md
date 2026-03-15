# Aura — Roadmap

_Last updated: 2026-03-15_

---

## Current State

Aura is a functional, privacy-first macOS menu bar meeting assistant. Core recording, transcription, and summarization are working and stable.

### Shipped & Working

| Area | Notes |
|------|-------|
| Mic capture (cpal) | |
| System audio loopback (ScreenCaptureKit) | |
| Local Whisper transcription (whisper.cpp) | WhisperContext cached — model loaded once per session |
| **VAD-driven real-time transcription** | Sentence-boundary triggered (~1–2 s latency); 32 s fallback |
| Multi-provider LLM summarization | Ollama, OpenAI, Anthropic, Gemini, LM Studio (OpenAI-compat) |
| LLM output cleanup | Strips `<think>` tags and preamble text before saving notes |
| Apple Calendar integration (EventKit) | Upcoming meetings list; attendees fed to diarization |
| Speaker diarization via LLM | Uses calendar attendee context |
| Glassmorphic menu bar UI | 380×600 tray window |
| Session management | Create, list, view, rename, delete |
| Live transcript editing | Inline double-click to edit chunks |
| **Notes editing** | Click to edit rendered Markdown notes |
| **Markdown notes rendering** | react-markdown with compact styling |
| Custom summarization prompts | Per-provider setting |
| SQLite persistence | Versioned schema migrations |
| Frontend + Rust test suites | `npm run test:all` |
| **CI/CD (GitHub Actions)** | Test on push/PR; Universal DMG release on tag push |
| Memory bounded during recording | pstate drain keeps buffer ≤ 2 s tail |

---

## Phase 1 — Quick Wins
_Small, high-value UX improvements. Each is self-contained._

### 1.1 Copy to Clipboard
One-click copy of transcript or notes. Add copy buttons to the session detail header. Use Tauri's clipboard API.

### 1.2 Global Hotkey
`Cmd+Shift+R` to start/stop recording from anywhere (even when Aura is hidden). Register via Tauri's `GlobalShortcut` plugin. Add a setting to customize the key.

### 1.3 Audio Level Meter
Real-time VU bar during recording. Backend emits `audio:level` events (~10/s) from the VAD drain loop; frontend renders a minimal bar. Gives visual confirmation that audio is being captured.

### 1.4 Session Export Improvements
Currently exports plain Markdown. Add:
- Copy-to-clipboard button (overlaps with 1.1)
- Formatted export with frontmatter (date, duration, attendees)

---

## Phase 2 — Intelligence & Search
_Higher complexity; improves quality and discoverability._

### 2.1 Full-Text Search
SQLite FTS5 virtual table over transcripts and notes. Search bar in the session list. Highlight matching terms in results.

### 2.2 Meeting Notifications + One-Click Record
Background calendar poller (every 60 s). When a meeting starts within 2 minutes, show a notification banner with "Record" action. Clicking "Record" starts a session pre-titled with the event name and attendees loaded.

This is the **killer workflow** feature — zero friction from calendar → transcript → notes.

### 2.3 Silero VAD Upgrade
Replace the current RMS energy threshold VAD with the Silero ONNX model (~2 MB). Significantly better silence detection in noisy environments (fans, background speech, keyboard). Bundle the model file; fall back to energy VAD if ONNX fails.

### 2.4 Session Tags & Filtering
`tags` table + `session_tags` join. Tag chips in the session list for filtering. Auto-suggest tags based on meeting title or attendees.

---

## Phase 3 — Power User Features
_Workflow integration and personalization._

### 3.1 Meeting Templates
Named summarization prompts (e.g., "Standup", "1:1", "Interview"). Selectable when starting a recording or during processing. Stored in SQLite, not just settings.

### 3.2 Speaker Roster
Persistent `speakers` table. Rename "Speaker 1" → "Alice" once, propagate across all sessions. UI accessible from session detail and a dedicated roster view.

### 3.3 Auto-Export to Obsidian / Notes Folder
After summarization, optionally write `{date}-{title}.md` to a configured directory (Obsidian vault, iCloud Notes folder, etc.). Include frontmatter with session metadata.

### 3.4 Session Analytics
Aggregate stats: meetings per week, total recording time, average duration, talk time per speaker. Simple CSS-only bar charts to avoid heavy charting dependencies.

---

## Phase 4 — Infrastructure & Scale

### 4.1 Backup & Restore
Zip/unzip `~/Library/Application Support/app.aura.aura/`. Progress indicator. Schema version validation on restore.

### 4.2 Long Recording Memory Optimization
For recordings > 1 hour: spill older audio chunks to temporary disk files rather than holding them in RAM. Current bounded-pstate approach handles memory during recording; this targets the final post-processing pass on very long sessions.

### 4.3 Code Modularization
Convert to a Cargo workspace. Extract `aura-audio`, `aura-llm`, `aura-session` as independent crates. `src-tauri` becomes a thin Tauri shell. Enables independent testing and reuse.

---

## Not Planned

- Windows / Linux support (ScreenCaptureKit is macOS-only; would need a full audio pipeline rewrite)
- Cloud sync or server component (privacy-first means local-only)
- Mac App Store distribution (sandboxing conflicts with Screen Recording entitlement)

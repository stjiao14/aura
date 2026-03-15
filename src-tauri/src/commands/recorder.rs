use crate::audio;
use crate::commands::models::load_model_config;
use crate::commands::session::{ActiveSessions, ProgressiveState};
use crate::models::ModelConfig;
use crate::session::{self, TranscriptChunk};
use crate::storage;
use crate::summarize;
use crate::transcribe;
use rusqlite::Connection;
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

/// Safely acquire a mutex lock inside the RT loop / post-processing where
/// we cannot use `?` (the outer function is not Result-returning).
/// Panics only on truly poisoned locks with a descriptive message.
fn rt_lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|e| panic!("[aura] RT mutex poisoned: {e}"))
}

pub(crate) const RT_INTERVAL_SECS: u64 = 8;

// ─── Shared helpers ──────────────────────────────────────────────────────────

/// Convert raw transcription output into `TranscriptChunk` structs, filtering
/// out any chunks whose timestamps fall within the overlap region.
/// Also deduplicates consecutive identical/near-identical segments — a common
/// whisper hallucination pattern on trailing silence.
fn build_chunks(
    raw: Vec<(f64, String)>,
    session_id: &str,
    overlap_secs: f64,
    time_offset: f64,
) -> Vec<TranscriptChunk> {
    let mut chunks = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    const MAX_REPEATS: usize = 2; // allow at most 2 identical segments

    for (ts, text) in raw {
        if ts < overlap_secs {
            continue;
        }
        if is_hallucination(&text) {
            continue;
        }

        let lower = text.trim().to_lowercase();
        let count = seen.iter().filter(|s| **s == lower).count();
        if count >= MAX_REPEATS {
            tracing::debug!("Dropping repeated segment ({}x): {}", count + 1, text);
            continue;
        }
        seen.push(lower);

        chunks.push(TranscriptChunk {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            speaker_label: None,
            timestamp_secs: time_offset + (ts - overlap_secs),
            text,
        });
    }
    chunks
}

/// Persist chunks to the database, logging any insertion failures.
/// Returns the number of successfully inserted chunks.
fn persist_chunks(conn: &Connection, chunks: &[TranscriptChunk], session_id: &str) -> usize {
    let mut inserted = 0;
    for chunk in chunks {
        match session::insert_chunk(conn, chunk) {
            Ok(()) => inserted += 1,
            Err(e) => tracing::warn!("Chunk insert failed for {}: {}", session_id, e),
        }
    }
    inserted
}

/// Transcribe audio, diarize, delete the old chunk window, and insert
/// replacements. This is the shared "overlapping window" logic used by
/// both progressive RT processing and final post-processing.
///
/// Opens its own DB connection to avoid holding a `Connection` across await
/// points (rusqlite::Connection is not `Sync`).
/// Shared overlapping-window transcription used by both progressive RT
/// processing and final post-processing.
///
/// When `skip_diarization` is true (during live recording), the LLM-based
/// speaker identification step is skipped so it doesn't block the RT loop.
/// Diarization runs during final post-processing when latency doesn't matter.
async fn transcribe_and_replace_window(
    config: &ModelConfig,
    data_dir: &std::path::Path,
    session_id: &str,
    samples: &[f32],
    overlap_samples: usize,
    start_offset: f64,
    attendee_info: Option<&str>,
    skip_diarization: bool,
) -> anyhow::Result<()> {
    let chunks_raw = transcribe::transcribe(samples, config, data_dir).await?;

    let overlap_secs = overlap_samples as f64 / 16_000.0;
    let duration_secs = (samples.len() - overlap_samples) as f64 / 16_000.0;
    let to_secs = start_offset + duration_secs;

    let mut chunks = build_chunks(chunks_raw, session_id, overlap_secs, start_offset);

    if !skip_diarization {
        if let Err(e) =
            crate::transcribe::diarize::diarize_chunks(&mut chunks, attendee_info, config).await
        {
            tracing::warn!("Diarization failed for {}: {}", session_id, e);
        }
    }

    // DB writes happen after all async work is done — open a fresh connection
    let conn = storage::open(data_dir)?;
    session::delete_chunks_in_range(&conn, session_id, start_offset, to_secs)?;
    persist_chunks(&conn, &chunks, session_id);
    Ok(())
}

// ─── VAD-driven drain task ────────────────────────────────────────────────────

/// Run VAD on f32 samples (16 kHz mono). Returns `true` if speech is detected.
fn has_speech(samples: &[f32]) -> bool {
    let pcm_i16: Vec<i16> = samples.iter().map(|&s| (s * 32767.0) as i16).collect();
    audio::vad::is_speech(&pcm_i16, audio::vad::DEFAULT_RMS_THRESHOLD)
}

/// Drain audio from the active capture handle.
/// Returns `None` if the session is no longer active (signals loop exit).
fn drain_audio(app: &AppHandle, session_id: &str) -> Option<Vec<f32>> {
    let map = app.state::<ActiveSessions>();
    let guard = rt_lock(&map.0);
    guard.get(session_id).map(|active| active.handle.drain())
}

/// Polls audio every 100 ms, detects sentence boundaries via VAD, and sends
/// complete sentences over `tx` for the RT loop to transcribe.
///
/// Sentence end = ≥ 0.5 s of silence after ≥ 0.3 s of speech.
/// Safety valve: emits after 25 s even without a silence boundary.
pub(crate) fn spawn_vad_drain(
    app: AppHandle,
    _data_dir: std::path::PathBuf,
    session_id: String,
    tx: tokio::sync::mpsc::UnboundedSender<Vec<f32>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // 100 ms poll — fine-grained enough to detect ~0.5 s pauses.
        const POLL_MS: u64 = 100;
        // Number of consecutive silent polls required to trigger emit.
        const SILENCE_POLLS: u32 = 5; // 5 × 100 ms = 0.5 s
                                      // Minimum speech polls before we trust the buffer is real speech.
        const MIN_SPEECH_POLLS: u32 = 3; // 3 × 100 ms = 0.3 s
                                         // Maximum audio buffer before force-emit (prevents unbounded growth
                                         // when someone talks without pauses).
        const MAX_BUF_SAMPLES: usize = 25 * 16_000; // 25 s at 16 kHz
                                                    // Context tail kept between sentences for whisper overlap.
        const TAIL_SAMPLES: usize = 32_000; // 2 s

        let mut buf: Vec<f32> = Vec::new();
        let mut speech_polls: u32 = 0;
        let mut silence_polls: u32 = 0;

        loop {
            tokio::time::sleep(std::time::Duration::from_millis(POLL_MS)).await;

            let chunk = match drain_audio(&app, &session_id) {
                Some(c) => c,
                None => break, // session removed → exit
            };

            if chunk.is_empty() {
                // Still count silence even when nothing new arrived
                if speech_polls >= MIN_SPEECH_POLLS {
                    silence_polls = silence_polls.saturating_add(1);
                }
            } else {
                let speech = has_speech(&chunk);
                buf.extend_from_slice(&chunk);

                if speech {
                    speech_polls += 1;
                    silence_polls = 0;
                } else if speech_polls >= MIN_SPEECH_POLLS {
                    silence_polls += 1;
                } else {
                    // Still in pre-speech noise: trim buf to keep only recent tail
                    if buf.len() > TAIL_SAMPLES {
                        let keep_from = buf.len() - TAIL_SAMPLES;
                        buf.drain(..keep_from);
                    }
                }
            }

            let silence_triggered =
                speech_polls >= MIN_SPEECH_POLLS && silence_polls >= SILENCE_POLLS;
            let overflow_triggered = buf.len() >= MAX_BUF_SAMPLES;

            if (silence_triggered || overflow_triggered) && buf.len() > TAIL_SAMPLES {
                tracing::info!(
                    "VAD emit: {:.1}s ({}) speech_polls={} silence_polls={}",
                    buf.len() as f64 / 16_000.0,
                    if silence_triggered {
                        "sentence end"
                    } else {
                        "overflow"
                    },
                    speech_polls,
                    silence_polls,
                );
                // Keep tail as context for next sentence
                let tail_start = buf.len().saturating_sub(TAIL_SAMPLES);
                let tail = buf[tail_start..].to_vec();
                let _ = tx.send(std::mem::replace(&mut buf, tail));
                speech_polls = 0;
                silence_polls = 0;
            }
        }
    })
}

// ─── Real-time transcription loop ────────────────────────────────────────────

pub(crate) fn spawn_rt_loop(
    app: AppHandle,
    data_dir: std::path::PathBuf,
    session_id: String,
    progressive_state: std::sync::Arc<std::sync::Mutex<ProgressiveState>>,
    mut sentence_rx: tokio::sync::mpsc::UnboundedReceiver<Vec<f32>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut time_offset = 0.0f64;
        let mut prev_tail: Vec<f32> = vec![];
        let mut recent_texts: Vec<String> = vec![];
        const MAX_RECENT: usize = 5;
        // Fallback: if no VAD sentence arrives within this window (e.g. continuous
        // speech without pauses), drain whatever the VAD task hasn't emitted yet.
        const FALLBACK_SECS: u64 = RT_INTERVAL_SECS * 4; // 32 s

        loop {
            // Wait for a VAD-triggered sentence OR the fallback timer.
            let samples: Vec<f32> = tokio::select! {
                msg = sentence_rx.recv() => {
                    match msg {
                        Some(s) => s,
                        None => break, // channel closed = VAD task exited
                    }
                }
                _ = tokio::time::sleep(std::time::Duration::from_secs(FALLBACK_SECS)) => {
                    // VAD hasn't triggered in a while; drain any leftover audio.
                    match drain_audio(&app, &session_id) {
                        Some(s) if s.len() >= 16_000 => s,
                        _ => continue,
                    }
                }
            };

            if samples.len() < 8_000 {
                continue; // too short to transcribe meaningfully (< 0.5 s)
            }

            // Accumulate 2 s tail for final post-processing overlap
            {
                let mut pstate = rt_lock(&progressive_state);
                pstate.samples.extend_from_slice(&samples);
                let keep = 32_000usize;
                if pstate.samples.len() > keep {
                    let drain_len = pstate.samples.len() - keep;
                    pstate.start_offset += drain_len as f64 / 16_000.0;
                    pstate.prev_tail = pstate.samples[drain_len..].to_vec();
                    pstate.samples.drain(..drain_len);
                }
            }

            // Prepare input: prepend 2 s overlap tail from previous sentence
            let mut input = prev_tail.clone();
            let overlap_len = input.len();
            input.extend_from_slice(&samples);

            let duration = samples.len() as f64 / 16_000.0;
            let offset = time_offset;
            time_offset += duration;
            prev_tail = samples[samples.len().saturating_sub(32_000)..].to_vec();

            // Load config (short-lived, dropped before await)
            let config = {
                let Ok(conn) = storage::open(&data_dir) else {
                    continue;
                };
                match load_model_config(&conn) {
                    Ok(c) => c,
                    Err(_) => continue,
                }
            };

            tracing::info!(
                "RT transcribing {:.1}s (offset={:.1}s, overlap={:.1}s)",
                input.len() as f64 / 16_000.0,
                offset,
                overlap_len as f64 / 16_000.0,
            );

            match transcribe::transcribe(&input, &config, &data_dir).await {
                Ok(raw_chunks) => {
                    let overlap_secs = overlap_len as f64 / 16_000.0;
                    let chunks = build_chunks(raw_chunks, &session_id, overlap_secs, offset);
                    tracing::info!("{} RT chunks after filtering", chunks.len());

                    if let Ok(conn) = storage::open(&data_dir) {
                        for chunk in &chunks {
                            if is_duplicate(&chunk.text, &recent_texts) {
                                continue;
                            }
                            if let Err(e) = session::insert_chunk(&conn, chunk) {
                                tracing::warn!("RT insert failed for {}: {}", session_id, e);
                            }
                            recent_texts.push(chunk.text.clone());
                            if recent_texts.len() > MAX_RECENT {
                                recent_texts.remove(0);
                            }
                        }
                        if let Ok(s) = session::get(&conn, &session_id) {
                            app.emit("session:updated", &s).ok();
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("RT transcription error for {}: {}", session_id, e);
                }
            }
        }
    })
}

// ─── Filters ─────────────────────────────────────────────────────────────────

fn is_duplicate(new_text: &str, recent: &[String]) -> bool {
    let new = new_text.trim().to_lowercase();
    if new.is_empty() {
        return true;
    }
    for prev in recent {
        let prev_lower = prev.trim().to_lowercase();
        if prev_lower.is_empty() {
            continue;
        }
        if new == prev_lower {
            return true;
        }
        if new.contains(&prev_lower) || prev_lower.contains(&new) {
            return true;
        }
    }
    false
}

fn is_hallucination(text: &str) -> bool {
    let lower = text.trim().to_lowercase();
    let hallucinations = [
        "thebettagaming.com",
        "amara.org",
        "subs by",
        "subtitles by",
        "[blank_audio]",
        "[silence]",
        "(silence)",
        "[music]",
        "(music)",
        "thank you.",
        "thank you!",
        "thanks for watching",
    ];

    for h in hallucinations {
        if lower.contains(h) {
            return true;
        }
    }

    if lower == "thank you" || lower == "thank you." {
        return true;
    }

    // Catch Whisper's common non-English hallucination loop:
    // "I'm not sure if I can translate this into English"
    if lower.contains("not sure if i can translate")
        || lower.contains("i can't translate")
        || lower.contains("i cannot translate")
    {
        return true;
    }

    false
}

// ─── Public helpers ──────────────────────────────────────────────────────────

pub fn friendly_error(raw: &str) -> String {
    if raw.contains("model_path") || raw.contains("model file") || raw.contains("No such file") {
        "No Whisper model configured. Go to Settings → Transcription and set a model.".into()
    } else if raw.contains("api_key")
        || raw.contains("API key")
        || raw.contains("Unauthorized")
        || raw.contains("401")
    {
        "LLM API key missing or invalid. Check Settings → Summarization.".into()
    } else if raw.contains("Connection refused") || raw.contains("Failed to reach") {
        "Could not reach the LLM provider. Is Ollama running? Check Settings → Summarization."
            .into()
    } else if raw.contains("No speech") || raw.contains("empty transcript") {
        "No speech detected in the recording.".into()
    } else {
        format!("Processing failed: {}", raw)
    }
}

// ─── Final post-processing ───────────────────────────────────────────────────

pub(crate) async fn run_post_processing(
    app: &AppHandle,
    data_dir: &std::path::Path,
    session_id: &str,
    samples: Vec<f32>,
    start_offset: f64,
    overlap_samples: usize,
    attendee_info: Option<String>,
) -> anyhow::Result<()> {
    let conn = storage::open(data_dir)?;
    let config = load_model_config(&conn)?;

    crate::commands::models::validate_config_with_data_dir(&config, Some(data_dir))?;

    if samples.len() > overlap_samples {
        tracing::info!(
            "Final progressive transcription: {} samples ({:.1}s)",
            samples.len(),
            samples.len() as f64 / 16_000.0
        );

        app.emit("processing:step", "transcribing").ok();
        app.emit("processing:step", "diarizing...").ok();

        transcribe_and_replace_window(
            &config,
            data_dir,
            session_id,
            &samples,
            overlap_samples,
            start_offset,
            attendee_info.as_deref(),
            false, // run diarization during final post-processing
        )
        .await?;
    }

    let all_chunks = session::get_chunks(&conn, session_id)?;
    app.emit("processing:step", "summarizing").ok();
    let notes = if all_chunks.is_empty() {
        "_No speech detected._".to_string()
    } else if !config.summarization.enabled {
        app.emit("processing:step", "saving transcript").ok();
        all_chunks
            .iter()
            .map(|c| format!("**{:.0}s** {}", c.timestamp_secs, c.text))
            .collect::<Vec<_>>()
            .join("\n\n")
    } else {
        match summarize::summarize(&all_chunks, None, &config, Some(app)).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("Summarization skipped: {}. Saving transcript only.", e);
                all_chunks
                    .iter()
                    .map(|c| format!("**{:.0}s** {}", c.timestamp_secs, c.text))
                    .collect::<Vec<_>>()
                    .join("\n\n")
            }
        }
    };

    let notes_path = data_dir.join(format!("{}.md", session_id));
    std::fs::write(&notes_path, &notes)?;

    conn.execute(
        "UPDATE sessions SET status = 'done', notes_path = ?1,
         ended_at = datetime('now'),
         duration_secs = (JULIANDAY(datetime('now')) - JULIANDAY(started_at)) * 86400.0
         WHERE id = ?2",
        rusqlite::params![notes_path.to_str(), session_id],
    )?;

    let sess = session::get(&conn, session_id)?;
    app.emit("session:updated", &sess).ok();
    tracing::info!("Session {} complete", session_id);
    Ok(())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_chunks_filters_overlap() {
        let raw = vec![
            (0.5, "in overlap".into()),
            (2.0, "after overlap".into()),
            (3.5, "later".into()),
        ];
        let chunks = build_chunks(raw, "s1", 1.0, 10.0);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].text, "after overlap");
        assert!((chunks[0].timestamp_secs - 11.0).abs() < 0.001); // 10.0 + (2.0 - 1.0)
        assert_eq!(chunks[1].text, "later");
        assert!((chunks[1].timestamp_secs - 12.5).abs() < 0.001); // 10.0 + (3.5 - 1.0)
    }

    #[test]
    fn build_chunks_empty_input() {
        let chunks = build_chunks(vec![], "s1", 0.0, 0.0);
        assert!(chunks.is_empty());
    }

    #[test]
    fn build_chunks_deduplicates_repeated_segments() {
        // Simulate whisper hallucination: same sentence repeated 5 times
        let raw = vec![
            (1.0, "Let's discuss the plan.".into()),
            (3.0, "Let's discuss the plan.".into()),
            (5.0, "Let's discuss the plan.".into()),
            (7.0, "Let's discuss the plan.".into()),
            (9.0, "Let's discuss the plan.".into()),
        ];
        let chunks = build_chunks(raw, "s1", 0.0, 0.0);
        // MAX_REPEATS = 2, so only the first 2 survive
        assert_eq!(chunks.len(), 2);
    }

    #[test]
    fn build_chunks_filters_hallucinations() {
        let raw = vec![
            (1.0, "Real content here".into()),
            (3.0, "Thank you.".into()),
            (5.0, "[MUSIC]".into()),
            (7.0, "More real content".into()),
        ];
        let chunks = build_chunks(raw, "s1", 0.0, 0.0);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].text, "Real content here");
        assert_eq!(chunks[1].text, "More real content");
    }

    #[test]
    fn is_duplicate_catches_exact_match() {
        assert!(is_duplicate("Hello world", &["hello world".into()]));
    }

    #[test]
    fn is_duplicate_catches_substring() {
        assert!(is_duplicate("Hello world today", &["hello world".into()]));
    }

    #[test]
    fn is_duplicate_allows_unique() {
        assert!(!is_duplicate("Something new", &["hello world".into()]));
    }

    #[test]
    fn is_duplicate_rejects_empty() {
        assert!(is_duplicate("  ", &[]));
    }

    #[test]
    fn is_hallucination_catches_known_patterns() {
        assert!(is_hallucination("Thank you."));
        assert!(is_hallucination("  [silence]  "));
        assert!(is_hallucination("Subs by example.com"));
        assert!(is_hallucination("[MUSIC]"));
    }

    #[test]
    fn is_hallucination_allows_real_text() {
        assert!(!is_hallucination("Thank you for coming to the meeting."));
        assert!(!is_hallucination("Let's discuss the roadmap."));
    }

    #[test]
    fn friendly_error_maps_known_errors() {
        assert!(friendly_error("model_path is missing").contains("Settings"));
        assert!(friendly_error("api_key invalid").contains("API key"));
        assert!(friendly_error("Connection refused").contains("Ollama"));
        assert!(friendly_error("No speech detected").contains("No speech"));
    }

    #[test]
    fn friendly_error_preserves_unknown() {
        let msg = friendly_error("something unknown");
        assert!(msg.contains("something unknown"));
    }
}

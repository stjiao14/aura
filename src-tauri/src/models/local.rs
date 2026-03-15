use super::{TranscriptionConfig, TranscriptionProvider};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Process-level cache so the model is loaded once and reused across RT passes.
/// Keyed by model path string; reloads only when the path changes.
static WHISPER_CACHE: OnceLock<Mutex<Option<(String, WhisperContext)>>> = OnceLock::new();

fn cached_context(
    model_str: &str,
) -> Result<std::sync::MutexGuard<'static, Option<(String, WhisperContext)>>> {
    let cache = WHISPER_CACHE.get_or_init(|| Mutex::new(None));
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());

    let needs_load = guard
        .as_ref()
        .map(|(cached_path, _)| cached_path.as_str() != model_str)
        .unwrap_or(true);

    if needs_load {
        tracing::info!("Loading Whisper model: {}", model_str);
        let ctx = WhisperContext::new_with_params(model_str, WhisperContextParameters::default())
            .context("Failed to load Whisper model")?;
        *guard = Some((model_str.to_string(), ctx));
        tracing::info!("Whisper model loaded and cached");
    }

    Ok(guard)
}

/// Transcription via a local GGUF whisper.cpp model file.
pub struct LocalFileProvider {
    pub model_path: PathBuf,
    /// ISO-639-1 language code, or None for auto-detect.
    pub language: Option<String>,
}

impl LocalFileProvider {
    pub fn new(config: &TranscriptionConfig) -> Result<Self> {
        let path = config
            .model_path
            .as_deref()
            .context("local_file provider requires model_path to be set in Settings")?;
        let model_path = PathBuf::from(path);
        anyhow::ensure!(
            model_path.exists(),
            "Whisper model file not found: {}\nSet the correct path in Settings → Transcription.",
            model_path.display()
        );
        let language = config.language.clone().filter(|l| l != "auto");
        Ok(Self {
            model_path,
            language,
        })
    }
}

#[async_trait]
impl TranscriptionProvider for LocalFileProvider {
    async fn transcribe(&self, samples: &[f32]) -> Result<Vec<(f64, String)>> {
        let model_path = self.model_path.clone();
        let samples = samples.to_vec();
        let language = self.language.clone();

        // whisper-rs is sync + CPU-heavy; run on blocking thread pool
        tokio::task::spawn_blocking(move || run_whisper(&model_path, &samples, language.as_deref()))
            .await
            .context("Whisper task panicked")?
    }
}

/// Returns a language-appropriate initial prompt to steer Whisper output.
/// Using an initial prompt in the target language prevents hallucination loops
/// where Whisper tries to "translate" non-English audio into English gibberish.
fn initial_prompt_for_language(lang: Option<&str>) -> &'static str {
    match lang {
        Some("ja") => "会議の文字起こし：",
        Some("zh") => "会议记录：",
        Some("ko") => "회의 기록:",
        Some("es") => "Transcripción de la reunión:",
        Some("fr") => "Transcription de la réunion :",
        Some("de") => "Besprechungsprotokoll:",
        Some("pt") => "Transcrição da reunião:",
        Some("ru") => "Протокол совещания:",
        Some("ar") => "محضر الاجتماع:",
        Some("hi") => "बैठक प्रतिलेख:",
        Some("it") => "Trascrizione della riunione:",
        // Auto-detect (None) and English both use the English prompt.
        // This matches the original behavior and works well for most meetings.
        _ => "Meeting transcript:",
    }
}

pub fn run_whisper(
    model_path: &Path,
    samples: &[f32],
    language: Option<&str>,
) -> Result<Vec<(f64, String)>> {
    let model_str = model_path
        .to_str()
        .context("Model path contains invalid UTF-8")?;

    // Load model once; reuse on subsequent calls with the same path.
    // The lock is held for the full inference duration (WhisperContext is not Sync).
    let guard = cached_context(model_str)?;
    let ctx = &guard.as_ref().unwrap().1;

    let mut state = ctx
        .create_state()
        .context("Failed to create Whisper state")?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 5 });
    params.set_language(language);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_single_segment(false);
    params.set_token_timestamps(false);
    params.set_no_speech_thold(0.6); // suppress hallucinations on near-silence
    params.set_entropy_thold(2.4); // discard low confidence
    params.set_initial_prompt(initial_prompt_for_language(language));

    state
        .full(params, samples)
        .context("Whisper inference failed")?;

    let n = state
        .full_n_segments()
        .context("Failed to get segment count")?;
    let mut result = Vec::with_capacity(n as usize);

    for i in 0..n {
        let text = state
            .full_get_segment_text(i)
            .context("Failed to get segment text")?;
        let t0_cs = state
            .full_get_segment_t0(i)
            .context("Failed to get segment timestamp")?;
        let t0_secs = t0_cs as f64 * 0.01; // centiseconds → seconds

        let trimmed = text.trim().to_string();
        if !trimmed.is_empty() {
            result.push((t0_secs, trimmed));
        }
    }

    tracing::info!("Whisper produced {} segments", result.len());
    Ok(result)
}

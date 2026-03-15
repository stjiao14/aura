#[cfg(feature = "local-whisper")]
use crate::models::{HfDownloadProvider, LocalFileProvider};
use crate::models::{
    ModelConfig, TranscriptionProvider, TranscriptionProviderKind, WhisperApiProvider,
};
use anyhow::Result;
pub mod diarize;

use std::path::Path;

/// Whisper sentinel tokens that indicate no real speech was detected.
/// Filter these out before storing chunks.
const BLANK_TOKENS: &[&str] = &[
    "[BLANK_AUDIO]",
    "[MUSIC]",
    "[NOISE]",
    "[SILENCE]",
    "[INAUDIBLE]",
    "(Music)",
    "(Applause)",
    "(Inaudible)",
    "(noise)",
    "(silence)",
];

pub fn is_blank(text: &str) -> bool {
    let t = text.trim();
    t.is_empty() || BLANK_TOKENS.iter().any(|tok| t.eq_ignore_ascii_case(tok))
}

pub async fn transcribe(
    samples: &[f32],
    config: &ModelConfig,
    cache_dir: &Path,
) -> Result<Vec<(f64, String)>> {
    let provider: Box<dyn TranscriptionProvider> = match config.transcription.provider {
        #[cfg(feature = "local-whisper")]
        TranscriptionProviderKind::LocalFile => {
            Box::new(LocalFileProvider::new(&config.transcription)?)
        }
        #[cfg(feature = "local-whisper")]
        TranscriptionProviderKind::HfDownload => {
            Box::new(HfDownloadProvider::new(&config.transcription, cache_dir).await?)
        }
        #[cfg(not(feature = "local-whisper"))]
        TranscriptionProviderKind::LocalFile | TranscriptionProviderKind::HfDownload => {
            anyhow::bail!("Local whisper not available (built without local-whisper feature)")
        }
        TranscriptionProviderKind::WhisperApi => {
            Box::new(WhisperApiProvider::new(&config.transcription)?)
        }
    };

    let raw = provider.transcribe(samples).await?;
    // Strip whisper no-speech sentinel tokens before returning
    Ok(raw
        .into_iter()
        .filter(|(_, text)| !is_blank(text))
        .collect())
}

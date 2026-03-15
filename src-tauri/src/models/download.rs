use super::{TranscriptionConfig, TranscriptionProvider};
use crate::models::local::run_whisper;
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::{Path, PathBuf};

/// Download a GGUF whisper model from HuggingFace (no auth for public models),
/// cache it, then transcribe using the same whisper-rs path as LocalFileProvider.
pub struct HfDownloadProvider {
    pub model_path: PathBuf,
    pub language: Option<String>,
}

impl HfDownloadProvider {
    pub async fn new(config: &TranscriptionConfig, cache_dir: &Path) -> Result<Self> {
        let model_id = config
            .model_id
            .as_deref()
            .context("hf_download provider requires model_id")?;

        let filename = filename_from_id(model_id);
        let cached_path = cache_dir.join("models").join(&filename);

        if !cached_path.exists() {
            download(model_id, &cached_path).await?;
        } else {
            tracing::info!("Using cached model: {}", cached_path.display());
        }

        let language = config.language.clone().filter(|l| l != "auto");
        Ok(Self {
            model_path: cached_path,
            language,
        })
    }
}

#[async_trait]
impl TranscriptionProvider for HfDownloadProvider {
    async fn transcribe(&self, samples: &[f32]) -> Result<Vec<(f64, String)>> {
        let model_path = self.model_path.clone();
        let samples = samples.to_vec();
        let language = self.language.clone();
        tokio::task::spawn_blocking(move || run_whisper(&model_path, &samples, language.as_deref()))
            .await
            .context("Whisper task panicked")?
    }
}

fn filename_from_id(model_id: &str) -> String {
    model_id
        .split('/')
        .next_back()
        .unwrap_or(model_id)
        .to_string()
}

async fn download(model_id: &str, dest: &Path) -> Result<()> {
    // Accepted formats:
    //   "ggerganov/whisper.cpp"                          → ggml-base.en.bin (default)
    //   "ggerganov/whisper.cpp/ggml-base.en.bin"         → explicit file
    let parts: Vec<&str> = model_id.split('/').collect();
    let url = match parts.len() {
        2 => format!(
            "https://huggingface.co/{}/{}/resolve/main/ggml-base.en.bin",
            parts[0], parts[1]
        ),
        3 => format!(
            "https://huggingface.co/{}/{}/resolve/main/{}",
            parts[0], parts[1], parts[2]
        ),
        _ => anyhow::bail!(
            "Invalid model_id '{}'. Use 'owner/repo' or 'owner/repo/file.bin'",
            model_id
        ),
    };

    tracing::info!("Downloading whisper model from {}", url);

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let client = reqwest::Client::new();
    let mut response = client
        .get(&url)
        .send()
        .await
        .context("Model download request failed")?;

    anyhow::ensure!(
        response.status().is_success(),
        "Model download failed with status {}",
        response.status()
    );

    let tmp = dest.with_extension("tmp");
    let mut file = tokio::fs::File::create(&tmp).await?;

    use tokio::io::AsyncWriteExt;
    while let Some(chunk) = response.chunk().await? {
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    drop(file);

    tokio::fs::rename(&tmp, dest).await?;
    tracing::info!("Model saved to {}", dest.display());
    Ok(())
}

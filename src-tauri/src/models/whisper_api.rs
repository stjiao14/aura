//! Transcription via OpenAI Whisper API.
//!
//! Encodes f32 samples as WAV, POSTs to /v1/audio/transcriptions,
//! and returns (timestamp_secs, text) segments.

use super::{TranscriptionConfig, TranscriptionProvider};
use anyhow::{Context, Result};
use async_trait::async_trait;

pub struct WhisperApiProvider {
    api_key: String,
    base_url: String,
    language: Option<String>,
}

impl WhisperApiProvider {
    pub fn new(config: &TranscriptionConfig) -> Result<Self> {
        let api_key = config
            .api_key
            .clone()
            .context("OpenAI API key required. Add it in Settings → Transcription.")?;
        let language = config.language.clone().filter(|l| l != "auto");
        Ok(Self {
            api_key,
            base_url: "https://api.openai.com".into(),
            language,
        })
    }
}

#[async_trait]
impl TranscriptionProvider for WhisperApiProvider {
    async fn transcribe(&self, samples: &[f32]) -> Result<Vec<(f64, String)>> {
        let wav = encode_wav_i16(samples, 16_000);

        let part = reqwest::multipart::Part::bytes(wav)
            .file_name("audio.wav")
            .mime_str("audio/wav")?;

        let mut form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("model", "whisper-1")
            .text("response_format", "verbose_json");

        if let Some(ref lang) = self.language {
            form = form.text("language", lang.clone());
        }

        let resp = reqwest::Client::new()
            .post(format!("{}/v1/audio/transcriptions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .multipart(form)
            .send()
            .await
            .context("Failed to reach OpenAI Whisper API")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Whisper API error {}: {}", status, body);
        }

        let data: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse Whisper API response")?;

        let mut result = Vec::new();
        if let Some(segs) = data["segments"].as_array() {
            for seg in segs {
                let start = seg["start"].as_f64().unwrap_or(0.0);
                let text = seg["text"].as_str().unwrap_or("").trim().to_string();
                if !text.is_empty() {
                    result.push((start, text));
                }
            }
        } else if let Some(text) = data["text"].as_str() {
            // Fallback: no segments, just full text at t=0
            let trimmed = text.trim().to_string();
            if !trimmed.is_empty() {
                result.push((0.0, trimmed));
            }
        }

        Ok(result)
    }
}

/// Encode mono f32 samples at the given sample rate as a 16-bit PCM WAV.
fn encode_wav_i16(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let pcm: Vec<i16> = samples
        .iter()
        .map(|&s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
        .collect();

    let data_bytes = (pcm.len() * 2) as u32;
    let mut wav = Vec::with_capacity(44 + data_bytes as usize);

    // RIFF header
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    // fmt chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&sample_rate.to_le_bytes()); // sample rate
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
    wav.extend_from_slice(&2u16.to_le_bytes()); // block align
    wav.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
                                                 // data chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_bytes.to_le_bytes());
    for s in pcm {
        wav.extend_from_slice(&s.to_le_bytes());
    }
    wav
}

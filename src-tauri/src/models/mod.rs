mod anthropic;
mod download;
mod local;
mod ollama;
mod openai;
mod whisper_api;

pub use anthropic::AnthropicProvider;
pub use download::HfDownloadProvider;
pub use local::LocalFileProvider;
pub use ollama::OllamaProvider;
pub use openai::OpenAiProvider;
pub use whisper_api::WhisperApiProvider;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// The plugin contract every LLM provider must implement.
///
/// `on_token` — if provided, the provider streams tokens to this callback as
/// they arrive. The function still returns the complete collected text when done.
/// Providers that don't support streaming ignore the callback and return once.
#[async_trait]
pub trait SummarizationProvider: Send + Sync {
    async fn generate(
        &self,
        prompt: &str,
        on_token: Option<std::sync::Arc<dyn Fn(String) + Send + Sync>>,
    ) -> Result<String>;
}

/// The plugin contract for transcription providers.
/// Takes pre-processed mono f32 samples at 16 kHz (whisper's expected format).
/// Returns (timestamp_secs, text) tuples.
#[async_trait]
pub trait TranscriptionProvider: Send + Sync {
    async fn transcribe(&self, samples: &[f32]) -> Result<Vec<(f64, String)>>;
}

// --- Config types (mirrored in TypeScript) ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CaptureMode {
    MicOnly,
    MicAndSystem,
}

impl Default for CaptureMode {
    fn default() -> Self {
        Self::MicOnly
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelConfig {
    #[serde(default)]
    pub capture_mode: CaptureMode,
    pub transcription: TranscriptionConfig,
    pub summarization: SummarizationConfig,
    /// Global hotkey to toggle recording. None = use default "CmdOrCtrl+Shift+R".
    #[serde(default)]
    pub hotkey_toggle: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionConfig {
    pub provider: TranscriptionProviderKind,
    pub model_path: Option<String>,
    pub model_id: Option<String>,
    /// API key — used by whisper_api provider
    pub api_key: Option<String>,
    /// ISO-639-1 language code (e.g. "en", "ja", "zh").
    /// `None` or `"auto"` = auto-detect.
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionProviderKind {
    LocalFile,
    HfDownload,
    WhisperApi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SummarizationConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub provider: SummarizationProviderKind,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub prompt: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SummarizationProviderKind {
    LocalFile,
    HfDownload,
    Ollama,
    Openai,
    Anthropic,
    Gemini,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            capture_mode: CaptureMode::MicOnly,
            transcription: TranscriptionConfig {
                provider: TranscriptionProviderKind::LocalFile,
                model_path: None,
                model_id: None,
                api_key: None,
                language: None,
            },
            summarization: SummarizationConfig {
                enabled: true,
                provider: SummarizationProviderKind::Ollama,
                base_url: Some("http://localhost:11434".into()),
                api_key: None,
                model: Some("llama3.2".into()),
                prompt: None,
            },
            hotkey_toggle: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_preamble_removes_text_before_first_heading() {
        let input =
            "Analyze the Request:\nRole: note-taker\n\n## Summary\nActual notes.".to_string();
        let out = strip_preamble(input);
        assert!(out.starts_with("## Summary"), "got: {}", out);
    }

    #[test]
    fn strip_preamble_noop_when_starts_with_heading() {
        let input = "## Summary\nContent here.".to_string();
        assert_eq!(strip_preamble(input.clone()), input);
    }

    #[test]
    fn strip_preamble_noop_when_no_heading() {
        let input = "Just some text without headings.".to_string();
        assert_eq!(strip_preamble(input.clone()), input);
    }

    #[test]
    fn strip_thinking_removes_think_tags() {
        let input = "<think>some reasoning\nmore reasoning</think>\nActual answer".to_string();
        assert_eq!(strip_thinking(input), "Actual answer");
    }

    #[test]
    fn strip_thinking_removes_thinking_tags() {
        let input = "<thinking>chain of thought</thinking>\nResult".to_string();
        assert_eq!(strip_thinking(input), "Result");
    }

    #[test]
    fn strip_thinking_noop_without_tags() {
        let input = "Plain response without tags".to_string();
        assert_eq!(strip_thinking(input), "Plain response without tags");
    }

    #[test]
    fn strip_thinking_removes_multiple_blocks() {
        let input = "<think>first</think>\nmiddle<think>second</think>\nend".to_string();
        assert_eq!(strip_thinking(input), "middle\nend");
    }

    #[test]
    fn strip_thinking_case_insensitive() {
        let input = "<THINK>ignored</THINK>\nAnswer".to_string();
        assert_eq!(strip_thinking(input), "Answer");
    }

    #[test]
    fn model_config_default_is_valid() {
        let c = ModelConfig::default();
        assert_eq!(c.capture_mode, CaptureMode::MicOnly);
        assert_eq!(
            c.transcription.provider,
            TranscriptionProviderKind::LocalFile
        );
        assert_eq!(c.summarization.provider, SummarizationProviderKind::Ollama);
        assert!(c.summarization.base_url.is_some());
        assert!(c.summarization.model.is_some());
    }

    #[test]
    fn model_config_serializes_to_camel_case() {
        let c = ModelConfig::default();
        let json = serde_json::to_value(&c).unwrap();
        assert!(
            json.get("captureMode").is_some(),
            "captureMode must be camelCase"
        );
        assert!(json.get("transcription").is_some());
        assert!(json.get("summarization").is_some());
        // snake_case keys must NOT appear at top level
        assert!(
            json.get("capture_mode").is_none(),
            "must not expose snake_case keys"
        );
    }

    #[test]
    fn capture_mode_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&CaptureMode::MicOnly).unwrap(),
            "\"mic_only\""
        );
        assert_eq!(
            serde_json::to_string(&CaptureMode::MicAndSystem).unwrap(),
            "\"mic_and_system\""
        );
    }

    #[test]
    fn transcription_provider_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&TranscriptionProviderKind::LocalFile).unwrap(),
            "\"local_file\""
        );
        assert_eq!(
            serde_json::to_string(&TranscriptionProviderKind::WhisperApi).unwrap(),
            "\"whisper_api\""
        );
        assert_eq!(
            serde_json::to_string(&TranscriptionProviderKind::HfDownload).unwrap(),
            "\"hf_download\""
        );
    }

    #[test]
    fn model_config_round_trips_through_json() {
        let original = ModelConfig {
            capture_mode: CaptureMode::MicAndSystem,
            transcription: TranscriptionConfig {
                provider: TranscriptionProviderKind::WhisperApi,
                model_path: None,
                model_id: None,
                api_key: Some("sk-test".into()),
                language: Some("ja".into()),
            },
            summarization: SummarizationConfig {
                enabled: true,
                provider: SummarizationProviderKind::Anthropic,
                base_url: None,
                api_key: Some("ant-key".into()),
                model: Some("claude-sonnet-4-6".into()),
                prompt: None,
            },
            hotkey_toggle: Some("CmdOrCtrl+Shift+R".into()),
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: ModelConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.capture_mode, CaptureMode::MicAndSystem);
        assert_eq!(
            parsed.transcription.provider,
            TranscriptionProviderKind::WhisperApi
        );
        assert_eq!(parsed.transcription.api_key.as_deref(), Some("sk-test"));
        assert_eq!(
            parsed.summarization.provider,
            SummarizationProviderKind::Anthropic
        );
    }

    #[test]
    fn old_config_without_language_deserializes() {
        // Simulate a config saved before the `language` field was added
        let old_json = r#"{
            "captureMode":"mic_only",
            "transcription":{"provider":"local_file","modelPath":null,"modelId":null,"apiKey":null},
            "summarization":{"enabled":true,"provider":"ollama","baseUrl":"http://localhost:11434","apiKey":null,"model":"llama3.2","prompt":null}
        }"#;
        let parsed: ModelConfig = serde_json::from_str(old_json).unwrap();
        assert!(parsed.transcription.language.is_none());
    }

    // ─── Contract tests ─────────────────────────────────────────────────
    // These tests define the JSON shape that the frontend sends/receives.
    // When you add a field to any config struct, you MUST update the
    // canonical JSON here.  If the test fails, it means the frontend
    // contract is out of sync.

    /// The canonical JSON the frontend sends for ModelConfig.
    /// Must stay in sync with DEFAULT_CONFIG in Settings.tsx and
    /// CANONICAL_CONFIG in config-contract.test.ts.
    const FRONTEND_CANONICAL_JSON: &str = r#"{
        "captureMode": "mic_only",
        "transcription": {
            "provider": "local_file",
            "modelPath": null,
            "modelId": null,
            "apiKey": null,
            "language": null
        },
        "summarization": {
            "enabled": true,
            "provider": "ollama",
            "baseUrl": "http://localhost:11434",
            "apiKey": null,
            "model": "llama3.2",
            "prompt": null
        }
    }"#;

    #[test]
    fn frontend_canonical_json_deserializes() {
        let parsed: ModelConfig = serde_json::from_str(FRONTEND_CANONICAL_JSON)
            .expect("FRONTEND_CANONICAL_JSON must deserialize into ModelConfig — \
                     if this fails, a field was added to the Rust struct but not to the canonical JSON");
        assert_eq!(parsed.capture_mode, CaptureMode::MicOnly);
        assert_eq!(
            parsed.transcription.provider,
            TranscriptionProviderKind::LocalFile
        );
    }

    #[test]
    fn serialized_config_has_all_expected_keys() {
        // Ensure Rust → JSON includes every key the frontend expects.
        // If you add a field, add it to the expected lists below.
        let config = ModelConfig::default();
        let json = serde_json::to_value(&config).unwrap();

        let transcription = json.get("transcription").unwrap().as_object().unwrap();
        let expected_transcription_keys =
            vec!["apiKey", "language", "modelId", "modelPath", "provider"];
        let mut actual: Vec<&str> = transcription.keys().map(|s| s.as_str()).collect();
        actual.sort();
        assert_eq!(
            actual, expected_transcription_keys,
            "TranscriptionConfig keys changed — update this test, \
             DEFAULT_CONFIG in Settings.tsx, and CANONICAL_CONFIG in config-contract.test.ts"
        );

        let summarization = json.get("summarization").unwrap().as_object().unwrap();
        let expected_summarization_keys = vec![
            "apiKey", "baseUrl", "enabled", "model", "prompt", "provider",
        ];
        let mut actual: Vec<&str> = summarization.keys().map(|s| s.as_str()).collect();
        actual.sort();
        assert_eq!(
            actual, expected_summarization_keys,
            "SummarizationConfig keys changed — update this test, \
             DEFAULT_CONFIG in Settings.tsx, and CANONICAL_CONFIG in config-contract.test.ts"
        );
    }

    #[test]
    fn all_summarization_providers_are_handled() {
        // Verify build_summarization_provider doesn't panic for any variant.
        // Providers that need network/API keys will return Err, not panic.
        let variants = vec![
            SummarizationProviderKind::Ollama,
            SummarizationProviderKind::Openai,
            SummarizationProviderKind::Anthropic,
            SummarizationProviderKind::Gemini,
            SummarizationProviderKind::LocalFile,
            SummarizationProviderKind::HfDownload,
        ];
        for variant in variants {
            let config = SummarizationConfig {
                enabled: true,
                provider: variant.clone(),
                base_url: Some("http://localhost:1234".into()),
                api_key: Some("test-key".into()),
                model: Some("test-model".into()),
                prompt: None,
            };
            // Should not panic — Ok or Err is fine
            let _ = build_summarization_provider(&config);
        }
    }
}

/// Strip any preamble text that appears before the first Markdown heading.
/// Some models output "Analyze the Request:" or similar meta-commentary before
/// the actual notes. Since our output always starts with a `#` heading, we can
/// safely drop everything before the first line that begins with `#`.
/// If no heading is found, the original text is returned unchanged.
pub fn strip_preamble(text: String) -> String {
    if let Some(pos) = text.find("\n#") {
        text[pos + 1..].trim_start_matches('\n').to_string()
    } else if text.starts_with('#') {
        text
    } else {
        text
    }
}

/// Strip LLM thinking/reasoning tags from response text.
/// Models like Qwen, DeepSeek wrap chain-of-thought in <think>/<thinking> tags.
pub fn strip_thinking(mut text: String) -> String {
    for tag in ["think", "thinking"] {
        let open = format!("<{}>", tag);
        let close = format!("</{}>", tag);
        loop {
            let lower = text.to_lowercase();
            if let Some(start) = lower.find(&open) {
                if let Some(rel_end) = lower[start..].find(&close) {
                    text = format!(
                        "{}{}",
                        &text[..start],
                        &text[start + rel_end + close.len()..]
                    );
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }
    text.trim().to_string()
}

/// Build a concrete SummarizationProvider from config.
pub fn build_summarization_provider(
    config: &SummarizationConfig,
) -> Result<Box<dyn SummarizationProvider>> {
    match config.provider {
        SummarizationProviderKind::Ollama => Ok(Box::new(OllamaProvider::new(config)?)),
        SummarizationProviderKind::Openai => Ok(Box::new(OpenAiProvider::new(config)?)),
        SummarizationProviderKind::Anthropic => Ok(Box::new(AnthropicProvider::new(config)?)),
        SummarizationProviderKind::Gemini => {
            // Gemini uses an OpenAI-compatible endpoint; delegate to OpenAiProvider
            Ok(Box::new(OpenAiProvider::new_with_base(
                config,
                "https://generativelanguage.googleapis.com/v1beta/openai",
            )?))
        }
        SummarizationProviderKind::LocalFile => {
            // "Custom (OpenAI-compatible)" — local servers (LM Studio, etc.)
            // API key is optional for local providers.
            Ok(Box::new(OpenAiProvider::new_custom(
                config,
                "http://localhost:1234/v1",
            )?))
        }
        // No wildcard `_ =>` here — if a new variant is added to the enum,
        // the compiler will force you to handle it.
        SummarizationProviderKind::HfDownload => {
            anyhow::bail!(
                "HfDownload is a transcription-only provider, not usable for summarization"
            )
        }
    }
}

use crate::commands::FriendlyError;
use crate::models::ModelConfig;
use crate::storage::{self, AppDataDir};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State};

// ─── Settings ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_model_config(data_dir: State<'_, AppDataDir>) -> Result<ModelConfig, String> {
    let conn = storage::open(&data_dir.0).friendly()?;
    load_model_config(&conn).friendly()
}

#[tauri::command]
pub fn set_model_config(
    app: AppHandle,
    config: ModelConfig,
    data_dir: State<'_, AppDataDir>,
) -> Result<(), String> {
    let conn = storage::open(&data_dir.0).friendly()?;
    let json = serde_json::to_string(&config).friendly()?;
    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES ('model_config', ?1)",
        rusqlite::params![json],
    )
    .friendly()?;

    // Re-register global hotkey with new binding
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let hotkey = config
        .hotkey_toggle
        .as_deref()
        .unwrap_or("CmdOrCtrl+Shift+R");
    let _ = app.global_shortcut().unregister_all();
    app.global_shortcut().register(hotkey).friendly()?;

    Ok(())
}

pub fn load_model_config(conn: &rusqlite::Connection) -> anyhow::Result<ModelConfig> {
    let json: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'model_config'",
            [],
            |row| row.get(0),
        )
        .ok();

    match json {
        Some(j) => Ok(serde_json::from_str(&j)?),
        None => Ok(ModelConfig::default()),
    }
}

pub fn validate_config_with_data_dir(
    config: &ModelConfig,
    data_dir: Option<&std::path::Path>,
) -> anyhow::Result<()> {
    use crate::models::TranscriptionProviderKind;
    match config.transcription.provider {
        TranscriptionProviderKind::LocalFile => {
            let path = config.transcription.model_path.as_deref().unwrap_or("");
            if path.is_empty() {
                anyhow::bail!("No Whisper model configured. Go to Settings → Transcription.");
            }
            if !std::path::Path::new(path).exists() {
                anyhow::bail!(
                    "Whisper model not found: {}. Check Settings → Transcription.",
                    path
                );
            }
        }
        TranscriptionProviderKind::HfDownload => {
            let model_id = config.transcription.model_id.as_deref().unwrap_or("");
            if model_id.is_empty() {
                anyhow::bail!("No model selected. Go to Settings → Transcription.");
            }
            // If we know the data dir, verify the cached model file exists
            if let Some(dir) = data_dir {
                let cached = dir.join("models").join(model_id);
                if !cached.exists() {
                    anyhow::bail!(
                        "Model '{}' not downloaded yet. Go to Settings → Transcription and click Download.",
                        model_id
                    );
                }
            }
        }
        TranscriptionProviderKind::WhisperApi => {
            if config
                .transcription
                .api_key
                .as_deref()
                .unwrap_or("")
                .is_empty()
            {
                anyhow::bail!("OpenAI API key required. Go to Settings → Transcription.");
            }
        }
    }
    Ok(())
}

// ─── File picker ─────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn pick_model_file(window: tauri::WebviewWindow) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    use tokio::sync::oneshot;

    // Suppress auto-hide before the dialog opens so the window doesn't
    // disappear when it loses focus to the native file picker.
    crate::SUPPRESS_AUTOHIDE.store(true, Ordering::SeqCst);
    let _ = window.set_always_on_top(true);

    let (tx, rx) = oneshot::channel::<Option<tauri_plugin_dialog::FilePath>>();

    window
        .app_handle()
        .dialog()
        .file()
        .add_filter("Whisper model", &["bin", "gguf"])
        .pick_file(move |path| {
            let _ = tx.send(path);
        });

    let result = rx
        .await
        .ok()
        .flatten()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().to_string());

    let _ = window.set_always_on_top(false);
    crate::SUPPRESS_AUTOHIDE.store(false, Ordering::SeqCst);
    result
}

// ─── Model download ───────────────────────────────────────────────────────────

#[tauri::command]
pub async fn download_model(
    model_id: String,
    app: AppHandle,
    data_dir: State<'_, AppDataDir>,
) -> Result<String, String> {
    use reqwest::Client;
    use tokio::io::AsyncWriteExt;

    let cache_dir = data_dir.0.join("models");
    tokio::fs::create_dir_all(&cache_dir).await.friendly()?;

    let dest = cache_dir.join(&model_id);
    if dest.exists() {
        return Ok(dest.to_string_lossy().to_string());
    }

    let url = format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
        model_id
    );

    let client = Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Download failed: HTTP {}", resp.status()));
    }

    let total = resp.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let mut stream = resp.bytes_stream();

    let tmp = dest.with_extension("tmp");
    let mut file = tokio::fs::File::create(&tmp).await.friendly()?;

    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.friendly()?;
        file.write_all(&chunk).await.friendly()?;
        downloaded += chunk.len() as u64;
        if total > 0 {
            let pct = downloaded as f64 / total as f64;
            app.emit("model:download_progress", serde_json::json!({ "pct": pct }))
                .ok();
        }
    }
    file.flush().await.friendly()?;
    drop(file);

    tokio::fs::rename(&tmp, &dest).await.friendly()?;

    Ok(dest.to_string_lossy().to_string())
}

/// Check if a whisper model is already cached locally.
#[tauri::command]
pub fn check_model_downloaded(model_id: String, data_dir: State<'_, AppDataDir>) -> bool {
    data_dir.0.join("models").join(&model_id).exists()
}

// ─── Provider Test ────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn test_provider_connection(data_dir: State<'_, AppDataDir>) -> Result<String, String> {
    let conn = storage::open(&data_dir.0).friendly()?;
    let config = load_model_config(&conn).friendly()?;

    let provider = crate::models::build_summarization_provider(&config.summarization)
        .map_err(|e| crate::commands::recorder::friendly_error(&e.to_string()))?;

    let prompt =
        crate::summarize::build_prompt("Speaker: Hello, this is a connection test.", None, None);

    provider
        .generate(&prompt, None)
        .await
        .map(|_| "Connection successful".to_string())
        .map_err(|e| crate::commands::recorder::friendly_error(&e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        CaptureMode, SummarizationConfig, SummarizationProviderKind, TranscriptionConfig,
        TranscriptionProviderKind,
    };

    fn base_config() -> ModelConfig {
        ModelConfig {
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

    #[test]
    fn local_file_with_no_path_fails() {
        let c = base_config();
        assert!(validate_config_with_data_dir(&c, None).is_err());
    }

    #[test]
    fn local_file_with_nonexistent_path_fails() {
        let mut c = base_config();
        c.transcription.model_path = Some("/no/such/file.bin".into());
        let err = validate_config_with_data_dir(&c, None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("not found"), "error was: {err}");
    }

    #[test]
    fn hf_download_with_no_model_id_fails() {
        let mut c = base_config();
        c.transcription.provider = TranscriptionProviderKind::HfDownload;
        assert!(validate_config_with_data_dir(&c, None).is_err());
    }

    #[test]
    fn hf_download_with_model_id_passes() {
        let mut c = base_config();
        c.transcription.provider = TranscriptionProviderKind::HfDownload;
        c.transcription.model_id = Some("ggml-base.bin".into());
        assert!(validate_config_with_data_dir(&c, None).is_ok());
    }

    #[test]
    fn whisper_api_with_no_key_fails() {
        let mut c = base_config();
        c.transcription.provider = TranscriptionProviderKind::WhisperApi;
        assert!(validate_config_with_data_dir(&c, None).is_err());
    }

    #[test]
    fn whisper_api_with_key_passes() {
        let mut c = base_config();
        c.transcription.provider = TranscriptionProviderKind::WhisperApi;
        c.transcription.api_key = Some("sk-test".into());
        assert!(validate_config_with_data_dir(&c, None).is_ok());
    }
}

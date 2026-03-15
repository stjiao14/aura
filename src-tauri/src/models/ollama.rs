use super::{strip_thinking, SummarizationConfig, SummarizationProvider};
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::json;
use std::sync::Arc;

pub struct OllamaProvider {
    client: Client,
    base_url: String,
    model: String,
}

impl OllamaProvider {
    pub fn new(config: &SummarizationConfig) -> Result<Self> {
        Ok(Self {
            client: Client::new(),
            base_url: config
                .base_url
                .clone()
                .unwrap_or_else(|| "http://localhost:11434".into()),
            model: config.model.clone().unwrap_or_else(|| "llama3.2".into()),
        })
    }
}

#[async_trait]
impl SummarizationProvider for OllamaProvider {
    async fn generate(
        &self,
        prompt: &str,
        on_token: Option<Arc<dyn Fn(String) + Send + Sync>>,
    ) -> Result<String> {
        let url = format!("{}/api/generate", self.base_url);
        let streaming = on_token.is_some();

        let body = json!({
            "model": self.model,
            "prompt": prompt,
            "stream": streaming
        });

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to reach Ollama")?;

        if !streaming {
            let val: serde_json::Value = resp
                .json()
                .await
                .context("Failed to parse Ollama response")?;
            return val["response"]
                .as_str()
                .map(|s| strip_thinking(s.to_string()))
                .context("Unexpected Ollama response shape");
        }

        // ── Streaming: Ollama sends one JSON object per line (NDJSON) ──
        let cb = on_token.unwrap();
        let mut collected = String::new();
        let mut buf = String::new();
        let mut stream = resp.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let bytes = chunk.context("Ollama stream error")?;
            buf.push_str(&String::from_utf8_lossy(&bytes));

            // Process every complete newline-terminated JSON object
            while let Some(nl) = buf.find('\n') {
                let line = buf[..nl].trim().to_string();
                buf = buf[nl + 1..].to_string();
                if line.is_empty() {
                    continue;
                }
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
                    if let Some(token) = val["response"].as_str() {
                        if !token.is_empty() {
                            collected.push_str(token);
                            cb(token.to_string());
                        }
                    }
                    if val["done"].as_bool() == Some(true) {
                        return Ok(strip_thinking(collected));
                    }
                }
            }
        }

        Ok(strip_thinking(collected))
    }
}

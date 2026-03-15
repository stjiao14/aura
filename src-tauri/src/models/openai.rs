use super::{strip_thinking, SummarizationConfig, SummarizationProvider};
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::json;
use std::sync::Arc;

pub struct OpenAiProvider {
    client: Client,
    base_url: String,
    api_key: String,
    model: String,
}

impl OpenAiProvider {
    pub fn new(config: &SummarizationConfig) -> Result<Self> {
        Self::new_with_base(config, "https://api.openai.com/v1")
    }

    pub fn new_with_base(config: &SummarizationConfig, base_url: &str) -> Result<Self> {
        Ok(Self {
            client: Client::new(),
            base_url: config.base_url.clone().unwrap_or_else(|| base_url.into()),
            api_key: config
                .api_key
                .clone()
                .context("OpenAI provider requires an API key")?,
            model: config.model.clone().unwrap_or_else(|| "gpt-4o".into()),
        })
    }

    pub fn new_custom(config: &SummarizationConfig, default_base: &str) -> Result<Self> {
        Ok(Self {
            client: Client::new(),
            base_url: config
                .base_url
                .clone()
                .unwrap_or_else(|| default_base.into()),
            api_key: config.api_key.clone().unwrap_or_default(),
            model: config.model.clone().unwrap_or_else(|| "default".into()),
        })
    }
}

#[async_trait]
impl SummarizationProvider for OpenAiProvider {
    async fn generate(
        &self,
        prompt: &str,
        on_token: Option<Arc<dyn Fn(String) + Send + Sync>>,
    ) -> Result<String> {
        let url = format!("{}/chat/completions", self.base_url);
        let streaming = on_token.is_some();

        let mut body = json!({
            "model": self.model,
            "messages": [{ "role": "user", "content": prompt }]
        });
        if streaming {
            body["stream"] = json!(true);
        }

        let mut req = self.client.post(&url).json(&body);
        if !self.api_key.is_empty() {
            req = req.bearer_auth(&self.api_key);
        }

        let resp = req
            .send()
            .await
            .context("Failed to reach OpenAI-compatible API")?;

        if !streaming {
            let val: serde_json::Value = resp
                .json()
                .await
                .context("Failed to parse OpenAI-compatible response")?;
            return val["choices"][0]["message"]["content"]
                .as_str()
                .map(|s| strip_thinking(s.to_string()))
                .context("Unexpected OpenAI response shape");
        }

        // ── Streaming: OpenAI SSE — lines prefixed with "data: " ──
        let cb = on_token.unwrap();
        let mut collected = String::new();
        let mut buf = String::new();
        let mut stream = resp.bytes_stream();

        'outer: while let Some(chunk) = stream.next().await {
            let bytes = chunk.context("OpenAI stream error")?;
            buf.push_str(&String::from_utf8_lossy(&bytes));

            while let Some(nl) = buf.find('\n') {
                let line = buf[..nl].trim().to_string();
                buf = buf[nl + 1..].to_string();

                if line.is_empty() || line.starts_with(':') {
                    continue;
                }
                let data = match line.strip_prefix("data: ") {
                    Some(d) => d,
                    None => continue,
                };
                if data == "[DONE]" {
                    break 'outer;
                }
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(token) = val["choices"][0]["delta"]["content"].as_str() {
                        if !token.is_empty() {
                            collected.push_str(token);
                            cb(token.to_string());
                        }
                    }
                }
            }
        }

        Ok(strip_thinking(collected))
    }
}

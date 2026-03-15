use super::{strip_thinking, SummarizationConfig, SummarizationProvider};
use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::json;
use std::sync::Arc;

pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    model: String,
}

impl AnthropicProvider {
    pub fn new(config: &SummarizationConfig) -> Result<Self> {
        Ok(Self {
            client: Client::new(),
            api_key: config
                .api_key
                .clone()
                .context("Anthropic provider requires an API key")?,
            model: config
                .model
                .clone()
                .unwrap_or_else(|| "claude-sonnet-4-6".into()),
        })
    }
}

#[async_trait]
impl SummarizationProvider for AnthropicProvider {
    async fn generate(
        &self,
        prompt: &str,
        on_token: Option<Arc<dyn Fn(String) + Send + Sync>>,
    ) -> Result<String> {
        let streaming = on_token.is_some();

        let mut body = json!({
            "model": self.model,
            "max_tokens": 2048,
            "messages": [{ "role": "user", "content": prompt }]
        });
        if streaming {
            body["stream"] = json!(true);
        }

        let resp = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .context("Failed to reach Anthropic API")?;

        if !streaming {
            let val: serde_json::Value = resp
                .json()
                .await
                .context("Failed to parse Anthropic response")?;
            let text = val["content"]
                .as_array()
                .and_then(|blocks| {
                    let s: String = blocks
                        .iter()
                        .filter(|b| b["type"].as_str() == Some("text"))
                        .filter_map(|b| b["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("");
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                })
                .context("Unexpected Anthropic response shape")?;
            return Ok(strip_thinking(text));
        }

        // ── Streaming: Anthropic SSE ──
        // Relevant event: content_block_delta with delta.type == "text_delta"
        let cb = on_token.unwrap();
        let mut collected = String::new();
        let mut buf = String::new();
        let mut stream = resp.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let bytes = chunk.context("Anthropic stream error")?;
            buf.push_str(&String::from_utf8_lossy(&bytes));

            while let Some(nl) = buf.find('\n') {
                let line = buf[..nl].trim().to_string();
                buf = buf[nl + 1..].to_string();

                // Anthropic SSE lines: "event: ..." then "data: ..."
                // We only care about data lines
                let data = match line.strip_prefix("data: ") {
                    Some(d) => d,
                    None => continue,
                };
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(data) {
                    if val["type"].as_str() == Some("content_block_delta") {
                        if let Some(token) = val["delta"]["text"].as_str() {
                            if !token.is_empty() {
                                collected.push_str(token);
                                cb(token.to_string());
                            }
                        }
                    }
                }
            }
        }

        Ok(strip_thinking(collected))
    }
}

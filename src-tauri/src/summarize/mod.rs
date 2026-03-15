use crate::models::{build_summarization_provider, strip_preamble, ModelConfig};
use crate::session::TranscriptChunk;
use anyhow::Result;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

pub async fn summarize(
    chunks: &[TranscriptChunk],
    context: Option<&str>,
    config: &ModelConfig,
    app: Option<&AppHandle>,
) -> Result<String> {
    let transcript = chunks
        .iter()
        .map(|c| {
            let speaker = c.speaker_label.as_deref().unwrap_or("Speaker");
            let time = format_time(c.timestamp_secs);
            format!("[{}] {}: {}", time, speaker, c.text)
        })
        .collect::<Vec<_>>()
        .join("\n");

    let prompt = build_prompt(&transcript, context, config.summarization.prompt.as_deref());

    let provider = build_summarization_provider(&config.summarization)?;

    // Build streaming callback if we have an AppHandle to emit events to.
    // Tokens are emitted raw (before strip_thinking/strip_preamble) so the
    // frontend sees them immediately; the final clean version arrives via
    // session:updated once the file is written.
    let on_token: Option<Arc<dyn Fn(String) + Send + Sync>> = app.map(|a| {
        let a = a.clone();
        Arc::new(move |token: String| {
            a.emit("notes:chunk", &token).ok();
        }) as Arc<dyn Fn(String) + Send + Sync>
    });

    let raw = provider.generate(&prompt, on_token).await?;
    Ok(strip_preamble(raw))
}

pub fn build_prompt(
    transcript: &str,
    context: Option<&str>,
    custom_prompt: Option<&str>,
) -> String {
    let context_section = context
        .map(|c| format!("## Meeting Context\n{c}\n\n"))
        .unwrap_or_default();

    let instructions = custom_prompt.unwrap_or(
        r#"You are a professional meeting note-taker.

BEGIN YOUR RESPONSE IMMEDIATELY WITH THE FIRST MARKDOWN HEADING. DO NOT explain your approach, analyze the request, or write any introductory text before the first heading.

Produce concise Markdown notes with these sections:

## Summary
2-4 sentences.

## Key Decisions
Bullet points. Omit section if none.

## Action Items
Bullet points with owner if identifiable. Omit section if none.

## Full Notes
Main discussion points."#,
    );

    format!("{instructions}\n\n{context_section}## Transcript\n{transcript}\n")
}

fn format_time(secs: f64) -> String {
    let m = (secs / 60.0) as u64;
    let s = (secs % 60.0) as u64;
    format!("{:02}:{:02}", m, s)
}

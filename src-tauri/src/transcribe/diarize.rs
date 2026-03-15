use crate::models::{build_summarization_provider, ModelConfig};
use crate::session::TranscriptChunk;
use anyhow::Result;
use std::collections::HashMap;

pub async fn diarize_chunks(
    chunks: &mut [TranscriptChunk],
    attendee_info: Option<&str>,
    config: &ModelConfig,
) -> Result<()> {
    if chunks.is_empty() {
        return Ok(());
    }

    let transcript_text = chunks
        .iter()
        .map(|c| format!("**{:.0}s**: {}", c.timestamp_secs, c.text))
        .collect::<Vec<_>>()
        .join("\n");

    let context_section = attendee_info
        .map(|a| format!("## Attendees\n{a}\n\n"))
        .unwrap_or_default();

    let prompt = format!(
        r#"You are a professional meeting assistant. Your job is to identify the likely speaker for each line in the transcript segment.

{context_section}## Transcript Segment
{transcript_text}

## Instructions
For each line in the transcript, identify the speaker. If you know the attendees, use their names based on context. If you don't know, use generically "Speaker 1", "Speaker 2", etc., but be consistent.
Output EXACTLY a JSON object mapping the timestamp (as a string, e.g. "15s") to the speaker's name.

Example output:
{{
  "15s": "Alice",
  "18s": "Bob"
}}

Output ONLY valid JSON, no markdown formatting blocks, no preamble."#
    );

    let provider = build_summarization_provider(&config.summarization)?;
    let response = provider.generate(&prompt, None).await?;

    // The LLM response might be wrapped in markdown code blocks like ```json ... ```
    let cleaned = response
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    // Fallback: if JSON parsing fails, we just log a warning and don't diarize this time
    // We do not return an Err because we don't want to fail the transcription pipeline
    if let Ok(map) = serde_json::from_str::<HashMap<String, String>>(cleaned) {
        for chunk in chunks.iter_mut() {
            let key = format!("{:.0}s", chunk.timestamp_secs);
            if let Some(speaker) = map.get(&key) {
                chunk.speaker_label = Some(speaker.clone());
            }
        }
    } else {
        tracing::warn!("Failed to parse diarization JSON from LLM: {}", cleaned);
    }

    Ok(())
}

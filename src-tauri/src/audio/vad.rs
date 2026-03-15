//! Voice Activity Detection (VAD)
//!
//! Sits in front of the transcription pipeline to drop silent audio frames.
//! Prevents whisper.cpp from hallucinating on background noise.
//!
//! Implementation plan:
//!   - Phase 1: energy-based threshold (simple, zero dependencies)
//!   - Phase 2: WebRTC VAD via webrtc-vad crate
//!   - Phase 3: Silero VAD ONNX model for higher accuracy

/// Returns true if the given 16-bit PCM frame contains speech.
/// Phase 1: simple RMS energy threshold.
pub fn is_speech(samples: &[i16], threshold_rms: f32) -> bool {
    if samples.is_empty() {
        return false;
    }
    let rms = (samples.iter().map(|&s| (s as f64).powi(2)).sum::<f64>() / samples.len() as f64)
        .sqrt() as f32;
    rms > threshold_rms
}

pub const DEFAULT_RMS_THRESHOLD: f32 = 800.0;

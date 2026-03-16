//! Audio capture pipeline — mic (cpal) + system loopback (ScreenCaptureKit on macOS,
//! WASAPI on Windows).
//!
//! Both streams are started together. On stop, the samples are mixed and
//! resampled to 16 kHz mono — the format whisper.cpp expects.

// Platform-specific loopback backends, both exposed as the `loopback` module
// so the rest of this file stays platform-agnostic.
#[cfg(target_os = "macos")]
#[path = "loopback.rs"]
pub mod loopback;

#[cfg(target_os = "windows")]
#[path = "loopback_windows.rs"]
pub mod loopback;

pub mod mic;
pub mod resample;
pub mod vad;

use anyhow::Result;
use resample::to_whisper_format;

/// An active recording session. Drop or call `stop()` to end it.
pub struct CaptureHandle {
    pub mic: Option<mic::MicHandle>,
    pub loopback_active: bool,
}

/// Start capturing audio according to the configured capture mode.
/// `system_audio` = true means also capture loopback audio.
pub fn start_capture(system_audio: bool) -> Result<CaptureHandle> {
    let mic = match mic::start() {
        Ok(h) => {
            tracing::info!("Mic capture started");
            Some(h)
        }
        Err(e) => {
            tracing::warn!("Mic unavailable: {}. Continuing without mic.", e);
            None
        }
    };

    let loopback_active = if system_audio {
        let ok = loopback::start();
        if ok {
            tracing::info!("Loopback capture initiated");
        } else {
            tracing::warn!("Loopback capture failed to start");
        }
        ok
    } else {
        false
    };

    if system_audio && !loopback_active {
        if let Some(m) = mic {
            m.stop(); // Clean up mic if loopback failed
        }
        #[cfg(target_os = "macos")]
        anyhow::bail!("Screen Recording access denied or invalid. If you just granted it, you MUST completely Quit (Cmd+Q) and reopen the app to apply the fix.");
        #[cfg(not(target_os = "macos"))]
        anyhow::bail!("System audio loopback capture failed to start.");
    }

    if mic.is_none() && !loopback_active {
        anyhow::bail!("No audio source available. Grant Microphone permission in System Settings.");
    }

    Ok(CaptureHandle {
        mic,
        loopback_active,
    })
}

impl CaptureHandle {
    /// Drain audio accumulated since the last drain (or start), mixed and resampled to 16 kHz mono.
    /// Does NOT stop the capture streams. Returns empty vec if no audio yet.
    pub fn drain(&self) -> Vec<f32> {
        let mic_16k = self.mic.as_ref().and_then(|h| {
            let raw: Vec<f32> = std::mem::take(
                &mut *h
                    .samples
                    .lock()
                    .unwrap_or_else(|e| panic!("[aura] Mic samples lock poisoned: {e}")),
            );
            if raw.is_empty() {
                return None;
            }
            Some(to_whisper_format(&raw, h.sample_rate, h.channels))
        });

        let lb_16k = if self.loopback_active {
            let (raw, rate) = loopback::drain();
            if raw.is_empty() {
                None
            } else {
                Some(to_whisper_format(&raw, rate, 1))
            }
        } else {
            None
        };

        match (mic_16k, lb_16k) {
            (Some(m), Some(l)) => mix(&m, &l),
            (Some(m), None) => m,
            (None, Some(l)) => l,
            (None, None) => vec![],
        }
    }

    /// Stop all capture, drain remaining samples, mix streams, and return 16 kHz mono f32 samples.
    pub fn stop(self) -> Vec<f32> {
        // Stop mic and resample
        let mic_16k = self.mic.map(|h| {
            let (raw, rate, ch) = h.stop();
            tracing::info!("Mic: {} samples @ {}Hz {} ch", raw.len(), rate, ch);
            to_whisper_format(&raw, rate, ch)
        });

        // Stop loopback and resample
        let lb_16k = if self.loopback_active {
            let (raw, rate) = loopback::stop();
            tracing::info!("Loopback: {} samples @ {}Hz mono", raw.len(), rate);
            Some(to_whisper_format(&raw, rate, 1))
        } else {
            None
        };

        // Mix the two streams
        match (mic_16k, lb_16k) {
            (Some(mic), Some(lb)) => mix(&mic, &lb),
            (Some(mic), None) => mic,
            (None, Some(lb)) => lb,
            (None, None) => vec![],
        }
    }
}

/// Element-wise mix of two 16 kHz mono streams.
/// Pads the shorter stream with silence. Levels chosen so neither clips.
pub fn mix(a: &[f32], b: &[f32]) -> Vec<f32> {
    let len = a.len().max(b.len());
    let mut out = vec![0f32; len];
    for (i, &s) in a.iter().enumerate() {
        out[i] += s * 0.6;
    }
    for (i, &s) in b.iter().enumerate() {
        out[i] += s * 0.6;
    }
    out
}//! Audio capture pipeline — mic (cpal) + system loopback (ScreenCaptureKit).
//!
//! Both streams are started together. On stop, the samples are mixed and
//! resampled to 16 kHz mono — the format whisper.cpp expects.

pub mod loopback;
pub mod mic;
pub mod resample;
pub mod vad;

use anyhow::Result;
use resample::to_whisper_format;

/// An active recording session. Drop or call `stop()` to end it.
pub struct CaptureHandle {
    pub mic: Option<mic::MicHandle>,
    pub loopback_active: bool,
}

/// Start capturing audio according to the configured capture mode.
/// `system_audio` = true means also capture loopback via ScreenCaptureKit.
pub fn start_capture(system_audio: bool) -> Result<CaptureHandle> {
    let mic = match mic::start() {
        Ok(h) => {
            tracing::info!("Mic capture started");
            Some(h)
        }
        Err(e) => {
            tracing::warn!("Mic unavailable: {}. Continuing without mic.", e);
            None
        }
    };

    let loopback_active = if system_audio {
        let ok = loopback::start();
        if ok {
            tracing::info!("Loopback capture initiated (SCStream async setup)");
        } else {
            tracing::warn!("Loopback capture failed to start (screen recording permission?)");
        }
        ok
    } else {
        false
    };

    if system_audio && !loopback_active {
        if let Some(m) = mic {
            m.stop(); // Clean up mic if loopback failed
        }
        anyhow::bail!("Screen Recording access denied or invalid. If you just granted it, you MUST completely Quit (Cmd+Q) and reopen the app to apply the fix.");
    }

    if mic.is_none() && !loopback_active {
        anyhow::bail!("No audio source available. Grant Microphone permission in System Settings.");
    }

    Ok(CaptureHandle {
        mic,
        loopback_active,
    })
}

impl CaptureHandle {
    /// Drain audio accumulated since the last drain (or start), mixed and resampled to 16 kHz mono.
    /// Does NOT stop the capture streams. Returns empty vec if no audio yet.
    pub fn drain(&self) -> Vec<f32> {
        let mic_16k = self.mic.as_ref().and_then(|h| {
            let raw: Vec<f32> = std::mem::take(
                &mut *h
                    .samples
                    .lock()
                    .unwrap_or_else(|e| panic!("[aura] Mic samples lock poisoned: {e}")),
            );
            if raw.is_empty() {
                return None;
            }
            Some(to_whisper_format(&raw, h.sample_rate, h.channels))
        });

        let lb_16k = if self.loopback_active {
            let (raw, rate) = loopback::drain();
            if raw.is_empty() {
                None
            } else {
                Some(to_whisper_format(&raw, rate, 1))
            }
        } else {
            None
        };

        match (mic_16k, lb_16k) {
            (Some(m), Some(l)) => mix(&m, &l),
            (Some(m), None) => m,
            (None, Some(l)) => l,
            (None, None) => vec![],
        }
    }

    /// Stop all capture, drain remaining samples, mix streams, and return 16 kHz mono f32 samples.
    pub fn stop(self) -> Vec<f32> {
        // Stop mic and resample
        let mic_16k = self.mic.map(|h| {
            let (raw, rate, ch) = h.stop();
            tracing::info!("Mic: {} samples @ {}Hz {} ch", raw.len(), rate, ch);
            to_whisper_format(&raw, rate, ch)
        });

        // Stop loopback and resample
        let lb_16k = if self.loopback_active {
            let (raw, rate) = loopback::stop();
            tracing::info!("Loopback: {} samples @ {}Hz mono", raw.len(), rate);
            Some(to_whisper_format(&raw, rate, 1))
        } else {
            None
        };

        // Mix the two streams
        match (mic_16k, lb_16k) {
            (Some(mic), Some(lb)) => mix(&mic, &lb),
            (Some(mic), None) => mic,
            (None, Some(lb)) => lb,
            (None, None) => vec![],
        }
    }
}

/// Element-wise mix of two 16 kHz mono streams.
/// Pads the shorter stream with silence. Levels chosen so neither clips.
pub fn mix(a: &[f32], b: &[f32]) -> Vec<f32> {
    let len = a.len().max(b.len());
    let mut out = vec![0f32; len];
    for (i, &s) in a.iter().enumerate() {
        out[i] += s * 0.6;
    }
    for (i, &s) in b.iter().enumerate() {
        out[i] += s * 0.6;
    }
    out
}

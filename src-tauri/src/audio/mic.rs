//! Microphone capture via cpal (CoreAudio on macOS, WASAPI on Windows).

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SupportedStreamConfig};
use std::sync::{Arc, Mutex};

pub struct MicHandle {
    _stream: SendStream,
    pub samples: Arc<Mutex<Vec<f32>>>,
    pub sample_rate: u32,
    pub channels: usize,
}

#[allow(dead_code)]
struct SendStream(cpal::Stream);
// SAFETY: cpal's stream handle is internally thread-safe on all supported platforms.
unsafe impl Send for SendStream {}

impl MicHandle {
    pub fn stop(self) -> (Vec<f32>, u32, usize) {
        drop(self._stream); // stops the audio callback
        let samples = std::mem::take(
            &mut *self
                .samples
                .lock()
                .unwrap_or_else(|e| panic!("[aura] Mic samples lock poisoned: {e}")),
        );
        (samples, self.sample_rate, self.channels)
    }
}

// --- Platform-specific permission handling --------------------------------

#[cfg(target_os = "macos")]
extern "C" {
    fn aura_mic_auth_status() -> i32;
    fn aura_request_mic_access() -> bool;
}

/// Ensure microphone permission is granted before opening a cpal stream.
///
/// On macOS this checks/requests access via the Swift AVFoundation layer so
/// that the system permission dialog appears before cpal touches CoreAudio.
///
/// On Windows, WASAPI surfaces the permission prompt automatically when the
/// stream is opened, so no pre-check is needed.
#[cfg(target_os = "macos")]
fn ensure_mic_permission() -> Result<()> {
    let status = unsafe { aura_mic_auth_status() };
    match status {
        1 => Ok(()), // already authorized
        2 => anyhow::bail!(
            "Microphone access denied. Grant permission in System Settings → Privacy & Security → Microphone."
        ),
        3 => anyhow::bail!("Microphone access restricted by system policy."),
        _ => {
            // Not determined — request now (blocks until user responds)
            tracing::info!("Requesting microphone permission…");
            let granted = unsafe { aura_request_mic_access() };
            if granted {
                Ok(())
            } else {
                anyhow::bail!(
                    "Microphone access denied. Grant permission in System Settings → Privacy & Security → Microphone."
                )
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn ensure_mic_permission() -> Result<()> {
    // On Windows (and other platforms) WASAPI / the OS handles permission
    // prompts automatically when the audio stream is opened.
    Ok(())
}

// --- Stream setup ---------------------------------------------------------

pub fn start() -> Result<MicHandle> {
    ensure_mic_permission()?;

    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .context("No default audio input device")?;

    tracing::info!("Mic device: {}", device.name().unwrap_or_default());

    let (config, sample_format) = preferred_config(&device)?;
    let sample_rate = config.sample_rate().0;
    let channels = config.channels() as usize;

    tracing::info!(
        "Mic config: {}Hz {} ch {:?}",
        sample_rate,
        channels,
        sample_format
    );

    let samples: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let samples_cb = samples.clone();

    let stream = match sample_format {
        SampleFormat::F32 => build_stream::<f32>(&device, &config.into(), samples_cb)?,
        SampleFormat::I16 => build_stream::<i16>(&device, &config.into(), samples_cb)?,
        SampleFormat::U16 => build_stream::<u16>(&device, &config.into(), samples_cb)?,
        _ => anyhow::bail!("Unsupported mic sample format: {:?}", sample_format),
    };
    stream.play().context("Failed to start mic stream")?;

    Ok(MicHandle {
        _stream: SendStream(stream),
        samples,
        sample_rate,
        channels,
    })
}

fn preferred_config(device: &cpal::Device) -> Result<(SupportedStreamConfig, SampleFormat)> {
    let mut configs: Vec<_> = device.supported_input_configs()?.collect();
    configs.sort_by_key(|c| std::cmp::Reverse(c.max_sample_rate().0));
    for fmt in [SampleFormat::F32, SampleFormat::I16, SampleFormat::U16] {
        if let Some(c) = configs.iter().find(|c| c.sample_format() == fmt) {
            return Ok(((*c).with_max_sample_rate(), fmt));
        }
    }
    anyhow::bail!("No supported mic input config")
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    samples: Arc<Mutex<Vec<f32>>>,
) -> Result<cpal::Stream>
where
    T: cpal::Sample + cpal::SizedSample + ToF32,
{
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let floats: Vec<f32> = data.iter().map(|s| s.to_f32()).collect();
            samples
                .lock()
                .unwrap_or_else(|e| panic!("[aura] Mic callback lock poisoned: {e}"))
                .extend(floats);
        },
        |err| tracing::error!("Mic stream error: {}", err),
        None,
    )?;
    Ok(stream)
}

pub trait ToF32: Copy {
    fn to_f32(self) -> f32;
}
impl ToF32 for f32 {
    fn to_f32(self) -> f32 {
        self
    }
}
impl ToF32 for i16 {
    fn to_f32(self) -> f32 {
        self as f32 / i16::MAX as f32
    }
}
impl ToF32 for u16 {
    fn to_f32(self) -> f32 {
        (self as f32 / u16::MAX as f32) * 2.0 - 1.0
    }
}//! Microphone capture via cpal (CoreAudio on macOS).

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SupportedStreamConfig};
use std::sync::{Arc, Mutex};

pub struct MicHandle {
    _stream: SendStream,
    pub samples: Arc<Mutex<Vec<f32>>>,
    pub sample_rate: u32,
    pub channels: usize,
}

#[allow(dead_code)]
struct SendStream(cpal::Stream);
// SAFETY: cpal's CoreAudio stream handle is internally thread-safe.
unsafe impl Send for SendStream {}

impl MicHandle {
    pub fn stop(self) -> (Vec<f32>, u32, usize) {
        drop(self._stream); // stops the CoreAudio callback
        let samples = std::mem::take(
            &mut *self
                .samples
                .lock()
                .unwrap_or_else(|e| panic!("[aura] Mic samples lock poisoned: {e}")),
        );
        (samples, self.sample_rate, self.channels)
    }
}

extern "C" {
    fn aura_mic_auth_status() -> i32;
    fn aura_request_mic_access() -> bool;
}

/// Ensure microphone permission is granted before opening a cpal stream.
/// This avoids cpal triggering the macOS permission dialog on every call.
fn ensure_mic_permission() -> Result<()> {
    let status = unsafe { aura_mic_auth_status() };
    match status {
        1 => Ok(()), // already authorized
        2 => anyhow::bail!("Microphone access denied. Grant permission in System Settings → Privacy & Security → Microphone."),
        3 => anyhow::bail!("Microphone access restricted by system policy."),
        _ => {
            // Not determined — request now (blocks until user responds)
            tracing::info!("Requesting microphone permission…");
            let granted = unsafe { aura_request_mic_access() };
            if granted {
                Ok(())
            } else {
                anyhow::bail!("Microphone access denied. Grant permission in System Settings → Privacy & Security → Microphone.")
            }
        }
    }
}

pub fn start() -> Result<MicHandle> {
    ensure_mic_permission()?;

    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .context("No default audio input device")?;

    tracing::info!("Mic device: {}", device.name().unwrap_or_default());

    let (config, sample_format) = preferred_config(&device)?;
    let sample_rate = config.sample_rate().0;
    let channels = config.channels() as usize;

    tracing::info!(
        "Mic config: {}Hz {} ch {:?}",
        sample_rate,
        channels,
        sample_format
    );

    let samples: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let samples_cb = samples.clone();

    let stream = match sample_format {
        SampleFormat::F32 => build_stream::<f32>(&device, &config.into(), samples_cb)?,
        SampleFormat::I16 => build_stream::<i16>(&device, &config.into(), samples_cb)?,
        SampleFormat::U16 => build_stream::<u16>(&device, &config.into(), samples_cb)?,
        _ => anyhow::bail!("Unsupported mic sample format: {:?}", sample_format),
    };
    stream.play().context("Failed to start mic stream")?;

    Ok(MicHandle {
        _stream: SendStream(stream),
        samples,
        sample_rate,
        channels,
    })
}

fn preferred_config(device: &cpal::Device) -> Result<(SupportedStreamConfig, SampleFormat)> {
    let mut configs: Vec<_> = device.supported_input_configs()?.collect();
    configs.sort_by_key(|c| std::cmp::Reverse(c.max_sample_rate().0));
    for fmt in [SampleFormat::F32, SampleFormat::I16, SampleFormat::U16] {
        if let Some(c) = configs.iter().find(|c| c.sample_format() == fmt) {
            return Ok(((*c).with_max_sample_rate(), fmt));
        }
    }
    anyhow::bail!("No supported mic input config")
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    samples: Arc<Mutex<Vec<f32>>>,
) -> Result<cpal::Stream>
where
    T: cpal::Sample + cpal::SizedSample + ToF32,
{
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let floats: Vec<f32> = data.iter().map(|s| s.to_f32()).collect();
            samples
                .lock()
                .unwrap_or_else(|e| panic!("[aura] Mic callback lock poisoned: {e}"))
                .extend(floats);
        },
        |err| tracing::error!("Mic stream error: {}", err),
        None,
    )?;
    Ok(stream)
}

pub trait ToF32: Copy {
    fn to_f32(self) -> f32;
}
impl ToF32 for f32 {
    fn to_f32(self) -> f32 {
        self
    }
}
impl ToF32 for i16 {
    fn to_f32(self) -> f32 {
        self as f32 / i16::MAX as f32
    }
}
impl ToF32 for u16 {
    fn to_f32(self) -> f32 {
        (self as f32 / u16::MAX as f32) * 2.0 - 1.0
    }
}

//! System audio loopback capture on Windows via WASAPI.
//!
//! Uses WASAPI's loopback flag on the default render (output) device so that
//! whatever the user hears is captured — no special OS permission is required.
//!
//! The public API mirrors the macOS `loopback` module: `start()`, `drain()`, `stop()`.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{thread, time::Duration};

// --- Global sample buffer --------------------------------------------------

struct LoopbackState {
    samples: Vec<f32>,
    source_rate: u32,
}

static STATE: Mutex<LoopbackState> = Mutex::new(LoopbackState {
    samples: Vec::new(),
    source_rate: 44100,
});

static RUNNING: AtomicBool = AtomicBool::new(false);

// --- Public API -----------------------------------------------------------

/// Start loopback capture. Spawns a background WASAPI capture thread.
/// Always returns `true`; errors are logged from the background thread.
pub fn start() -> bool {
    {
        let mut s = STATE
            .lock()
            .unwrap_or_else(|e| panic!("[aura] Loopback lock poisoned: {e}"));
        s.samples.clear();
        s.source_rate = 44100;
    }
    RUNNING.store(true, Ordering::SeqCst);
    thread::spawn(capture_loop);
    true
}

/// Drain accumulated samples without stopping capture.
/// Returns `(samples, source_rate_hz)`.
pub fn drain() -> (Vec<f32>, u32) {
    let mut s = STATE
        .lock()
        .unwrap_or_else(|e| panic!("[aura] Loopback lock poisoned: {e}"));
    let samples = std::mem::take(&mut s.samples);
    let rate = s.source_rate;
    (samples, rate)
}

/// Stop loopback capture and return `(samples, source_rate_hz)`.
pub fn stop() -> (Vec<f32>, u32) {
    RUNNING.store(false, Ordering::SeqCst);
    // Give the background thread a moment to flush its last buffer.
    thread::sleep(Duration::from_millis(150));
    let mut s = STATE
        .lock()
        .unwrap_or_else(|e| panic!("[aura] Loopback lock poisoned: {e}"));
    let samples = std::mem::take(&mut s.samples);
    let rate = s.source_rate;
    (samples, rate)
}

// --- Background capture thread --------------------------------------------

fn capture_loop() {
    if let Err(e) = unsafe { wasapi_capture() } {
        tracing::error!("[aura] WASAPI loopback error: {e}");
    }
}

/// WASAPI loopback capture.
///
/// Steps:
///   1. CoInitializeEx on this thread (WASAPI is COM-based).
///   2. Enumerate the default *render* (output) device — loopback captures
///      whatever that device is playing.
///   3. Activate an IAudioClient, initialize it with AUDCLNT_STREAMFLAGS_LOOPBACK.
///   4. Loop: poll `GetNextPacketSize`, then `GetBuffer` / `ReleaseBuffer`.
///   5. Convert raw PCM to f32 and push into STATE.
unsafe fn wasapi_capture() -> windows::core::Result<()> {
    use windows::Win32::{
        Media::Audio::{
            eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator,
            MMDeviceEnumerator, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK,
        },
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
        },
    };

    CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;

    let enumerator: IMMDeviceEnumerator =
        CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

    // Get the default *output* device — loopback taps the render pipeline.
    let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
    let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;

    // Query the device's native mix format (usually 32-bit float, stereo).
    let fmt_ptr = client.GetMixFormat()?;
    let fmt = &*fmt_ptr;
    let sample_rate = fmt.nSamplesPerSec;
    let channels = fmt.nChannels as usize;
    let bits_per_sample = fmt.wBitsPerSample;

    tracing::info!(
        "[aura] WASAPI loopback: {}Hz, {} ch, {} bps",
        sample_rate,
        channels,
        bits_per_sample
    );

    {
        let mut s = STATE
            .lock()
            .unwrap_or_else(|e| panic!("[aura] Loopback lock poisoned: {e}"));
        s.source_rate = sample_rate;
    }

    // Initialize for shared-mode loopback capture.
    client.Initialize(
        AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_LOOPBACK,
        0, // buffer duration (0 = use default)
        0, // periodicity (must be 0 for shared mode)
        fmt_ptr,
        None,
    )?;

    let capture: IAudioCaptureClient = client.GetService()?;
    client.Start()?;

    while RUNNING.load(Ordering::SeqCst) {
        // Poll for available packets.
        let mut packet_size = 0u32;
        capture.GetNextPacketSize(&mut packet_size)?;

        if packet_size == 0 {
            thread::sleep(Duration::from_millis(10));
            continue;
        }

        let mut data: *mut u8 = std::ptr::null_mut();
        let mut frames_available = 0u32;
        let mut flags = 0u32;

        capture.GetBuffer(&mut data, &mut frames_available, &mut flags, None, None)?;

        if frames_available > 0 && !data.is_null() {
            let floats = pcm_to_f32(data, frames_available as usize, channels, bits_per_sample);
            STATE
                .lock()
                .unwrap_or_else(|e| panic!("[aura] Loopback lock poisoned: {e}"))
                .samples
                .extend_from_slice(&floats);
        }

        capture.ReleaseBuffer(frames_available)?;
    }

    client.Stop()?;
    CoUninitialize();
    Ok(())
}

// --- PCM → f32 conversion -------------------------------------------------

/// Convert raw WASAPI PCM bytes to a flat f32 slice (interleaved channels).
///
/// WASAPI shared mode most commonly delivers IEEE 32-bit float; 16-bit integer
/// is also handled. Unsupported depths output silence and log a warning.
fn pcm_to_f32(data: *const u8, num_frames: usize, channels: usize, bits_per_sample: u16) -> Vec<f32> {
    let num_samples = num_frames * channels;
    let mut out = Vec::with_capacity(num_samples);

    match bits_per_sample {
        32 => {
            // IEEE float — the default for WASAPI shared mode.
            let floats =
                unsafe { std::slice::from_raw_parts(data as *const f32, num_samples) };
            out.extend_from_slice(floats);
        }
        16 => {
            let shorts =
                unsafe { std::slice::from_raw_parts(data as *const i16, num_samples) };
            out.extend(shorts.iter().map(|&s| s as f32 / i16::MAX as f32));
        }
        _ => {
            tracing::warn!(
                "[aura] Unsupported WASAPI bit depth: {bits_per_sample}, substituting silence"
            );
            out.resize(num_samples, 0.0);
        }
    }

    out
      }

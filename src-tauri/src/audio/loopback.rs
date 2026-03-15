//! Rust FFI bindings for the AuraAudio Swift library (ScreenCaptureKit loopback).
//!
//! The Swift side captures system audio at 44100 Hz mono and calls our C callback
//! with each chunk. We accumulate samples in a global buffer and return them when
//! `stop()` is called.

use std::sync::Mutex;

// --- Global sample buffer --------------------------------------------------
// Only one loopback session is active at a time, so a module-level static is fine.

struct LoopbackState {
    samples: Vec<f32>,
    source_rate: u32,
}

static STATE: Mutex<LoopbackState> = Mutex::new(LoopbackState {
    samples: Vec::new(),
    source_rate: 44100,
});

// --- Swift FFI declarations ------------------------------------------------

extern "C" {
    /// Defined in AuraAudio Swift package (Loopback.swift).
    fn aura_loopback_start(callback: extern "C" fn(*const f32, i32, f64)) -> bool;
    fn aura_loopback_stop();
}

// --- Callback (called from Swift on audio thread) -------------------------

extern "C" fn on_chunk(samples: *const f32, count: i32, sample_rate: f64) {
    if samples.is_null() || count <= 0 {
        return;
    }
    let slice = unsafe { std::slice::from_raw_parts(samples, count as usize) };
    let mut state = STATE
        .lock()
        .unwrap_or_else(|e| panic!("[aura] Loopback state lock poisoned: {e}"));
    state.samples.extend_from_slice(slice);
    state.source_rate = sample_rate as u32;
}

// --- Public API -----------------------------------------------------------

/// Start loopback capture. Returns false if SCShareableContent setup fails early.
/// (Permission prompt may appear; actual audio starts slightly after this returns.)
pub fn start() -> bool {
    // Reset buffer before each session
    {
        let mut state = STATE
            .lock()
            .unwrap_or_else(|e| panic!("[aura] Loopback state lock poisoned: {e}"));
        state.samples.clear();
        state.source_rate = 44100;
    }
    unsafe { aura_loopback_start(on_chunk) }
}

/// Drain accumulated samples without stopping capture. Returns (samples, source_rate_hz).
pub fn drain() -> (Vec<f32>, u32) {
    let mut state = STATE
        .lock()
        .unwrap_or_else(|e| panic!("[aura] Loopback state lock poisoned: {e}"));
    let samples = std::mem::take(&mut state.samples);
    let rate = state.source_rate;
    (samples, rate)
}

/// Stop loopback capture and return (samples, source_rate_hz).
pub fn stop() -> (Vec<f32>, u32) {
    unsafe { aura_loopback_stop() }
    let mut state = STATE
        .lock()
        .unwrap_or_else(|e| panic!("[aura] Loopback state lock poisoned: {e}"));
    let samples = std::mem::take(&mut state.samples);
    let rate = state.source_rate;
    (samples, rate)
}

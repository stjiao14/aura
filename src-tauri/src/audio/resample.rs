//! Simple linear-interpolation resampler + mono mixer.
//!
//! Whisper expects: mono f32 at 16 kHz, values in [-1.0, 1.0].

/// Mix interleaved multi-channel samples to mono, then resample to 16 kHz.
pub fn to_whisper_format(samples: &[f32], from_rate: u32, channels: usize) -> Vec<f32> {
    let mono = mix_to_mono(samples, channels);
    resample(&mono, from_rate, 16_000)
}

fn mix_to_mono(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels == 1 {
        return samples.to_vec();
    }
    samples
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

fn resample(mono: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || mono.is_empty() {
        return mono.to_vec();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = ((mono.len() as f64) / ratio).ceil() as usize;
    (0..out_len)
        .map(|i| {
            let src = i as f64 * ratio;
            let idx = src as usize;
            let frac = (src - idx as f64) as f32;
            let s0 = mono[idx.min(mono.len() - 1)];
            let s1 = mono[(idx + 1).min(mono.len() - 1)];
            s0 + (s1 - s0) * frac
        })
        .collect()
}

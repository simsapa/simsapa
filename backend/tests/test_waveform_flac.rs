//! Waveform path against a real `.flac` produced by the recorder's encode path.
//!
//! Confirms `get_waveform_peaks` / `get_audio_duration_ms` decode a FLAC written
//! by `encode_canonical_pcm_to_flac` (the format user recordings now use after
//! the move off Qt Multimedia) and return a non-empty, plausible waveform.

#![cfg(feature = "audio")]

use simsapa_backend::audio::format::CANONICAL_SAMPLE_RATE;
use simsapa_backend::audio::recorder::encode_canonical_pcm_to_flac;
use simsapa_backend::waveform::{get_audio_duration_ms, get_waveform_peaks};

/// Generate `seconds` of a mono sine wave at `freq` Hz as canonical 16-bit PCM.
fn sine_pcm(freq: f32, seconds: f32) -> Vec<i16> {
    let n = (CANONICAL_SAMPLE_RATE as f32 * seconds) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / CANONICAL_SAMPLE_RATE as f32;
            let v = (2.0 * std::f32::consts::PI * freq * t).sin() * 0.5;
            (v * i16::MAX as f32) as i16
        })
        .collect()
}

#[test]
fn waveform_peaks_from_recorded_flac_are_non_empty() {
    let dir = tempfile::tempdir().expect("tempdir");
    let flac_path = dir.path().join("waveform.flac");

    // 2 s sine → enough samples to fill many buckets.
    let pcm = sine_pcm(440.0, 2.0);
    encode_canonical_pcm_to_flac(&pcm, &flac_path).expect("encode flac");

    let path = flac_path.to_str().expect("utf8 path");

    let num_bars = 64;
    let peaks = get_waveform_peaks(path, num_bars).expect("waveform peaks");
    assert_eq!(peaks.len(), num_bars, "one peak per requested bar");

    // A 0.5-amplitude sine must produce audible, normalized peaks.
    assert!(
        peaks.iter().all(|&p| (0.0..=1.0).contains(&p)),
        "peaks normalized to 0..=1"
    );
    let global_max = peaks.iter().copied().fold(0.0f32, f32::max);
    assert!(global_max > 0.5, "waveform has real signal, max={global_max}");

    // Duration is read from container metadata (~2 s, allow one FLAC block slack).
    let duration_ms = get_audio_duration_ms(path);
    assert!(
        (1900..=2200).contains(&duration_ms),
        "duration ~2000 ms, got {duration_ms}"
    );
}

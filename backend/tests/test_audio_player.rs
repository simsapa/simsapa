//! Player tests against a real generated FLAC fixture (encoded via the Task 1
//! recorder path), exercising the decode → [`PlaybackCore`] boundary logic
//! without an audio output device: duration/position math, seek rounding, and
//! range + loop behaviour.

#![cfg(feature = "audio")]

use simsapa_backend::audio::format::CANONICAL_SAMPLE_RATE;
use simsapa_backend::audio::player::{decode_file_to_core, PlayerState};
use simsapa_backend::audio::recorder::encode_canonical_pcm_to_flac;

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

/// Encode a 1-second fixture FLAC and decode it into a `PlaybackCore`.
fn fixture_core(dir: &std::path::Path) -> simsapa_backend::audio::player::PlaybackCore {
    let flac_path = dir.join("fixture.flac");
    let pcm = sine_pcm(440.0, 1.0);
    encode_canonical_pcm_to_flac(&pcm, &flac_path).expect("encode fixture flac");
    decode_file_to_core(&flac_path).expect("decode fixture flac")
}

#[test]
fn decoded_fixture_reports_canonical_duration() {
    let dir = tempfile::tempdir().expect("tempdir");
    let core = fixture_core(dir.path());
    // The encoder zero-pads up to one FLAC block (4096 samples ≈ 85 ms at
    // 48 kHz), so the duration is the 1000 ms input rounded up by < one block.
    let duration = core.duration_ms();
    assert!(
        (1000..1100).contains(&duration),
        "fixture duration {duration} ms within one block of 1000 ms"
    );
    assert_eq!(core.position_ms(), 0);
    assert_eq!(core.state(), PlayerState::Stopped);
}

#[test]
fn seek_target_rounds_and_clamps_on_real_fixture() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut core = fixture_core(dir.path());

    core.seek_ms(250);
    assert_eq!(core.position_ms(), 250);

    // Seeking past the end clamps to the duration.
    core.seek_ms(10_000);
    assert_eq!(core.position_ms(), core.duration_ms());
}

#[test]
fn range_playback_stops_at_end_on_real_fixture() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut core = fixture_core(dir.path());

    core.play_range(100, 200, false);
    assert_eq!(core.position_ms(), 100);
    assert_eq!(core.state(), PlayerState::Playing);

    // Pull more than 100 ms of audio at 48 kHz to run past the range end.
    let mut out = vec![0.0f32; CANONICAL_SAMPLE_RATE as usize / 4]; // 250 ms mono
    core.fill_output(&mut out, 1);

    assert_eq!(core.state(), PlayerState::Stopped);
    assert!(core.take_finished());
    assert_eq!(core.position_ms(), 200, "stopped at the range end");
}

#[test]
fn range_loop_wraps_on_real_fixture() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut core = fixture_core(dir.path());

    core.play_range(100, 150, true);

    // 100 ms of output spans the 50 ms range twice, so it must wrap.
    let mut out = vec![0.0f32; CANONICAL_SAMPLE_RATE as usize / 10]; // 100 ms mono
    core.fill_output(&mut out, 1);

    assert_eq!(core.state(), PlayerState::Playing, "still looping");
    assert!(!core.take_finished());
    // It only stays Playing because it wrapped at the range end instead of
    // stopping, so the position must lie within [start, end].
    let pos = core.position_ms();
    assert!(
        (100..=150).contains(&pos),
        "looped position {pos} ms is within the range"
    );
}

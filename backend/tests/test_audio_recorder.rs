//! Recorder encode-path tests: synthesize canonical PCM, encode to FLAC via the
//! recorder's encode path, then decode it back with symphonia and verify the
//! container reports the canonical format. (Live cpal capture needs a real input
//! device, so only the encode/decode path is exercised here.)

#![cfg(feature = "audio")]

use std::path::Path;

use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use simsapa_backend::audio::format::{CANONICAL_CHANNELS, CANONICAL_SAMPLE_RATE};
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

/// Decode a FLAC file with symphonia, returning (sample_rate, channels, total_frames).
fn decode_flac(path: &Path) -> (u32, usize, u64) {
    let file = std::fs::File::open(path).expect("open flac");
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    hint.with_extension("flac");

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .expect("probe flac");

    let mut format = probed.format;
    let track = format.default_track().expect("default track").clone();
    let track_id = track.id;

    let sample_rate = track.codec_params.sample_rate.expect("sample rate");
    let channels = track.codec_params.channels.expect("channels").count();

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .expect("make decoder");

    let mut total_frames: u64 = 0;
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(_) => break,
        };
        if packet.track_id() != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(decoded) => total_frames += decoded.frames() as u64,
            Err(_) => break,
        }
    }

    (sample_rate, channels, total_frames)
}

#[test]
fn encode_then_decode_roundtrip_canonical_format() {
    let dir = tempfile::tempdir().expect("tempdir");
    let flac_path = dir.path().join("roundtrip.flac");

    let pcm = sine_pcm(440.0, 1.0);
    let expected_frames = pcm.len() as u64;

    encode_canonical_pcm_to_flac(&pcm, &flac_path).expect("encode flac");

    assert!(flac_path.exists(), "flac file was written");
    let meta = std::fs::metadata(&flac_path).expect("flac metadata");
    assert!(meta.len() > 0, "flac file is non-empty");

    let (sample_rate, channels, total_frames) = decode_flac(&flac_path);

    assert_eq!(sample_rate, CANONICAL_SAMPLE_RATE, "decoded sample rate");
    assert_eq!(channels, CANONICAL_CHANNELS as usize, "decoded channel count");
    // The encoder zero-pads up to one FLAC block (4096 samples) so all frames
    // are full length; the decoded count is the input rounded up to a block.
    assert!(
        total_frames >= expected_frames && total_frames - expected_frames < 4096,
        "decoded frame count {} within one block of {}",
        total_frames,
        expected_frames
    );
}

#[test]
fn encode_short_buffer_is_decodable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let flac_path = dir.path().join("short.flac");

    // ~50 ms of audio — shorter than one FLAC block.
    let pcm = sine_pcm(220.0, 0.05);
    encode_canonical_pcm_to_flac(&pcm, &flac_path).expect("encode short flac");

    let (sample_rate, channels, total_frames) = decode_flac(&flac_path);
    assert_eq!(sample_rate, CANONICAL_SAMPLE_RATE);
    assert_eq!(channels, CANONICAL_CHANNELS as usize);
    assert!(total_frames > 0, "short clip decodes to a non-empty frame count");
}

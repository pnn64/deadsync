use deadlib_audio_decode as decode;
use ebur128::{EbuR128, Mode};
use std::path::Path;

/// EBU R 128 / `ReplayGain` 2.0 reference loudness.
const TARGET_LUFS: f32 = -18.0;
const DB_TO_LOG2: f32 = std::f32::consts::LOG2_10 / 20.0;
/// Hard ceiling on the linear gain factor we will apply (+12 dB).
const MAX_GAIN_LINEAR: f32 = 4.0;
/// Linear gain returned for silence / un-analyzable tracks.
pub const UNITY_GAIN: f32 = 1.0;

/// Maximum frames fed to the analyzer per call. Decoders emit short packets,
/// but we still cap so a buggy decoder cannot blow the stack of `add_frames`.
const ANALYZE_CHUNK_FRAMES: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReplayGainInfo {
    pub lufs: f32,
    pub true_peak_linear: f32,
}

/// Convert LUFS + true peak into a linear playback gain, applying a peak
/// limit so we never amplify into clipping and clamping to a sensible ceiling.
#[inline]
#[must_use]
pub fn gain_linear_from_info(info: ReplayGainInfo) -> f32 {
    if !info.lufs.is_finite() || info.lufs <= -69.5 {
        return UNITY_GAIN;
    }
    let raw_linear = ((TARGET_LUFS - info.lufs) * DB_TO_LOG2).exp2();
    let peak_limited = if info.true_peak_linear > f32::EPSILON {
        (1.0 / info.true_peak_linear).max(0.0)
    } else {
        MAX_GAIN_LINEAR
    };
    raw_linear.min(peak_limited).clamp(0.0, MAX_GAIN_LINEAR)
}

/// Measures integrated loudness and true peak using the caller's decode options.
pub fn compute_loudness(
    path: &Path,
    options: decode::DecodeOptions,
) -> Result<ReplayGainInfo, String> {
    let opened = decode::open_file(path, options).map_err(|e| e.to_string())?;
    let channels = opened.channels.max(1);
    let sample_rate = opened.sample_rate_hz.max(1);
    if channels > 8 {
        return Err(format!(
            "ReplayGain: refusing to analyze {channels} channels"
        ));
    }

    let mut analyzer = EbuR128::new(channels as u32, sample_rate, Mode::I | Mode::TRUE_PEAK)
        .map_err(|e| format!("ebur128 init failed: {e:?}"))?;

    let mut reader = opened.reader;
    let mut buf: Vec<i16> = Vec::with_capacity(ANALYZE_CHUNK_FRAMES * channels);
    let mut had_samples = false;

    loop {
        buf.clear();
        match reader.read_dec_packet_into(&mut buf) {
            Ok(false) => break,
            Ok(true) => {}
            Err(e) => return Err(e.to_string()),
        }
        if buf.is_empty() {
            continue;
        }
        // Truncate to a whole number of frames in case the decoder produced
        // a partial frame.
        let frames_in_buf = buf.len() / channels;
        if frames_in_buf == 0 {
            continue;
        }
        had_samples = true;
        let usable = frames_in_buf * channels;
        analyzer
            .add_frames_i16(&buf[..usable])
            .map_err(|e| format!("ebur128 add_frames failed: {e:?}"))?;
    }

    if !had_samples {
        return Err("decoder produced no samples".to_string());
    }

    let lufs = analyzer
        .loudness_global()
        .map_err(|e| format!("ebur128 loudness_global failed: {e:?}"))? as f32;

    let mut true_peak = 0.0_f64;
    for ch in 0..channels {
        if let Ok(peak) = analyzer.true_peak(ch as u32)
            && peak > true_peak
        {
            true_peak = peak;
        }
    }

    Ok(ReplayGainInfo {
        lufs,
        true_peak_linear: true_peak as f32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn gain_unity_for_target_loudness() {
        let g = gain_linear_from_info(ReplayGainInfo {
            lufs: TARGET_LUFS,
            true_peak_linear: 0.5,
        });
        assert!((g - 1.0).abs() < 1e-4, "expected ~1.0, got {g}");
    }

    #[test]
    fn gain_boost_for_quiet_track() {
        // -30 LUFS -> +12 dB ideal, but peak 0.5 -> ceiling +6 dB (= 2.0).
        let g = gain_linear_from_info(ReplayGainInfo {
            lufs: -30.0,
            true_peak_linear: 0.5,
        });
        assert!(g <= 2.0 + 1e-4 && g > 1.0, "got {g}");
    }

    #[test]
    fn gain_cut_for_loud_track() {
        // -10 LUFS -> -8 dB -> ~=0.398.
        let g = gain_linear_from_info(ReplayGainInfo {
            lufs: -10.0,
            true_peak_linear: 0.99,
        });
        assert!((g - 0.398).abs() < 0.01, "got {g}");
    }

    #[test]
    fn gain_unity_for_silence() {
        let g = gain_linear_from_info(ReplayGainInfo {
            lufs: f32::NEG_INFINITY,
            true_peak_linear: 0.0,
        });
        assert_eq!(g, UNITY_GAIN);
    }

    #[test]
    fn gain_capped_at_max() {
        let g = gain_linear_from_info(ReplayGainInfo {
            lufs: -100.0,
            true_peak_linear: 0.0,
        });
        assert!(g <= MAX_GAIN_LINEAR + 1e-4);
    }

    #[test]
    fn gain_conversion_matches_known_db_ratios_and_limits() {
        let cases = [
            (
                ReplayGainInfo {
                    lufs: -24.0,
                    true_peak_linear: 0.0,
                },
                1.995_262_4,
            ),
            (
                ReplayGainInfo {
                    lufs: -12.0,
                    true_peak_linear: 0.0,
                },
                0.501_187_2,
            ),
            (
                ReplayGainInfo {
                    lufs: -30.0,
                    true_peak_linear: 0.25,
                },
                3.981_071_7,
            ),
            (
                ReplayGainInfo {
                    lufs: -30.0,
                    true_peak_linear: 0.5,
                },
                2.0,
            ),
            (
                ReplayGainInfo {
                    lufs: f32::NAN,
                    true_peak_linear: 0.5,
                },
                UNITY_GAIN,
            ),
            (
                ReplayGainInfo {
                    lufs: f32::INFINITY,
                    true_peak_linear: 0.5,
                },
                UNITY_GAIN,
            ),
        ];

        for (info, expected) in cases {
            let actual = gain_linear_from_info(info);
            assert!(
                (actual - expected).abs() <= 2e-6,
                "info={info:?}, expected={expected}, actual={actual}"
            );
        }
    }

    #[test]
    fn computes_loudness_using_requested_mp3_timeline() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../deadlib-audio-decode/tests/fixtures/timeline.mp3");
        let gapless =
            compute_loudness(&path, decode::DecodeOptions::default()).expect("analyze gapless MP3");
        let untrimmed = compute_loudness(
            &path,
            decode::DecodeOptions {
                mp3_gapless: false,
                mp3_info_silence: true,
            },
        )
        .expect("analyze untrimmed MP3");
        assert!(gapless.lufs.is_finite());
        assert!(untrimmed.lufs.is_finite());
        assert_ne!(gapless, untrimmed);
    }
}

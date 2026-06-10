pub mod cache;

use deadsync_audio_decode as decode;
use ebur128::{EbuR128, Mode};
use std::path::Path;

pub use cache::{
    ReplayGainCacheEntry, ReplayGainCacheFile, decode_replaygain_cache, encode_replaygain_cache,
    read_replaygain_cache_file, replaygain_cache_entry_for_path, replaygain_cache_info_if_fresh,
    replaygain_path_hash, replaygain_source_mtime_unix_nanos, write_replaygain_cache_file,
};

/// EBU R 128 / ReplayGain 2.0 reference loudness.
const TARGET_LUFS: f64 = -18.0;
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
pub fn gain_linear_from_info(info: ReplayGainInfo) -> f32 {
    if !info.lufs.is_finite() || info.lufs <= -69.5 {
        return UNITY_GAIN;
    }
    let gain_db = TARGET_LUFS - f64::from(info.lufs);
    let raw_linear = 10f64.powf(gain_db / 20.0) as f32;
    let peak_limited = if info.true_peak_linear > f32::EPSILON {
        (1.0 / info.true_peak_linear).max(0.0)
    } else {
        MAX_GAIN_LINEAR
    };
    raw_linear.min(peak_limited).clamp(0.0, MAX_GAIN_LINEAR)
}

pub fn compute_loudness(path: &Path) -> Result<ReplayGainInfo, String> {
    let opened = decode::open_file(path).map_err(|e| e.to_string())?;
    let channels = opened.channels.max(1);
    let sample_rate = opened.sample_rate_hz.max(1);
    if channels > 8 {
        return Err(format!(
            "ReplayGain: refusing to analyze {} channels",
            channels
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
            lufs: TARGET_LUFS as f32,
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
    fn computes_loudness_for_credits_ogg() {
        let path = PathBuf::from("../../assets/music/credits.ogg");
        if !path.exists() {
            return;
        }
        let info = compute_loudness(&path).expect("loudness");
        assert!(info.lufs.is_finite(), "lufs={}", info.lufs);
        assert!(
            info.true_peak_linear >= 0.0,
            "peak={}",
            info.true_peak_linear
        );
    }
}

//! Differential checks and paired benchmarks against main e8705d58b3.
use super::*;
use std::hint::black_box;
#[path = "../../../tests/support/paired_bench.rs"]
mod paired_bench;

#[test]
fn ramp_conversion_matches_original_sample_bits_and_final_gain() {
    let gains = [
        (0.0, 1.0),
        (1.0, 0.0),
        (0.0, 0.0),
        (-0.0, -0.0),
        (0.125, 0.125),
        (0.0, MUSIC_GAIN_MAX_STEP),
        (0.0, -MUSIC_GAIN_MAX_STEP),
        (0.99, 1.0),
        (1.0, 0.99),
        (0.0, -0.0),
        (0.0, f32::INFINITY),
        (f32::INFINITY, f32::INFINITY),
        (f32::NAN, 1.0),
        (1.0, f32::NAN),
    ];
    for channels in [1, 2, 3, 6, 8] {
        for frames in [0, 1, 2, 7, 257, 2048, 4096] {
            let src: Vec<_> = (0..frames * channels + channels - 1)
                .map(|i| [i16::MIN, -12345, -1, 0, 1, 12345, i16::MAX][i % 7])
                .collect();
            for dst_len in [frames * channels, src.len(), src.len() + channels] {
                for (initial, target) in gains {
                    for volume in [0.0, -0.0, 0.375, 1.0, -1.0, f32::INFINITY, f32::NAN] {
                        let (mut old_gain, mut new_gain) = (initial, initial);
                        let mut old = vec![123.0; dst_len];
                        let mut new = old.clone();
                        original::convert_music_samples(
                            &src,
                            &mut old,
                            channels,
                            volume,
                            target,
                            &mut old_gain,
                        );
                        convert_music_samples(
                            &src,
                            &mut new,
                            channels,
                            volume,
                            target,
                            &mut new_gain,
                        );
                        assert!(
                            old_gain.to_bits() == new_gain.to_bits()
                                || old_gain.is_nan() && new_gain.is_nan()
                        );
                        for (a, b) in old.iter().zip(new) {
                            assert!(
                                a.to_bits() == b.to_bits() || a.is_nan() && b.is_nan(),
                                "channels={channels}, frames={frames}, dst={dst_len}, gain={initial}->{target}, vol={volume}: {a:?} != {b:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "paired release benchmark; run alone with --ignored --nocapture --test-threads=1"]
fn benchmark_music_gain_ramp() {
    for channels in [2, 6] {
        for (label, frames, initial, target) in [
            ("settles first frame", 2048, 0.9999, 1.0),
            ("settles near middle", 2048, 0.75, 1.0),
            ("settles short callback", 256, 0.99, 1.0),
            ("full ramp control", 2048, 0.0, 1.0),
            ("short ramp control", 256, 0.0, 1.0),
            ("flat gain control", 2048, 1.0, 1.0),
        ] {
            let src: Vec<_> = (0..frames * channels).map(|i| i as i16).collect();
            let mut old = vec![0.0; src.len()];
            let mut new = old.clone();
            paired_bench::compare(
                &format!("gain {label}, {channels} channels"),
                10000,
                |current| {
                    let output = if current { &mut new } else { &mut old };
                    let convert = if current {
                        convert_music_samples
                    } else {
                        original::convert_music_samples
                    };
                    let mut gain = black_box(initial);
                    convert(
                        black_box(&src),
                        output,
                        black_box(channels),
                        black_box(0.8),
                        black_box(target),
                        &mut gain,
                    );
                    black_box(output);
                    black_box(gain);
                },
            );
            assert_eq!(old, new);
        }
    }
}

mod original {
    use super::super::{advance_gain, i16_to_f32};
    pub(super) fn convert_music_samples(
        src: &[i16],
        dst: &mut [f32],
        channels: usize,
        music_vol: f32,
        target_gain: f32,
        current_gain: &mut f32,
    ) {
        if *current_gain == target_gain {
            let scale = music_vol * target_gain;
            if scale == 0.0 {
                // Multiplication by signed zero preserves the exact sign bit that
                // the general `(sample / 32768) * scale` path produced, while
                // skipping its division for muted output.
                for (dst, &src) in dst.iter_mut().zip(src) {
                    *dst = f32::from(src) * scale;
                }
                return;
            }
            if scale == 1.0 {
                for (dst, &src) in dst.iter_mut().zip(src) {
                    *dst = i16_to_f32(src);
                }
                return;
            }
            for (dst, &src) in dst.iter_mut().zip(src) {
                *dst = i16_to_f32(src) * scale;
            }
            return;
        }
        if channels == 2 {
            let src_frames = src.as_chunks::<2>().0;
            let dst_frames = dst.as_chunks_mut::<2>().0;
            for (src_frame, dst_frame) in src_frames.iter().zip(dst_frames) {
                advance_gain(current_gain, target_gain);
                let scale = music_vol * *current_gain;
                dst_frame[0] = i16_to_f32(src_frame[0]) * scale;
                dst_frame[1] = i16_to_f32(src_frame[1]) * scale;
            }
            return;
        }
        for (src_frame, dst_frame) in src
            .chunks_exact(channels)
            .zip(dst.chunks_exact_mut(channels))
        {
            advance_gain(current_gain, target_gain);
            let scale = music_vol * *current_gain;
            for (dst, &src) in dst_frame.iter_mut().zip(src_frame) {
                *dst = i16_to_f32(src) * scale;
            }
        }
    }
}

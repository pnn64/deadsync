//! Packet work compared with 0.5.1218 (c5fb50443).
use super::{Encoding, decode_packet_into};
use crate::resample;
use std::hint::black_box;

#[allow(dead_code)]
#[path = "resample_baseline.rs"]
mod old_resample;
#[allow(dead_code)]
#[path = "../../../../tests/support/perf.rs"]
mod perf;

macro_rules! wav_access {
    () => {
        pub(super) fn decode(
            bytes: &[u8],
            kind: usize,
            out: &mut Vec<i16>,
        ) -> Result<(), &'static str> {
            decode_packet_into(
                bytes,
                [
                    Encoding::Pcm8,
                    Encoding::Pcm16,
                    Encoding::Pcm24,
                    Encoding::Pcm32,
                    Encoding::Float32,
                    Encoding::Float64,
                ][kind],
                out,
            )
        }
    };
}
#[allow(dead_code)]
mod old_wav {
    include!("wav_baseline.rs");
    wav_access!();
}
wav_access!();

fn random(seed: &mut u64) -> u64 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    *seed
}

fn floats(n: usize, offset: usize) -> Vec<f32> {
    (0..n)
        .map(|i| ((i * 7919 + offset) % 65536) as f32 / 32768.0 - 1.0)
        .collect()
}

fn compare_output(planar: &[Vec<f32>], requested: usize, channels: usize) {
    for initial in [0, 7, 5000] {
        let mut new = vec![123; initial];
        let mut old = new.clone();
        let a = resample::write_resampler_output(planar, requested, channels, &mut new);
        let b = old_resample::write_resampler_output(planar, requested, channels, &mut old);
        assert_eq!(a, b);
        assert_eq!(new, old);
    }
}

#[test]
fn resampler_output_preserves_channel_mapping_lengths_and_edges() {
    for input_channels in [0, 1, 2, 3, 6, 8] {
        for len in [0, 1, 2, 7, 256, 4096] {
            let mut planar: Vec<_> = (0..input_channels).map(|c| floats(len, c * 173)).collect();
            for output_channels in [0, 1, 2, 3, 6, 8] {
                compare_output(&planar, len + 5, output_channels);
                compare_output(&planar, len / 2, output_channels);
            }
            // Even an unused short channel limits the old mono output length.
            if let Some(last) = planar.last_mut() {
                last.truncate(len / 3);
            }
            compare_output(&planar, len, 1);
        }
    }
    let specials = vec![
        f32::NEG_INFINITY,
        f32::MIN,
        -2.0,
        -1.0,
        -0.0,
        0.0,
        f32::from_bits(1),
        0.5,
        1.0,
        2.0,
        f32::MAX,
        f32::INFINITY,
        f32::from_bits(0x7f800001),
        f32::from_bits(0xffc12345),
    ];
    for channels in [1, 2, 3, 6] {
        compare_output(std::slice::from_ref(&specials), specials.len(), channels);
    }
}

#[test]
fn resampler_output_matches_arbitrary_float_bits() {
    let mut seed = 1219;
    for case in 0..300 {
        let planar: Vec<Vec<f32>> = (0..1 + case % 6)
            .map(|c| {
                (0..case % 97 + c)
                    .map(|_| f32::from_bits(random(&mut seed) as u32))
                    .collect()
            })
            .collect();
        compare_output(&planar, case % 101, 1 + case % 4);
    }
}

fn compare_fade(samples: &[i16], channels: usize, start: u64, fade: (i64, i64)) {
    let mut new = samples.to_vec();
    let mut old = samples.to_vec();
    resample::apply_fade_envelope(&mut new, channels, start, fade);
    old_resample::apply_fade_envelope(&mut old, channels, start, fade);
    assert_eq!(
        new, old,
        "channels={channels}, start={start}, fade={fade:?}"
    );
}

#[test]
fn fade_preserves_boundaries_partial_frames_and_extreme_positions() {
    for channels in [0, 1, 2, 3, 6, 8] {
        for len in [0, 1, 2, 7, 255, 512] {
            let samples: Vec<_> = (0..len).map(|i| (i * 7919) as i16).collect();
            for start in [0, 1, 127, 128, 255, 256, 511, i64::MAX as u64, u64::MAX] {
                for fade in [
                    (0, 0),
                    (0, 256),
                    (256, 0),
                    (-100, 100),
                    (100, -100),
                    (i64::MIN, i64::MAX),
                    (i64::MAX - 1, i64::MAX),
                    (i64::MAX, i64::MIN),
                ] {
                    compare_fade(&samples, channels, start, fade);
                }
            }
        }
    }
    let all_i16: Vec<_> = (i16::MIN..=i16::MAX).collect();
    for channels in [1, 2, 6] {
        for fade in [
            (-32768, 65536),
            (65536, -32768),
            (-2, -1),
            (0, 1_000_000_000),
        ] {
            compare_fade(&all_i16, channels, 0, fade);
        }
    }
}

#[test]
fn fade_matches_random_packets_and_near_unity_rounding() {
    let mut seed = 98765;
    for case in 0..2000 {
        let channels = case % 9;
        let samples: Vec<_> = (0..case % 521).map(|_| random(&mut seed) as i16).collect();
        let start = random(&mut seed) % 100_000;
        let full = (random(&mut seed) % 200_000) as i64 - 50_000;
        let silence = (random(&mut seed) % 200_000) as i64 - 50_000;
        compare_fade(&samples, channels, start, (full, silence));
    }
    for start in 0..130 {
        compare_fade(&[i16::MIN, -1, 0, 1, i16::MAX], 1, start, (0, 1_000_000));
    }
}

fn compare_wav(bytes: &[u8], kind: usize) {
    for initial in [0, 3, 5000] {
        let mut new = vec![123; initial];
        let mut old = new.clone();
        assert_eq!(
            decode(bytes, kind, &mut new),
            old_wav::decode(bytes, kind, &mut old)
        );
        assert_eq!(new, old, "encoding {kind}");
    }
}

#[test]
fn wav_packets_preserve_formats_nonfinite_samples_and_error_mutation() {
    let mut seed = 3456;
    for (kind, width) in [1, 2, 3, 4, 4, 8].into_iter().enumerate() {
        for count in [0, 1, 3, 256, 4096] {
            let mut bytes: Vec<u8> = (0..count * width)
                .map(|_| random(&mut seed) as u8)
                .collect();
            compare_wav(&bytes, kind);
            for _ in 1..width {
                bytes.push(0xa5);
                compare_wav(&bytes, kind);
            }
        }
    }
    let special = [
        f64::NEG_INFINITY,
        f64::MIN,
        -2.0,
        -1.0,
        -0.0,
        0.0,
        f64::from_bits(1),
        0.5,
        1.0,
        2.0,
        f64::MAX,
        f64::INFINITY,
        f64::from_bits(0x7ff0000000000001),
        f64::from_bits(0xfff8123456789abc),
    ];
    compare_wav(
        &special
            .iter()
            .flat_map(|v| (*v as f32).to_le_bytes())
            .collect::<Vec<_>>(),
        4,
    );
    compare_wav(
        &special
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>(),
        5,
    );
}

#[test]
fn wav_float_rounding_matches_all_i16_halfway_boundaries() {
    let values: Vec<_> = (-32769..=32768)
        .flat_map(|i| {
            let value = (f64::from(i) + 0.5) / 32767.0;
            [value.next_down(), value, value.next_up()]
        })
        .collect();
    compare_wav(
        &values
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>(),
        5,
    );
    compare_wav(
        &values
            .iter()
            .flat_map(|v| (*v as f32).to_le_bytes())
            .collect::<Vec<_>>(),
        4,
    );
}

#[test]
fn retained_packet_buffers_keep_capacity_and_zero_churn() {
    let planar = vec![floats(4096, 0)];
    let bytes: Vec<_> = planar[0].iter().flat_map(|v| v.to_le_bytes()).collect();
    let mut out = vec![0; 4096];
    let capacity = out.capacity();
    let address = out.as_ptr();
    perf::assert_no_churn(|| {
        for _ in 0..32 {
            out.truncate(1);
            decode(black_box(&bytes), 4, &mut out).unwrap();
            resample::write_resampler_output(black_box(&planar), 4096, 1, &mut out);
            resample::apply_fade_envelope(&mut out, 2, 0, (0, 4096));
            resample::apply_fade_envelope(&mut out, 2, 4096, (0, 1024));
        }
    });
    assert_eq!(out.capacity(), capacity);
    assert_eq!(out.as_ptr(), address);
}

#[test]
#[ignore = "manual release comparison; run serially with --nocapture"]
fn benchmark_packet_work() {
    let versions = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    for (input_channels, output_channels, frames) in [
        (1, 1, 16),
        (1, 1, 256),
        (1, 1, 4096),
        (2, 1, 256),
        (6, 1, 256),
        (1, 2, 256),
        (2, 2, 256),
        (6, 6, 256),
    ] {
        let planar: Vec<_> = (0..input_channels)
            .map(|c| floats(frames, c * 199))
            .collect();
        compare_output(&planar, frames, output_channels);
        let mut out = vec![0; frames * output_channels];
        for previous in versions {
            let run = black_box(if previous {
                old_resample::write_resampler_output
            } else {
                resample::write_resampler_output
            });
            perf::measure_sampled(
                &format!(
                    "resample/{input_channels}to{output_channels}-{frames}/{}",
                    if previous { "old" } else { "new" }
                ),
                4096,
                frames * output_channels,
                || {
                    black_box(run(
                        black_box(&planar),
                        black_box(frames),
                        black_box(output_channels),
                        black_box(&mut out),
                    ));
                    black_box(&out);
                },
            );
        }
    }
    for (label, channels, start, fade) in [
        ("stereo-fade", 2, 0, (0, 4096)),
        ("mono-fade", 1, 0, (0, 4096)),
        ("surround-fade", 6, 0, (0, 4096)),
        ("silent", 2, 8192, (0, 4096)),
        ("unity", 2, 0, (4096, 8192)),
        ("fade-in", 2, 0, (4096, 0)),
    ] {
        let input: Vec<i16> = (0..512 * channels).map(|i| (i * 7919) as i16).collect();
        compare_fade(&input, channels, start, fade);
        let mut out = input.clone();
        for previous in versions {
            let run = black_box(if previous {
                old_resample::apply_fade_envelope
            } else {
                resample::apply_fade_envelope
            });
            perf::measure_sampled(
                &format!("fade/{label}/{}", if previous { "old" } else { "new" }),
                2048,
                input.len(),
                || {
                    out.copy_from_slice(black_box(&input));
                    run(
                        black_box(&mut out),
                        black_box(channels),
                        black_box(start),
                        black_box(fade),
                    );
                    black_box(&out);
                },
            );
        }
    }
    for kind in [1, 4, 5] {
        let values = (0..8192).map(|i| ((i * 7919 % 65536) as f64 - 32768.0) / 32768.0);
        let bytes: Vec<_> = match kind {
            1 => values
                .flat_map(|v| ((v * 32767.0) as i16).to_le_bytes())
                .collect(),
            4 => values.flat_map(|v| (v as f32).to_le_bytes()).collect(),
            _ => values.flat_map(f64::to_le_bytes).collect(),
        };
        compare_wav(&bytes, kind);
        let mut out = vec![0; 8192];
        for mode in ["warm", "regrow", "cold"] {
            for previous in versions {
                let run = black_box(if previous { old_wav::decode } else { decode });
                perf::measure_sampled(
                    &format!("wav/{kind}-{mode}/{}", if previous { "old" } else { "new" }),
                    512,
                    8192,
                    || {
                        if mode == "cold" {
                            let mut out = Vec::new();
                            run(black_box(&bytes), black_box(kind), &mut out).unwrap();
                            black_box(&out);
                        } else {
                            if mode == "regrow" {
                                out.truncate(1);
                            }
                            run(black_box(&bytes), black_box(kind), black_box(&mut out)).unwrap();
                            black_box(&out);
                        }
                    },
                );
            }
        }
    }
}

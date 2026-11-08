use super::{sample_to_i16, write_resampler_output};
use std::{hint::black_box, time::Instant};

use super::kernel_bench as bench;

// The scalar conversion before the bounded, packed-conversion change.
#[inline(always)]
fn original_sample(sample: f32) -> i16 {
    let scaled = sample * 32767.0;
    let whole = scaled as i16;
    let fraction = scaled - f32::from(whole);
    (i32::from(whole) + i32::from(fraction >= 0.5) - i32::from(fraction <= -0.5))
        .clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

fn samples() -> Vec<f32> {
    let mut values = vec![
        0.0,
        -0.0,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::MAX,
        f32::MIN,
        f32::from_bits(1),
        -f32::from_bits(1),
        1.0,
        -1.0,
        1.5,
        -1.5,
    ];
    for bits in [
        0x7f80_0001,
        0x7fc0_0000,
        0x7fff_ffff,
        0xff80_0001,
        0xffc0_1234,
    ] {
        values.push(f32::from_bits(bits));
    }
    // Neighbors of every positive/negative half tie and clipping boundary.
    for whole in -32769..=32768 {
        let sample = (whole as f32 + 0.5) / 32767.0;
        values.extend([sample.next_down(), sample, sample.next_up()]);
    }
    let mut state = 0x1234_5678_u32;
    for _ in 0..65536 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        values.push(f32::from_bits(state));
    }
    values
}

#[test]
fn pcm_rounding_matches_original_and_round_at_boundaries() {
    for sample in samples() {
        let expected = original_sample(sample);
        assert_eq!(expected, (sample * 32767.0).round() as i16);
        assert_eq!(
            sample_to_i16(black_box(sample)),
            expected,
            "{:08x}",
            sample.to_bits()
        );
    }
}

#[test]
fn pcm_output_matches_scalar_across_layouts_tails_and_storage_reuse() {
    let values = samples();
    // Exercise all rounding boundaries and random bit patterns through SIMD too.
    for (in_ch, out_ch) in [(1, 1), (1, 2), (2, 2)] {
        let input: Vec<Vec<_>> = (0..in_ch)
            .map(|channel| {
                values
                    .iter()
                    .cycle()
                    .skip(channel)
                    .take(values.len())
                    .copied()
                    .collect()
            })
            .collect();
        let mut output = Vec::new();
        assert_eq!(
            write_resampler_output(&input, values.len(), out_ch, &mut output),
            values.len()
        );
        for (frame, actual) in output.chunks_exact(out_ch).enumerate() {
            for (channel, &actual) in actual.iter().enumerate() {
                assert_eq!(actual, original_sample(input[channel % in_ch][frame]));
            }
        }
    }
    let mut cases = 0;
    for in_ch in 0..=8 {
        for out_ch in 0..=8 {
            for frames in [0, 1, 2, 3, 4, 5, 7, 8, 15, 255, 256, 257, 1024] {
                for shortened in 0..=in_ch {
                    let input: Vec<Vec<_>> = (0..in_ch)
                        .map(|channel| {
                            let len = if shortened == channel {
                                frames / 2
                            } else {
                                frames
                            };
                            (0..len)
                                .map(|i| values[(i * 977 + channel) % values.len()])
                                .collect()
                        })
                        .collect();
                    let expected_frames = if out_ch == 0 {
                        0
                    } else {
                        (frames + 1).min(input.iter().map(Vec::len).min().unwrap_or(0))
                    };
                    let expected: Vec<_> = (0..expected_frames)
                        .flat_map(|frame| (0..out_ch).map(move |channel| (frame, channel)))
                        .map(|(frame, channel)| original_sample(input[channel % in_ch][frame]))
                        .collect();
                    for initial_len in [0, frames * 12] {
                        let mut output = vec![123; initial_len];
                        let pointer = output.as_ptr();
                        let capacity = output.capacity();
                        assert_eq!(
                            write_resampler_output(&input, frames + 1, out_ch, &mut output),
                            expected_frames
                        );
                        assert_eq!(
                            output, expected,
                            "{in_ch}->{out_ch}, frames={frames}, short={shortened}"
                        );
                        if capacity >= expected.len() {
                            assert_eq!(output.capacity(), capacity);
                            assert_eq!(output.as_ptr(), pointer);
                        }
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 10530);
}

#[test]
#[ignore = "exhaustive 2^32 float comparison; run in release with --ignored --nocapture"]
fn exhaustive_pcm_conversion_matches_every_f32_pattern() {
    let start = Instant::now();
    for bits in 0..=u32::MAX {
        let input = black_box(f32::from_bits(bits));
        assert_eq!(
            sample_to_i16(input),
            original_sample(input),
            "input={bits:08x}"
        );
    }
    println!(
        "All 4294967296 f32 bit patterns match in {:?}",
        start.elapsed()
    );
}

#[inline(never)]
fn convert(input: &[Vec<f32>], frames: usize, channels: usize, output: &mut Vec<i16>) -> usize {
    write_resampler_output(input, frames, channels, output)
}

#[test]
#[ignore = "release benchmark; archive before/after binaries and run alone with --ignored --nocapture --test-threads=1"]
fn benchmark_resampler_pcm_conversion() {
    bench::pin_thread();
    let rotation = std::env::var("RESAMPLER_BENCH_ROTATION")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let cases = [
        ("stereo-4", 2, 2, 4),
        ("stereo-256", 2, 2, 256),
        ("stereo-257", 2, 2, 257),
        ("stereo-1024", 2, 2, 1024),
        ("stereo-4096", 2, 2, 4096),
        ("mono-to-stereo-256", 1, 2, 256),
        ("mono-to-stereo-1024", 1, 2, 1024),
        ("mono-256", 1, 1, 256),
        ("mono-1024", 1, 1, 1024),
        ("six-to-stereo-256", 6, 2, 256),
        ("stereo-to-six-256", 2, 6, 256),
        ("stereo-1", 2, 2, 1),
        ("empty-output", 2, 2, 0),
    ];
    for i in 0..cases.len() {
        let (name, in_ch, out_ch, frames) = cases[(i + rotation) % cases.len()];
        let input: Vec<Vec<f32>> = (0..in_ch)
            .map(|channel| {
                (0..frames)
                    .map(|j| ((j * 977 + channel * 37) % 65536) as f32 / 32768.0 - 1.0)
                    .collect()
            })
            .collect();
        let mut output = Vec::with_capacity(frames * out_ch);
        assert_eq!(convert(&input, frames, out_ch, &mut output), frames);
        let iterations = 1_000_000 / frames.max(1) + 500;
        for round in 0..10 {
            let cycles = bench::cycles();
            let start = Instant::now();
            for _ in 0..iterations {
                black_box(convert(
                    black_box(&input),
                    black_box(frames),
                    black_box(out_ch),
                    black_box(&mut output),
                ));
            }
            black_box(&output);
            let ns = start.elapsed().as_nanos() as f64 / iterations as f64;
            let cycles = (bench::cycles() - cycles) as f64 / iterations as f64;
            println!("resampler-pcm,{name},{frames},{round},{iterations},{cycles:.4},{ns:.4}");
        }
        assert_eq!(output.capacity(), frames * out_ch);
    }
}

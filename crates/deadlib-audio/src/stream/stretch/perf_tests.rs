//! Production SOLA kernels, scalar oracle, and full-stream PCM fingerprints.
use super::super::perf_tests::allocations;
use super::*;
use std::hint::black_box;

#[path = "../../../../../tests/support/kernel_bench.rs"]
pub(crate) mod bench;
#[path = "rate_bench.rs"]
mod rates;

fn noise(len: usize, mut seed: u32) -> Vec<i16> {
    (0..len)
        .map(|_| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 16) as i16
        })
        .collect()
}

pub(super) fn reference(buffer: &[i16], pattern: &[i16]) -> usize {
    let mut best = f32::INFINITY;
    let mut offset = 0;
    for (index, candidate) in buffer.windows(pattern.len()).enumerate() {
        let score = candidate
            .iter()
            .zip(pattern)
            .fold(0.0f32, |score, (&a, &b)| {
                score + (f32::from(a) * I16_SCALE - f32::from(b) * I16_SCALE).abs()
            });
        if score < best {
            best = score;
            offset = index;
        }
    }
    offset
}

#[test]
fn correlation_matches_scalar_oracle() {
    for len in [1, 7, 8, 9, 15, 16, 17, 31, 64, 127, 360, 720] {
        for seed in 0..24 {
            let mut left = noise(len + 97, seed);
            let mut right = noise(len + 97, seed + 100);
            let mut lp = noise(len, seed + 200);
            let mut rp = noise(len, seed + 300);
            match seed % 8 {
                0 => {
                    left.fill(0);
                    lp.fill(0);
                }
                1 => {
                    right.fill(0);
                    rp.fill(0);
                }
                2 => {
                    left.fill(i16::MIN);
                    lp.fill(0);
                }
                3 => {
                    left[3] = i16::MIN;
                    right[4] = i16::MAX;
                }
                4 => {
                    left[97..].copy_from_slice(&lp);
                    right[31..31 + len].copy_from_slice(&rp);
                }
                5 => {
                    right.clone_from(&left);
                    rp.clone_from(&lp);
                }
                6 => {
                    left.fill(8192);
                    right.fill(-8192);
                }
                _ => {}
            }
            let expected = (reference(&left, &lp), reference(&right, &rp));
            assert_eq!(find_closest_match(&left, &lp), expected.0);
            assert_eq!(
                find_closest_match_stereo(&left, &lp, &right, &rp),
                expected,
                "len={len} seed={seed}"
            );
        }
    }
}

#[test]
fn correlation_preserves_winners_at_the_exact_float_limit() {
    for len in [264, 272, 360, 720, 1440] {
        let pattern = vec![i16::MIN; len];
        for extra in [254, 255, 256, 257, 258] {
            // 256 full-scale differences plus 256 PCM quanta is exactly 512.
            // Exercise integer winners and float-rounded ties on either side.
            let mut input = vec![i16::MIN; len + 97];
            input[..256].fill(i16::MAX);
            input[256] = i16::MIN + extra;
            assert_eq!(
                find_closest_match(&input, &pattern),
                reference(&input, &pattern)
            );
        }
        for seed in 0..12 {
            let mut input = noise(len + 97, seed);
            let mut pattern = noise(len, seed + 12);
            if seed % 2 == 0 {
                input.iter_mut().for_each(|sample| *sample /= 128);
                pattern.iter_mut().for_each(|sample| *sample /= 128);
            }
            assert_eq!(
                find_closest_match(&input, &pattern),
                reference(&input, &pattern)
            );
        }
    }
}

#[test]
fn retiring_and_refilling_preserves_live_pcm_with_packet_size_changes() {
    for channels in [1, 2, 6, 16] {
        let mut stretch = SolaStretcher::new(channels, 48_000);
        let mut expected = vec![Vec::new(); channels];
        for (iteration, frames) in [65_536, 1, 137, 997, 4096, 32_768, 137, 997]
            .into_iter()
            .enumerate()
        {
            let retired = stretch.data_avail_frames.saturating_sub(73);
            for channel in &mut stretch.state {
                channel.correlated_pos = retired;
            }
            stretch.min_correlated_pos = retired;
            stretch.max_correlated_pos = retired;
            stretch.uncorrelated_pos = retired;
            stretch.erase_front(retired);
            for channel in &mut expected {
                channel.drain(..retired);
            }
            let input = noise(frames * channels, iteration as u32);
            stretch.push_interleaved_i16(&input);
            for (index, (channel, expected)) in stretch.state.iter().zip(&mut expected).enumerate()
            {
                expected.extend(input.iter().skip(index).step_by(channels).copied());
                assert_eq!(&channel.data[stretch.data_start..], expected);
            }
        }
    }
}

#[test]
fn grouped_pcm_scores_preserve_rounding_and_rejection() {
    for len in [
        0, 1, 7, 8, 9, 31, 255, 256, 257, 263, 264, 265, 271, 272, 273, 360, 720, 1440, 4097,
    ] {
        for seed in 0..18 {
            let mut frames = noise(len, seed);
            let mut pattern = noise(len, seed + 7);
            if seed == 0 {
                frames.fill(i16::MAX);
                pattern.fill(i16::MIN);
            }
            if seed == 1 {
                frames.fill(1);
                pattern.fill(i16::MIN);
            }
            if seed == 16 {
                // Exactly 496 normalized units after 256 samples, followed
                // by ordinary PCM that crosses into scalar float rounding.
                frames[..len.min(256)].fill(30_720);
                pattern[..len.min(256)].fill(i16::MIN);
            }
            if seed == 17 {
                frames.iter_mut().for_each(|v| *v /= 8192);
                pattern.iter_mut().for_each(|v| *v /= 8192);
            }
            for best in [
                0.0,
                I16_SCALE,
                7.0 * I16_SCALE,
                8.0 * I16_SCALE,
                1.0,
                8.0,
                128.0,
                496.0 - I16_SCALE,
                496.0,
                496.0 + I16_SCALE,
                512.0 - I16_SCALE,
                512.0,
                512.0 + 2.0 * I16_SCALE,
                1000.0,
                f32::MAX,
                f32::INFINITY,
            ] {
                let mut expected = 0.0f32;
                for (index, (&a, &b)) in frames.iter().zip(&pattern).enumerate() {
                    expected += (f32::from(a) * I16_SCALE - f32::from(b) * I16_SCALE).abs();
                    if (index + 1) % 8 == 0 && expected >= best {
                        break;
                    }
                }
                assert_eq!(
                    correlation_score(&frames, &pattern, best).to_bits(),
                    expected.to_bits(),
                    "len={len} seed={seed} best={best}"
                );
            }
        }
    }
}

fn stream_input(channels: usize, kind: &str) -> Vec<i16> {
    stream_input_frames(channels, kind, 12_001)
}

fn stream_input_frames(channels: usize, kind: &str, frames: usize) -> Vec<i16> {
    let mut state = 7u32;
    (0..frames * channels)
        .map(|index| {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            if kind == "silence" || (kind == "left-silent" && index % channels == 0) {
                0
            } else if kind == "tone" {
                ((index / channels) as f32 * 0.057)
                    .sin()
                    .mul_add(12_000.0, 0.0) as i16
            } else {
                (state >> 16) as i16
            }
        })
        .collect()
}

#[test]
fn deinterleaving_preserves_every_i16_value_and_channel() {
    for channels in [1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 16] {
        let input = (0..65_536 * channels)
            .map(|i| ((i / channels) as u16).wrapping_add((i % channels) as u16 * 97) as i16)
            .collect::<Vec<_>>();
        let mut stretch = SolaStretcher::new(channels, 48_000);
        stretch.push_interleaved_i16(&input);
        for (index, channel) in stretch.state.iter().enumerate() {
            assert_eq!(channel.data.len(), 65_536);
            for (frame, &sample) in channel.data.iter().enumerate() {
                assert_eq!(sample, input[frame * channels + index]);
            }
        }
    }
}

#[test]
fn fixed_layout_unpacking_handles_block_boundaries_and_reused_buffers() {
    for channels in [6, 8, 16] {
        let frames = 4096 / channels + 3;
        let input = (0..frames * channels)
            .map(|i| (i as u32).wrapping_mul(503).wrapping_add(65_500) as i16)
            .collect::<Vec<_>>();
        let mut stretch = SolaStretcher::new(channels, 48_000);
        for _ in 0..2 {
            stretch.push_interleaved_i16(&input);
            assert_eq!(stretch.buffered_source_frames(), frames);
            for (index, channel) in stretch.state.iter().enumerate() {
                assert_eq!(channel.data.len(), frames);
                for (frame, &sample) in channel.data.iter().enumerate() {
                    assert_eq!(sample, input[frame * channels + index]);
                }
            }
            stretch.reset();
        }
    }
}

#[test]
fn dispatched_crossfade_matches_scalar_bits() {
    for frames in [0, 1, 7, 8, 15, 31, 256, 1440] {
        let prev = noise(frames, 7);
        let current = noise(frames, 11);
        let weights = (0..frames)
            .map(|i| (i + 137) as f32 / 2048.0)
            .collect::<Vec<_>>();
        let expected = prev
            .iter()
            .zip(&current)
            .zip(&weights)
            .map(|((&a, &b), &t)| {
                let a = f32::from(a) * I16_SCALE;
                let b = f32::from(b) * I16_SCALE;
                (b - a).mul_add(t, a).to_bits()
            })
            .collect::<Vec<_>>();
        let mut actual = Vec::new();
        append_crossfade(&prev, &current, &weights, &mut actual);
        assert_eq!(
            actual.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            expected
        );
        actual.clear();
        append_crossfade(&prev, &prev, &weights, &mut actual);
        assert_eq!(
            actual.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
            prev.iter()
                .map(|&v| (f32::from(v) * I16_SCALE).to_bits())
                .collect::<Vec<_>>()
        );
    }
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    println!(
        "FMA available: {}; AVX2 available: {}",
        std::arch::is_x86_feature_detected!("fma"),
        std::arch::is_x86_feature_detected!("avx2")
    );
}

#[test]
fn crossfade_preserves_original_float_formula_across_pcm_range() {
    let previous: Vec<_> = (0..=u16::MAX).map(|sample| sample as i16).collect();
    let current: Vec<_> = (0..=u16::MAX).map(|sample| !sample as i16).collect();
    let weights: Vec<_> = (0..previous.len())
        .map(|i| (i % 1440 + 137) as f32 / 2048.0)
        .collect();
    let mut output = Vec::new();
    append_crossfade(&previous, &current, &weights, &mut output);
    assert_eq!(output.len(), previous.len());
    for (((&a, &b), &weight), &actual) in previous.iter().zip(&current).zip(&weights).zip(&output) {
        let a = f32::from(a) * I16_SCALE;
        let b = f32::from(b) * I16_SCALE;
        assert_eq!(actual.to_bits(), (b - a).mul_add(weight, a).to_bits());
    }
}

#[test]
#[ignore = "writes complete PCM for byte comparison between archived sources"]
fn dump_sola_pcm() {
    let mut bytes = Vec::new();
    for (channels, hz, kind, frames) in [
        (1, 8_000, "noise", 12_001),
        (1, 48_000, "tone", 12_001),
        (2, 44_100, "noise", 12_001),
        (2, 48_000, "left-silent", 12_001),
        (2, 96_000, "tone", 12_001),
        (2, 48_000, "silence", 12_001),
        (5, 48_000, "noise", 12_001),
        (6, 48_000, "noise", 12_001),
        (8, 44_100, "tone", 12_001),
        (16, 48_000, "noise", 12_001),
        (2, 48_000, "noise", 65_536),
        (6, 48_000, "silence", 65_536),
    ] {
        let input = stream_input_frames(channels, kind, frames);
        for (packet, cap) in [(137, 7), (997, 256), (4096, 997), (65_536, 256)] {
            let mut stretch = SolaStretcher::new(channels, hz);
            let mut output = vec![Vec::new(); channels];
            for _ in 0..2 {
                stretch.reset();
                for (index, input) in input.chunks(packet * channels).enumerate() {
                    stretch.set_speed_ratio([1.2, 0.8, 2.0, 0.5][index % 4]);
                    stretch.push_interleaved_i16(input);
                    loop {
                        for ch in &mut output {
                            ch.clear();
                        }
                        let count = stretch.pull(&mut output, cap);
                        bytes.extend_from_slice(&(count as u64).to_le_bytes());
                        for ch in &output {
                            for sample in ch {
                                bytes.extend_from_slice(&sample.to_bits().to_le_bytes());
                            }
                        }
                        if count == 0 {
                            break;
                        }
                    }
                }
                stretch.finish();
                loop {
                    for ch in &mut output {
                        ch.clear();
                    }
                    let count = stretch.pull(&mut output, cap);
                    bytes.extend_from_slice(&(count as u64).to_le_bytes());
                    for ch in &output {
                        for sample in ch {
                            bytes.extend_from_slice(&sample.to_bits().to_le_bytes());
                        }
                    }
                    if count == 0 {
                        break;
                    }
                }
            }
        }
    }
    std::fs::write(
        std::env::var_os("SOLA_PCM_OUT").expect("SOLA_PCM_OUT path"),
        bytes,
    )
    .unwrap();
}

fn drain(stretch: &mut SolaStretcher, input: &[i16], output: &mut [Vec<f32>]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    let mut consume = |stretch: &mut SolaStretcher| {
        loop {
            for channel in output.iter_mut() {
                channel.clear();
            }
            if stretch.pull(output, 256) == 0 {
                break;
            }
            for channel in output.iter() {
                for sample in channel {
                    hash = (hash ^ u64::from(sample.to_bits())).wrapping_mul(0x100000001b3);
                }
            }
        }
    };
    for packet in input.chunks(997 * stretch.channels) {
        stretch.push_interleaved_i16(packet);
        consume(stretch);
    }
    stretch.finish();
    consume(stretch);
    hash
}

#[test]
#[ignore = "CPU-cycle benchmark; run with one test thread"]
fn benchmark_sola_kernels() {
    for channels in [6, 8, 16] {
        let input = stream_input(channels, "noise");
        let (storage, counts) = allocations::measure(|| {
            let mut stretch = SolaStretcher::new(channels, 48_000);
            stretch.push_interleaved_i16(&input);
            stretch
        });
        println!("sola/append-{channels}ch: storage {counts:?}");
        black_box(storage);
        let mut stretch = SolaStretcher::new(channels, 48_000);
        stretch.push_interleaved_i16(&input);
        let mut work = || {
            stretch.reset();
            stretch.push_interleaved_i16(black_box(&input));
            black_box(&stretch.state);
        };
        let label = format!("sola/append-{channels}ch");
        let (_, counts) = allocations::measure(&mut work);
        println!("{label}: allocation {counts:?}");
        bench::measure(&label, 1024, work);
    }
    for frames in [256, 1440] {
        let prev = noise(frames, 7);
        let current = noise(frames, 11);
        let weights = (0..frames)
            .map(|i| i as f32 / frames as f32)
            .collect::<Vec<_>>();
        let mut output = Vec::with_capacity(frames);
        let mut work = || {
            output.clear();
            append_crossfade(
                black_box(&prev),
                black_box(&current),
                black_box(&weights),
                &mut output,
            );
            black_box(&output);
        };
        let label = format!("sola/crossfade-{frames}");
        let (_, counts) = allocations::measure(&mut work);
        println!("{label}: allocation {counts:?}");
        bench::measure(&label, 16_384, work);
    }
    for len in [360, 720] {
        let left = noise(len * 2, 1);
        let right = noise(len * 2, 2);
        let lp = noise(len, 3);
        let rp = noise(len, 4);
        let zero = vec![0; len * 2];
        for kind in ["mono", "stereo", "left-silent", "right-silent"] {
            let work = || match kind {
                "mono" => {
                    black_box(find_closest_match(black_box(&left), black_box(&lp)));
                }
                "left-silent" => {
                    black_box(find_closest_match_stereo(
                        black_box(&zero),
                        black_box(&zero[..len]),
                        black_box(&right),
                        black_box(&rp),
                    ));
                }
                "right-silent" => {
                    black_box(find_closest_match_stereo(
                        black_box(&left),
                        black_box(&lp),
                        black_box(&zero),
                        black_box(&zero[..len]),
                    ));
                }
                _ => {
                    black_box(find_closest_match_stereo(
                        black_box(&left),
                        black_box(&lp),
                        black_box(&right),
                        black_box(&rp),
                    ));
                }
            };
            let label = format!("sola/{kind}/{len}");
            let (_, counts) = allocations::measure(work);
            println!("{label}: allocation {counts:?}");
            bench::measure(&label, 256, work);
        }
    }
    for (channels, kind, rate) in [
        (1, "noise", 1.2),
        (2, "noise", 0.8),
        (2, "tone", 1.2),
        (2, "left-silent", 1.2),
        (2, "silence", 1.2),
        (6, "noise", 1.2),
    ] {
        let input = stream_input(channels, kind);
        let mut stretch = SolaStretcher::new(channels, 48_000);
        stretch.set_speed_ratio(rate);
        let mut output = vec![Vec::with_capacity(256); channels];
        let hash = drain(&mut stretch, &input, &mut output);
        let label = format!("sola/stream-{channels}ch-{kind}-{rate}");
        println!("{label}: fingerprint {hash:016x}");
        let mut work = || {
            stretch.reset();
            black_box(drain(&mut stretch, black_box(&input), &mut output));
        };
        let (_, counts) = allocations::measure(&mut work);
        println!("{label}: allocation {counts:?}");
        bench::measure(&label, 32, work);
    }
}

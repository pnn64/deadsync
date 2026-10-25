use deadlib_audio_decode::resample::{
    append_channel_mapped_i16, apply_fade_envelope, write_resampler_output,
};
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

#[test]
fn pcm_rounding_matches_reference_at_every_half_boundary_and_float_extremes() {
    let mut input = Vec::new();
    for integer in -32770..=32770 {
        let center = (integer as f32 + 0.5) / 32767.0;
        input.extend([
            center.next_down().next_down(),
            center.next_down(),
            center,
            center.next_up(),
            center.next_up().next_up(),
        ]);
    }
    // Sample every exponent, both signs and NaN kinds, plus arbitrary mantissas.
    for exponent in 0..=255u32 {
        for sign in [0, 1 << 31] {
            for mantissa in [0, 1, 0x3f_ffff, 0x40_0000, 0x7f_fffe, 0x7f_ffff] {
                input.push(f32::from_bits(sign | (exponent << 23) | mantissa));
            }
        }
    }
    let mut state = 0x1234_5678u32;
    for _ in 0..1_000_000 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        input.push(f32::from_bits(state));
    }
    let expected: Vec<_> = input
        .iter()
        .map(|sample| (sample * 32767.0).round() as i16)
        .collect();
    let mut output = Vec::with_capacity(input.len());
    let input = [input];
    perf::assert_no_churn(|| {
        assert_eq!(
            write_resampler_output(&input, input[0].len(), 1, &mut output),
            input[0].len()
        );
    });
    assert_eq!(output, expected);
}

#[test]
fn mono_append_preserves_all_samples_prefix_tail_and_allocation() {
    let input: Vec<_> = (i16::MIN..=i16::MAX).collect();
    for channels in [2usize, 3, 4, 6, 8, 9] {
        for tail in 0..channels {
            let len = input.len() - tail;
            let mut output = Vec::with_capacity(2 + len / channels);
            output.extend([123, -456]);
            let pointer = output.as_ptr();
            let expected: Vec<_> = [123, -456]
                .into_iter()
                .chain((0..len / channels).map(|frame| input[frame * channels]))
                .collect();
            perf::assert_no_churn(|| {
                assert_eq!(
                    append_channel_mapped_i16(&input[..len], channels, 1, &mut output),
                    len / channels
                );
            });
            assert_eq!(output, expected);
            assert_eq!(output.as_ptr(), pointer);
        }
    }
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_slice_kernels() {
    for frames in [64usize, 256, 4096] {
        for (in_ch, out_ch) in [(2, 1), (6, 1), (8, 1), (2, 2), (1, 2), (2, 6)] {
            let input: Vec<i16> = (0..frames * in_ch)
                .map(|i| i.wrapping_mul(71) as i16)
                .collect();
            let mut output = Vec::with_capacity(frames * out_ch + 2);
            output.extend([123, -456]);
            let label = format!("append/{in_ch}to{out_ch}/frames={frames}");
            let mut work = || {
                output.truncate(2);
                let append = black_box(
                    append_channel_mapped_i16 as fn(&[i16], usize, usize, &mut Vec<i16>) -> usize,
                );
                black_box(append(
                    black_box(&input),
                    black_box(in_ch),
                    black_box(out_ch),
                    black_box(&mut output),
                ));
                black_box(&output);
            };
            work();
            let (_, counts) = perf::measure(&mut work);
            println!("{label}: allocation {counts:?}");
            bench::measure(&label, (1_000_000 / frames).max(100), &mut work);
        }
        for channels in [1, 2, 6] {
            let input: Vec<i16> = (0..frames * channels)
                .map(|i| i.wrapping_mul(71) as i16)
                .collect();
            let mut output = input.clone();
            let label = format!("fade/channels={channels}/frames={frames}");
            let mut work = || {
                output.copy_from_slice(&input);
                let fade = black_box(apply_fade_envelope as fn(&mut [i16], usize, u64, (i64, i64)));
                fade(
                    black_box(&mut output),
                    black_box(channels),
                    black_box(50),
                    black_box((0, 5000)),
                );
                black_box(&output);
            };
            let (_, counts) = perf::measure(&mut work);
            println!("{label}: allocation {counts:?}");
            bench::measure(&label, (1_000_000 / frames).max(100), &mut work);
        }
        for channels in [1usize, 2, 6] {
            let input: Vec<Vec<f32>> = (0..channels)
                .map(|ch| {
                    (0..frames)
                        .map(|i| ((i * 71 + ch) as i16) as f32 / 32768.0)
                        .collect()
                })
                .collect();
            let mut output = Vec::with_capacity(frames * channels);
            let label = format!("resampler/channels={channels}/frames={frames}");
            let mut work = || {
                let write = black_box(
                    write_resampler_output as fn(&[Vec<f32>], usize, usize, &mut Vec<i16>) -> usize,
                );
                black_box(write(
                    black_box(&input),
                    black_box(frames),
                    black_box(channels),
                    black_box(&mut output),
                ));
                black_box(&output);
            };
            work();
            let (_, counts) = perf::measure(&mut work);
            println!("{label}: allocation {counts:?}");
            bench::measure(&label, (1_000_000 / frames).max(100), &mut work);
        }
    }
}

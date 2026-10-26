use deadlib_audio_decode::resample::{PlanarAccum, append_channel_mapped_i16};
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

#[test]
fn planar_preserves_every_pcm_value_and_partial_frames() {
    for channels in [1usize, 2, 3, 4, 6, 8] {
        let input: Vec<_> = (i16::MIN..=i16::MAX).collect();
        let mut planar = PlanarAccum::new(channels, 0);
        planar.push_i16_interleaved(&input, channels);
        let frames = input.len() / channels;
        for (index, channel) in planar.channels.iter().enumerate() {
            let expected: Vec<_> = input[..frames * channels]
                .iter()
                .skip(index)
                .step_by(channels)
                .map(|&value| (f32::from(value) / 32768.0).to_bits())
                .collect();
            assert_eq!(
                channel.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
                expected
            );
        }
    }
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_planar_kernel() {
    for channels in [1usize, 2, 3, 4, 6, 8] {
        for frames in [64, 4096] {
            let input: Vec<i16> = (0..frames * channels)
                .map(|i| i.wrapping_mul(71) as i16)
                .collect();
            let mut planar = PlanarAccum::new(channels, frames);
            let label = format!("planar/channels={channels}/frames={frames}");
            let (_, counts) = perf::measure(|| planar.push_i16_interleaved(&input, channels));
            println!("{label}: allocation {counts:?}");
            bench::measure(&label, 2_000_000 / (frames * channels), || {
                let append: fn(&mut PlanarAccum, &[i16], usize) =
                    black_box(PlanarAccum::push_i16_interleaved);
                planar.clear();
                append(
                    black_box(&mut planar),
                    black_box(&input),
                    black_box(channels),
                );
                black_box(&planar.channels);
            });
        }
    }
}

#[test]
fn channel_mapping_preserves_prefix_and_complete_frames() {
    let input: Vec<i16> = (i16::MIN..i16::MIN + 257).collect();
    for in_ch in 0..=9 {
        for out_ch in 0..=9 {
            for length in [0, 1, 3, 16, 257] {
                let mut actual = vec![123, -456];
                let mut expected = actual.clone();
                let frames = if in_ch == 0 || out_ch == 0 {
                    0
                } else {
                    length / in_ch
                };
                for frame in 0..frames {
                    for channel in 0..out_ch {
                        expected.push(input[frame * in_ch + channel % in_ch]);
                    }
                }
                assert_eq!(
                    append_channel_mapped_i16(&input[..length], in_ch, out_ch, &mut actual),
                    frames
                );
                assert_eq!(actual, expected);
            }
        }
    }
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_mapping_kernel() {
    for (in_ch, out_ch) in [
        (6usize, 1),
        (8, 1),
        (2, 6),
        (2, 8),
        (6, 8),
        (8, 3),
        (2, 2),
        (1, 2),
        (6, 2),
    ] {
        for frames in [64, 4096] {
            let input: Vec<i16> = (0..frames * in_ch)
                .map(|i| i.wrapping_mul(71) as i16)
                .collect();
            let mut output = Vec::with_capacity(frames * out_ch + 2);
            output.extend_from_slice(&[123, -456]);
            let label = format!("mapping/{in_ch}to{out_ch}/frames={frames}");
            let (_, counts) =
                perf::measure(|| append_channel_mapped_i16(&input, in_ch, out_ch, &mut output));
            println!("{label}: allocation {counts:?}");
            bench::measure(&label, 2_000_000 / (frames * out_ch), || {
                let append: fn(&[i16], usize, usize, &mut Vec<i16>) -> usize =
                    black_box(append_channel_mapped_i16);
                output.truncate(2);
                black_box(append(
                    black_box(&input),
                    black_box(in_ch),
                    black_box(out_ch),
                    black_box(&mut output),
                ));
                black_box(&output);
            });
        }
    }
}

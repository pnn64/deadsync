use deadlib_audio_decode::resample::write_resampler_output;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

type WriteOutput = fn(&[Vec<f32>], usize, usize, &mut Vec<i16>) -> usize;

#[test]
fn resampler_keeps_channel_order_shortest_input_and_complete_frames() {
    let values = [
        0.0,
        -0.0,
        0.5,
        -0.5,
        1.0,
        -1.0,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
        f32::MAX,
    ];
    for in_ch in 0..=9 {
        for out_ch in 0..=9 {
            for frames in [0, 1, 3, 17] {
                let input: Vec<Vec<_>> = (0..in_ch)
                    .map(|channel| {
                        (0..19 - channel)
                            .map(|frame| values[(frame + channel) % values.len()])
                            .collect()
                    })
                    .collect();
                let produced = if in_ch == 0 || out_ch == 0 {
                    0
                } else {
                    frames.min(input.iter().map(Vec::len).min().unwrap())
                };
                let expected: Vec<_> = (0..produced)
                    .flat_map(|frame| {
                        input
                            .iter()
                            .cycle()
                            .take(out_ch)
                            .map(move |channel| (channel[frame] * 32767.0).round() as i16)
                    })
                    .collect();
                let mut actual = Vec::with_capacity(200);
                actual.extend([123, -456]);
                perf::assert_no_churn(|| {
                    assert_eq!(
                        write_resampler_output(&input, frames, out_ch, &mut actual),
                        produced
                    );
                });
                assert_eq!(actual, expected, "{in_ch} -> {out_ch}, frames={frames}");
            }
        }
    }
}

#[test]
fn multichannel_rounding_preserves_half_boundaries_and_float_bits() {
    let mut input = Vec::new();
    for integer in -32770..=32770 {
        let v = (integer as f32 + 0.5) / 32767.0;
        input.extend([v.next_down(), v, v.next_up()]);
    }
    let mut state = 0x1234_5678u32;
    for _ in 0..1_000_000 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        input.push(f32::from_bits(state));
    }
    let input = [input, vec![0.25; 1_200_000], vec![-0.25; 1_200_000]];
    let mut output = Vec::new();
    assert_eq!(
        write_resampler_output(&input, input[0].len(), 4, &mut output),
        input[0].len()
    );
    for (frame, output) in output.as_chunks::<4>().0.iter().enumerate() {
        let expected = (input[0][frame] * 32767.0).round() as i16;
        assert_eq!(
            *output,
            [expected, 8192, -8192, expected],
            "{:08x}",
            input[0][frame].to_bits()
        );
    }
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_resampler_output() {
    for (in_ch, out_ch) in [
        (1, 1),
        (1, 2),
        (2, 2),
        (2, 6),
        (3, 3),
        (4, 6),
        (6, 6),
        (6, 8),
        (8, 3),
        (8, 8),
    ] {
        for frames in [64usize, 4096] {
            let input: Vec<Vec<f32>> = (0..in_ch)
                .map(|channel| {
                    (0..frames)
                        .map(|frame| {
                            (frame.wrapping_mul(71).wrapping_add(channel) as i16) as f32 / 32768.0
                        })
                        .collect()
                })
                .collect();
            let mut output = Vec::with_capacity(frames * out_ch);
            let label = format!("resampler/{in_ch}to{out_ch}/frames={frames}");
            let mut work = || {
                let write: WriteOutput = black_box(write_resampler_output);
                black_box(write(
                    black_box(&input),
                    black_box(frames),
                    black_box(out_ch),
                    black_box(&mut output),
                ));
                black_box(&output);
            };
            work();
            let (_, counts) = perf::measure(&mut work);
            println!("{label}: allocation {counts:?}");
            bench::measure(&label, (1_000_000 / (frames * out_ch)).max(32), &mut work);
        }
    }
}

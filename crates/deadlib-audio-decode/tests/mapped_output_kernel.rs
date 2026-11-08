use deadlib_audio_decode::resample::write_channel_mapped_i16;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

#[test]
fn mapped_output_keeps_complete_frames_and_reuses_storage() {
    let input: Vec<_> = (i16::MIN..=i16::MAX).collect();
    for in_ch in 0..=9 {
        for out_ch in 0..=9 {
            for len in [0, 1, 3, 16, 257, input.len()] {
                let mut output = Vec::with_capacity(input.len() * 9);
                output.extend_from_slice(&[123, -456]);
                let pointer = output.as_ptr();
                let frames = if in_ch == 0 || out_ch == 0 {
                    0
                } else {
                    len / in_ch
                };
                let expected: Vec<_> = (0..frames)
                    .flat_map(|frame| {
                        let input = &input;
                        (0..out_ch).map(move |channel| input[frame * in_ch + channel % in_ch])
                    })
                    .collect();
                perf::assert_no_churn(|| {
                    assert_eq!(
                        write_channel_mapped_i16(&input[..len], in_ch, out_ch, &mut output),
                        frames
                    );
                });
                assert_eq!(output, expected, "{in_ch} -> {out_ch}, len={len}");
                assert_eq!(output.as_ptr(), pointer);
            }
        }
    }
}

#[test]
#[ignore = "release CPU and allocation benchmark"]
fn benchmark_mapped_output() {
    for (in_ch, out_ch) in [
        (1usize, 3),
        (1, 6),
        (1, 8),
        (2, 1),
        (3, 1),
        (4, 1),
        (6, 1),
        (8, 1),
        (2, 3),
        (2, 5),
        (2, 6),
        (2, 8),
        (3, 5),
        (6, 3),
        (6, 6),
        (6, 8),
        (8, 3),
        (2, 2),
        (1, 2),
        (6, 2),
    ] {
        for frames in [64, 4096] {
            for phase in [0usize, 128] {
                let storage: Vec<_> = (0..frames * in_ch + 2048)
                    .map(|i| i.wrapping_mul(71) as i16)
                    .collect();
                let mut output = Vec::with_capacity(frames * out_ch);
                // Control the relative 4 KiB address offset: memcpy can be
                // much slower at matching offsets, even with identical code.
                let offset = (output.as_ptr() as usize)
                    .wrapping_sub(storage.as_ptr() as usize)
                    .wrapping_add(phase)
                    & 4095;
                let input = &storage[offset / 2..offset / 2 + frames * in_ch];
                let label = format!("mapped/{in_ch}to{out_ch}/frames={frames}/phase={phase}");
                let mut work = || {
                    let write: fn(&[i16], usize, usize, &mut Vec<i16>) -> usize =
                        black_box(write_channel_mapped_i16);
                    black_box(write(
                        black_box(input),
                        black_box(in_ch),
                        black_box(out_ch),
                        black_box(&mut output),
                    ));
                    black_box(&output);
                };
                let (_, counts) = perf::measure(&mut work);
                println!("{label}: allocation {counts:?}");
                bench::measure(&label, 2_000_000 / (frames * out_ch), &mut work);
            }
        }
    }
}

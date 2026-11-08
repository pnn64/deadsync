use deadsync_gameplay::quantization_index_from_beat;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

fn original(beat: f32) -> u8 {
    let row = (beat * 48.0).round() as i32;
    for (index, divisor) in [48, 24, 16, 12, 8, 6, 4, 3].into_iter().enumerate() {
        if row.rem_euclid(divisor) == 0 {
            return index as u8;
        }
    }
    8
}

#[test]
fn quantization_preserves_ties_nonfinite_saturation_and_arbitrary_bits() {
    let check = |beat: f32| {
        assert_eq!(
            quantization_index_from_beat(beat),
            original(beat),
            "{:08x}",
            beat.to_bits()
        )
    };
    for exponent in 0..=255 {
        for sign in [0, 1 << 31] {
            for mantissa in [0, 1, 0x3fffff, 0x400000, 0x7fffff] {
                check(f32::from_bits(sign | exponent << 23 | mantissa));
            }
        }
    }
    for row in -4096..=4096 {
        let beat = (row as f32 + 0.5) / 48.0;
        for beat in [beat.next_down(), beat, beat.next_up()] {
            check(beat);
        }
    }
    for row in [8_388_608.0_f32, 16_777_216.0, 2_147_483_648.0] {
        for sign in [-1.0, 1.0] {
            let beat = row * sign / 48.0;
            for beat in [beat.next_down(), beat, beat.next_up()] {
                check(beat);
            }
        }
    }
    let mut state = 0x12345678u32;
    for _ in 0..1_000_000 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        check(f32::from_bits(state));
    }
}

#[test]
#[ignore = "release CPU-cycle and allocation benchmark"]
fn benchmark_quantization_kernel() {
    for kind in ["chart", "signed", "bits"] {
        let mut state = 0x12345678u32;
        let beats: Vec<_> = (0..4096)
            .map(|i| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                match kind {
                    "chart" => i as f32 / 12.0,
                    "signed" => (i as f32 - 2048.5) / 48.0,
                    _ => f32::from_bits(state),
                }
            })
            .collect();
        let label = format!("quantization/{kind}");
        let work = || {
            for &beat in &beats {
                black_box(black_box(quantization_index_from_beat as fn(f32) -> u8)(
                    black_box(beat),
                ));
            }
        };
        let (_, counts) = perf::measure(work);
        println!("{label}: allocation {counts:?}");
        assert_eq!(counts, perf::Churn::default());
        bench::measure(&label, 300, work);
    }
}

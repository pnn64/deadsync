use deadsync_core::timing::beat_to_note_row;
use std::hint::black_box;

#[path = "../../../tests/perf/core_alloc.rs"]
mod alloc;
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;

fn check(beat: f32) {
    assert_eq!(
        beat_to_note_row(beat),
        (beat * 48.0).round_ties_even() as i32,
        "beat bits {:08x}",
        beat.to_bits()
    );
}

#[test]
fn row_rounding_matches_original_at_ties_and_float_boundaries() {
    for row in (-65_536..=65_536).chain([
        8_388_607,
        8_388_608,
        16_777_216,
        -8_388_607,
        -8_388_608,
        -16_777_216,
        i32::MIN,
        i32::MAX,
    ]) {
        let beat = (row as f32 + 0.5) / 48.0;
        for bits in beat.to_bits().saturating_sub(2)..=beat.to_bits().saturating_add(2) {
            check(f32::from_bits(bits));
        }
    }
    for exponent in 0..=255 {
        for mantissa in [0, 1, 0x3f_ffff, 0x40_0000, 0x7f_fffe, 0x7f_ffff] {
            for sign in [0, 0x8000_0000] {
                check(f32::from_bits(sign | (exponent << 23) | mantissa));
            }
        }
    }
    let mut state = 0x1234_5678_u32;
    for _ in 0..1_000_000 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        check(f32::from_bits(state));
    }
}

#[test]
#[ignore = "release CPU benchmark"]
fn benchmark_note_rows() {
    for kind in ["chart", "signed", "bits"] {
        let beats: Vec<_> = (0..4096_u32)
            .map(|i| match kind {
                "chart" => i as f32 * (1.0 / 48.0),
                "signed" => (i as f32 - 2048.0) * 0.137,
                _ => f32::from_bits(i.wrapping_mul(0x9e37_79b9)),
            })
            .collect();
        let work = || {
            let f: fn(f32) -> i32 = black_box(beat_to_note_row);
            for &beat in black_box(&beats) {
                black_box(f(beat));
            }
        };
        let (_, churn) = alloc::measure(work);
        println!("rows/{kind}/4096: allocation {churn:?}");
        bench::measure(&format!("rows/{kind}/4096"), 1000, work);
    }
}

use deadsync_rules::golf;
use std::hint::black_box;
#[path = "../../../tests/support/perf.rs"]
mod alloc;
#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "support/golf_original.rs"]
mod original;

#[test]
fn curve_matches_original_at_every_interval_and_boundary() {
    for ms in -1050..=1050 {
        let ms = f64::from(ms) / 10.0;
        for ms in [ms.next_down(), ms, ms.next_up()] {
            let offset = ms / 1000.0;
            assert_eq!(
                golf::tap_strokes_seconds(Some(offset)),
                original::tap_strokes_seconds(Some(offset)),
                "{offset:?}"
            );
        }
    }
    for value in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MIN,
        f64::MAX,
        -0.0,
        0.0,
    ] {
        assert_eq!(
            golf::tap_strokes_seconds(Some(value)),
            original::tap_strokes_seconds(Some(value))
        );
    }
    // Include adjacent representable f32 errors, the gameplay-facing API.
    for ms in -104..=104 {
        let ms = ms as f32;
        for ms in [ms.next_down(), ms, ms.next_up()] {
            assert_eq!(golf::tap_strokes(Some(ms)), original::tap_strokes(Some(ms)));
        }
    }
    assert_eq!(golf::tap_strokes(None), original::tap_strokes(None));
    for ms in [5.0_f64, 23.0, 45.0, 103.0, 103.5] {
        let offset = ms / 1000.0;
        for offset in [offset.next_down(), offset, offset.next_up()] {
            for offset in [offset, -offset] {
                assert_eq!(
                    golf::tap_strokes_seconds(Some(offset)),
                    original::tap_strokes_seconds(Some(offset))
                );
            }
        }
    }
    let mut bits = 0x12345678u32;
    for _ in 0..100_000 {
        bits ^= bits << 13;
        bits ^= bits >> 17;
        bits ^= bits << 5;
        let ms = Some(f32::from_bits(bits));
        assert_eq!(golf::tap_strokes(ms), original::tap_strokes(ms));
    }
    alloc::assert_no_churn(|| {
        black_box(golf::tap_strokes(Some(50.5)));
    });
}

#[test]
#[ignore = "paired release CPU benchmark"]
fn benchmark_golf_kernel() {
    for (name, range) in [("accurate", 24), ("mixed", 104), ("late", 60)] {
        let inputs: Vec<_> = (0..1024)
            .map(|i| {
                let ms = ((i * 73) % range) as f32 + 0.25;
                Some(if name == "late" { ms + 44.0 } else { ms })
            })
            .collect();
        bench::compare(&format!("golf/{name}/1024"), 2000, |current| {
            let score: fn(Option<f32>) -> u32 = if current {
                golf::tap_strokes
            } else {
                original::tap_strokes
            };
            let score = black_box(score);
            for &input in black_box(&inputs) {
                black_box(score(black_box(input)));
            }
        });
    }
}

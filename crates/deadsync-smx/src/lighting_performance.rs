use super::{BYTES_PER_PAD_25, BrightnessScale, PLAYER_LIGHT_FRAME_BYTES, player_light_frame};
use crate::panels::smx_panel_for_col;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "lighting_original.rs"]
mod original;

fn random(state: &mut u64) -> usize {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state as usize
}

fn pixels(length: usize, salt: usize) -> Vec<u8> {
    (0..length)
        .map(|index| index.wrapping_mul(37).wrapping_add(salt) as u8)
        .collect()
}

fn percentages() -> Vec<[u8; 2]> {
    let mut values = vec![[100, 100], [0, 0], [100, 50], [0, 100], [255, 255], [1, 99]];
    values.extend((0..=u8::MAX).map(|pct| [pct, u8::MAX - pct]));
    values
}

#[test]
fn sdk_shape_matches_kernel_fixture() {
    assert_eq!(BYTES_PER_PAD_25, 675);
    assert_eq!(PLAYER_LIGHT_FRAME_BYTES, 1350);
}

#[test]
fn mappings_preserve_native_fields_invalid_columns_and_saturation() {
    for width in 0..=20 {
        for players in 0..=5 {
            for column in 0..=128 {
                assert_eq!(
                    smx_panel_for_col(width, players, column),
                    original::panels::smx_panel_for_col(width, players, column)
                );
            }
        }
    }
    for width in [
        0,
        1,
        2,
        3,
        4,
        5,
        6,
        8,
        9,
        10,
        11,
        usize::MAX / 2 + 1,
        usize::MAX,
    ] {
        for players in [0, 1, 2, 3, usize::MAX] {
            for column in [0, 1, 3, 4, 7, 8, 9, 10, 15, 31, usize::MAX - 1, usize::MAX] {
                assert_eq!(
                    smx_panel_for_col(width, players, column),
                    original::panels::smx_panel_for_col(width, players, column)
                );
            }
        }
    }
    let mut state = 0x9922_3377_64fa_1324;
    for _ in 0..50_000 {
        let values = [random(&mut state), random(&mut state), random(&mut state)];
        assert_eq!(
            smx_panel_for_col(values[0], values[1], values[2]),
            original::panels::smx_panel_for_col(values[0], values[1], values[2])
        );
    }
}

#[test]
fn cache_outputs_keys_and_tables_preserve_every_percent_and_odd_tail() {
    for length in [0, 1, 2, 3, 7, 75, 674, 675, 676, 1349, 1350, 1351, 4097] {
        let mut live = BrightnessScale::new();
        let mut old = original::BrightnessScale::new();
        for pct in percentages() {
            for repeat in 0..3 {
                let mut expected = pixels(length, usize::from(pct[0]) + repeat);
                let mut actual = expected.clone();
                old.apply(&mut expected, pct);
                live.apply(&mut actual, pct);
                assert_eq!(actual, expected, "length={length}, pct={pct:?}");
                assert_eq!(live.pct, old.pct);
                assert_eq!(live.table, old.table);
            }
        }
    }
}

#[test]
fn solid_wire_frames_preserve_absent_pads_and_all_percent_values() {
    for colors in [
        [None, None],
        [Some([255, 0, 1]), None],
        [None, Some([128, 254, 85])],
        [Some([255, 0, 1]), Some([128, 254, 85])],
        [Some([0, 0, 0]), Some([255, 255, 255])],
    ] {
        for pct in percentages() {
            assert_eq!(
                player_light_frame(colors, pct),
                original::player_light_frame(colors, pct),
                "colors={colors:?}, pct={pct:?}"
            );
        }
    }
}

#[test]
#[ignore = "archive deterministic wire bytes, mapping and complete cache state"]
fn dump_lighting_trace() {
    let mut trace = Vec::new();
    for width in 0..=20 {
        for players in 0..=5 {
            for column in 0..=128 {
                match smx_panel_for_col(width, players, column) {
                    None => trace.push(0),
                    Some((pad, panel)) => {
                        trace.push(1);
                        trace.extend_from_slice(&(pad as u64).to_le_bytes());
                        trace.extend_from_slice(&(panel as u64).to_le_bytes());
                    }
                }
            }
        }
    }
    for colors in [
        [None, None],
        [Some([255, 0, 1]), None],
        [Some([255, 0, 1]), Some([128, 254, 85])],
    ] {
        for pct in percentages() {
            trace.extend_from_slice(&player_light_frame(colors, pct));
        }
    }
    for length in [0, 1, 75, 675, 1350, 1351] {
        let mut scale = BrightnessScale::new();
        for pct in percentages() {
            let mut frame = pixels(length, usize::from(pct[0]));
            scale.apply(&mut frame, pct);
            trace.extend_from_slice(&frame);
            trace.extend_from_slice(&scale.pct);
            trace.extend_from_slice(&scale.table[0]);
            trace.extend_from_slice(&scale.table[1]);
        }
    }
    std::fs::write(
        std::env::var_os("SMX_LIGHTING_TRACE_OUT").expect("trace path"),
        trace,
    )
    .unwrap();
}

fn compare(label: &str, iterations: usize, mut work: impl FnMut(bool)) {
    bench::compare(label, iterations, &mut work);
    for (variant, current) in [("original", false), ("current", true)] {
        let (_, churn) = crate::lighting_allocations::measure(|| work(current));
        assert_eq!(
            churn.allocs
                + churn.reallocs
                + churn.frees
                + churn.allocated_bytes
                + churn.freed_bytes
                + churn.peak_bytes,
            0
        );
        println!("{label}/{variant}: allocation {churn:?}");
    }
}

#[test]
#[ignore = "optimized paired CPU and allocation benchmark"]
fn benchmark_lighting() {
    for (label, widths) in [
        ("mapping/dance/256", [4, 8, 4, 8]),
        ("mapping/pump/256", [5, 10, 5, 10]),
        ("mapping/mixed/256", [3, 6, 13, usize::MAX]),
    ] {
        let inputs: Vec<_> = (0..256)
            .map(|index| {
                let width = widths[index % 4];
                (width, 1 + index % 2, index % width.min(16))
            })
            .collect();
        compare(label, 2_000, |current| {
            let mapper = black_box(if current {
                smx_panel_for_col as fn(usize, usize, usize) -> Option<(usize, usize)>
            } else {
                original::panels::smx_panel_for_col
                    as fn(usize, usize, usize) -> Option<(usize, usize)>
            });
            for &(width, players, column) in &inputs {
                let (width, players, column) = black_box((width, players, column));
                black_box(mapper(width, players, column));
            }
        });
    }
    let seed: [u8; PLAYER_LIGHT_FRAME_BYTES] =
        std::array::from_fn(|index| index.wrapping_mul(37) as u8);
    for (label, pct) in [
        ("scale/dark/1350", [0, 0]),
        ("scale/half_dark/1350", [0, 50]),
        ("scale/half_full/1350", [100, 50]),
        ("scale/dim/1350", [50, 73]),
    ] {
        let mut live = BrightnessScale::new();
        let mut old = original::BrightnessScale::new();
        let mut live_warmup = seed;
        let mut old_warmup = seed;
        live.apply(&mut live_warmup, pct);
        old.apply(&mut old_warmup, pct);
        compare(label, 2_000, |current| {
            let mut frame = black_box(seed);
            if current {
                live.apply(&mut frame, black_box(pct));
            } else {
                old.apply(&mut frame, black_box(pct));
            }
            black_box(frame);
        });
    }
    compare("scale/cold_mixed/1350", 300, |current| {
        let mut frame = black_box(seed);
        if current {
            let mut scale = BrightnessScale::new();
            scale.apply(&mut frame, black_box([0, 50]));
        } else {
            let mut scale = original::BrightnessScale::new();
            scale.apply(&mut frame, black_box([0, 50]));
        }
        black_box(frame);
    });
    for (label, pct) in [
        ("solid/dim/1350", [50, 73]),
        ("solid/half_full/1350", [100, 50]),
        ("solid/zero/1350", [0, 0]),
        ("solid/full/1350", [100, 100]),
    ] {
        compare(label, 500, |current| {
            let colors = black_box([Some([201, 100, 52]), Some([255, 128, 1])]);
            black_box(if current {
                player_light_frame(colors, black_box(pct))
            } else {
                original::player_light_frame(colors, black_box(pct))
            });
        });
    }
}

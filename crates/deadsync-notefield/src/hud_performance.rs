use super::*;
use crate::error_bar::{error_bar_boundaries_s, error_bar_tick_alpha};
use crate::transforms::numeric_performance::allocations;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "hud_original.rs"]
mod original;

fn random(state: &mut u64) -> u32 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state as u32
}

fn float_inputs() -> Vec<f32> {
    let mut out = Vec::new();
    for center in [
        0,
        1,
        0x007f_ffff,
        f32::MIN_POSITIVE.to_bits(),
        f32::EPSILON.to_bits(),
        0.03_f32.to_bits(),
        0.1_f32.to_bits(),
        0.35_f32.to_bits(),
        1.0_f32.to_bits(),
        10.0_f32.to_bits(),
        15.0_f32.to_bits(),
        f32::MAX.to_bits(),
        f32::INFINITY.to_bits(),
        0x7fc0_0000,
        0x7fff_ffff,
    ] {
        for delta in 0..=8 {
            for bits in [
                center.saturating_sub(delta),
                center.saturating_add(delta).min(0x7fff_ffff),
            ] {
                out.extend([f32::from_bits(bits), f32::from_bits(bits | 0x8000_0000)]);
            }
        }
    }
    let mut state = 0x1234_a85d_9832_ffff;
    out.extend((0..100_000).map(|_| f32::from_bits(random(&mut state))));
    out
}

fn row_cases(mut visit: impl FnMut(TapJudgmentRowsParams)) {
    let values = float_inputs();
    for grade in [
        JudgeGrade::Fantastic,
        JudgeGrade::Excellent,
        JudgeGrade::Great,
        JudgeGrade::Decent,
        JudgeGrade::WayOff,
        JudgeGrade::Miss,
    ] {
        for window in [
            None,
            Some(TimingWindow::W0),
            Some(TimingWindow::W1),
            Some(TimingWindow::W2),
            Some(TimingWindow::W3),
            Some(TimingWindow::W4),
            Some(TimingWindow::W5),
        ] {
            for frame_rows in [0, 6, 7, usize::MAX] {
                for flags in 0..16 {
                    for &time_error_ms in values.iter().step_by(997).chain(values[324..432].iter())
                    {
                        visit(TapJudgmentRowsParams {
                            grade,
                            window,
                            time_error_ms,
                            frame_rows,
                            show_fa_plus_window: flags & 1 != 0,
                            fa_plus_10ms_blue_window: flags & 2 != 0,
                            split_15_10ms: flags & 4 != 0,
                            custom_fantastic_window: flags & 8 != 0,
                        });
                    }
                }
            }
        }
    }
}

fn boundary_cases(mut visit: impl FnMut([f32; 5], Option<f32>, bool, usize)) {
    let values = float_inputs();
    for (index, &value) in values.iter().step_by(97).enumerate() {
        let windows = [
            value,
            -value,
            0.0,
            f32::from_bits(0x8000_0000),
            values[(index * 61) % values.len()],
        ];
        for plus in [None, Some(value)] {
            for show in [false, true] {
                for max in [0, 1, 2, 3, 4, 5, usize::MAX] {
                    visit(windows, plus, show, max);
                }
            }
        }
    }
}

#[test]
fn glow_and_zoom_preserve_exact_float_results() {
    const GLOW: [u32; 4] = [
        itg_actor_glow_alpha(-0.0).to_bits(),
        itg_actor_glow_alpha(f32::INFINITY).to_bits(),
        itg_actor_glow_alpha(f32::NAN).to_bits(),
        itg_actor_glow_alpha(2.0).to_bits(),
    ];
    assert_eq!(GLOW, [0x8000_0000, 0, 0, 1.0_f32.to_bits()]);
    for value in float_inputs() {
        assert_eq!(
            itg_actor_glow_alpha(value).to_bits(),
            original::itg_actor_glow_alpha(value).to_bits(),
            "glow {:08x}",
            value.to_bits()
        );
        assert_eq!(
            combo_actor_zoom(value).to_bits(),
            original::combo_actor_zoom(value).to_bits(),
            "combo {:08x}",
            value.to_bits()
        );
        for back in [false, true] {
            assert_eq!(
                judgment_actor_zoom(value, back, 0.0, 0.0).to_bits(),
                original::judgment_actor_zoom(value, back, 0.0, 0.0).to_bits(),
                "judgment {:08x} {back}",
                value.to_bits()
            );
        }
    }
}

#[test]
fn tick_alpha_preserves_duration_and_signed_zero_contracts() {
    for value in float_inputs() {
        for duration in [
            -1.0,
            -0.0,
            0.0,
            0.02,
            0.03,
            0.031,
            0.3,
            1.0,
            f32::MIN_POSITIVE,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
        ] {
            for multi in [false, true] {
                assert_eq!(
                    error_bar_tick_alpha(value, duration, multi).to_bits(),
                    original::error_bar_tick_alpha(value, duration, multi).to_bits(),
                    "{:08x} {:08x} {multi}",
                    value.to_bits(),
                    duration.to_bits()
                );
            }
        }
    }
}

#[test]
fn judgment_rows_preserve_all_flags_grades_windows_and_error_boundaries() {
    row_cases(|params| {
        assert_eq!(
            tap_judgment_rows(params),
            original::tap_judgment_rows(params),
            "{params:?}"
        )
    });
}

#[test]
fn boundaries_preserve_complete_array_bits_and_length() {
    boundary_cases(|windows, plus, show, max| {
        let (old, old_len) = original::error_bar_boundaries_s(windows, plus, show, max);
        let (new, new_len) = error_bar_boundaries_s(windows, plus, show, max);
        assert_eq!(old_len, new_len);
        assert_eq!(old.map(f32::to_bits), new.map(f32::to_bits));
    });
}

#[test]
#[ignore = "archive exact HUD input/output bits"]
fn dump_hud_trace() {
    let mut out = Vec::new();
    for value in float_inputs() {
        out.extend_from_slice(&value.to_bits().to_le_bytes());
        for result in [
            itg_actor_glow_alpha(value),
            combo_actor_zoom(value),
            judgment_actor_zoom(value, false, 0.0, 0.0),
            judgment_actor_zoom(value, true, 0.0, 0.0),
        ] {
            out.extend_from_slice(&result.to_bits().to_le_bytes());
        }
        for duration in [-1.0, -0.0, 0.0, 0.03, 0.3, f32::INFINITY, f32::NAN] {
            for multi in [false, true] {
                out.extend_from_slice(
                    &error_bar_tick_alpha(value, duration, multi)
                        .to_bits()
                        .to_le_bytes(),
                );
            }
        }
    }
    row_cases(|params| {
        let (base, overlay) = tap_judgment_rows(params);
        out.extend_from_slice(&(base as u64).to_le_bytes());
        out.extend_from_slice(&overlay.map_or(u64::MAX, |value| value as u64).to_le_bytes());
    });
    boundary_cases(|windows, plus, show, max| {
        let (values, len) = error_bar_boundaries_s(windows, plus, show, max);
        for value in values {
            out.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        out.extend_from_slice(&(len as u64).to_le_bytes());
    });
    std::fs::write(
        std::env::var_os("NOTEFIELD_HUD_TRACE_OUT").expect("trace path"),
        out,
    )
    .unwrap();
}

fn compare(label: &str, mut work: impl FnMut(bool)) {
    bench::compare(label, 1000, &mut work);
    for (variant, current) in [("original", false), ("current", true)] {
        let (_, churn) = allocations::measure(|| work(current));
        assert_eq!(churn, allocations::Churn::default());
        println!("{label}/{variant}: allocation {churn:?}");
    }
}

#[test]
#[ignore = "paired CPU cycles and allocations"]
fn benchmark_hud() {
    for mode in ["ordinary", "inactive", "zero", "saturated", "exceptional"] {
        let special = [
            0.0,
            -0.0,
            0.03,
            -1.0,
            f32::MIN_POSITIVE,
            f32::EPSILON,
            1.0,
            2.0,
            f32::MAX,
            -f32::MAX,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::from_bits(0x7f80_0001),
            f32::from_bits(0xffc0_1234),
        ];
        let values: Vec<_> = (0..512)
            .map(|index| match mode {
                "ordinary" => index as f32 * 0.0015,
                "zero" => {
                    if index & 1 == 0 {
                        0.0
                    } else {
                        -0.0
                    }
                }
                "inactive" => -((index % 8) as f32) * 0.1,
                "saturated" => 1.0 + index as f32 * 0.03,
                _ => special[index % special.len()],
            })
            .collect();
        for kernel in ["glow", "combo", "judgment_back", "judgment_front"] {
            compare(&format!("{kernel}/{mode}/512"), |current| {
                let call = black_box(match (kernel, current) {
                    ("glow", true) => itg_actor_glow_alpha as fn(f32) -> f32,
                    ("glow", false) => original::itg_actor_glow_alpha as fn(f32) -> f32,
                    ("combo", true) => combo_actor_zoom as fn(f32) -> f32,
                    ("combo", false) => original::combo_actor_zoom as fn(f32) -> f32,
                    ("judgment_back", true) => |x| judgment_actor_zoom(x, true, 0.0, 0.0),
                    ("judgment_back", false) => {
                        |x| original::judgment_actor_zoom(x, true, 0.0, 0.0)
                    }
                    (_, true) => |x| judgment_actor_zoom(x, false, 0.0, 0.0),
                    (_, false) => |x| original::judgment_actor_zoom(x, false, 0.0, 0.0),
                });
                for &value in &values {
                    black_box(call(black_box(value)));
                }
            });
        }
        let rows: Vec<_> = values
            .iter()
            .enumerate()
            .map(|(index, &time_error_ms)| TapJudgmentRowsParams {
                grade: if mode == "ordinary" {
                    JudgeGrade::Fantastic
                } else {
                    [
                        JudgeGrade::Excellent,
                        JudgeGrade::Great,
                        JudgeGrade::Decent,
                        JudgeGrade::WayOff,
                        JudgeGrade::Miss,
                        JudgeGrade::Fantastic,
                    ][index % 6]
                },
                window: Some(if index % 2 == 0 {
                    TimingWindow::W0
                } else {
                    TimingWindow::W1
                }),
                time_error_ms: time_error_ms * 100.0,
                frame_rows: if mode == "saturated" { 6 } else { 7 },
                show_fa_plus_window: true,
                fa_plus_10ms_blue_window: index & 1 != 0,
                split_15_10ms: index & 2 != 0,
                custom_fantastic_window: index & 4 != 0,
            })
            .collect();
        compare(&format!("rows/{mode}/512"), |current| {
            let call = black_box(if current {
                tap_judgment_rows as fn(TapJudgmentRowsParams) -> (usize, Option<usize>)
            } else {
                original::tap_judgment_rows as fn(TapJudgmentRowsParams) -> (usize, Option<usize>)
            });
            for &params in &rows {
                black_box(call(black_box(params)));
            }
        });
        compare(&format!("tick/{mode}/512"), |current| {
            let call = black_box(if current {
                error_bar_tick_alpha as fn(f32, f32, bool) -> f32
            } else {
                original::error_bar_tick_alpha as fn(f32, f32, bool) -> f32
            });
            for (index, &value) in values.iter().enumerate() {
                black_box(call(
                    black_box(value),
                    black_box(0.3),
                    black_box(index & 1 != 0),
                ));
            }
        });
    }
}

use super::*;
use crate::transforms::numeric_performance::allocations;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "mini_indicator_original.rs"]
mod original;

fn random(state: &mut u64) -> u32 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state as u32
}

fn progress(values: [u32; 9]) -> MiniIndicatorProgress {
    MiniIndicatorProgress {
        white_count: values[0],
        white_10ms_count: values[1],
        w2: values[2],
        w3: values[3],
        w4: values[4],
        w5: values[5],
        miss: values[6],
        let_go: values[7],
        mines_hit: values[8],
        kept_percent: 96.75,
        judged_any: true,
        ..MiniIndicatorProgress::default()
    }
}

fn point_inputs(mode: &str) -> Vec<MiniIndicatorProgress> {
    let mut seed = 0x8877_2211_ffff_aadd;
    (0..64)
        .map(|i| {
            let mut values = [0; 9];
            for value in &mut values {
                *value = if mode == "limits" {
                    random(&mut seed)
                } else {
                    random(&mut seed) % (i + 1)
                };
            }
            progress(values)
        })
        .collect()
}

fn grade(index: usize) -> Option<JudgeGrade> {
    [
        None,
        Some(JudgeGrade::Fantastic),
        Some(JudgeGrade::Excellent),
        Some(JudgeGrade::Great),
        Some(JudgeGrade::Decent),
        Some(JudgeGrade::WayOff),
        Some(JudgeGrade::Miss),
    ][index % 7]
}

fn color_params(index: usize, elapsed_s: f32) -> ZmodComboColorParams {
    ZmodComboColorParams {
        style: ZmodComboColorStyle::Glow,
        full_combo_mode: index & 1 != 0,
        combo: index as u32,
        full_combo_grade: grade(index),
        current_combo_grade: grade(index / 7),
        quint_active: index & 2 != 0,
        elapsed_s,
    }
}

fn verify_points(mut visit: impl FnMut(u32)) {
    let mut seed = 0x9883_2234_3312_fff7;
    let values = [
        0,
        1,
        2,
        10,
        11,
        u32::MAX / 7,
        u32::MAX / 7 + 1,
        u32::MAX / 5,
        u32::MAX / 5 + 1,
        u32::MAX / 3,
        u32::MAX / 3 + 1,
        u32::MAX / 2,
        u32::MAX / 2 + 1,
        u32::MAX - 1,
        u32::MAX,
    ];
    let mut check = |p: MiniIndicatorProgress| {
        for score in [
            MiniIndicatorScoreType::Itg,
            MiniIndicatorScoreType::Ex,
            MiniIndicatorScoreType::HardEx,
        ] {
            let old = original::zmod_subtractive_points(&p, score);
            let new = zmod_subtractive_points(&p, score);
            assert_eq!(new, old, "{p:?}, {score:?}");
            let oracle = match score {
                MiniIndicatorScoreType::Itg => {
                    (i64::from(p.current_possible_dp) - i64::from(p.actual_dp))
                        .clamp(0, i64::from(i32::MAX)) as u32
                }
                MiniIndicatorScoreType::Ex => (u64::from(p.white_count)
                    + 3 * u64::from(p.w2)
                    + 5 * u64::from(p.w3)
                    + 7 * (u64::from(p.w4) + u64::from(p.w5) + u64::from(p.miss))
                    + 2 * (u64::from(p.let_go) + u64::from(p.mines_hit)))
                .min(u64::from(u32::MAX)) as u32,
                MiniIndicatorScoreType::HardEx => (u64::from(p.white_10ms_count)
                    + 5 * u64::from(p.w2)
                    + 7 * (u64::from(p.w3) + u64::from(p.w4) + u64::from(p.w5) + u64::from(p.miss))
                    + 2 * (u64::from(p.let_go) + u64::from(p.mines_hit)))
                .min(u64::from(u32::MAX)) as u32,
            };
            assert_eq!(new, oracle);
            visit(new);
        }
    };
    for &a in &values {
        for &b in &values {
            for field in 0..9 {
                let mut counts = [a; 9];
                counts[field] = b;
                check(progress(counts));
            }
        }
    }
    for index in 0..300_000 {
        let mut counts = [0; 9];
        for value in &mut counts {
            let bits = random(&mut seed);
            *value = match index % 4 {
                0 => bits,
                1 => bits % 4096,
                2 => bits % 1_000_000_000,
                _ => {
                    if bits & 1 == 0 {
                        0
                    } else {
                        bits
                    }
                }
            };
        }
        let mut p = progress(counts);
        p.current_possible_dp = random(&mut seed) as i32;
        p.actual_dp = random(&mut seed) as i32;
        check(p);
    }
}

fn verify_counts(mut visit: impl FnMut(u32)) {
    let mut seed = 0x9384_aadd_1200_fff1;
    for i in 0..210_000 {
        let mut values = [0; 9];
        for (field, value) in values.iter_mut().enumerate() {
            *value = if i < 4096 {
                if i & (1 << field) == 0 {
                    0
                } else {
                    [1, 10, 11, u32::MAX][(i >> 9) % 4]
                }
            } else {
                random(&mut seed)
            };
        }
        let p = progress(values);
        for score in [
            MiniIndicatorScoreType::Itg,
            MiniIndicatorScoreType::Ex,
            MiniIndicatorScoreType::HardEx,
        ] {
            let old = original::zmod_subtractive_counter_state(&p, score);
            let new = zmod_subtractive_counter_state(&p, score);
            assert_eq!(new, old);
            visit(new.0);
            visit(u32::from(new.1));
        }
        let counts = WindowCounts {
            w0: values[0],
            w1: values[1],
            w2: values[2],
            w3: values[3],
            w4: values[4],
            w5: values[5],
            miss: values[6],
        };
        for show in [false, true] {
            let new = zmod_combo_quint_active(show, counts);
            assert_eq!(new, original::zmod_combo_quint_active(show, counts));
            visit(u32::from(new));
        }
    }
}

fn verify_glows(mut visit: impl FnMut(u32)) {
    let mut seed = 0xff77_9999_2356_2323;
    let mut elapsed = vec![
        0.0,
        -0.0,
        0.8,
        -0.8,
        f32::MIN_POSITIVE,
        f32::MAX,
        -f32::MAX,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
    ];
    // Both sides of the division-overflow boundary and many NaN payloads.
    let edge = (f32::MAX * 0.8).to_bits();
    for bits in edge - 32..=edge + 32 {
        elapsed.extend([f32::from_bits(bits), -f32::from_bits(bits)]);
    }
    for _ in 0..20_000 {
        elapsed.push(f32::from_bits(random(&mut seed)));
    }
    for time in elapsed {
        for index in 0..196 {
            let mut params = color_params(index, time);
            params.combo = [0, 1, 1000, 1 << 24, u32::MAX, 3_141_592_653][index % 6];
            for style in [
                ZmodComboColorStyle::None,
                ZmodComboColorStyle::Rainbow,
                ZmodComboColorStyle::RainbowScroll,
                ZmodComboColorStyle::Glow,
                ZmodComboColorStyle::Solid,
            ] {
                params.style = style;
                let old = original::zmod_resolved_combo_color(params).map(f32::to_bits);
                let new = zmod_resolved_combo_color(params).map(f32::to_bits);
                assert_eq!(new, old, "{params:?}");
                for bits in new {
                    visit(bits);
                }
            }
        }
    }
}

#[test]
fn subtractive_points_preserve_saturation_and_integer_extremes() {
    verify_points(|_| {});
}

#[test]
fn judgment_flags_preserve_combinations_and_full_width_counts() {
    verify_counts(|_| {});
}

#[test]
fn combo_colors_preserve_all_styles_and_exceptional_elapsed_bits() {
    verify_glows(|_| {});
}

fn stream_segments(mode: &str, len: usize) -> Vec<StreamSegment> {
    (0..len)
        .map(|i| {
            let is_break = mode == "breaks" || (mode == "mixed" && i % 3 == 1);
            StreamSegment::new(
                i as u32 * 8,
                i as u32 * 8 + if is_break { 2 } else { 8 },
                is_break,
            )
        })
        .collect()
}

fn verify_streams(mut visit: impl FnMut(u32)) {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    let mut seed = 0x9984_fff3_2231_111a;
    for index in 0..2000 {
        let segments: Vec<_> = (0..index % 64)
            .map(|_| {
                let start = random(&mut seed) % 1_000_000;
                let length = random(&mut seed) % 16;
                StreamSegment::new(start, start + length, random(&mut seed) & 1 != 0)
            })
            .collect();
        for start in (0..=segments.len()).chain([usize::MAX]) {
            let old = original::broken_run_end_and_next(&segments, start);
            let new = broken_run_end_and_next(&segments, start);
            assert_eq!(old, new);
            visit(new.0 as u32);
            visit(u32::from(new.1));
            visit(new.2 as u32);
        }
        let old = original::BrokenRunLookup::new(&segments);
        let new = BrokenRunLookup::new(&segments);
        assert_eq!(format!("{old:?}"), format!("{new:?}"));
        for measure in [
            f32::NAN,
            f32::NEG_INFINITY,
            -1.0,
            0.0,
            1000.0,
            f32::INFINITY,
            0.0,
        ] {
            assert_eq!(old.segment(measure), new.segment(measure));
            assert_eq!(
                old.segment_indices(&segments, measure),
                new.segment_indices(&segments, measure)
            );
            assert_eq!(format!("{old:?}"), format!("{new:?}"));
        }
    }
    for segments in [
        vec![
            StreamSegment::new(0, u32::MAX, false),
            StreamSegment::new(0, 8, false),
        ],
        vec![
            StreamSegment::new(0, 8, false),
            StreamSegment::new(10, 1, false),
        ],
        vec![
            StreamSegment::new(0, u32::MAX, false),
            StreamSegment::new(0, 2, true),
            StreamSegment::new(0, 3, false),
        ],
    ] {
        let old = catch_unwind(AssertUnwindSafe(|| {
            original::broken_run_end_and_next(&segments, 0)
        }))
        .ok();
        let new = catch_unwind(AssertUnwindSafe(|| broken_run_end_and_next(&segments, 0))).ok();
        assert_eq!(old, new);
        visit(u32::from(new.is_some()));
        if let Some((end, broken, next)) = new {
            visit(end as u32);
            visit(u32::from(broken));
            visit(next as u32);
        }
    }
    assert_eq!(
        std::mem::size_of::<BrokenRunLookup>(),
        std::mem::size_of::<original::BrokenRunLookup>()
    );
}

#[test]
fn stream_scans_preserve_spans_cursors_and_overflow_behavior() {
    verify_streams(|_| {});
}

#[test]
#[ignore = "write exact per-profile output corpus"]
fn dump_mini_indicator_trace() {
    use std::io::Write;
    let file = std::fs::File::create(
        std::env::var_os("NOTEFIELD_MINI_INDICATOR_TRACE_OUT").expect("trace path"),
    )
    .unwrap();
    let mut out = std::io::BufWriter::with_capacity(1024 * 1024, file);
    let mut visit = |value: u32| out.write_all(&value.to_le_bytes()).unwrap();
    verify_points(&mut visit);
    verify_counts(&mut visit);
    verify_glows(&mut visit);
    verify_streams(&mut visit);
    out.flush().unwrap();
}

fn compare(label: &str, mut work: impl FnMut(bool)) {
    bench::compare(label, 10_000, &mut work);
    for (variant, current) in [("original", false), ("current", true)] {
        let (_, churn) = allocations::measure(|| work(current));
        assert_eq!(churn, allocations::Churn::default());
        println!("{label}/{variant}: allocation {churn:?}");
    }
}

#[test]
#[ignore = "paired thread cycles, throughput and allocations"]
fn benchmark_mini_indicator() {
    for scroll in [false, true] {
        let inputs: Vec<_> = (0..64)
            .map(|i| (i as f32 * 0.317 - 4.0, (i * 17) as u32))
            .collect();
        let label = if scroll {
            "rainbow/scroll/64"
        } else {
            "rainbow/fixed/64"
        };
        compare(label, |current| {
            let call = black_box(if current {
                zmod_combo_rainbow_color
            } else {
                original::zmod_combo_rainbow_color
            } as fn(f32, bool, u32) -> [f32; 4]);
            for &(time, combo) in &inputs {
                black_box(call(black_box(time), black_box(scroll), black_box(combo)));
            }
        });
    }
    for (mode, len) in [("runs", 64), ("runs", 512), ("mixed", 512), ("breaks", 64)] {
        let inputs = stream_segments(mode, len);
        compare(&format!("stream/{mode}/{len}"), |current| {
            let call = black_box(if current {
                zmod_broken_run_end
            } else {
                original::zmod_broken_run_end
            } as fn(&[StreamSegment], usize) -> (i32, bool));
            black_box(call(black_box(&inputs), black_box(0)));
        });
    }
    let inputs = stream_segments("mixed", 512);
    bench::compare("lookup/build/512", 10_000, |current| {
        if current {
            black_box(BrokenRunLookup::new(black_box(&inputs)));
        } else {
            black_box(original::BrokenRunLookup::new(black_box(&inputs)));
        }
    });
    let (_, old_churn) = allocations::measure(|| {
        black_box(original::BrokenRunLookup::new(black_box(&inputs)));
    });
    let (_, new_churn) = allocations::measure(|| {
        black_box(BrokenRunLookup::new(black_box(&inputs)));
    });
    assert_eq!(old_churn, new_churn);
    println!("lookup/build/512/original: allocation {old_churn:?}");
    println!("lookup/build/512/current: allocation {new_churn:?}");
    for mode in ["normal", "limits"] {
        let inputs = point_inputs(mode);
        for (score, name) in [
            (MiniIndicatorScoreType::Ex, "ex"),
            (MiniIndicatorScoreType::HardEx, "hard"),
        ] {
            compare(&format!("points/{name}/{mode}/64"), |current| {
                let call = black_box(if current {
                    zmod_subtractive_points
                } else {
                    original::zmod_subtractive_points
                }
                    as fn(&MiniIndicatorProgress, MiniIndicatorScoreType) -> u32);
                for p in &inputs {
                    black_box(call(black_box(p), black_box(score)));
                }
            });
        }
    }
    for mode in ["clean", "late", "early", "mixed"] {
        let inputs: Vec<_> = (0..64)
            .map(|i| {
                let mut p = progress([0; 9]);
                match mode {
                    "early" => p.w3 = 1,
                    "late" => p.mines_hit = 1,
                    "mixed" if i % 3 == 0 => p.w4 = 1,
                    _ => {}
                }
                p.w2 = i % 12;
                p.white_count = i % 16;
                p.white_10ms_count = i % 20;
                p
            })
            .collect();
        compare(&format!("counter/{mode}/64"), |current| {
            let call = black_box(if current {
                zmod_subtractive_counter_state
            } else {
                original::zmod_subtractive_counter_state
            }
                as fn(&MiniIndicatorProgress, MiniIndicatorScoreType) -> (u32, bool));
            for (i, p) in inputs.iter().enumerate() {
                black_box(call(
                    black_box(p),
                    black_box(
                        [
                            MiniIndicatorScoreType::Itg,
                            MiniIndicatorScoreType::Ex,
                            MiniIndicatorScoreType::HardEx,
                        ][i % 3],
                    ),
                ));
            }
        });
    }
    for mode in ["clear", "mixed"] {
        let inputs: Vec<_> = (0..64)
            .map(|i| WindowCounts {
                w0: 13,
                w1: if mode == "mixed" { i % 3 } else { 0 },
                w5: if mode == "mixed" { i % 5 } else { 0 },
                ..WindowCounts::default()
            })
            .collect();
        compare(&format!("quint/{mode}/64"), |current| {
            let call = black_box(if current {
                zmod_combo_quint_active
            } else {
                original::zmod_combo_quint_active
            } as fn(bool, WindowCounts) -> bool);
            for &p in &inputs {
                black_box(call(black_box(true), black_box(p)));
            }
        });
    }
    for mode in ["white", "active", "mixed", "exception"] {
        let inputs: Vec<_> = (0..64)
            .map(|i| {
                let mut p = color_params(i, i as f32 * 0.017);
                let grade = match mode {
                    "white" => Some(JudgeGrade::Miss),
                    "active" => Some(JudgeGrade::Excellent),
                    _ => grade(i),
                };
                p.full_combo_grade = grade;
                p.current_combo_grade = grade;
                if mode == "exception" {
                    p.elapsed_s = [f32::NAN, f32::INFINITY, f32::MAX, -f32::MAX][i % 4];
                }
                p
            })
            .collect();
        compare(&format!("glow/{mode}/64"), |current| {
            let call = black_box(if current {
                zmod_resolved_combo_color
            } else {
                original::zmod_resolved_combo_color
            } as fn(ZmodComboColorParams) -> [f32; 4]);
            for &p in &inputs {
                black_box(call(black_box(p)));
            }
        });
    }
    for (score, name) in [
        (MiniIndicatorScoreType::Ex, "ex"),
        (MiniIndicatorScoreType::HardEx, "hard"),
    ] {
        let inputs = point_inputs("normal");
        let params = ZmodMiniIndicatorParams {
            mode: MiniIndicatorMode::SubtractiveScoring,
            color_style: MiniIndicatorColorStyle::Default,
            subtractive_display: MiniIndicatorSubtractiveDisplay::Points,
            score_type: score,
            combo_color: [1.0; 4],
            is_failing: false,
            life: 1.0,
            rival_score_percent: 95.0,
            target_score_percent: 99.0,
            stream_completion: None,
        };
        compare(&format!("output/{name}/64"), |current| {
            let call = black_box(if current {
                zmod_mini_indicator_output
            } else {
                original::zmod_mini_indicator_output
            }
                as fn(
                    &MiniIndicatorProgress,
                    ZmodMiniIndicatorParams,
                ) -> Option<ZmodMiniIndicatorOutput>);
            for p in &inputs {
                black_box(call(black_box(p), black_box(params)));
            }
        });
    }
}

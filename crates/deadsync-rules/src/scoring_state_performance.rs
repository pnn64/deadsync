use super::*;
use crate::note::{HoldLifeAdvance, MAX_HOLD_LIFE, advance_hold_life_ns, hold_window_seconds};
use crate::scroll::ScrollSpeedSetting;
use crate::{
    combo::*, judgment::JudgeGrade, life::*, perf_alloc as allocations, perf_bench as bench,
};
use deadsync_core::note::NoteType;
use deadsync_core::song_time::{SongTimeNs, song_time_ns_delta_seconds, song_time_ns_from_seconds};
use std::hint::black_box;
use std::str::FromStr;

#[path = "scoring_state_original.rs"]
mod original;

fn random(state: &mut u64) -> u32 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state as u32
}

fn grade(index: u32) -> JudgeGrade {
    [
        JudgeGrade::Fantastic,
        JudgeGrade::Excellent,
        JudgeGrade::Great,
        JudgeGrade::Decent,
        JudgeGrade::WayOff,
        JudgeGrade::Miss,
    ][index as usize % 6]
}

fn life_state(meter: LifeMeter) -> (u32, u32, bool, Option<u32>) {
    (
        meter.life.to_bits(),
        meter.combo_after_miss,
        meter.is_failing,
        meter.fail_time.map(f32::to_bits),
    )
}

fn verify_life(mut visit: impl FnMut(u64)) {
    let mut seed = 0x1234_9393_9876_111a;
    let lives = [
        0.0,
        -0.0,
        0.5,
        1.0,
        1.5,
        -1.0,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ];
    let deltas = [
        -0.10,
        -0.05,
        -0.0,
        0.0,
        0.008,
        1.0,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ];
    let mut check = |input: LifeMeter, time: f32, delta: f32| {
        for fail_at_zero in [false, true] {
            let mut old = input;
            let mut new = input;
            let a = original::apply_life_delta_mode(&mut old, time, delta, fail_at_zero);
            let b = apply_life_delta_mode(&mut new, time, delta, fail_at_zero);
            assert_eq!(
                life_state(old),
                life_state(new),
                "{input:?} {time:?} {delta:?} {fail_at_zero}"
            );
            assert_eq!(
                (a.old_life.to_bits(), a.new_life.to_bits(), a.failed_now),
                (b.old_life.to_bits(), b.new_life.to_bits(), b.failed_now)
            );
            visit(u64::from(new.life.to_bits()));
            visit(u64::from(new.combo_after_miss));
            visit(u64::from(new.is_failing));
            visit(
                new.fail_time
                    .map_or(u64::MAX, |value| u64::from(value.to_bits())),
            );
            visit(u64::from(b.old_life.to_bits()));
            visit(u64::from(b.new_life.to_bits()));
            visit(u64::from(b.failed_now));
        }
    };
    for &life in &lives {
        for &delta in &deltas {
            for lock in [0, 1, 4, 5, 9, 10, 11, u32::MAX - 5, u32::MAX] {
                for failing in [false, true] {
                    check(
                        LifeMeter {
                            life,
                            combo_after_miss: lock,
                            is_failing: failing,
                            fail_time: Some(-0.0),
                        },
                        f32::NAN,
                        delta,
                    );
                }
            }
        }
    }
    for index in 0..200_000 {
        let input = LifeMeter {
            life: f32::from_bits(random(&mut seed)),
            combo_after_miss: if index < 65536 {
                index
            } else {
                random(&mut seed)
            },
            is_failing: index % 7 == 0,
            fail_time: if index & 1 == 0 {
                None
            } else {
                Some(f32::from_bits(random(&mut seed)))
            },
        };
        check(
            input,
            f32::from_bits(random(&mut seed)),
            if index < 65536 {
                -0.05
            } else {
                f32::from_bits(random(&mut seed))
            },
        );
    }
}

#[test]
fn life_updates_preserve_all_fields_float_bits_and_regen_limits() {
    verify_life(|_| {});
}

fn verify_combos(mut visit: impl FnMut(u64)) {
    let mut seed = 0x8811_2223_9876_abc3;
    for index in 0..500_000u32 {
        let combo = if index < 100_000 {
            index
        } else {
            random(&mut seed)
        };
        let input = ComboState {
            combo,
            miss_combo: random(&mut seed),
            full_combo_grade: if index % 7 == 0 {
                None
            } else {
                Some(grade(index))
            },
            current_combo_grade: if index % 11 == 0 {
                None
            } else {
                Some(grade(index / 7))
            },
            first_fc_attempt_broken: index & 1 != 0,
        };
        let added = [0, 1, 2, 4, 99, 100, 999, 1000, 10_001, u32::MAX][index as usize % 10];
        let final_grade = grade(index / 11);
        let mut old = input;
        let mut new = input;
        let missed = if index % 17 == 0 {
            0
        } else {
            random(&mut seed)
        };
        let a = original::apply_row_combo_state(&mut old, final_grade, added, missed);
        let b = apply_row_combo_state(&mut new, final_grade, added, missed);
        assert_eq!(old, new);
        assert_eq!(a, b);
        visit(u64::from(new.combo));
        visit(u64::from(new.miss_combo));
        visit(new.full_combo_grade.map_or(7, |value| value as u64));
        visit(new.current_combo_grade.map_or(7, |value| value as u64));
        visit(u64::from(new.first_fc_attempt_broken));
        visit(u64::from(b.combo_broken));
        visit(u64::from(b.hit_hundred_milestone));
        visit(u64::from(b.hit_thousand_milestone));
    }
    for combo in [
        0,
        99,
        100,
        999,
        1000,
        u32::MAX - 1000,
        u32::MAX - 1,
        u32::MAX,
    ] {
        for added in [0, 1, 2, 100, 1000, u32::MAX] {
            let mut old = ComboState {
                combo,
                ..ComboState::default()
            };
            let mut new = old;
            assert_eq!(
                original::apply_row_combo_state(&mut old, JudgeGrade::Fantastic, added, 0),
                apply_row_combo_state(&mut new, JudgeGrade::Fantastic, added, 0)
            );
            assert_eq!(old, new);
        }
    }
}

#[test]
fn combo_updates_preserve_milestones_saturation_and_all_grade_fields() {
    verify_combos(|_| {});
}

fn double_values() -> Vec<f64> {
    let mut out = vec![
        0.0,
        -0.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MAX,
        -f64::MAX,
    ];
    for point in (0..=104).map(f64::from).chain([103.5]) {
        let bits = point.to_bits();
        for value in bits.saturating_sub(4)..=bits + 4 {
            out.extend([f64::from_bits(value), -f64::from_bits(value)]);
        }
    }
    let mut seed = 0x2943_9991_1112_abc3;
    for _ in 0..200_000 {
        out.push(f64::from_bits(
            (u64::from(random(&mut seed)) << 32) | u64::from(random(&mut seed)),
        ));
    }
    out
}

fn verify_golf(mut visit: impl FnMut(u64)) {
    for value in double_values() {
        let old = original::strokes_from_ms(Some(value.abs()));
        let new = strokes_from_abs_ms(Some(value.abs()));
        assert_eq!(old, new, "ms bits {:016x}", value.to_bits());
        assert_eq!(old, original::strokes_lower_u32(Some(value.abs())));
        assert_eq!(old, original::strokes_upper_usize(Some(value.abs())));
        visit(u64::from(new));
        let old = original::tap_strokes_seconds(Some(value));
        let new = tap_strokes_seconds(Some(value));
        assert_eq!(old, new);
        assert_eq!(old, original::seconds_lower_u32(Some(value)));
        assert_eq!(old, original::seconds_upper_usize(Some(value)));
        visit(u64::from(new));
    }
    assert_eq!(strokes_from_abs_ms(None), original::strokes_from_ms(None));
    let mut seed = 0x1233_9394_ffa2_1101;
    for index in 0..200_000 {
        let value = if index % 17 == 0 {
            None
        } else {
            Some(f32::from_bits(random(&mut seed)))
        };
        let old = original::tap_strokes(value);
        let new = tap_strokes(value);
        assert_eq!(old, new);
        visit(u64::from(new));
        let input = GolfScore {
            raw_strokes: if index % 3 == 0 {
                u64::MAX - u64::from(random(&mut seed) % 128)
            } else {
                u64::from(random(&mut seed))
            },
            judged_rows: if index % 3 == 0 {
                u32::MAX
            } else {
                random(&mut seed)
            },
        };
        let mut old = input;
        let mut new = input;
        let a = original::record_golf_tap(&mut old, value);
        let b = new.record_tap(value);
        assert_eq!(a, b);
        assert_eq!(old, new);
        visit(new.raw_strokes);
        visit(u64::from(new.judged_rows));
    }
}

#[test]
#[ignore = "isolate normalized bound and conversion width"]
fn benchmark_golf_ablations() {
    let values: Vec<_> = (0..64).map(|i| Some(i as f64 * 1.5)).collect();
    for (label, before) in [
        (
            "ablate/bounds_core/64",
            original::strokes_lower_u32 as fn(Option<f64>) -> u32,
        ),
        (
            "ablate/index_core/64",
            original::strokes_upper_usize as fn(Option<f64>) -> u32,
        ),
    ] {
        compare(label, |current| {
            let call = black_box(if current {
                strokes_from_abs_ms as fn(Option<f64>) -> u32
            } else {
                before
            });
            for &value in &values {
                black_box(call(black_box(value)));
            }
        });
    }
    let values: Vec<_> = (0..64).map(|i| Some(i as f64 * 0.003 - 0.096)).collect();
    for (label, before) in [
        (
            "ablate/bounds_seconds/64",
            original::seconds_lower_u32 as fn(Option<f64>) -> u32,
        ),
        (
            "ablate/index_seconds/64",
            original::seconds_upper_usize as fn(Option<f64>) -> u32,
        ),
    ] {
        compare(label, |current| {
            let call = black_box(if current {
                tap_strokes_seconds as fn(Option<f64>) -> u32
            } else {
                before
            });
            for &value in &values {
                black_box(call(black_box(value)));
            }
        });
    }
}

#[test]
fn golf_bounds_preserve_signed_zero_nan_payloads_cutoffs_and_score_saturation() {
    verify_golf(|_| {});
}

fn verify_scroll(mut visit: impl FnMut(u64)) {
    let mut check = |text: &str| {
        let old = original::parse_scroll(text);
        let new = ScrollSpeedSetting::from_str(text);
        match (old, new) {
            (Ok(old), Ok(new)) => {
                let state = |value| match value {
                    ScrollSpeedSetting::CMod(n) => (0, n.to_bits()),
                    ScrollSpeedSetting::XMod(n) => (1, n.to_bits()),
                    ScrollSpeedSetting::MMod(n) => (2, n.to_bits()),
                };
                assert_eq!(state(old), state(new), "{text:?}");
                let (tag, bits) = state(new);
                visit(tag);
                visit(u64::from(bits));
            }
            (Err(old), Err(new)) => {
                assert_eq!(old, new);
                assert_eq!(old.capacity(), new.capacity());
                visit(3);
                visit(new.len() as u64);
                visit(new.capacity() as u64);
                for chunk in new.as_bytes().chunks(8) {
                    let mut bytes = [0; 8];
                    bytes[..chunk.len()].copy_from_slice(chunk);
                    visit(u64::from_le_bytes(bytes));
                }
            }
            (old, new) => panic!("{text:?}: {old:?} != {new:?}"),
        }
    };
    let suffixes = [
        "",
        "0",
        "-0",
        "1",
        "1.25",
        "600.5",
        "-4",
        "NaN",
        "nan",
        "inf",
        "+inf",
        "-inf",
        "1e30",
        "1e-999",
        "true",
        " 650 ",
        " \t+1.5",
        "\0",
        "1.2.3",
        "1_000",
        "１２３",
        "0x10",
        "infinity",
        "+NaN",
    ];
    for prefix in 0..=255u8 {
        for suffix in suffixes {
            for whitespace in ["", " ", "\t\r\n", "\u{2003}"] {
                check(&format!(
                    "{whitespace}{}{suffix}{whitespace}",
                    char::from(prefix)
                ));
            }
        }
    }
    let alphabet: Vec<_> = "CXMcxm0123456789eE+-. \t\r\nç🦀\0∞".chars().collect();
    let mut seed = 0x9278_abcd_ff33_1112;
    for index in 0..10_000 {
        let text: String = (0..index % 32)
            .map(|_| alphabet[random(&mut seed) as usize % alphabet.len()])
            .collect();
        check(&text);
    }
}

#[test]
fn scroll_parsing_preserves_unicode_float_bits_errors_and_capacities() {
    verify_scroll(|_| {});
}

fn verify_holds(mut visit: impl FnMut(u64)) {
    let mut seed = 0x1982_3334_abcd_9987;
    let kinds = [
        NoteType::Tap,
        NoteType::Hold,
        NoteType::Roll,
        NoteType::Mine,
        NoteType::Lift,
        NoteType::Fake,
    ];
    for index in 0..200_000 {
        let life = f32::from_bits(random(&mut seed));
        let rate = f32::from_bits(random(&mut seed));
        let elapsed = [i64::MIN, -1, 0, 1, 1_000_000, 1_000_000_000, i64::MAX][index % 7];
        for kind in kinds {
            for pressed in [false, true] {
                let old = original::advance_hold_life_ns(kind, life, pressed, elapsed, rate);
                let new = advance_hold_life_ns(kind, life, pressed, elapsed, rate);
                assert_eq!(
                    (old.life_after.to_bits(), old.zero_elapsed_music_ns),
                    (new.life_after.to_bits(), new.zero_elapsed_music_ns)
                );
                visit(u64::from(new.life_after.to_bits()));
                visit(new.zero_elapsed_music_ns.map_or(0, |_| 1));
                visit(new.zero_elapsed_music_ns.unwrap_or(0) as u64);
            }
        }
    }
    for life in [
        0.0,
        -0.0,
        0.5,
        1.0,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ] {
        for elapsed in [-1, 0, 1, i64::MAX] {
            for pressed in [false, true] {
                let old =
                    original::advance_hold_life_ns(NoteType::Hold, life, pressed, elapsed, 1.0);
                let new = advance_hold_life_ns(NoteType::Hold, life, pressed, elapsed, 1.0);
                assert_eq!(
                    (old.life_after.to_bits(), old.zero_elapsed_music_ns),
                    (new.life_after.to_bits(), new.zero_elapsed_music_ns)
                );
            }
        }
    }
}

#[test]
fn hold_life_preserves_all_note_types_float_bits_and_zero_elapsed_timestamps() {
    verify_holds(|_| {});
}

#[test]
#[ignore = "write exact state and result corpus"]
fn dump_scoring_state_trace() {
    use std::io::Write;
    let file = std::fs::File::create(
        std::env::var_os("NOTEFIELD_SCORING_STATE_TRACE_OUT").expect("trace path"),
    )
    .unwrap();
    let mut out = std::io::BufWriter::with_capacity(1024 * 1024, file);
    let mut visit = |value: u64| out.write_all(&value.to_le_bytes()).unwrap();
    verify_life(&mut visit);
    verify_combos(&mut visit);
    verify_golf(&mut visit);
    verify_scroll(&mut visit);
    verify_holds(&mut visit);
    out.flush().unwrap();
}

fn compare(label: &str, mut work: impl FnMut(bool)) {
    bench::compare(label, 10_000, &mut work);
    for (variant, current) in [("original", false), ("current", true)] {
        let (_, churn) = allocations::measure(|| work(current));
        if label != "scroll/errors/64" {
            assert_eq!(churn, allocations::Churn::default());
        }
        println!("{label}/{variant}: allocation {churn:?}");
    }
}

#[test]
#[ignore = "paired CPU cycles, throughput and allocation counts"]
fn benchmark_scoring_state() {
    for mode in ["pressed", "released", "roll", "no_elapsed"] {
        let inputs: Vec<_> = (0..64)
            .map(|i| {
                (
                    0.2 + i as f32 * 0.01,
                    if mode == "roll" {
                        NoteType::Roll
                    } else {
                        NoteType::Hold
                    },
                    mode == "pressed",
                    if mode == "no_elapsed" { 0 } else { 1_000_000 },
                )
            })
            .collect();
        compare(&format!("hold/{mode}/64"), |current| {
            let call = black_box(if current {
                advance_hold_life_ns
            } else {
                original::advance_hold_life_ns
            }
                as fn(NoteType, f32, bool, i64, f32) -> HoldLifeAdvance);
            for &(life, kind, pressed, elapsed) in &inputs {
                black_box(call(
                    black_box(kind),
                    black_box(life),
                    black_box(pressed),
                    black_box(elapsed),
                    black_box(1.0),
                ));
            }
        });
    }
    for mode in ["c", "x", "m", "errors"] {
        let inputs: Vec<_> = (0..64)
            .map(|i| match mode {
                "c" => format!("c{}", 600 + i),
                "x" => format!("X{}.{:02}", 1 + i / 32, i * 3 % 100),
                "m" => format!(" M {:.2} ", 777.25 + i as f32),
                _ => ["", "B600", "C0", "Mhello", "ç1", "XM3", "\0", " \t"][i % 8].to_owned(),
            })
            .collect();
        compare(&format!("scroll/{mode}/64"), |current| {
            let call = black_box(if current {
                ScrollSpeedSetting::from_str
            } else {
                original::parse_scroll
            } as fn(&str) -> Result<ScrollSpeedSetting, String>);
            for text in &inputs {
                drop(black_box(call(black_box(text))));
            }
        });
    }
    for mode in ["damage", "locked", "healing", "failed"] {
        let inputs: Vec<_> = (0..64)
            .map(|i| LifeMeter {
                life: 0.5 + i as f32 * 0.001,
                combo_after_miss: if mode == "locked" {
                    MAX_REGEN_COMBO_AFTER_MISS
                } else {
                    i % 10
                },
                is_failing: mode == "failed",
                fail_time: None,
            })
            .collect();
        let delta = if mode == "healing" { 0.008 } else { -0.05 };
        compare(&format!("life/{mode}/64"), |current| {
            let call = black_box(if current {
                apply_life_delta_mode
            } else {
                original::apply_life_delta_mode
            }
                as fn(&mut LifeMeter, f32, f32, bool) -> LifeDeltaResult);
            for &input in &inputs {
                let mut meter = black_box(input);
                black_box(call(
                    &mut meter,
                    black_box(123.0),
                    black_box(delta),
                    black_box(true),
                ));
                black_box(meter);
            }
        });
    }
    for (mode, added) in [("single", 1), ("small", 4), ("large", 1001), ("zero", 0)] {
        let inputs: Vec<_> = (0..64)
            .map(|i| ComboState {
                combo: i * 17,
                full_combo_grade: Some(JudgeGrade::Fantastic),
                current_combo_grade: Some(JudgeGrade::Excellent),
                ..ComboState::default()
            })
            .collect();
        compare(&format!("combo/{mode}/64"), |current| {
            let call = black_box(if current {
                apply_row_combo_state
            } else {
                original::apply_row_combo_state
            }
                as fn(&mut ComboState, JudgeGrade, u32, u32) -> ComboUpdate);
            for &input in &inputs {
                let mut state = black_box(input);
                black_box(call(
                    &mut state,
                    black_box(JudgeGrade::Fantastic),
                    black_box(added),
                    black_box(0),
                ));
                black_box(state);
            }
        });
    }
    for mode in ["none", "mixed", "hits"] {
        let inputs: Vec<_> = (0..64)
            .map(|i| {
                (
                    ComboState {
                        combo: i * 37,
                        miss_combo: i * 19,
                        ..ComboState::default()
                    },
                    if mode == "none" || (mode == "mixed" && i % 3 != 0) {
                        0
                    } else {
                        1
                    },
                )
            })
            .collect();
        compare(&format!("break/{mode}/64"), |current| {
            let call = black_box(if current {
                break_combo_state
            } else {
                original::break_combo_state
            } as fn(&mut ComboState, u32) -> ComboUpdate);
            for &(input, delta) in &inputs {
                let mut state = black_box(input);
                black_box(call(&mut state, black_box(delta)));
                black_box(state);
            }
        });
    }
    for mode in ["valid", "mixed"] {
        let values: Vec<_> = (0..64)
            .map(|i| {
                Some(
                    (if mode == "valid" {
                        i as f64 * 1.5
                    } else {
                        [
                            0.0,
                            -0.0,
                            103.5,
                            103.50000001,
                            f64::NAN,
                            f64::INFINITY,
                            -10.0,
                            50.0,
                        ][i % 8]
                    })
                    .abs(),
                )
            })
            .collect();
        compare(&format!("golf/core_{mode}/64"), |current| {
            let call = black_box(if current {
                strokes_from_abs_ms
            } else {
                original::strokes_from_ms
            } as fn(Option<f64>) -> u32);
            for &value in &values {
                black_box(call(black_box(value)));
            }
        });
    }
    let ms: Vec<_> = (0..64).map(|i| Some(i as f32 * 3.0 - 96.0)).collect();
    compare("golf/public_ms/64", |current| {
        let call = black_box(if current {
            tap_strokes
        } else {
            original::tap_strokes
        } as fn(Option<f32>) -> u32);
        for &value in &ms {
            black_box(call(black_box(value)));
        }
    });
    let seconds: Vec<_> = (0..64).map(|i| Some(i as f64 * 0.003 - 0.096)).collect();
    compare("golf/seconds/64", |current| {
        let call = black_box(if current {
            tap_strokes_seconds
        } else {
            original::tap_strokes_seconds
        } as fn(Option<f64>) -> u32);
        for &value in &seconds {
            black_box(call(black_box(value)));
        }
    });
    compare("golf/record/64", |current| {
        let call = black_box(if current {
            GolfScore::record_tap
        } else {
            original::record_golf_tap
        } as fn(&mut GolfScore, Option<f32>) -> u32);
        let mut score = black_box(GolfScore::default());
        for &value in &ms {
            black_box(call(&mut score, black_box(value)));
        }
        black_box(score);
    });
}

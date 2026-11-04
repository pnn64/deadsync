use super::*;
use crate::transforms::numeric_performance::allocations;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "text_original.rs"]
mod original;

fn random(state: &mut u64) -> u32 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state as u32
}

fn output(capacity: usize, prefix: &str) -> String {
    let mut out = String::with_capacity(capacity);
    out.push_str(prefix);
    out
}

fn equal(old: &String, new: &String) {
    assert_eq!(old, new);
    assert_eq!(old.capacity(), new.capacity(), "{old:?}");
}

fn text_inputs() -> Vec<String> {
    let mut out: Vec<String> = [
        "",
        "Boost",
        "Dynamic Sudden",
        " leading",
        "trailing ",
        "  ",
        "a  b",
        "a\tb",
        "a\nb",
        "a\0b",
        "a\u{00a0}b",
        "é雪🦀",
        "🦀  雪",
        ".00",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let mut state = 0xab25_9912_3374_1155;
    for _ in 0..1000 {
        let mut value = String::new();
        for _ in 0..16 {
            let bits = random(&mut state);
            value.push(if bits & 3 == 0 {
                ' '
            } else {
                char::from_u32(bits % 0x11_0000).unwrap_or('\u{fffd}')
            });
        }
        out.push(value);
    }
    out
}

fn float_inputs() -> Vec<f32> {
    let mut out = Vec::new();
    for center in [
        0,
        1,
        0x007f_ffff,
        0.005_f32.to_bits(),
        0.01_f32.to_bits(),
        0.1_f32.to_bits(),
        1.0_f32.to_bits(),
        10.0_f32.to_bits(),
        100.0_f32.to_bits(),
        f32::MAX.to_bits(),
        f32::INFINITY.to_bits(),
        0x7fc0_0000,
        0x7fff_ffff,
    ] {
        for delta in 0..=4 {
            for bits in [
                center.saturating_sub(delta),
                center.saturating_add(delta).min(0x7fff_ffff),
            ] {
                out.extend([f32::from_bits(bits), f32::from_bits(bits | 0x8000_0000)]);
            }
        }
    }
    let mut state = 0x4344_9821_ffff_9831;
    out.extend((0..10_000).map(|_| f32::from_bits(random(&mut state))));
    out
}

fn params(index: usize, mode: usize, noteskin: &str) -> GameplayModsTextParams<'_> {
    let percent = (index as u16).wrapping_mul(523) as i16;
    let dense = mode == 2;
    GameplayModsTextParams {
        speed: match index % 3 {
            0 => ScrollSpeedSetting::CMod(600.0 + index as f32 * 0.01),
            1 => ScrollSpeedSetting::XMod(1.0 + index as f32 * 0.03),
            _ => ScrollSpeedSetting::MMod(f32::from_bits((index as u32).wrapping_mul(982_151_111))),
        },
        noteskin,
        insert_mask: if dense { 255 } else { index as u8 & 3 },
        remove_mask: if dense { 255 } else { 0 },
        holds_mask: if dense { 255 } else { 0 },
        turn_bits: if dense { 255 } else { index as u16 & 3 },
        attack_mode: [
            GameplayModsAttackMode::On,
            GameplayModsAttackMode::Off,
            GameplayModsAttackMode::Random,
            GameplayModsAttackMode::RandomVanish,
        ][index % 4],
        mini_percent: if mode == 0 { 0 } else { percent },
        spacing_percent: if dense { percent } else { 0 },
        visual_delay_ms: if dense { percent } else { 0 },
        average_error_bar_active: mode != 0,
        avg_error_bar_intensity_centi: percent,
        avg_error_bar_interval_ms: index as u16,
        accel: if dense {
            [percent; 5]
        } else {
            [50, 0, 0, 0, 0]
        },
        visual: if dense {
            [percent; 9]
        } else {
            [100, 0, 0, 0, 0, 0, 0, 0, 0]
        },
        appearance: if dense {
            [percent; 6]
        } else {
            [0, 0, 20, 0, 0, 0]
        },
        scroll: if dense { [percent; 5] } else { [0; 5] },
        perspective_tilt: (index % 601) as i16 - 300,
        perspective_skew: ((index * 31) % 601) as i16 - 300,
        dark: if dense { percent } else { 0 },
        blind: if dense { percent } else { 0 },
        cover: if dense { percent } else { 25 },
        disabled_timing_windows: if dense { 255 } else { index as u8 & 3 },
    }
}

fn benchmark_params(index: usize, mode: usize, noteskin: &str) -> GameplayModsTextParams<'_> {
    let mut value = params(index, mode, noteskin);
    value.speed = match index % 3 {
        0 => ScrollSpeedSetting::CMod(600.0 + index as f32 * 25.0),
        1 => ScrollSpeedSetting::XMod(1.25 + index as f32 * 0.05),
        _ => ScrollSpeedSetting::MMod(700.0 + index as f32 * 10.0),
    };
    if mode == 4 {
        value.insert_mask = 0;
        value.remove_mask = 0;
        value.holds_mask = 0;
        value.turn_bits = 0;
        value.attack_mode = GameplayModsAttackMode::On;
        value.mini_percent = 0;
        value.spacing_percent = 0;
        value.visual_delay_ms = 0;
        value.average_error_bar_active = false;
        value.accel.fill(0);
        value.visual.fill(0);
        value.appearance.fill(0);
        value.scroll.fill(0);
        value.perspective_tilt = 0;
        value.perspective_skew = 0;
        value.dark = 0;
        value.blind = 0;
        value.cover = 0;
        value.disabled_timing_windows = 0;
    }
    value
}

#[test]
fn atomic_text_preserves_utf8_spaces_and_capacity() {
    for text in text_inputs() {
        for capacity in [0, 1, 4, 8, 31, 64, 127] {
            for prefix in ["", "abc0", "prefix.00雪"] {
                let mut old = output(capacity, prefix);
                let mut new = output(capacity, prefix);
                original::push_atomic_text(&mut old, &text);
                push_atomic_text(&mut new, &text);
                equal(&old, &new);
            }
        }
    }
}

#[test]
fn float_text_preserves_bits_special_values_and_capacity() {
    for value in float_inputs() {
        for capacity in [0, 8, 64] {
            for prefix in ["", "prefix.00"] {
                let mut old = output(capacity, prefix);
                let mut new = output(capacity, prefix);
                original::push_trimmed_float2(&mut old, value);
                push_trimmed_float2(&mut new, value);
                equal(&old, &new);
            }
        }
    }
}

#[test]
fn every_i16_preserves_modifier_text_and_capacity() {
    for value in i16::MIN..=i16::MAX {
        for capacity in [0, 4, 8, 64] {
            for prefix in ["", "abcd"] {
                for kind in 0..3 {
                    let mut old = output(capacity, prefix);
                    let mut new = output(capacity, prefix);
                    match kind {
                        0 => {
                            original::append_mini_part(&mut old, value);
                            append_mini_part(&mut new, value);
                        }
                        1 => {
                            original::append_spacing_part(&mut old, value);
                            append_spacing_part(&mut new, value);
                        }
                        _ => {
                            original::append_mod_part(&mut old, value, "Dynamic Sudden");
                            append_mod_part(&mut new, value, "Dynamic Sudden");
                        }
                    }
                    equal(&old, &new);
                }
            }
        }
    }
}

#[test]
fn complete_mod_strings_preserve_bytes_order_and_capacity() {
    let long = "雪  Custom Skin  ".repeat(32);
    for index in 0..4096 {
        let noteskin = ["", "cel", "a  b\t雪🦀", long.as_str()][index % 4];
        let params = params(index, index % 4, noteskin);
        equal(
            &original::gameplay_mods_text(params),
            &gameplay_mods_text(params),
        );
    }
    for mode in 0..5 {
        for index in 0..16 {
            let params = benchmark_params(index, mode, "cel");
            equal(
                &original::gameplay_mods_text(params),
                &gameplay_mods_text(params),
            );
        }
    }
}

#[test]
fn selection_masks_preserve_every_bit_pattern_and_capacity() {
    for bits in u16::MIN..=u16::MAX {
        for capacity in [0, 4, 8, 64] {
            for prefix in ["", "abcd"] {
                let mut old = output(capacity, prefix);
                let mut new = output(capacity, prefix);
                original::append_turn_parts(&mut old, bits);
                append_turn_parts(&mut new, bits);
                equal(&old, &new);
            }
        }
    }
    for bits in u8::MIN..=u8::MAX {
        for capacity in [0, 4, 8, 64] {
            for prefix in ["", "abcd"] {
                let mut old = output(capacity, prefix);
                let mut new = output(capacity, prefix);
                original::append_disabled_timing_windows(&mut old, bits);
                append_disabled_timing_windows(&mut new, bits);
                equal(&old, &new);
            }
        }
    }
}

#[test]
fn transform_masks_preserve_bytes_capacity_and_ignored_bits() {
    let mut inputs = vec![(0, 0, 0), (255, 255, 255), (0, 0, 0xe0)];
    for bits in u8::MIN..=u8::MAX {
        inputs.extend([(bits, 0, 0), (0, bits, 0), (0, 0, bits)]);
    }
    let mut state = 0x3839_ffff_2221_1114;
    inputs.extend((0..5000).map(|_| {
        (
            random(&mut state) as u8,
            random(&mut state) as u8,
            random(&mut state) as u8,
        )
    }));
    for (insert, remove, holds) in inputs {
        for capacity in [0, 4, 8, 64] {
            for prefix in ["", "abcd"] {
                let mut old = output(capacity, prefix);
                let mut new = output(capacity, prefix);
                original::push_transform_parts(&mut old, insert, remove, holds);
                push_transform_parts(&mut new, insert, remove, holds);
                equal(&old, &new);
            }
        }
    }
}

fn record(trace: &mut Vec<u8>, value: &String) {
    trace.extend_from_slice(&(value.len() as u64).to_le_bytes());
    trace.extend_from_slice(&(value.capacity() as u64).to_le_bytes());
    trace.extend_from_slice(value.as_bytes());
}

#[test]
#[ignore = "archive exact modifier bytes and returned capacities"]
fn dump_text_trace() {
    let mut trace = Vec::new();
    for text in text_inputs() {
        let mut out = output(4, "abc");
        push_atomic_text(&mut out, &text);
        record(&mut trace, &out);
    }
    for value in float_inputs() {
        trace.extend_from_slice(&value.to_bits().to_le_bytes());
        let mut out = output(8, "prefix.00");
        push_trimmed_float2(&mut out, value);
        record(&mut trace, &out);
    }
    for value in i16::MIN..=i16::MAX {
        let mut out = output(4, "abcd");
        append_mini_part(&mut out, value);
        record(&mut trace, &out);
    }
    let long = "雪  Custom Skin  ".repeat(32);
    for index in 0..4096 {
        let noteskin = ["", "cel", "a  b\t雪🦀", long.as_str()][index % 4];
        record(
            &mut trace,
            &gameplay_mods_text(params(index, index % 4, noteskin)),
        );
    }
    for bits in u16::MIN..=u16::MAX {
        let mut out = output(4, "abcd");
        append_turn_parts(&mut out, bits);
        record(&mut trace, &out);
    }
    for bits in u8::MIN..=u8::MAX {
        let mut out = output(4, "abcd");
        append_disabled_timing_windows(&mut out, bits);
        record(&mut trace, &out);
    }
    std::fs::write(
        std::env::var_os("NOTEFIELD_TEXT_TRACE_OUT").expect("trace path"),
        trace,
    )
    .unwrap();
}

fn compare(label: &str, mut work: impl FnMut(bool)) {
    bench::compare(label, 250, &mut work);
    let mut counts = Vec::new();
    for (variant, current) in [("original", false), ("current", true)] {
        let (_, churn) = allocations::measure(|| work(current));
        println!("{label}/{variant}: allocation {churn:?}");
        counts.push(churn);
    }
    assert_eq!(counts[0], counts[1], "{label}");
}

#[test]
#[ignore = "paired CPU cycles and allocation counts"]
fn benchmark_text() {
    let mut out = String::with_capacity(4096);
    for (label, texts) in [
        (
            "atomic/simple/64",
            (0..64).map(|i| format!("Modifier{i}")).collect::<Vec<_>>(),
        ),
        (
            "atomic/spaced/64",
            (0..64).map(|i| format!("  Modifier  {i}  ")).collect(),
        ),
        (
            "atomic/unicode/64",
            (0..64).map(|i| format!("雪🦀{i}\u{00a0}é")).collect(),
        ),
    ] {
        compare(label, |current| {
            let call = black_box(if current {
                push_atomic_text as fn(&mut String, &str)
            } else {
                original::push_atomic_text as fn(&mut String, &str)
            });
            for text in &texts {
                out.clear();
                call(black_box(&mut out), black_box(text));
                black_box(&out);
            }
        });
    }
    for mode in ["integer", "decimal", "special"] {
        let values: Vec<_> = (0..64)
            .map(|i| match mode {
                "integer" => i as f32 - 32.0,
                "decimal" => (i as f32 - 32.0) * 0.13,
                _ => [
                    0.0,
                    -0.0,
                    f32::MAX,
                    -f32::MAX,
                    f32::INFINITY,
                    f32::NEG_INFINITY,
                    f32::NAN,
                ][i % 7],
            })
            .collect();
        compare(&format!("float/{mode}/64"), |current| {
            let call = black_box(if current {
                push_trimmed_float2 as fn(&mut String, f32)
            } else {
                original::push_trimmed_float2 as fn(&mut String, f32)
            });
            for &value in &values {
                out.clear();
                call(black_box(&mut out), black_box(value));
                black_box(&out);
            }
        });
    }
    let values: Vec<_> = (0..128).map(|i| (i * 523 - 32768) as i16).collect();
    for kind in ["mini", "spacing", "mod"] {
        compare(&format!("integer/{kind}/128"), |current| {
            let call = black_box(match (kind, current) {
                ("mini", true) => append_mini_part as fn(&mut String, i16),
                ("mini", false) => original::append_mini_part as fn(&mut String, i16),
                ("spacing", true) => append_spacing_part as fn(&mut String, i16),
                ("spacing", false) => original::append_spacing_part as fn(&mut String, i16),
                (_, true) => {
                    |out: &mut String, value: i16| append_mod_part(out, value, "Dynamic Sudden")
                }
                (_, false) => |out: &mut String, value: i16| {
                    original::append_mod_part(out, value, "Dynamic Sudden")
                },
            });
            for &value in &values {
                out.clear();
                call(black_box(&mut out), black_box(value));
                black_box(&out);
            }
        });
    }
    for (mode, label) in ["basic", "mixed", "dense", "unicode", "default"]
        .into_iter()
        .enumerate()
    {
        let inputs: Vec<_> = (0..16)
            .map(|index| {
                benchmark_params(
                    index,
                    mode,
                    if mode == 3 {
                        "雪🦀  Custom Skin"
                    } else {
                        "cel"
                    },
                )
            })
            .collect();
        compare(&format!("whole/{label}/16"), |current| {
            let call = black_box(if current {
                gameplay_mods_text as fn(GameplayModsTextParams<'_>) -> String
            } else {
                original::gameplay_mods_text as fn(GameplayModsTextParams<'_>) -> String
            });
            for &params in &inputs {
                black_box(call(black_box(params)));
            }
        });
    }
    for mode in ["sparse", "dense", "mixed", "zero"] {
        let turns: Vec<_> = (0..128)
            .map(|i| match mode {
                "sparse" => 1 << (i % 8),
                "dense" => u16::MAX,
                "mixed" => (i as u16).wrapping_mul(523),
                _ => 0,
            })
            .collect();
        compare(&format!("turns/{mode}/128"), |current| {
            let call = black_box(if current {
                append_turn_parts as fn(&mut String, u16)
            } else {
                original::append_turn_parts as fn(&mut String, u16)
            });
            for &bits in &turns {
                out.clear();
                call(black_box(&mut out), black_box(bits));
                black_box(&out);
            }
        });
        let windows: Vec<_> = (0..128)
            .map(|i| match mode {
                "sparse" => 1 << (i % 5),
                "dense" => u8::MAX,
                "mixed" => (i as u8).wrapping_mul(53),
                _ => 0,
            })
            .collect();
        compare(&format!("windows/{mode}/128"), |current| {
            let call = black_box(if current {
                append_disabled_timing_windows as fn(&mut String, u8)
            } else {
                original::append_disabled_timing_windows as fn(&mut String, u8)
            });
            for &bits in &windows {
                out.clear();
                call(black_box(&mut out), black_box(bits));
                black_box(&out);
            }
        });
        let transforms: Vec<_> = (0..32)
            .map(|i| match mode {
                "sparse" => (1 << (i % 8), 0, 0),
                "dense" => (255, 255, 255),
                "mixed" => (
                    (i as u8).wrapping_mul(17),
                    (i as u8).wrapping_mul(53),
                    (i as u8).wrapping_mul(13),
                ),
                _ => (0, 0, 0),
            })
            .collect();
        compare(&format!("transforms/{mode}/32"), |current| {
            let call = black_box(if current {
                push_transform_parts as fn(&mut String, u8, u8, u8)
            } else {
                original::push_transform_parts as fn(&mut String, u8, u8, u8)
            });
            for &(insert, remove, holds) in &transforms {
                out.clear();
                call(
                    black_box(&mut out),
                    black_box(insert),
                    black_box(remove),
                    black_box(holds),
                );
                black_box(&out);
            }
        });
    }
}

use super::*;
use mlua::{Lua, Value};
use std::hint::black_box;

#[path = "alignment_access_baseline.rs"]
mod baseline;

fn results(value: &Value, old: bool) -> (Option<u32>, Option<u32>, Option<TextAlign>) {
    if old {
        (
            baseline::song_lua_halign_value(value).map(f32::to_bits),
            baseline::song_lua_valign_value(value).map(f32::to_bits),
            baseline::song_lua_text_align_value(value),
        )
    } else {
        (
            song_lua_halign_value(value).map(f32::to_bits),
            song_lua_valign_value(value).map(f32::to_bits),
            song_lua_text_align_value(value),
        )
    }
}

#[test]
fn lua_access_alignment_preserves_numbers_quotes_prefix_order_and_invalid_values() {
    let lua = Lua::new();
    let mut values = vec![
        Value::Nil,
        Value::Boolean(false),
        Value::Boolean(true),
        Value::Integer(i64::MAX),
        Value::Integer(-3),
        Value::Number(-0.0),
        Value::Number(f64::NAN),
        Value::Number(f64::INFINITY),
        Value::Number(f64::MAX),
        Value::Table(lua.create_table().unwrap()),
        Value::String(lua.create_string([0xff]).unwrap()),
    ];
    for raw in [
        "left",
        "CENTER",
        "middle",
        "Right",
        "top",
        "BOTTOM",
        "",
        " -0 ",
        "NaN",
        "-NaN",
        "+inf",
        "1e999",
        "1e-999",
        "1.25",
        "\0left",
        "éあ💃",
        "'0.5'",
        "HorizAlign_0.5",
        " left ",
        "' left '",
    ] {
        for prefix in [
            "",
            "HorizAlign_",
            "VertAlign_",
            "HorizAlign_HorizAlign_",
            "VertAlign_VertAlign_",
            "HorizAlign_VertAlign_",
            "VertAlign_HorizAlign_",
        ] {
            for text in [
                format!("{prefix}{raw}"),
                format!("\u{2003}\"'{prefix}{raw}'\"\t"),
                format!("'\"{prefix}{raw}\"'"),
            ] {
                values.push(Value::String(lua.create_string(text).unwrap()));
            }
        }
    }
    for len in [31, 32, 33, 64, 1024] {
        values.push(Value::String(
            lua.create_string(format!("{}LEFT", "hOrIzAlIgN_".repeat(len)))
                .unwrap(),
        ));
        values.push(Value::String(lua.create_string("あ".repeat(len)).unwrap()));
    }
    for value in &values {
        assert_eq!(results(value, false), results(value, true), "{value:?}");
    }
    let left = Value::String(lua.create_string("HorizAlign_LEFT").unwrap());
    assert_eq!(song_lua_halign_value(&left), Some(0.0));
    assert_eq!(song_lua_valign_value(&left), None);
}

#[test]
fn lua_access_alignment_matches_generated_tokens() {
    let lua = Lua::new();
    let pieces = [
        "left",
        "TOP",
        "middle",
        "HorizAlign_",
        "vertalign_",
        "'",
        "\"",
        " ",
        "\u{2003}",
        "0",
        ".",
        "-",
        "NaN",
        "é",
        "\0",
    ];
    let mut seed = 23u64;
    for _ in 0..10_000 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let mut text = String::new();
        for _ in 0..seed as usize % 12 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            text.push_str(pieces[(seed >> 32) as usize % pieces.len()]);
        }
        let value = Value::String(lua.create_string(text).unwrap());
        assert_eq!(results(&value, false), results(&value, true));
    }
}

#[test]
fn lua_access_alignment_has_no_warm_churn_for_common_values() {
    let lua = Lua::new();
    let values: Vec<_> = [
        "HorizAlign_Left",
        "VertAlign_Bottom",
        "CENTER",
        "0.75",
        "bad",
        "",
    ]
    .into_iter()
    .map(|s| Value::String(lua.create_string(s).unwrap()))
    .chain([
        Value::Number(-0.0),
        Value::Nil,
        Value::Table(lua.create_table().unwrap()),
    ])
    .collect();
    for value in &values {
        black_box(results(value, false));
    }
    lua.gc_stop();
    crate::perf::assert_no_churn(|| {
        for value in &values {
            black_box(results(black_box(value), false));
        }
    });
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_access_bench_alignment() {
    let lua = Lua::new();
    let long = format!("{}LEFT", "HorizAlign_".repeat(100));
    for (name, inputs) in [
        (
            "align_names",
            vec!["left", "VertAlign_Bottom", "HorizAlign_Center", "RIGHT"],
        ),
        ("align_numbers", vec!["-0", "0.75", "NaN", "1e999"]),
        ("align_invalid", vec!["", "bad", "éあ", "\0"]),
        ("align_long", vec![long.as_str()]),
    ] {
        let values: Vec<_> = inputs
            .iter()
            .map(|s| Value::String(lua.create_string(s).unwrap()))
            .collect();
        for value in &values {
            assert_eq!(results(value, true), results(value, false));
        }
        lua.gc_stop();
        for cold in [false, true] {
            for old in order() {
                crate::perf::measure_sampled(
                    &format!("{name}_cold_{cold}/{}", if old { "old" } else { "new" }),
                    128,
                    inputs.len() * 16,
                    || {
                        for _ in 0..16 {
                            for (input, value) in inputs.iter().zip(&values) {
                                if cold {
                                    let value =
                                        Value::String(lua.create_string(black_box(input)).unwrap());
                                    black_box(results(&value, old));
                                } else {
                                    black_box(results(black_box(value), old));
                                }
                            }
                        }
                    },
                );
            }
        }
    }
    let values = [
        Value::Number(0.5),
        Value::Integer(1),
        Value::Number(f64::NAN),
        Value::Nil,
    ];
    for value in &values {
        assert_eq!(results(value, true), results(value, false));
    }
    for old in order() {
        crate::perf::measure_sampled(
            &format!("align_native/{}", if old { "old" } else { "new" }),
            512,
            64,
            || {
                for _ in 0..16 {
                    for value in &values {
                        black_box(results(black_box(value), old));
                    }
                }
            },
        );
    }
}

fn order() -> [bool; 2] {
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    }
}

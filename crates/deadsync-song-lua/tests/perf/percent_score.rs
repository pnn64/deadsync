use super::*;
use std::hint::black_box;

#[path = "percent_score_baseline.rs"]
mod baseline;

fn callback(lua: &Lua, old: bool) -> Function {
    if old {
        lua.create_function(baseline::format_percent_score).unwrap()
    } else {
        lua.create_function(format_percent_score).unwrap()
    }
}

#[test]
fn percent_score_stack_format_preserves_coercions_defaults_and_extra_arguments() {
    let lua = Lua::new();
    let old = callback(&lua, true);
    let new = callback(&lua, false);
    let mut values = vec![
        Value::Nil,
        Value::Boolean(false),
        Value::Integer(i64::MAX),
        Value::Integer(i64::MIN),
        Value::Table(lua.create_table().unwrap()),
        Value::Number(f64::NAN),
        Value::Number(f64::INFINITY),
        Value::Number(f64::MAX),
    ];
    for text in [
        "",
        "0.99995",
        "  -0.0  ",
        "NaN",
        "inf",
        "-inf",
        "1e40",
        "abc",
        "\u{2003}0.25\u{a0}",
    ] {
        values.push(Value::String(lua.create_string(text).unwrap()));
    }
    values.push(Value::String(lua.create_string([255, 0, 254]).unwrap()));
    for value in values {
        let a = old
            .call::<mlua::LuaString>((value.clone(), 999, "ignored"))
            .unwrap();
        let b = new
            .call::<mlua::LuaString>((value, 999, "ignored"))
            .unwrap();
        assert_eq!(a.as_bytes().as_ref(), b.as_bytes().as_ref());
    }
    assert_eq!(
        old.call::<String>(()).unwrap(),
        new.call::<String>(()).unwrap()
    );
    install_basic_globals(
        &lua,
        &SongLuaCompileContext::new("", "Score formatting"),
        false,
    )
    .unwrap();
    let installed: Function = lua.globals().get("FormatPercentScore").unwrap();
    assert_eq!(installed.call::<String>(0.125).unwrap(), "12.50%");
}

#[test]
fn percent_score_stack_format_matches_frozen_callback_for_float_extremes_and_random_bits() {
    let lua = Lua::new();
    lua.gc_stop();
    let old = callback(&lua, true);
    let new = callback(&lua, false);
    let mut bits = 0x9876_5432_u32;
    let values = [
        0.0,
        -0.0,
        f32::MAX,
        -f32::MAX,
        f32::MIN_POSITIVE,
        f32::EPSILON,
        0.99995,
        0.99985,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
    ];
    for value in values.into_iter().chain((0..16384).map(|_| {
        bits ^= bits << 13;
        bits ^= bits >> 17;
        bits ^= bits << 5;
        f32::from_bits(bits)
    })) {
        let a = old.call::<mlua::LuaString>(f64::from(value)).unwrap();
        let b = new.call::<mlua::LuaString>(f64::from(value)).unwrap();
        assert_eq!(
            a.as_bytes().as_ref(),
            b.as_bytes().as_ref(),
            "{:08x}",
            value.to_bits()
        );
        assert!(b.as_bytes().len() <= 64);
    }
}

#[test]
fn percent_score_stack_format_removes_temporary_allocation() {
    let lua = Lua::new();
    lua.gc_stop();
    for old in [true, false] {
        black_box(
            if old {
                baseline::format_percent_score(&lua, MultiValue::new())
            } else {
                format_percent_score(&lua, MultiValue::new())
            }
            .unwrap(),
        );
    }
    crate::perf::assert_no_churn(|| {
        black_box(format_percent_score(&lua, MultiValue::new()).unwrap());
    });
    crate::perf::assert_reduced_churn(
        || {
            black_box(baseline::format_percent_score(&lua, MultiValue::new()).unwrap());
        },
        || {
            black_box(format_percent_score(&lua, MultiValue::new()).unwrap());
        },
    );
    let old = callback(&lua, true);
    let new = callback(&lua, false);
    for value in [0.0, 0.98765, -0.25, f64::from(f32::MAX)] {
        old.call::<Value>(value).unwrap();
        new.call::<Value>(value).unwrap();
        crate::perf::assert_reduced_churn(
            || {
                black_box(old.call::<Value>(value).unwrap());
            },
            || {
                black_box(new.call::<Value>(value).unwrap());
            },
        );
    }
}

#[test]
#[ignore = "manual old/new CPU, throughput and allocator benchmark"]
fn percent_score_bench() {
    let lua = Lua::new();
    lua.gc_stop();
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    for (name, value) in [
        ("zero", 0.0),
        ("normal", 0.98765),
        ("negative", -0.25),
        ("large", f64::from(f32::MAX / 128.0)),
        ("overflow", f64::from(f32::MAX)),
        ("nan", f64::NAN),
    ] {
        for old in order {
            let function = callback(&lua, old);
            let mode = if old { "old" } else { "new" };
            crate::perf::measure_sampled(&format!("percent/{name}/{mode}"), 4096, 64, || {
                for _ in 0..64 {
                    black_box(function.call::<Value>(black_box(value)).unwrap());
                }
            });
        }
    }
}

use super::*;
use std::hint::black_box;

#[path = "sequence_capacity_baseline.rs"]
mod baseline;

fn split(lua: &Lua, text: &str, separator: &str, old: bool) -> Table {
    if old {
        baseline::create_split_table(lua, text, separator)
    } else {
        create_split_table(lua, text, separator)
    }
    .unwrap()
}

fn range(lua: &Lua, args: &MultiValue, old: bool) -> Value {
    if old {
        baseline::create_range_table(lua, args)
    } else {
        create_range_table(lua, args)
    }
    .unwrap()
}

fn fingerprint(value: Value) -> String {
    match value {
        Value::Number(n) => format!("number:{:x}", n.to_bits()),
        Value::String(s) => format!("string:{:?}", s.as_bytes().as_ref()),
        v => format!("{v:?}"),
    }
}

fn snapshot(value: Value) -> (usize, Vec<String>) {
    if let Value::Table(table) = value {
        let len = table.raw_len();
        let values = table
            .sequence_values::<Value>()
            .map(|v| fingerprint(v.unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(len, values.len());
        (len, values)
    } else {
        (0, vec![fingerprint(value)])
    }
}

#[test]
fn sequence_capacity_splits_preserve_bytes_empty_segments_and_lua_length() {
    let lua = Lua::new();
    for text in [
        "".to_string(),
        "a".to_string(),
        ",a,,b,".to_string(),
        "::a::::b::".to_string(),
        "\u{e9}\0\u{65e5}".to_string(),
        "abc".repeat(8192),
    ] {
        for separator in ["", ",", "::", "bc", "\0", "\u{e9}", "not present"] {
            let old = split(&lua, &text, separator, true);
            let new = split(&lua, &text, separator, false);
            assert_eq!(
                snapshot(Value::Table(old)),
                snapshot(Value::Table(new)),
                "separator={separator:?}"
            );
        }
    }
    let mut seed = 0x1635_u64;
    for _ in 0..256 {
        let mut text = String::new();
        for _ in 0..128 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            text.push(['a', ',', '\0', '\u{e9}'][(seed >> 32) as usize % 4]);
        }
        for separator in [",", "a,", "\0"] {
            assert_eq!(
                snapshot(Value::Table(split(&lua, &text, separator, true))),
                snapshot(Value::Table(split(&lua, &text, separator, false)))
            );
        }
    }
}

#[test]
fn sequence_capacity_ranges_preserve_float_bits_limits_coercions_and_direction() {
    let lua = Lua::new();
    let receiver = lua.create_table().unwrap();
    let mut cases = vec![
        MultiValue::new(),
        vec![Value::Nil].into_iter().collect(),
        vec![Value::Table(receiver)].into_iter().collect(),
    ];
    for (start, stop, step) in [
        (1.0, 64.0, 1.0),
        (64.0, 1.0, 1.0),
        (1.0, 64.0, -1.0),
        (0.0, 1.0, 0.1),
        (-0.0, 0.0, 1.0),
        (0.0, 1.0, 0.0),
        (0.0, 1.0, f32::EPSILON),
        (1e20, 1e20, 1.0),
        (0.0, 1e20, 1.0),
        (1e-6, 0.0, -1e-7),
        (0.0, 1.0, f32::INFINITY),
    ] {
        cases.push(
            [start, stop, step]
                .map(|v| Value::Number(f64::from(v)))
                .into_iter()
                .collect(),
        );
    }
    for text in ["10", "-5", "NaN", "inf", "-inf", "invalid"] {
        cases.push(
            vec![Value::String(lua.create_string(text).unwrap())]
                .into_iter()
                .collect(),
        );
    }
    for step in ["NaN", "inf", "-inf", "0.5", "0"] {
        cases.push(
            vec![
                Value::Integer(0),
                Value::Integer(10),
                Value::String(lua.create_string(step).unwrap()),
            ]
            .into_iter()
            .collect(),
        );
    }
    let mut seed = 0x1635_u64;
    for _ in 0..256 {
        let mut values = Vec::new();
        for _ in 0..3 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            values.push(Value::Number(f64::from(f32::from_bits(
                (seed >> 32) as u32,
            ))));
        }
        cases.push(values.into_iter().collect());
    }
    for mut args in cases {
        let expected = snapshot(range(&lua, &args, true));
        let actual = snapshot(range(&lua, &args, false));
        assert_eq!(expected, actual, "args={args:?}");
        assert!(actual.0 <= 10_000);
        args.extend(std::iter::repeat_n(Value::Nil, 257));
        assert_eq!(expected, snapshot(range(&lua, &args, false)));
    }
}

#[test]
fn sequence_capacity_dense_outputs_reduce_requested_bytes_and_growth() {
    let lua = Lua::new();
    lua.gc_stop();
    let text = "same,".repeat(1023) + "same";
    let args: MultiValue = vec![Value::Integer(1), Value::Integer(1024), Value::Integer(1)]
        .into_iter()
        .collect();
    for _ in 0..8 {
        black_box(split(&lua, &text, ",", true));
        black_box(split(&lua, &text, ",", false));
        black_box(range(&lua, &args, true));
        black_box(range(&lua, &args, false));
    }
    // Required Lua output tables still allocate. Bound the new operation to one
    // table object and one dense array allocation, with no growth reallocations.
    crate::perf::assert_churn_budget(2, 16 * 1024 + 128, || {
        black_box(split(&lua, &text, ",", false));
    });
    crate::perf::assert_churn_budget(2, 16 * 1024 + 128, || {
        black_box(range(&lua, &args, false));
    });
}

#[test]
#[ignore = "manual release comparison against 0.5.1634"]
fn retained_values_bench() {
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    };
    for count in [1, 8, 64, 1024] {
        let text = std::iter::repeat_n("same", count)
            .collect::<Vec<_>>()
            .join(",");
        for old in order {
            crate::perf::measure_sampled_with_setup(
                &format!("sequence/split/{count}/{}", if old { "old" } else { "new" }),
                128,
                count,
                || {
                    let lua = Lua::new();
                    lua.gc_stop();
                    lua.create_string("same").unwrap();
                    lua
                },
                |lua| {
                    black_box(split(lua, &text, ",", old));
                },
            );
        }
        let args: MultiValue = vec![
            Value::Integer(1),
            Value::Integer(count as i64),
            Value::Integer(1),
        ]
        .into_iter()
        .collect();
        for old in order {
            crate::perf::measure_sampled_with_setup(
                &format!("sequence/range/{count}/{}", if old { "old" } else { "new" }),
                128,
                count,
                || {
                    let lua = Lua::new();
                    lua.gc_stop();
                    lua
                },
                |lua| {
                    black_box(range(lua, &args, old));
                },
            );
        }
    }
    for (label, text, separator) in [
        ("empty", "", ","),
        ("characters", "\u{e9}\0abc", ""),
        ("overlap", "ababab", "aba"),
    ] {
        let units = if separator.is_empty() {
            text.chars().count().max(1)
        } else {
            text.split(separator).count()
        };
        for old in order {
            crate::perf::measure_sampled_with_setup(
                &format!("sequence/split/{label}/{}", if old { "old" } else { "new" }),
                128,
                units,
                || {
                    let lua = Lua::new();
                    lua.gc_stop();
                    lua
                },
                |lua| {
                    black_box(split(lua, text, separator, old));
                },
            );
        }
    }
    for (label, args, units) in [
        (
            "empty",
            vec![Value::Integer(1), Value::Integer(64), Value::Integer(-1)],
            1,
        ),
        (
            "fractional",
            vec![Value::Integer(0), Value::Integer(1), Value::Number(0.1)],
            11,
        ),
        (
            "capped",
            vec![
                Value::Integer(0),
                Value::Integer(1_000_000),
                Value::Integer(1),
            ],
            10_000,
        ),
    ] {
        let args: MultiValue = args.into_iter().collect();
        for old in order {
            crate::perf::measure_sampled_with_setup(
                &format!("sequence/range/{label}/{}", if old { "old" } else { "new" }),
                if label == "capped" { 32 } else { 128 },
                units,
                || {
                    let lua = Lua::new();
                    lua.gc_stop();
                    lua
                },
                |lua| {
                    black_box(range(lua, &args, old));
                },
            );
        }
    }
}

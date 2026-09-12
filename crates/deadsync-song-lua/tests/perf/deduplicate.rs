use super::*;
use std::hint::black_box;

#[path = "deduplicate_baseline.rs"]
mod baseline;

fn deduplicate(lua: &Lua, table: &Table, old: bool) -> Table {
    if old {
        baseline::deduplicate_lua_table(lua, table)
    } else {
        deduplicate_lua_table(lua, table)
    }
    .unwrap()
}

fn fingerprint(table: &Table) -> Vec<String> {
    table
        .sequence_values::<Value>()
        .map(|v| match v.unwrap() {
            Value::Number(n) => format!("number:{:x}", n.to_bits()),
            Value::String(s) => format!("bytes:{:?}", s.as_bytes().as_ref()),
            other => format!("{other:?}"),
        })
        .collect()
}

fn compare(lua: &Lua, values: Vec<Value>) -> Vec<String> {
    let input = lua.create_sequence_from(values).unwrap();
    let before = fingerprint(&input);
    let old = deduplicate(lua, &input, true);
    let new = deduplicate(lua, &input, false);
    assert_eq!(old.raw_len(), new.raw_len());
    assert!(new.metatable().is_none());
    assert_eq!(fingerprint(&old), fingerprint(&new));
    assert_eq!(fingerprint(&input), before);
    fingerprint(&new)
}

#[test]
fn lua_cleanup_deduplicate_preserves_order_across_index_thresholds() {
    let lua = Lua::new();
    for count in [0, 1, 8, 31, 32, 33, 64, 127, 128, 129, 256, 1024] {
        for unique in [1, 8, 31, 32, 33, 128, 1024] {
            let values = (0..count)
                .map(|i| Value::Integer((i % unique) as i64))
                .collect();
            let result = compare(&lua, values);
            assert_eq!(result.len(), count.min(unique));
        }
    }
}

#[test]
fn lua_cleanup_deduplicate_preserves_non_transitive_integer_float_comparisons() {
    let lua = Lua::new();
    let cases = [
        [
            Value::Integer(9_007_199_254_740_992),
            Value::Integer(9_007_199_254_740_993),
            Value::Number(9_007_199_254_740_992.0),
        ],
        [
            Value::Integer(i64::MAX),
            Value::Integer(i64::MAX - 1),
            Value::Number(i64::MAX as f64),
        ],
        [
            Value::Integer(i64::MIN),
            Value::Integer(i64::MIN + 1),
            Value::Number(i64::MIN as f64),
        ],
    ];
    for case in cases {
        for order in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            for prefix in [0, 40] {
                let mut values: Vec<_> = (0..prefix).map(Value::Integer).collect();
                values.extend(order.into_iter().map(|i| case[i].clone()));
                values.extend((0..128).map(|i| case[i % 3].clone()));
                let result = compare(&lua, values);
                assert_eq!(
                    result.len(),
                    prefix as usize + if order[0] == 2 { 1 } else { 2 }
                );
            }
        }
    }
}

#[test]
fn lua_cleanup_deduplicate_preserves_nan_signed_zero_and_infinities() {
    let lua = Lua::new();
    let mut values: Vec<_> = (1..=40).map(Value::Integer).collect();
    for _ in 0..20 {
        values.extend([
            Value::Number(-0.0),
            Value::Number(0.0),
            Value::Integer(0),
            Value::Number(f64::NAN),
            Value::Number(f64::from_bits(0x7ff8_0000_0000_0001)),
            Value::Number(f64::INFINITY),
            Value::Number(f64::NEG_INFINITY),
        ]);
    }
    let result = compare(&lua, values);
    assert_eq!(result.len(), 83);
    assert_eq!(result[40], "number:8000000000000000");
}

#[test]
fn lua_cleanup_deduplicate_preserves_utf8_validation_and_binary_strings() {
    let lua = Lua::new();
    let mut values: Vec<_> = (0..40)
        .map(|i| Value::String(lua.create_string(format!("prefix_{i}")).unwrap()))
        .collect();
    let valid = ["", "é\0中🦀", &"long".repeat(100)];
    for _ in 0..40 {
        for text in valid {
            values.push(Value::String(lua.create_string(text).unwrap()));
        }
        values.push(Value::String(lua.create_string([255, 0, 254]).unwrap()));
    }
    assert_eq!(compare(&lua, values).len(), 83);
}

#[test]
fn lua_cleanup_deduplicate_uses_table_identity_and_keeps_opaque_values() {
    let lua = Lua::new();
    let mt = lua.create_table().unwrap();
    mt.raw_set(
        "__eq",
        lua.create_function(|_, _: MultiValue| -> mlua::Result<bool> {
            Err(mlua::Error::runtime("eq trap"))
        })
        .unwrap(),
    )
    .unwrap();
    let first = lua.create_table().unwrap();
    first.set_metatable(Some(mt.clone())).unwrap();
    let second = lua.create_table().unwrap();
    second.set_metatable(Some(mt)).unwrap();
    let function = lua.create_function(|_, ()| Ok(())).unwrap();
    let mut values: Vec<_> = (0..40).map(Value::Integer).collect();
    for _ in 0..32 {
        values.extend([
            Value::Table(first.clone()),
            Value::Table(second.clone()),
            Value::Function(function.clone()),
            Value::Boolean(false),
        ]);
    }
    assert_eq!(compare(&lua, values).len(), 75);
}

#[test]
fn lua_cleanup_deduplicate_stops_at_holes_and_ignores_hash_fields() {
    let lua = Lua::new();
    let input = lua.create_sequence_from(0..256).unwrap();
    input.raw_set(45, Value::Nil).unwrap();
    input.raw_set("extra", 999).unwrap();
    let mt = lua.create_table().unwrap();
    for key in ["__index", "__pairs", "__len"] {
        mt.raw_set(
            key,
            lua.create_function(|_, _: MultiValue| -> mlua::Result<()> {
                Err(mlua::Error::runtime("metamethod trap"))
            })
            .unwrap(),
        )
        .unwrap();
    }
    input.set_metatable(Some(mt)).unwrap();
    let old = deduplicate(&lua, &input, true);
    let new = deduplicate(&lua, &input, false);
    assert_eq!(fingerprint(&old), fingerprint(&new));
    assert_eq!(new.raw_len(), 44);
}

#[test]
fn lua_cleanup_deduplicate_matches_parent_on_generated_mixed_sequences() {
    let lua = Lua::new();
    let shared = lua.create_table().unwrap();
    let function = lua.create_function(|_, ()| Ok(())).unwrap();
    let mut seed = 0x1234_5678_u64;
    for len in [16, 128, 2048] {
        let values = (0..len)
            .map(|_| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                let n = ((seed >> 32) % 129) as i64;
                match seed % 9 {
                    0 => Value::Integer(n),
                    1 => Value::Number(n as f64),
                    2 => Value::Number(f64::from_bits(0x7ff8_0000_0000_0000 | n as u64)),
                    3 => Value::String(lua.create_string(format!("value_{n}")).unwrap()),
                    4 => Value::Table(shared.clone()),
                    5 => Value::Function(function.clone()),
                    6 => Value::Boolean(n % 2 == 0),
                    7 => Value::String(lua.create_string([255, n as u8]).unwrap()),
                    _ => Value::Number(n as f64 / 4.0),
                }
            })
            .collect();
        compare(&lua, values);
    }
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_cleanup_bench_deduplicate() {
    for (kind, count, unique) in [
        ("integer", 0, 1),
        ("integer", 8, 8),
        ("integer", 32, 32),
        ("integer", 127, 127),
        ("integer", 128, 128),
        ("integer", 256, 256),
        ("integer", 1024, 1024),
        ("integer", 1024, 8),
        ("integer", 1024, 32),
        ("string", 8, 8),
        ("string", 128, 128),
        ("string", 512, 512),
        ("string", 1024, 8),
        ("table", 128, 128),
        ("table", 512, 512),
    ] {
        let lua = Lua::new();
        let pool: Vec<_> = (0..unique)
            .map(|i| match kind {
                "string" => Value::String(
                    lua.create_string(format!("value_{i:05}_song_actor"))
                        .unwrap(),
                ),
                "table" => Value::Table(lua.create_table().unwrap()),
                _ => Value::Integer(i as i64),
            })
            .collect();
        let input = lua
            .create_sequence_from((0..count).map(|i| pool[i % unique].clone()))
            .unwrap();
        assert_eq!(
            fingerprint(&deduplicate(&lua, &input, true)),
            fingerprint(&deduplicate(&lua, &input, false))
        );
        lua.gc_collect().unwrap();
        lua.gc_stop();
        let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            [false, true]
        } else {
            [true, false]
        };
        for old in order {
            crate::perf::measure_sampled(
                &format!(
                    "deduplicate_{kind}_{count}_{unique}/{}",
                    if old { "old" } else { "new" }
                ),
                32,
                count.max(1),
                || {
                    drop(black_box(deduplicate(
                        black_box(&lua),
                        black_box(&input),
                        black_box(old),
                    )));
                },
            );
        }
    }
}

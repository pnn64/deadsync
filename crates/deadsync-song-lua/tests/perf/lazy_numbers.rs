use super::*;
use std::hint::black_box;

#[path = "lazy_numbers_baseline.rs"]
mod baseline;

fn deduplicate(lua: &Lua, input: &Table, old: bool) -> Table {
    if old {
        baseline::deduplicate_lua_table(lua, input)
    } else {
        deduplicate_lua_table(lua, input)
    }
    .unwrap()
}

fn fingerprint(table: &Table) -> Vec<String> {
    table
        .sequence_values::<Value>()
        .map(|value| match value.unwrap() {
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
    assert_eq!(fingerprint(&new), fingerprint(&old));
    assert_eq!(fingerprint(&input), before);
    assert_eq!(new.raw_len(), old.raw_len());
    assert!(new.metatable().is_none());
    fingerprint(&new)
}

#[test]
fn lua_stream_pass_lazy_numbers_preserves_first_float_at_every_transition() {
    let lua = Lua::new();
    let a = 9_007_199_254_740_992_i64;
    for prefix in [0, 1, 31, 32, 33, 64, 512] {
        for first_float in [0.0, -0.0, a as f64, i64::MIN as f64, i64::MAX as f64, 0.5] {
            let mut values: Vec<_> = (0..prefix).map(Value::Integer).collect();
            values.extend([
                Value::Integer(a),
                Value::Integer(a + 1),
                Value::Integer(i64::MIN),
                Value::Integer(i64::MAX),
                Value::Number(first_float),
                Value::Integer(a + 2),
                Value::Number((a + 2) as f64),
                Value::Integer(-a - 1),
                Value::Number(-a as f64),
                Value::Number(first_float),
            ]);
            values.extend((0..128).map(|_| Value::Number(first_float)));
            compare(&lua, values);
        }
    }
}

#[test]
fn lua_stream_pass_lazy_numbers_preserves_rejected_float_then_new_integer_aliases() {
    let lua = Lua::new();
    let a = 9_007_199_254_740_992_i64;
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let mut values: Vec<_> = (0..128).map(Value::Integer).collect();
        // The first float is rejected. Later accepted integers must still
        // update the projection, and a rejected float must not reject an
        // unequal exact integer sharing its rounded representation.
        values.push(Value::Number(0.0));
        let triple = [
            Value::Integer(a),
            Value::Integer(a + 1),
            Value::Number(a as f64),
        ];
        values.extend(order.into_iter().map(|index| triple[index].clone()));
        let actual = compare(&lua, values);
        assert_eq!(actual.len(), 128 + if order[0] == 2 { 1 } else { 2 });
    }
}

#[test]
fn lua_stream_pass_lazy_numbers_matches_seeded_mixed_values_and_nil_boundary() {
    let lua = Lua::new();
    let function = lua.create_function(|_, ()| Ok(())).unwrap();
    let table = lua.create_table().unwrap();
    let pool = [
        Value::Integer(i64::MIN),
        Value::Integer(i64::MAX),
        Value::Integer(0),
        Value::Number(-0.0),
        Value::Number(f64::NAN),
        Value::Number(f64::INFINITY),
        Value::Number(f64::NEG_INFINITY),
        Value::Number(i64::MAX as f64),
        Value::Number(3.25),
        Value::Boolean(true),
        Value::Boolean(false),
        Value::String(lua.create_string([255, 0]).unwrap()),
        Value::String(lua.create_string("valid\0é").unwrap()),
        Value::Function(function),
        Value::Table(table),
    ];
    let mut seed = 0x95ce_271f_u32;
    for length in [8, 32, 127, 128, 256, 1024] {
        for _ in 0..16 {
            let mut values: Vec<_> = (0..64).map(Value::Integer).collect();
            for _ in 0..length {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                values.push(pool[seed as usize % pool.len()].clone());
            }
            values.extend([Value::Nil, Value::Integer(9999)]);
            compare(&lua, values);
        }
    }
}

#[test]
fn lua_stream_pass_lazy_numbers_uses_one_allocation_for_integer_index() {
    let values: Vec<_> = (0..512).map(Value::Integer).collect();
    drop(DeduplicateIndex::from_prefix(&values));
    crate::perf::assert_churn_budget(1, 10_000, || {
        drop(black_box(DeduplicateIndex::from_prefix(black_box(&values))))
    });
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_stream_pass_bench_lazy_numbers() {
    for (kind, count, unique) in [
        ("integer", 0, 1),
        ("integer", 8, 8),
        ("integer", 32, 32),
        ("integer", 127, 127),
        ("integer", 128, 128),
        ("integer", 256, 256),
        ("integer", 1024, 1024),
        ("integer", 4096, 4096),
        ("integer", 1024, 8),
        ("integer", 1024, 32),
        ("integer", 1024, 33),
        ("integer", 1024, 64),
        ("number", 1024, 1024),
        ("mixed", 1024, 1024),
        ("late_float", 1024, 1024),
        ("rejected_float", 1024, 1024),
        ("string", 128, 128),
        ("table", 512, 512),
    ] {
        let lua = Lua::new();
        let pool: Vec<_> = (0..unique)
            .map(|i| match kind {
                "string" => Value::String(lua.create_string(format!("item_{i}")).unwrap()),
                "table" => Value::Table(lua.create_table().unwrap()),
                "number" => Value::Number(i as f64 + 0.5),
                "mixed" if i % 2 != 0 => Value::Number(i as f64 + 0.5),
                "late_float" if i + 1 == unique => Value::Number(i as f64 + 0.5),
                "rejected_float" if i + 1 == unique => Value::Number(0.0),
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
                    "lazy_numbers_{kind}_{count}_{unique}/{}",
                    if old { "old" } else { "new" }
                ),
                32,
                count.max(1),
                || {
                    drop(black_box(deduplicate(
                        black_box(&lua),
                        black_box(&input),
                        black_box(old),
                    )))
                },
            );
        }
    }
}

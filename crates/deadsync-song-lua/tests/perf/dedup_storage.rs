use super::*;
use std::hint::black_box;

#[path = "dedup_storage_baseline.rs"]
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
fn lua_memory_dedup_storage_preserves_late_type_transitions_and_numeric_aliases() {
    let lua = Lua::new();
    let a = 9_007_199_254_740_992_i64;
    for kind in ["integer", "number", "string", "table"] {
        for order in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            let mut values: Vec<_> = (0..40)
                .map(|i| match kind {
                    "integer" => Value::Integer(i),
                    "number" => Value::Number(i as f64 + 0.5),
                    "string" => Value::String(lua.create_string(format!("prefix_{i}")).unwrap()),
                    _ => Value::Table(lua.create_table().unwrap()),
                })
                .collect();
            let triple = [
                Value::Integer(a),
                Value::Integer(a + 1),
                Value::Number(a as f64),
            ];
            values.extend(order.into_iter().map(|i| triple[i].clone()));
            for _ in 0..32 {
                values.extend([
                    Value::Boolean(false),
                    Value::Boolean(true),
                    Value::Integer(i64::MIN),
                    Value::Number(i64::MIN as f64),
                    Value::Number(-0.0),
                    Value::Integer(0),
                    Value::Number(f64::NAN),
                    Value::Number(f64::INFINITY),
                    Value::String(lua.create_string([255, 0]).unwrap()),
                ]);
            }
            compare(&lua, values);
        }
    }
}

#[test]
fn lua_memory_dedup_storage_preserves_transition_boundaries_and_repeated_inputs() {
    let lua = Lua::new();
    for count in [0, 8, 32, 33, 127, 128, 129, 1024] {
        for unique in [8, 32, 33, 64, 128] {
            assert_eq!(
                compare(
                    &lua,
                    (0..count)
                        .map(|i| Value::Integer((i % unique) as i64))
                        .collect()
                )
                .len(),
                count.min(unique)
            );
        }
    }
}

#[test]
fn lua_memory_dedup_storage_retains_strings_tables_and_opaque_values_after_gc() {
    struct Marker;
    impl mlua::UserData for Marker {}
    let lua = Lua::new();
    let userdata = lua.create_userdata(Marker).unwrap();
    let thread = lua
        .create_thread(lua.create_function(|_, ()| Ok(())).unwrap())
        .unwrap();
    let table = lua.create_table().unwrap();
    let mut values: Vec<_> = (0..64)
        .map(|i| {
            Value::String(
                lua.create_string(format!("{i}_{}", "é中🦀".repeat(64)))
                    .unwrap(),
            )
        })
        .collect();
    for _ in 0..32 {
        values.extend([
            Value::Table(table.clone()),
            Value::UserData(userdata.clone()),
            Value::Thread(thread.clone()),
        ]);
    }
    let input = lua.create_sequence_from(values).unwrap();
    let old = deduplicate(&lua, &input, true);
    let new = deduplicate(&lua, &input, false);
    let expected = fingerprint(&old);
    assert_eq!(new.raw_len(), 129);
    drop(input);
    drop(old);
    drop(table);
    drop(userdata);
    drop(thread);
    lua.gc_collect().unwrap();
    assert_eq!(fingerprint(&new), expected);
}

#[test]
fn lua_memory_dedup_storage_bounds_compact_integer_index_allocations() {
    let values: Vec<_> = (0..512).map(Value::Integer).collect();
    drop(DeduplicateIndex::from_prefix(&values));
    // Two compact pre-sized sets; no tagged-key storage or capacity growth.
    crate::perf::assert_churn_budget(2, 20_000, || {
        drop(black_box(DeduplicateIndex::from_prefix(&values)))
    });
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_memory_bench_dedup_storage() {
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
        ("integer", 1024, 33),
        ("integer", 1024, 64),
        ("number", 256, 256),
        ("number", 1024, 1024),
        ("mixed", 256, 256),
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
                "number" => Value::Number(i as f64 + 0.5),
                "mixed" => match i % 4 {
                    0 => Value::Integer(i as i64),
                    1 => Value::Number(i as f64 + 0.5),
                    2 => Value::String(lua.create_string(format!("item_{i}")).unwrap()),
                    _ => Value::Table(lua.create_table().unwrap()),
                },
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
                    "dedup_storage_{kind}_{count}_{unique}/{}",
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

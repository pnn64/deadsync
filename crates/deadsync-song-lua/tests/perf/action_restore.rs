use super::*;
use std::hint::black_box;

#[path = "action_restore_baseline.rs"]
mod baseline;

fn restore(snapshots: Vec<FunctionActionTableSnapshot>, old: bool) -> mlua::Result<()> {
    if old {
        baseline::restore_function_action_tables(snapshots)
    } else {
        restore_function_action_tables(snapshots)
    }
}

fn fingerprint(value: Value) -> String {
    match value {
        Value::Table(table) => {
            let mut entries: Vec<_> = table
                .pairs::<Value, Value>()
                .map(|pair| {
                    let (key, value) = pair.unwrap();
                    (fingerprint(key), fingerprint(value))
                })
                .collect();
            entries.sort();
            format!("{entries:?}")
        }
        Value::String(text) => format!("s:{:?}", text.as_bytes().as_ref()),
        Value::Number(number) => format!("n:{:x}", number.to_bits()),
        value => format!("{value:?}"),
    }
}

#[test]
fn lua_read_restore_preserves_mixed_keys_values_aliases_and_metatables() {
    let lua = Lua::new();
    let table = lua.create_table().unwrap();
    let nested = lua.create_table().unwrap();
    nested.set("nested", 17).unwrap();
    let function = lua
        .load("return function() return 42 end")
        .eval::<Function>()
        .unwrap();
    let invalid = lua.create_string([0xff, 0, 0xfe]).unwrap();
    for (key, value) in [
        (Value::Integer(-1), Value::Number(-0.0)),
        (Value::Integer(1), Value::Number(f64::NAN)),
        (Value::Number(1.5), Value::Number(f64::INFINITY)),
        (Value::Boolean(true), Value::Table(nested.clone())),
        (Value::String(invalid.clone()), Value::String(invalid)),
        (
            Value::Table(nested.clone()),
            Value::Function(function.clone()),
        ),
        (Value::Function(function), Value::Boolean(false)),
    ] {
        table.raw_set(key, value).unwrap();
    }
    let mt=lua.load("return {__mode='kv',__pairs=function() error('pairs invoked') end,__newindex=function() error('newindex invoked') end}").eval::<Table>().unwrap();
    table.set_metatable(Some(mt.clone())).unwrap();
    let alias = table.clone();
    let expected = fingerprint(Value::Table(table.clone()));
    lua.gc_stop();
    for old in [true, false] {
        let snapshots = vec![snapshot_function_action_table(table.clone()).unwrap()];
        table.raw_set(1, "changed").unwrap();
        table
            .raw_set("temporary", lua.create_table().unwrap())
            .unwrap();
        restore(snapshots, old).unwrap();
        assert_eq!(table.to_pointer(), alias.to_pointer());
        assert_eq!(fingerprint(Value::Table(alias.clone())), expected);
        assert_eq!(table.metatable().unwrap().to_pointer(), mt.to_pointer());
        assert_eq!(
            table.raw_get::<Table>(true).unwrap().to_pointer(),
            nested.to_pointer()
        );
        assert!(matches!(
            table.raw_get::<Value>("temporary").unwrap(),
            Value::Nil
        ));
    }
}

#[test]
fn lua_read_restore_matches_empty_dense_sparse_and_multiple_tables() {
    let lua = Lua::new();
    for count in [0, 1, 16, 128] {
        let tables: Vec<_> = (0..3)
            .map(|slot| {
                let table = lua.create_table().unwrap();
                for index in 0..count {
                    table.raw_set(index * 3 + 1, index + slot).unwrap();
                    table.raw_set(format!("field{index}"), index).unwrap();
                }
                table
            })
            .collect();
        let expected: Vec<_> = tables
            .iter()
            .map(|t| fingerprint(Value::Table(t.clone())))
            .collect();
        for old in [true, false] {
            let snapshots = tables
                .iter()
                .cloned()
                .map(snapshot_function_action_table)
                .collect::<mlua::Result<Vec<_>>>()
                .unwrap();
            for table in &tables {
                table.raw_set("temporary", true).unwrap();
                table.raw_set(10_000, 12).unwrap();
            }
            restore(snapshots, old).unwrap();
            assert_eq!(
                tables
                    .iter()
                    .map(|t| fingerprint(Value::Table(t.clone())))
                    .collect::<Vec<_>>(),
                expected
            );
        }
    }
}

#[test]
fn lua_read_restore_preserves_order_for_repeated_table_snapshots() {
    for old in [true, false] {
        let lua = Lua::new();
        let table = lua.create_table().unwrap();
        let other = lua.create_table().unwrap();
        table.raw_set(1, "first").unwrap();
        other.raw_set(1, "other").unwrap();
        let first = snapshot_function_action_table(table.clone()).unwrap();
        let middle = snapshot_function_action_table(other.clone()).unwrap();
        table.raw_set(1, "last").unwrap();
        table.raw_set(2, 42).unwrap();
        let last = snapshot_function_action_table(table.clone()).unwrap();
        table.raw_set(3, "temporary").unwrap();
        other.raw_set(2, "temporary").unwrap();
        restore(vec![first, middle, last], old).unwrap();
        assert_eq!(table.raw_get::<String>(1).unwrap(), "last");
        assert_eq!(table.raw_get::<i32>(2).unwrap(), 42);
        assert!(matches!(table.raw_get::<Value>(3).unwrap(), Value::Nil));
        assert_eq!(other.raw_get::<String>(1).unwrap(), "other");
        assert!(matches!(other.raw_get::<Value>(2).unwrap(), Value::Nil));
    }
}

#[test]
fn lua_read_restore_roundtrips_function_environment_and_globals() {
    for old in [true, false] {
        let lua = Lua::new();
        let environment = lua.create_table().unwrap();
        environment.set("original", 7).unwrap();
        let function = lua
            .load("return function() original=99; added=42 end")
            .set_environment(environment.clone())
            .eval::<Function>()
            .unwrap();
        lua.globals().set("sentinel", 12).unwrap();
        let snapshots = snapshot_function_action_tables(&lua, &function).unwrap();
        assert_eq!(snapshots.len(), 2);
        function.call::<()>(()).unwrap();
        lua.globals().set("sentinel", 55).unwrap();
        lua.globals().set("new_global", true).unwrap();
        restore(snapshots, old).unwrap();
        assert_eq!(environment.get::<i32>("original").unwrap(), 7);
        assert!(matches!(
            environment.get::<Value>("added").unwrap(),
            Value::Nil
        ));
        assert_eq!(lua.globals().get::<i32>("sentinel").unwrap(), 12);
        assert!(matches!(
            lua.globals().get::<Value>("new_global").unwrap(),
            Value::Nil
        ));
        assert_eq!(
            function.environment().unwrap().to_pointer(),
            environment.to_pointer()
        );
    }
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_read_bench_restore() {
    for count in [0, 8, 64, 512] {
        for string_keys in [false, true] {
            let lua = Lua::new();
            let table = lua.create_table().unwrap();
            for index in 0..count {
                if string_keys {
                    table.raw_set(format!("field{index}"), index).unwrap();
                } else {
                    table.raw_set(index + 1, index).unwrap();
                }
            }
            let expected = fingerprint(Value::Table(table.clone()));
            let roundtrip = |old| {
                let snapshot = snapshot_function_action_table(table.clone()).unwrap();
                table.raw_set("temporary", 42).unwrap();
                restore(vec![snapshot], old).unwrap();
            };
            for old in [true, false] {
                roundtrip(old);
                assert_eq!(fingerprint(Value::Table(table.clone())), expected);
            }
            lua.gc_stop();
            for old in order() {
                crate::perf::measure_sampled(
                    &format!(
                        "restore_{count}_strings_{string_keys}/{}",
                        if old { "old" } else { "new" }
                    ),
                    64,
                    count.max(1),
                    || {
                        roundtrip(black_box(old));
                        black_box(&table);
                    },
                );
            }
        }
    }
}

fn order() -> [bool; 2] {
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    }
}

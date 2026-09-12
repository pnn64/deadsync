use super::*;
use std::hint::black_box;

#[path = "action_snapshot_baseline.rs"]
mod baseline;

fn snapshot(
    lua: &Lua,
    function: &Function,
    old: bool,
) -> mlua::Result<Vec<FunctionActionTableSnapshot>> {
    if old {
        baseline::snapshot_function_action_tables(lua, function)
    } else {
        snapshot_function_action_tables(lua, function)
    }
}

fn ordered_entries(snapshot: &FunctionActionTableSnapshot) -> Vec<(String, String)> {
    fn value(value: &Value) -> String {
        match value {
            Value::String(text) => format!("s:{:?}", text.as_bytes().as_ref()),
            Value::Number(number) => format!("n:{:x}", number.to_bits()),
            value => format!("{value:?}"),
        }
    }
    snapshot
        .entries
        .iter()
        .map(|(k, v)| (value(k), value(v)))
        .collect()
}

fn entries(snapshot: &FunctionActionTableSnapshot) -> Vec<(String, String)> {
    let mut out = ordered_entries(snapshot);
    out.sort();
    out
}

fn fixture(lua: &Lua, kind: &str, count: usize) -> Function {
    let function = lua
        .load("return function() return sentinel end")
        .eval::<Function>()
        .unwrap();
    let globals = lua.create_table().unwrap();
    for i in 0..count {
        globals.raw_set(format!("global_{i}"), i).unwrap();
    }
    lua.set_globals(globals.clone()).unwrap();
    match kind {
        "native" => lua.create_function(|_, ()| Ok(())).unwrap(),
        "globals" => {
            function.set_environment(globals).unwrap();
            function
        }
        "environment" | "proxy" | "alias" => {
            let environment = lua.create_table().unwrap();
            if kind == "alias" {
                environment
                    .raw_set("__songlua_env_target", globals)
                    .unwrap();
            } else if kind == "proxy" {
                let target = lua.create_table().unwrap();
                for i in 0..count {
                    target.raw_set(format!("target_{i}"), i + 7).unwrap();
                }
                environment.raw_set("__songlua_env_target", target).unwrap();
            } else {
                for i in 0..count {
                    environment.raw_set(format!("env_{i}"), i + 7).unwrap();
                }
            }
            function.set_environment(environment).unwrap();
            function
        }
        _ => unreachable!(),
    }
}

#[test]
fn lua_capture_action_snapshot_preserves_targets_entries_and_rollback() {
    for kind in ["native", "globals", "environment", "proxy", "alias"] {
        for count in [0, 1, 64] {
            let lua = Lua::new();
            let function = fixture(&lua, kind, count);
            let globals = lua.globals();
            let nested = lua.create_table().unwrap();
            globals
                .raw_set(lua.create_string([0xff, 0]).unwrap(), nested.clone())
                .unwrap();
            globals.raw_set(true, Value::Number(f64::NAN)).unwrap();
            globals.raw_set(1, Value::Number(-0.0)).unwrap();
            globals.raw_set(nested.clone(), function.clone()).unwrap();
            let mt = lua.create_table().unwrap();
            mt.raw_set(
                "__newindex",
                lua.create_function(|_, (): ()| -> mlua::Result<()> {
                    Err(mlua::Error::runtime("unexpected write"))
                })
                .unwrap(),
            )
            .unwrap();
            mt.raw_set(
                "__pairs",
                lua.create_function(|_, (): ()| -> mlua::Result<()> {
                    Err(mlua::Error::runtime("unexpected pairs"))
                })
                .unwrap(),
            )
            .unwrap();
            globals.set_metatable(Some(mt.clone())).unwrap();
            let old = snapshot(&lua, &function, true).unwrap();
            let new = snapshot(&lua, &function, false).unwrap();
            let expected_count = if kind == "environment" || kind == "proxy" {
                2
            } else {
                1
            };
            assert_eq!(old.len(), expected_count);
            assert_eq!(new.len(), expected_count);
            for (old, new) in old.iter().zip(&new) {
                assert_eq!(old.table.to_pointer(), new.table.to_pointer());
                assert_eq!(ordered_entries(old), ordered_entries(new));
                assert_eq!(entries(old), entries(new));
            }
            let expected: Vec<_> = old.iter().map(entries).collect();
            for snapshots in [old, new] {
                for snapshot in &snapshots {
                    snapshot.table.raw_set("temporary", 99).unwrap();
                }
                restore_function_action_tables(snapshots).unwrap();
                let restored = snapshot(&lua, &function, false).unwrap();
                assert_eq!(restored.iter().map(entries).collect::<Vec<_>>(), expected);
                assert_eq!(globals.metatable().unwrap().to_pointer(), mt.to_pointer());
                assert_eq!(
                    globals
                        .raw_get::<Table>(lua.create_string([0xff, 0]).unwrap())
                        .unwrap()
                        .to_pointer(),
                    nested.to_pointer()
                );
            }
        }
    }
}

#[test]
fn lua_capture_action_snapshot_keeps_raw_target_lookup_and_conversion_errors() {
    for invalid in [false, true] {
        let lua = Lua::new();
        let function = fixture(&lua, "environment", 3);
        let environment = function.environment().unwrap();
        let mt = lua.create_table().unwrap();
        mt.raw_set(
            "__index",
            lua.create_function(|_, (): ()| -> mlua::Result<()> {
                Err(mlua::Error::runtime("unexpected index"))
            })
            .unwrap(),
        )
        .unwrap();
        environment.set_metatable(Some(mt)).unwrap();
        if invalid {
            environment.raw_set("__songlua_env_target", 42).unwrap();
        }
        let old = snapshot(&lua, &function, true);
        let new = snapshot(&lua, &function, false);
        assert_eq!(old.is_err(), invalid);
        assert_eq!(new.is_err(), invalid);
        if invalid {
            assert_eq!(
                old.err().unwrap().to_string(),
                new.err().unwrap().to_string()
            );
        } else {
            assert_eq!(
                old.unwrap().iter().map(entries).collect::<Vec<_>>(),
                new.unwrap().iter().map(entries).collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn lua_capture_action_snapshot_owns_shallow_values_and_exact_outer_capacity() {
    for kind in ["native", "environment"] {
        let lua = Lua::new();
        let function = fixture(&lua, kind, 0);
        let nested = lua.create_table().unwrap();
        lua.globals().raw_set("nested", nested.clone()).unwrap();
        let snapshots = snapshot(&lua, &function, false).unwrap();
        assert_eq!(snapshots.capacity(), snapshots.len());
        nested.raw_set("mutated", 7).unwrap();
        lua.globals().raw_remove("nested").unwrap();
        restore_function_action_tables(snapshots).unwrap();
        assert_eq!(
            lua.globals()
                .raw_get::<Table>("nested")
                .unwrap()
                .raw_get::<i32>("mutated")
                .unwrap(),
            7
        );
    }
    let lua = Lua::new();
    let function = fixture(&lua, "native", 0);
    snapshot(&lua, &function, false).unwrap();
    crate::perf::assert_churn_budget(
        1,
        std::mem::size_of::<FunctionActionTableSnapshot>(),
        || {
            drop(snapshot(&lua, &function, false).unwrap());
        },
    );
    for index in 0..4 {
        lua.globals()
            .raw_set(format!("key_{index}"), "value")
            .unwrap();
    }
    snapshot(&lua, &function, false).unwrap();
    // Only the outer snapshot and entry buffers allocate; no per-key cursor
    // handles are needed even though these are reference-valued keys/values.
    crate::perf::assert_churn_budget(
        2,
        std::mem::size_of::<FunctionActionTableSnapshot>()
            + 4 * std::mem::size_of::<(Value, Value)>(),
        || drop(snapshot(&lua, &function, false).unwrap()),
    );
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_capture_bench_action_snapshot() {
    for kind in ["native", "globals", "environment", "proxy", "alias"] {
        for count in [0, 16, 256] {
            let lua = Lua::new();
            let function = fixture(&lua, kind, count);
            let old = snapshot(&lua, &function, true).unwrap();
            let new = snapshot(&lua, &function, false).unwrap();
            assert_eq!(
                old.iter().map(entries).collect::<Vec<_>>(),
                new.iter().map(entries).collect::<Vec<_>>()
            );
            let units = old.iter().map(|s| s.entries.len()).sum::<usize>().max(1);
            drop((old, new));
            lua.gc_stop();
            let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
                [false, true]
            } else {
                [true, false]
            };
            for old in order {
                crate::perf::measure_sampled(
                    &format!(
                        "snapshot_{kind}_{count}/{}",
                        if old { "old" } else { "new" }
                    ),
                    128,
                    units,
                    || {
                        drop(black_box(
                            snapshot(&lua, &function, black_box(old)).unwrap(),
                        ))
                    },
                );
            }
        }
    }
}

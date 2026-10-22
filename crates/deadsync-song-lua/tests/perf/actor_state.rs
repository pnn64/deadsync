use super::*;

#[path = "actor_state_before.rs"]
mod before;

fn compare_value(left: Value, right: Value) {
    if let (Value::Table(left), Value::Table(right)) = (&left, &right) {
        assert_eq!(left.raw_len(), right.raw_len());
        let mut count = 0;
        left.for_each::<Value, Value>(|key, value| {
            compare_value(value, right.raw_get(key)?);
            count += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(count, right.pairs::<Value, Value>().count());
    } else {
        assert_eq!(left, right);
    }
}

fn compare_states(left: Vec<(String, Value)>, right: Vec<(String, Value)>) {
    assert_eq!(left.len(), right.len());
    for ((left_key, left_value), (right_key, right_value)) in left.into_iter().zip(right) {
        assert_eq!(left_key, right_key);
        if left_key == "__songlua_wrappers" {
            let (Value::Table(left), Value::Table(right)) = (left_value, right_value) else {
                panic!()
            };
            assert_ne!(left, right);
            assert_eq!(
                left.raw_get::<Table>(1).unwrap(),
                right.raw_get::<Table>(1).unwrap()
            );
        } else {
            compare_value(left_value, right_value);
        }
    }
}

fn prepared_actor(lua: &Lua) -> Table {
    let actor = lua
        .load(
            r#"
        local wrapper = {}
        wrapper.owner = wrapper
        return {__songlua_actor_type='Quad', Text=string.char(255,0),
            __songlua_state_x=12, __songlua_capture_current={1,2},
            __songlua_state_extra={a={42}, b=false},
            __songlua_wrappers={wrapper},
            [false]=true, [{}]=function() end,
            Name='kept outside native state', OnCommand=function() end}
    "#,
        )
        .eval::<Table>()
        .unwrap();
    let function = lua
        .load("return function() end")
        .eval::<Function>()
        .unwrap();
    for index in 1..=128 {
        actor.raw_set(format!("method{index}"), &function).unwrap();
    }
    for index in 1..=8 {
        actor
            .raw_set(format!("Event{index}MessageCommand"), &function)
            .unwrap();
    }
    actor
}

#[test]
fn actor_snapshots_preserve_order_wrappers_deep_values_and_live_destination() {
    for live in [false, true] {
        let lua = Lua::new();
        let actor = prepared_actor(&lua);
        if live {
            begin_live_runtime(&lua).unwrap();
            begin_live_actor_playback(&lua, std::slice::from_ref(&actor)).unwrap();
            capture_block_set_f32(&lua, &actor, "x", 87.0).unwrap();
        }
        for keep in [
            is_actor_mutable_state_key as fn(&str) -> bool,
            is_actor_semantic_state_key,
        ] {
            compare_states(
                before::snapshot_actor_state(&lua, &actor, keep).unwrap(),
                snapshot_actor_state(&lua, &actor, keep).unwrap(),
            );
        }
        let old = before::snapshot_actor_semantic_state_table(&lua, &actor).unwrap();
        let new = snapshot_actor_semantic_state_table(&lua, &actor).unwrap();
        compare_states(
            read_actor_semantic_state_table(&old).unwrap(),
            read_actor_semantic_state_table(&new).unwrap(),
        );
        assert_eq!(old.raw_len(), new.raw_len());
        for index in 1..=new.raw_len() {
            let entry = new.raw_get::<Table>(index).unwrap();
            assert_eq!(entry.raw_len(), 2);
            assert!(matches!(
                entry.raw_get::<Value>(1).unwrap(),
                Value::String(_)
            ));
        }
    }
    let source = Lua::new();
    let destination = Lua::new();
    let actor = source
        .load("{__songlua_state_x=12, Text='copied'}")
        .eval::<Table>()
        .unwrap();
    compare_states(
        before::snapshot_actor_state(&destination, &actor, is_actor_mutable_state_key).unwrap(),
        snapshot_actor_mutable_state(&destination, &actor).unwrap(),
    );
    let excluded = source
        .load("{Name='excluded', OnCommand=function() end}")
        .eval::<Table>()
        .unwrap();
    assert_eq!(
        before::snapshot_actor_semantic_state_table(&destination, &excluded)
            .unwrap()
            .raw_len(),
        0
    );
    assert_eq!(
        snapshot_actor_semantic_state_table(&destination, &excluded)
            .unwrap()
            .raw_len(),
        0
    );
}

#[test]
fn snapshot_key_reads_validate_excluded_binary_keys_and_keep_the_vm_stack() {
    let lua = Lua::new();
    for bytes in [
        vec![],
        b"Text".to_vec(),
        b"Text\0".to_vec(),
        b"__songlua_state_x".to_vec(),
        "longλ".repeat(100).into_bytes(),
        vec![0xff],
        vec![b'a', 0, 0xff],
    ] {
        let key = lua.create_string(&bytes).unwrap();
        for keep in [
            is_actor_mutable_state_key as fn(&str) -> bool,
            is_actor_semantic_state_key,
        ] {
            let expected = key
                .to_str()
                .map(|text| keep(&text).then(|| text.to_string()))
                .map_err(|e| e.to_string());
            lua.exec_raw_lua(|raw| unsafe {
                assert_ne!(ffi::lua_checkstack(raw.state(), 4), 0);
                raw.push(71).unwrap();
                let top = ffi::lua_gettop(raw.state());
                assert_eq!(
                    retained_key_text(&lua, &key, keep).map_err(|e| e.to_string()),
                    expected
                );
                assert_eq!(ffi::lua_gettop(raw.state()), top);
                assert_eq!(raw.pop::<i32>().unwrap(), 71);
            });
        }
    }
    let actor = prepared_actor(&lua);
    actor
        .raw_set(
            lua.create_string([0xff]).unwrap(),
            lua.create_table().unwrap(),
        )
        .unwrap();
    assert_eq!(
        before::snapshot_actor_state(&lua, &actor, is_actor_mutable_state_key)
            .unwrap_err()
            .to_string(),
        snapshot_actor_mutable_state(&lua, &actor)
            .unwrap_err()
            .to_string()
    );
    assert_eq!(
        before::snapshot_actor_semantic_state_table(&lua, &actor)
            .unwrap_err()
            .to_string(),
        snapshot_actor_semantic_state_table(&lua, &actor)
            .unwrap_err()
            .to_string()
    );
}

#[test]
fn scalar_restore_preserves_validation_before_writes_and_authored_setters() {
    for invalid in [false, true] {
        let mut outcomes = Vec::new();
        for current in [false, true] {
            let lua = Lua::new();
            lua.load(
                r#"
                saved=11; binary='a\000b'; removed=false; stable={}
                writes={}
                setmetatable(_G, {__newindex=function(t,k,v)
                    writes[#writes+1]=k; rawset(t,k,v)
                end})
            "#,
            )
            .exec()
            .unwrap();
            let snapshot = snapshot_scalar_globals(&lua).unwrap();
            lua.load("saved=99; binary='changed'; removed=nil; added=13; writes={}")
                .exec()
                .unwrap();
            if invalid {
                lua.globals()
                    .raw_set(
                        lua.create_string([0xff]).unwrap(),
                        lua.create_table().unwrap(),
                    )
                    .unwrap();
            }
            let result = if current {
                restore_scalar_globals(&lua, snapshot)
            } else {
                before::restore_scalar_globals(&lua, snapshot)
            };
            let globals = lua.globals();
            outcomes.push((
                result.map_err(|e| e.to_string()),
                globals.raw_get::<i64>("saved").unwrap(),
                globals.raw_get::<String>("binary").unwrap(),
                globals.raw_get::<Option<bool>>("removed").unwrap(),
                globals.raw_get::<Option<i64>>("added").unwrap(),
                globals
                    .raw_get::<Table>("writes")
                    .unwrap()
                    .sequence_values::<String>()
                    .collect::<mlua::Result<Vec<_>>>()
                    .unwrap(),
            ));
        }
        assert_eq!(outcomes[0], outcomes[1]);
        if invalid {
            assert!(outcomes[0].0.is_err());
            assert_eq!(outcomes[0].1, 99);
            assert_eq!(outcomes[0].5.len(), 0);
        } else {
            assert!(outcomes[0].0.is_ok());
            assert_eq!(outcomes[0].1, 11);
            assert_eq!(outcomes[0].3, Some(false));
            assert_eq!(outcomes[0].4, None);
        }
    }
}

#[test]
fn message_command_scanning_preserves_order_functions_invalid_keys_and_raw_lookup() {
    let lua = Lua::new();
    let actor = prepared_actor(&lua);
    let function = lua
        .load("return function() end")
        .eval::<Function>()
        .unwrap();
    for key in [
        "MessageCommand",
        "unicodeλMessageCommand",
        "embedded\0MessageCommand",
        "trailingMessageCommand\0",
    ] {
        actor.raw_set(key, &function).unwrap();
    }
    actor
        .raw_set(format!("{}MessageCommand", "longλ".repeat(256)), &function)
        .unwrap();
    actor
        .raw_set(lua.create_string([0xff]).unwrap(), &function)
        .unwrap();
    actor
        .raw_set(lua.create_string([0xfe]).unwrap(), 1)
        .unwrap();
    actor
        .raw_set(lua.create_table().unwrap(), &function)
        .unwrap();
    actor
        .raw_set("NotAFunctionMessageCommand", lua.create_table().unwrap())
        .unwrap();
    let mt = lua.create_table().unwrap();
    mt.raw_set(
        "__index",
        lua.create_function(|_, _: Value| -> mlua::Result<Value> {
            Err(mlua::Error::runtime(
                "message scanning must use raw traversal",
            ))
        })
        .unwrap(),
    )
    .unwrap();
    actor.set_metatable(Some(mt)).unwrap();
    let mut expected = Vec::new();
    before::for_each_actor_message_command(&actor, |name, function| {
        expected.push((name.to_owned(), function))
    })
    .unwrap();
    let mut actual = Vec::new();
    for_each_actor_message_command(&actor, |name, function| actual.push((name, function))).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 12);
    assert!(actual.iter().any(|(name, _)| name == "MessageCommand"));
    assert!(
        actual
            .iter()
            .any(|(name, _)| name == "embedded\0MessageCommand")
    );
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_actor_state_paths() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    let _affinity = pin_cpu2();
    let variants = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    };
    for workload in [
        "actor_vector_snapshot",
        "actor_table_snapshot",
        "restore_scalar_globals",
        "message_command_scan",
    ] {
        for current in variants {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let lua = Lua::new();
                let actor = prepared_actor(&lua);
                for index in 1..=128 {
                    lua.globals()
                        .raw_set(format!("global{index}"), lua.create_table().unwrap())
                        .unwrap();
                }
                let snapshots: Vec<_> = if workload == "restore_scalar_globals" {
                    (0..64)
                        .map(|_| snapshot_scalar_globals(&lua).unwrap())
                        .collect()
                } else {
                    Vec::new()
                };
                lua.gc_stop();
                let start = Instant::now();
                let begin = cycles();
                let (_, churn) = allocations::measure(|| {
                    if workload == "restore_scalar_globals" {
                        for snapshot in snapshots {
                            if current {
                                restore_scalar_globals(&lua, snapshot)
                            } else {
                                before::restore_scalar_globals(&lua, snapshot)
                            }
                            .unwrap();
                        }
                    } else {
                        for _ in 0..64 {
                            match workload {
                                "actor_vector_snapshot" => {
                                    black_box(
                                        if current {
                                            snapshot_actor_mutable_state(&lua, &actor)
                                        } else {
                                            before::snapshot_actor_state(
                                                &lua,
                                                &actor,
                                                is_actor_mutable_state_key,
                                            )
                                        }
                                        .unwrap(),
                                    );
                                }
                                "actor_table_snapshot" => {
                                    black_box(
                                        if current {
                                            snapshot_actor_semantic_state_table(&lua, &actor)
                                        } else {
                                            before::snapshot_actor_semantic_state_table(
                                                &lua, &actor,
                                            )
                                        }
                                        .unwrap(),
                                    );
                                }
                                _ => {
                                    let mut names = Vec::new();
                                    if current {
                                        for_each_actor_message_command(&actor, |name, _| {
                                            names.push(name)
                                        })
                                    } else {
                                        before::for_each_actor_message_command(&actor, |name, _| {
                                            names.push(name.to_owned())
                                        })
                                    }
                                    .unwrap();
                                    assert_eq!(names.len(), 8);
                                    black_box(names);
                                }
                            }
                        }
                    }
                });
                let used = cycles() - begin;
                let nanos = start.elapsed().as_nanos();
                if sample > 0 {
                    samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used,"units":64,
                    "allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
                }
            }
            println!(
                "DRAW_CAPTURE_METRIC {}",
                serde_json::json!({"workload":workload,"variant":if current {"after"} else {"before"},
                "unit":"operations","baseline_commit":"0908f3cff","samples":samples})
            );
        }
    }
}

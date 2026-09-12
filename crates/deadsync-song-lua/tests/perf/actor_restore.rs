use super::*;
use std::cell::RefCell;
use std::hint::black_box;
use std::rc::Rc;

#[path = "actor_restore_baseline.rs"]
mod baseline;

fn restore(
    actor: &Table,
    snapshot: Vec<(String, Value)>,
    semantic: bool,
    old: bool,
) -> mlua::Result<()> {
    let predicate = if semantic {
        is_actor_semantic_state_key
    } else {
        is_actor_mutable_state_key
    };
    if old {
        baseline::restore_actor_state(actor, snapshot, predicate)
    } else {
        restore_actor_state(actor, snapshot, predicate)
    }
}

fn fingerprint(table: &Table) -> Vec<(String, String)> {
    fn text(value: Value) -> String {
        match value {
            Value::String(s) => format!("bytes:{:?}", s.as_bytes().as_ref()),
            Value::Number(n) => format!("number:{:x}", n.to_bits()),
            other => format!("{other:?}"),
        }
    }
    let mut values: Vec<_> = table
        .pairs::<Value, Value>()
        .map(|pair| {
            let (k, v) = pair.unwrap();
            (text(k), text(v))
        })
        .collect();
    values.sort();
    values
}

#[test]
fn lua_memory_actor_restore_preserves_filters_values_and_spill_boundaries() {
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    let shared = lua.create_table().unwrap();
    let snapshot = vec![
        ("__songlua_state_x".to_string(), Value::Number(-0.0)),
        ("__songlua_state_nan".to_string(), Value::Number(f64::NAN)),
        (
            "__songlua_state_table".to_string(),
            Value::Table(shared.clone()),
        ),
        (
            "Text".to_string(),
            Value::String(lua.create_string([255, 0]).unwrap()),
        ),
        ("unfiltered_snapshot_key".to_string(), Value::Boolean(false)),
    ];
    for count in [0, 1, 8, 16, 17, 32, 65] {
        for semantic in [false, true] {
            let mut results = Vec::new();
            for old in [true, false] {
                actor.clear().unwrap();
                for i in 0..count {
                    actor
                        .raw_set(format!("__songlua_state_{i}_é\0"), i)
                        .unwrap();
                }
                actor.raw_set("__songlua_capture_mode", 7).unwrap();
                actor.raw_set("unrelated", &shared).unwrap();
                actor.raw_set(1, &shared).unwrap();
                actor.raw_set(&shared, 3).unwrap();
                restore(&actor, snapshot.clone(), semantic, old).unwrap();
                assert_eq!(
                    actor
                        .raw_get::<Value>("__songlua_capture_mode")
                        .unwrap()
                        .is_nil(),
                    !semantic
                );
                assert_eq!(
                    actor
                        .raw_get::<Table>("__songlua_state_table")
                        .unwrap()
                        .to_pointer(),
                    shared.to_pointer()
                );
                assert_eq!(
                    actor.raw_get::<f64>("__songlua_state_x").unwrap().to_bits(),
                    (-0.0_f64).to_bits()
                );
                results.push(fingerprint(&actor));
            }
            assert_eq!(results[0], results[1]);
        }
    }
}

#[test]
fn lua_memory_actor_restore_validates_ignored_string_keys_before_writes() {
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    actor.raw_set("__songlua_state_temporary", 12).unwrap();
    actor
        .raw_set(
            lua.create_string([255, 0]).unwrap(),
            lua.create_table().unwrap(),
        )
        .unwrap();
    let expected = fingerprint(&actor);
    let mut errors = Vec::new();
    for old in [true, false] {
        errors.push(
            restore(
                &actor,
                vec![("Text".to_string(), Value::Integer(9))],
                false,
                old,
            )
            .unwrap_err()
            .to_string(),
        );
        assert_eq!(fingerprint(&actor), expected);
    }
    assert_eq!(errors[0], errors[1]);
}

#[test]
fn lua_memory_actor_restore_preserves_metamethod_order_and_partial_failures() {
    for fail in [false, true] {
        let mut results = Vec::new();
        for old in [true, false] {
            let lua = Lua::new();
            let actor = lua.create_table().unwrap();
            actor.raw_set("__songlua_state_temporary", 9).unwrap();
            let trace = Rc::new(RefCell::new(Vec::new()));
            let captured = trace.clone();
            let mt = lua.create_table().unwrap();
            mt.raw_set(
                "__newindex",
                lua.create_function(move |_, (table, key, value): (Table, String, Value)| {
                    assert!(
                        table
                            .raw_get::<Value>("__songlua_state_temporary")?
                            .is_nil()
                    );
                    captured.borrow_mut().push(key.clone());
                    if fail && key == "second" {
                        return Err(mlua::Error::runtime("restore stopped"));
                    }
                    table.raw_set("__songlua_state_callback_added", 42)?;
                    table.raw_set(key, value)
                })
                .unwrap(),
            )
            .unwrap();
            for name in ["__pairs", "__index"] {
                mt.raw_set(
                    name,
                    lua.create_function(|_, _: MultiValue| -> mlua::Result<()> {
                        Err(mlua::Error::runtime("read trap"))
                    })
                    .unwrap(),
                )
                .unwrap();
            }
            actor.set_metatable(Some(mt)).unwrap();
            let snapshot = vec![
                ("first".into(), Value::Integer(1)),
                ("first".into(), Value::Integer(2)),
                ("second".into(), Value::Integer(3)),
                ("last".into(), Value::Integer(4)),
            ];
            let result = restore(&actor, snapshot, false, old);
            assert_eq!(result.is_err(), fail);
            assert_eq!(
                actor
                    .raw_get::<i64>("__songlua_state_callback_added")
                    .unwrap(),
                42
            );
            results.push((
                result.err().map(|e| e.to_string()),
                trace.borrow().clone(),
                fingerprint(&actor),
            ));
        }
        assert_eq!(results[0], results[1]);
    }
}

fn capture_actor(lua: &Lua, fields: usize) -> Table {
    let actor = lua.create_table().unwrap();
    install_actor_transform_methods(lua, &actor).unwrap();
    for i in 0..fields {
        actor
            .raw_set(format!("__songlua_state_custom_{i}"), i)
            .unwrap();
    }
    actor
        .raw_set(
            "TestCommand",
            lua.load("return function(self) self:x(12); self:y(34) end")
                .eval::<Function>()
                .unwrap(),
        )
        .unwrap();
    actor
}

fn capture(lua: &Lua, actor: &Table, old: bool) -> Result<Vec<SongLuaOverlayCommandBlock>, String> {
    if old {
        baseline::capture_actor_command_preserving_state(lua, actor, "TestCommand")
    } else {
        capture_actor_command_preserving_state(lua, actor, "TestCommand")
    }
}

#[test]
fn lua_memory_actor_restore_preserves_command_capture_and_restored_state() {
    fn deep(value: Value) -> String {
        match value {
            Value::Table(table) => {
                let mut entries: Vec<_> = table
                    .pairs::<Value, Value>()
                    .map(|pair| {
                        let (key, value) = pair.unwrap();
                        let (key, value) = (deep(key), deep(value));
                        format!("{}:{key}{}:{value}", key.len(), value.len())
                    })
                    .collect();
                entries.sort();
                format!("table:{}", entries.concat())
            }
            Value::String(s) => format!("bytes:{:?}", s.as_bytes().as_ref()),
            Value::Number(n) => format!("number:{:x}", n.to_bits()),
            other => format!("{other:?}"),
        }
    }
    let lua = Lua::new();
    let actor = capture_actor(&lua, 64);
    let old = capture(&lua, &actor, true).unwrap();
    let before = snapshot_actor_mutable_state(&lua, &actor).unwrap();
    let new = capture(&lua, &actor, false).unwrap();
    assert!(!new.is_empty());
    assert_eq!(old, new);
    let after = snapshot_actor_mutable_state(&lua, &actor).unwrap();
    let as_table = |values: Vec<(String, Value)>| {
        let out = lua.create_table().unwrap();
        for (key, value) in values {
            out.raw_set(key, value).unwrap();
        }
        out
    };
    assert_eq!(
        deep(Value::Table(as_table(before))),
        deep(Value::Table(as_table(after)))
    );
}

#[test]
fn lua_memory_actor_restore_ignored_non_string_keys_have_no_churn() {
    let lua = Lua::new();
    let actor = lua.create_table().unwrap();
    for i in 0..128 {
        actor.raw_set(lua.create_table().unwrap(), i).unwrap();
    }
    restore(&actor, Vec::new(), false, false).unwrap();
    crate::perf::assert_no_churn(|| restore(&actor, Vec::new(), false, false).unwrap());
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_memory_bench_actor_restore() {
    for (ignored, changed, saved, kind) in [
        (0, 0, 0, "string"),
        (0, 1, 0, "string"),
        (0, 8, 8, "string"),
        (0, 16, 16, "string"),
        (0, 17, 16, "string"),
        (256, 8, 8, "string"),
        (256, 64, 64, "string"),
        (0, 256, 64, "string"),
        (128, 0, 0, "table"),
    ] {
        let lua = Lua::new();
        let actor = lua.create_table().unwrap();
        for i in 0..ignored {
            if kind == "table" {
                actor.raw_set(lua.create_table().unwrap(), i).unwrap();
            } else {
                actor.raw_set(format!("ignored_{i}"), i).unwrap();
            }
        }
        let snapshot: Vec<_> = (0..saved)
            .map(|i| (format!("__songlua_state_{i}"), Value::Integer(i as i64)))
            .collect();
        for (key, value) in &snapshot {
            actor.raw_set(key.as_str(), value.clone()).unwrap();
        }
        let keys: Vec<_> = (0..changed)
            .map(|i| lua.create_string(format!("__songlua_state_{i}")).unwrap())
            .collect();
        let expected = fingerprint(&actor);
        let roundtrip = |old| {
            for key in &keys {
                actor.raw_set(key, 42).unwrap();
            }
            restore(&actor, snapshot.clone(), false, old).unwrap();
        };
        for old in [true, false] {
            roundtrip(old);
            assert_eq!(fingerprint(&actor), expected);
        }
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
                    "actor_restore_{ignored}_{changed}_{saved}_{kind}/{}",
                    if old { "old" } else { "new" }
                ),
                128,
                (ignored + changed + saved).max(1),
                || roundtrip(black_box(old)),
            );
        }
    }
    for fields in [0, 64] {
        let lua = Lua::new();
        let actor = capture_actor(&lua, fields);
        assert_eq!(
            capture(&lua, &actor, true).unwrap(),
            capture(&lua, &actor, false).unwrap()
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
                &format!("actor_capture_{fields}/{}", if old { "old" } else { "new" }),
                32,
                1,
                || drop(black_box(capture(&lua, &actor, black_box(old)).unwrap())),
            );
        }
    }
}

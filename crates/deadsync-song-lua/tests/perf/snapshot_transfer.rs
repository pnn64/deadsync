use super::*;
use std::hint::black_box;

#[path = "snapshot_transfer_baseline.rs"]
mod baseline;

fn snapshot(lua: &Lua, actor: &Table, old: bool) -> mlua::Result<Table> {
    if old {
        baseline::snapshot_actor_semantic_state_table(lua, actor)
    } else {
        snapshot_actor_semantic_state_table(lua, actor)
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

fn actor(lua: &Lua, ignored: usize, retained: usize, nested: bool) -> Table {
    let actor = lua.create_table().unwrap();
    for index in 0..ignored {
        actor.raw_set(format!("Method{index}"), index).unwrap();
    }
    for index in 0..retained {
        let key = format!("__songlua_state_value_{index}");
        if nested {
            let table = lua.create_table().unwrap();
            table.raw_set(1, index).unwrap();
            table.raw_set("text", "nested").unwrap();
            actor.raw_set(key, table).unwrap();
        } else {
            actor.raw_set(key, index).unwrap();
        }
    }
    actor
}

#[test]
fn lua_transfer_semantic_snapshots_match_order_values_and_filtering() {
    let lua = Lua::new();
    for ignored in [0, 64, 256] {
        for retained in [0, 1, 16, 64] {
            for nested in [false, true] {
                let actor = actor(&lua, ignored, retained, nested);
                for (key, value) in [
                    (
                        "Text",
                        Value::String(lua.create_string([0xff, 0, 0xfe]).unwrap()),
                    ),
                    ("__songlua_aux", Value::Number(-0.0)),
                    ("__songlua_state_α", Value::Number(f64::NAN)),
                    ("__songlua_state_inf", Value::Number(f64::INFINITY)),
                    ("__songlua_capture_cursor", Value::Number(1.5)),
                ] {
                    actor.raw_set(key, value).unwrap();
                }
                actor.raw_set(1, "numeric-key").unwrap();
                actor.raw_set(false, true).unwrap();
                let mt = lua.load("return {__pairs=function() error('must use raw iteration') end,__index=function() error('must use raw access') end}").eval::<Table>().unwrap();
                actor.set_metatable(Some(mt)).unwrap();
                let old = snapshot(&lua, &actor, true).unwrap();
                let new = snapshot(&lua, &actor, false).unwrap();
                assert_eq!(
                    fingerprint(Value::Table(old.clone())),
                    fingerprint(Value::Table(new.clone()))
                );
                assert_eq!(old.raw_len(), retained + 4);
                let old = read_actor_semantic_state_table(&old).unwrap();
                let new = read_actor_semantic_state_table(&new).unwrap();
                assert_eq!(
                    old.iter().map(|(key, _)| key).collect::<Vec<_>>(),
                    new.iter().map(|(key, _)| key).collect::<Vec<_>>()
                );
                assert!(
                    new.iter()
                        .all(|(key, _)| !key.starts_with("__songlua_capture_"))
                );
            }
        }
    }
}

#[test]
fn lua_transfer_semantic_snapshots_deep_copy_and_restore_without_changing_aliases() {
    for old in [true, false] {
        let lua = Lua::new();
        let actor = actor(&lua, 2, 2, true);
        let original: Table = actor.raw_get("__songlua_state_value_0").unwrap();
        let snapshot = snapshot(&lua, &actor, old).unwrap();
        original.raw_set("text", "modified").unwrap();
        actor.raw_set("__songlua_state_added", 123).unwrap();
        actor.raw_set("__songlua_capture_cursor", 9.0).unwrap();
        actor.raw_set("Method0", "changed").unwrap();
        let entries = read_actor_semantic_state_table(&snapshot).unwrap();
        restore_actor_semantic_state(&actor, entries).unwrap();
        let restored: Table = actor.raw_get("__songlua_state_value_0").unwrap();
        assert_ne!(restored.to_pointer(), original.to_pointer());
        assert_eq!(restored.raw_get::<String>("text").unwrap(), "nested");
        assert_eq!(original.raw_get::<String>("text").unwrap(), "modified");
        assert!(matches!(
            actor.raw_get::<Value>("__songlua_state_added").unwrap(),
            Value::Nil
        ));
        assert_eq!(actor.raw_get::<String>("Method0").unwrap(), "changed");
        assert_eq!(
            actor.raw_get::<f32>("__songlua_capture_cursor").unwrap(),
            9.0
        );
    }
}

#[test]
fn lua_transfer_semantic_snapshots_validate_excluded_string_keys() {
    let lua = Lua::new();
    for prefix in [
        b"ignored".as_slice(),
        b"__songlua_state_",
        b"__songlua_capture_",
    ] {
        let actor = actor(&lua, 8, 8, false);
        let mut key = prefix.to_vec();
        key.push(0xff);
        actor
            .raw_set(lua.create_string(&key).unwrap(), true)
            .unwrap();
        let before = fingerprint(Value::Table(actor.clone()));
        let old = snapshot(&lua, &actor, true).unwrap_err().to_string();
        let new = snapshot(&lua, &actor, false).unwrap_err().to_string();
        assert_eq!(old, new);
        assert_eq!(fingerprint(Value::Table(actor)), before);
    }
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn lua_transfer_bench_snapshots() {
    for (ignored, retained, nested) in [
        (0, 0, false),
        (128, 0, false),
        (64, 4, false),
        (64, 16, false),
        (256, 64, false),
        (64, 16, true),
    ] {
        let lua = Lua::new();
        let actor = actor(&lua, ignored, retained, nested);
        let old = snapshot(&lua, &actor, true).unwrap();
        let new = snapshot(&lua, &actor, false).unwrap();
        assert_eq!(
            fingerprint(Value::Table(old)),
            fingerprint(Value::Table(new))
        );
        lua.gc_stop();
        for old in order() {
            crate::perf::measure_sampled(
                &format!(
                    "snapshot_ignored_{ignored}_kept_{retained}_nested_{nested}/{}",
                    if old { "old" } else { "new" }
                ),
                32,
                1,
                || {
                    black_box(snapshot(black_box(&lua), black_box(&actor), old).unwrap());
                },
            );
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

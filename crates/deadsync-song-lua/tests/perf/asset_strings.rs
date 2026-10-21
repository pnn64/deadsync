use super::*;
use std::{path::PathBuf, sync::Arc};

#[path = "asset_strings_before.rs"]
mod before;

fn set(
    current: bool,
    texture: bool,
    actor: &Table,
    value: Option<&Value>,
    clear: bool,
) -> mlua::Result<()> {
    match (current, texture) {
        (false, true) => before::set_actor_texture_from_value(actor, value, clear),
        (true, true) => super::set_actor_texture_from_value(actor, value, clear),
        (false, false) => before::set_actor_sound_file_from_value(actor, value, clear),
        (true, false) => super::set_actor_sound_file_from_value(actor, value, clear),
    }
}

fn actor(lua: &Lua, kind: &str, fail: bool) -> (Table, Table) {
    lua.load(
        r#"
        local kind, fail = ...
        local log = {}
        local actor = {__songlua_actor_type=kind}
        setmetatable(actor, {
            __index=function(_, key) log[#log+1]='read:'..key end,
            __newindex=function(self, key, value)
                log[#log+1]='write:'..key..':'..type(value)..':'..tostring(value)
                collectgarbage('collect')
                if fail and (key == 'Texture' or key == 'File') then error('asset write denied') end
                rawset(self, key, value)
            end,
        })
        return actor, log
    "#,
    )
    .set_name("asset-string-oracle")
    .call((kind, fail))
    .unwrap()
}

fn compare_fields(old: &Table, new: &Table) {
    let mut count = 0;
    old.for_each::<Value, Value>(|key, value| {
        assert_eq!(value, new.raw_get::<Value>(key)?);
        count += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(count, new.pairs::<Value, Value>().count());
}

#[test]
fn asset_setters_preserve_bytes_errors_metamethods_foreign_vms_and_clearing() {
    let lua = Lua::new();
    let foreign = Lua::new();
    let default_texture = lua.create_table().unwrap();
    default_texture
        .raw_set("__songlua_default_texture", true)
        .unwrap();
    let captured = lua.create_table().unwrap();
    captured
        .raw_set("__songlua_aft_capture_name", "capture-7")
        .unwrap();
    let native = lua.create_table().unwrap();
    native
        .raw_set("__songlua_native_type", "RageTexture")
        .unwrap();
    native
        .raw_set("__songlua_texture_path", "sheet 2x6.png")
        .unwrap();
    let values = vec![
        None,
        Some(Value::Nil),
        Some(Value::Boolean(false)),
        Some(Value::Integer(17)),
        Some(Value::String(lua.create_string("").unwrap())),
        Some(Value::String(lua.create_string("texture.mp4").unwrap())),
        Some(Value::String(lua.create_string("tex\0name.png").unwrap())),
        Some(Value::String(
            lua.create_string("textures/\u{00e9}\u{1f3b5}.png").unwrap(),
        )),
        Some(Value::String(lua.create_string(b"bad\xff\0").unwrap())),
        Some(Value::String(
            foreign.create_string("foreign\0texture.png").unwrap(),
        )),
        Some(Value::String(
            foreign.create_string(b"foreign\xff").unwrap(),
        )),
        Some(Value::Table(default_texture)),
        Some(Value::Table(captured)),
        Some(Value::Table(native)),
    ];
    for texture in [false, true] {
        for kind in ["Sound", "Sprite", "ActorMultiVertex"] {
            for clear in [false, true] {
                for fail in [false, true] {
                    for value in &values {
                        let (old, old_log) = actor(&lua, kind, fail);
                        let (new, new_log) = actor(&lua, kind, fail);
                        let old_result = set(false, texture, &old, value.as_ref(), clear)
                            .map_err(|e| e.to_string());
                        let new_result = set(true, texture, &new, value.as_ref(), clear)
                            .map_err(|e| e.to_string());
                        assert_eq!(old_result, new_result);
                        compare_fields(&old, &new);
                        assert_eq!(
                            old_log
                                .sequence_values::<String>()
                                .collect::<mlua::Result<Vec<_>>>()
                                .unwrap(),
                            new_log
                                .sequence_values::<String>()
                                .collect::<mlua::Result<Vec<_>>>()
                                .unwrap()
                        );
                    }
                }
            }
        }
    }
    for texture in [false, true] {
        let mut outcomes = Vec::new();
        for current in [false, true] {
            let lua = Lua::new();
            let value = Value::String(lua.create_string("under-pressure.png").unwrap());
            let actor = lua.create_table().unwrap();
            actor.raw_set("__songlua_actor_type", "Sprite").unwrap();
            let metatable = lua.create_table().unwrap();
            metatable
                .raw_set(
                    "__newindex",
                    lua.create_function(|lua, ()| {
                        let slots = (0..4000)
                            .map(|_| lua.create_table())
                            .collect::<mlua::Result<Vec<_>>>()?;
                        std::hint::black_box(&slots);
                        Ok(())
                    })
                    .unwrap(),
                )
                .unwrap();
            actor.set_metatable(Some(metatable)).unwrap();
            let held: Vec<_> = (0..5000).filter_map(|_| lua.create_table().ok()).collect();
            outcomes.push(
                set(current, texture, &actor, Some(&value), false).map_err(|e| e.to_string()),
            );
            drop(held);
            assert_eq!(lua.load("return 6*7").eval::<i64>().unwrap(), 42);
        }
        assert_eq!(outcomes[0], outcomes[1]);
    }
}

fn key_paths() -> Vec<PathBuf> {
    let mut paths = vec![
        PathBuf::new(),
        "textures/normal 2x6.png".into(),
        "textures/\u{00e9}\u{1f3b5}.png".into(),
        format!("textures/{}.png", "x".repeat(4096)).into(),
    ];
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        paths.push(std::ffi::OsString::from_wide(&[b'a' as u16, 0xd800, b'b' as u16]).into());
    }
    paths
}

#[test]
fn texture_path_keys_preserve_lossy_unicode_and_independent_storage() {
    for path in key_paths() {
        let old = before::texture_path_key(&path);
        let new = super::texture_path_key(&path);
        assert_eq!(old.as_bytes(), new.as_bytes());
        assert_eq!(new.as_ref(), path.to_string_lossy().as_ref());
        assert!(!Arc::ptr_eq(&new, &super::texture_path_key(&path)));
    }
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_asset_strings() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    let _affinity = pin_cpu2();
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    };
    for (name, texture, foreign_value, kind, length) in [
        ("texture_short", true, false, "Sprite", 24),
        ("texture_long", true, false, "Sprite", 4096),
        ("texture_empty", true, false, "Sprite", 0),
        ("texture_foreign", true, true, "Sprite", 24),
        ("texture_amv", true, false, "ActorMultiVertex", 24),
        ("sound_short", false, false, "Sound", 24),
        ("sound_long", false, false, "Sound", 4096),
        ("sound_empty", false, false, "Sound", 0),
        ("sound_foreign", false, true, "Sound", 24),
    ] {
        for current in order {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let lua = Lua::new();
                let foreign = Lua::new();
                let actor = lua.create_table().unwrap();
                actor.raw_set("__songlua_actor_type", kind).unwrap();
                let value = Value::String(
                    if foreign_value { &foreign } else { &lua }
                        .create_string("x".repeat(length))
                        .unwrap(),
                );
                set(current, texture, &actor, Some(&value), false).unwrap();
                lua.gc_stop();
                foreign.gc_stop();
                let start = Instant::now();
                let begin = cycles();
                let (_, churn) = allocations::measure(|| {
                    for _ in 0..8192 {
                        black_box(set(current, texture, &actor, Some(&value), false).unwrap());
                    }
                });
                let used = cycles() - begin;
                let nanos = start.elapsed().as_nanos();
                if sample > 0 {
                    samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used,"units":8192,
                        "allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
                }
            }
            println!(
                "DRAW_CAPTURE_METRIC {}",
                serde_json::json!({"workload":name,"variant":if current{"after"}else{"before"},
                "unit":"setters","baseline_commit":"0c335501b","samples":samples})
            );
        }
    }
    for (name, path) in [
        "key_empty",
        "key_ascii",
        "key_unicode",
        "key_long",
        "key_lossy_utf16",
    ]
    .into_iter()
    .zip(key_paths())
    {
        for current in order {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let start = Instant::now();
                let begin = cycles();
                let (_, churn) = allocations::measure(|| {
                    for _ in 0..8192 {
                        black_box(if current {
                            super::texture_path_key(black_box(&path))
                        } else {
                            before::texture_path_key(black_box(&path))
                        });
                    }
                });
                let used = cycles() - begin;
                let nanos = start.elapsed().as_nanos();
                if sample > 0 {
                    samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used,"units":8192,
                        "allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
                }
            }
            println!(
                "DRAW_CAPTURE_METRIC {}",
                serde_json::json!({"workload":name,"variant":if current{"after"}else{"before"},
                "unit":"keys","baseline_commit":"0c335501b","samples":samples})
            );
        }
    }
}

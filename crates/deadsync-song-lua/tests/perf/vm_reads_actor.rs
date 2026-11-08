use super::*;

#[path = "vm_reads_actor_before.rs"]
mod before;

#[test]
fn method_name_flags_preserve_utf8_errors_prefixes_suffixes_and_nuls() {
    let lua = Lua::new();
    let mut cases: Vec<Vec<u8>> = [
        "",
        "name",
        "Command",
        "OnCommand",
        "__songlua_",
        "__songlua_xCommand",
        "SetUpdateFunction",
        "SetDrawFunction",
        "GetDrawFunction",
        "SetUpdateFunction\0",
        "éCommand",
    ]
    .into_iter()
    .map(|text| text.as_bytes().to_vec())
    .collect();
    cases.extend((0..128).map(|len| vec![b'a'; len]));
    cases.extend([
        vec![0xff],
        vec![b'a', 0, 0xff],
        "longλ".repeat(256).into_bytes(),
    ]);
    for bytes in cases {
        let key = lua.create_string(&bytes).unwrap();
        let expected = key
            .to_str()
            .map(|text| {
                (
                    !text.starts_with("__songlua_") && !text.ends_with("Command"),
                    matches!(
                        text.as_ref(),
                        "SetUpdateFunction" | "SetDrawFunction" | "GetDrawFunction"
                    ),
                )
            })
            .map_err(|error| error.to_string());
        let actual = method_name_flags(&lua, &key).map_err(|error| error.to_string());
        assert_eq!(actual, expected, "bytes={bytes:?}");
    }
}

#[test]
fn lookup_keeps_class_hooks_values_errors_and_frame_callback_policy() {
    let lua = Lua::new();
    lua.load(
        r#"
        calls = {}
        Actor = setmetatable({own = 17}, {__index = function(_, key)
            calls[#calls + 1] = key
            if key == 'fault' then error('class lookup fault') end
            if key == 'hook' then return false end
        end})
        ActorFrame = setmetatable({frame = 23}, {__index = Actor})
        Sprite = setmetatable({}, {__index = Actor})
    "#,
    )
    .exec()
    .unwrap();
    let actor = lua.create_table().unwrap();
    let methods = lua.create_table().unwrap();
    methods.raw_set("fallback", 31).unwrap();
    methods.raw_set("SetUpdateFunction", 37).unwrap();
    actor.raw_set(METHODS_KEY, methods).unwrap();
    for kind in [
        "Actor",
        "ActorFrame",
        "GraphDisplay",
        "Quad",
        "custom",
        &"customλ".repeat(32),
    ] {
        actor.raw_set("__songlua_actor_type", kind).unwrap();
        for bytes in [
            b"own".as_slice(),
            b"frame",
            b"fallback",
            b"hook",
            b"fault",
            b"SetUpdateFunction",
            b"name\0",
            &[0xff],
        ] {
            let key = Value::String(lua.create_string(bytes).unwrap());
            lua.globals()
                .raw_set("calls", lua.create_table().unwrap())
                .unwrap();
            let old = before::lookup(&lua, (actor.clone(), key.clone()))
                .map_err(|error| error.to_string());
            let calls = lua
                .globals()
                .raw_get::<Table>("calls")
                .unwrap()
                .sequence_values::<String>()
                .collect::<mlua::Result<Vec<_>>>()
                .unwrap();
            lua.globals()
                .raw_set("calls", lua.create_table().unwrap())
                .unwrap();
            let new = lookup(&lua, (actor.clone(), key)).map_err(|error| error.to_string());
            let actual = lua
                .globals()
                .raw_get::<Table>("calls")
                .unwrap()
                .sequence_values::<String>()
                .collect::<mlua::Result<Vec<_>>>()
                .unwrap();
            assert_eq!(new, old, "kind={kind}, bytes={bytes:?}");
            assert_eq!(actual, calls);
        }
    }
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_method_keys() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    let _affinity = pin_cpu2();
    let variants = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    };
    for workload in ["bind_keys128", "assign_keys128", "lookup_keys128"] {
        for current in variants {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let lua = Lua::new();
                let function = lua.create_function(|_, ()| Ok(7)).unwrap();
                let names = lua
                    .create_sequence_from((0..128).map(|index| format!("method{index}")))
                    .unwrap();
                let class = lua.create_table().unwrap();
                for index in 1..=128 {
                    class
                        .raw_set(names.raw_get::<Value>(index).unwrap(), &function)
                        .unwrap();
                }
                lua.globals().raw_set("ActorFrame", class).unwrap();
                let actors: Vec<_> = (0..64)
                    .map(|_| {
                        let actor = lua.create_table().unwrap();
                        actor.raw_set("__songlua_actor_type", "ActorFrame").unwrap();
                        if workload == "bind_keys128" {
                            for index in 1..=128 {
                                actor
                                    .raw_set(names.raw_get::<Value>(index).unwrap(), &function)
                                    .unwrap();
                            }
                        } else if workload == "assign_keys128" {
                            actor
                                .raw_set(METHODS_KEY, lua.create_table().unwrap())
                                .unwrap();
                        }
                        actor
                    })
                    .collect();
                let start = Instant::now();
                let begin = cycles();
                let (_, churn) = allocations::measure(|| {
                    for actor in &actors {
                        if workload == "bind_keys128" {
                            if current {
                                bind(&lua, actor)
                            } else {
                                before::bind(&lua, actor)
                            }
                            .unwrap();
                        } else {
                            for index in 1..=128 {
                                let key = names.raw_get::<Value>(index).unwrap();
                                if workload == "assign_keys128" {
                                    let args =
                                        (actor.clone(), key, Value::Function(function.clone()));
                                    if current {
                                        assign(&lua, args)
                                    } else {
                                        before::assign(&lua, args)
                                    }
                                    .unwrap();
                                } else {
                                    let args = (actor.clone(), key);
                                    let value = if current {
                                        lookup(&lua, args)
                                    } else {
                                        before::lookup(&lua, args)
                                    }
                                    .unwrap();
                                    black_box(value);
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
                "unit":"actors","baseline_commit":"4277c1b63","samples":samples})
            );
        }
    }
}

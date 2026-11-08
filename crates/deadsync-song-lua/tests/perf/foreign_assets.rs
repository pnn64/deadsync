use super::*;

#[path = "foreign_assets_before.rs"]
mod before;

fn set(current: bool, texture: bool, actor: &Table, value: &Value) -> mlua::Result<()> {
    match (current, texture) {
        (false, true) => before::set_actor_texture_from_value(actor, Some(value), false),
        (true, true) => super::set_actor_texture_from_value(actor, Some(value), false),
        (false, false) => before::set_actor_sound_file_from_value(actor, Some(value), false),
        (true, false) => super::set_actor_sound_file_from_value(actor, Some(value), false),
    }
}

fn state(actor: &Table) -> Vec<(String, String)> {
    let mut values = Vec::new();
    actor
        .for_each::<String, Value>(|key, value| {
            let value = match value {
                Value::String(s) => format!("{:?}", s.as_bytes().as_ref()),
                other => format!("{other:?}"),
            };
            values.push((key, value));
            Ok(())
        })
        .unwrap();
    values.sort();
    values
}

#[test]
fn foreign_asset_borrows_keep_bytes_errors_gc_callbacks_and_reference_pressure() {
    use std::sync::{Arc, Mutex};
    for texture in [false, true] {
        for fail in [false, true] {
            for bytes in [
                b"short.png".as_slice(),
                b"",
                b"name\0file.png",
                b"bad\xff",
                "textures/\u{00e9}\u{1f3b5}.png".as_bytes(),
            ] {
                let mut outcomes = Vec::new();
                for current in [false, true] {
                    let target = Lua::new();
                    let source = Lua::new();
                    let value = Value::String(source.create_string(bytes).unwrap());
                    let actor = target.create_table().unwrap();
                    actor.raw_set("__songlua_actor_type", "Sprite").unwrap();
                    let log = Arc::new(Mutex::new(Vec::new()));
                    let metatable = target.create_table().unwrap();
                    metatable
                        .raw_set(
                            "__newindex",
                            target
                                .create_function({
                                    let source = source.clone();
                                    let log = log.clone();
                                    move |_, (actor, key, value): (Table, String, Value)| {
                                        source.gc_collect()?;
                                        let bytes = match &value {
                                            Value::String(s) => s.as_bytes().to_vec(),
                                            _ => Vec::new(),
                                        };
                                        log.lock().unwrap().push((key.clone(), bytes));
                                        if fail {
                                            return Err(mlua::Error::runtime(
                                                "foreign asset write denied",
                                            ));
                                        }
                                        actor.raw_set(key, value)
                                    }
                                })
                                .unwrap(),
                        )
                        .unwrap();
                    actor.set_metatable(Some(metatable)).unwrap();
                    let result = set(current, texture, &actor, &value).map_err(|e| e.to_string());
                    outcomes.push((result, state(&actor), log.lock().unwrap().clone()));
                    assert_eq!(source.load("return 6*7").eval::<i64>().unwrap(), 42);
                    assert_eq!(target.load("return 6*7").eval::<i64>().unwrap(), 42);
                }
                assert_eq!(outcomes[0], outcomes[1]);
            }
        }
        let mut outcomes = Vec::new();
        for current in [false, true] {
            let target = Lua::new();
            let source = Lua::new();
            let value = Value::String(source.create_string("pressure.png").unwrap());
            let actor = target.create_table().unwrap();
            actor.raw_set("__songlua_actor_type", "Sprite").unwrap();
            let metatable = target.create_table().unwrap();
            metatable
                .raw_set(
                    "__newindex",
                    target
                        .create_function({
                            let source = source.clone();
                            move |_, ()| {
                                let slots = (0..4000)
                                    .map(|_| source.create_table())
                                    .collect::<mlua::Result<Vec<_>>>()?;
                                std::hint::black_box(&slots);
                                Ok(())
                            }
                        })
                        .unwrap(),
                )
                .unwrap();
            actor.set_metatable(Some(metatable)).unwrap();
            let held: Vec<_> = (0..5000)
                .filter_map(|_| source.create_table().ok())
                .collect();
            outcomes.push(set(current, texture, &actor, &value).map_err(|e| e.to_string()));
            drop(held);
            assert_eq!(source.load("return 6*7").eval::<i64>().unwrap(), 42);
        }
        assert_eq!(outcomes[0], outcomes[1]);
    }
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_foreign_assets() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    let _affinity = pin_cpu2();
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    };
    for (name, texture, foreign, length) in [
        ("foreign_texture_short", true, true, 24),
        ("foreign_texture_long", true, true, 4096),
        ("foreign_texture_empty", true, true, 0),
        ("foreign_sound_short", false, true, 24),
        ("foreign_sound_long", false, true, 4096),
        ("foreign_sound_empty", false, true, 0),
        ("same_texture_control", true, false, 24),
        ("same_sound_control", false, false, 24),
    ] {
        for current in order {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let target = Lua::new();
                let source = Lua::new();
                let value = Value::String(
                    if foreign { &source } else { &target }
                        .create_string("x".repeat(length))
                        .unwrap(),
                );
                let actor = target.create_table().unwrap();
                actor.raw_set("__songlua_actor_type", "Sprite").unwrap();
                set(current, texture, &actor, &value).unwrap();
                target.gc_stop();
                source.gc_stop();
                let start = Instant::now();
                let begin = cycles();
                let (_, churn) = allocations::measure(|| {
                    for _ in 0..8192 {
                        black_box(set(current, texture, &actor, &value).unwrap());
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
                "unit":"setters","baseline_commit":"494323364","samples":samples})
            );
        }
    }
}

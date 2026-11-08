use super::*;

#[path = "comparison_text_before.rs"]
mod before;

fn classify(current: bool, calls: &Table) -> Result<String, String> {
    let result = if current {
        super::classify_function_ease_probe(calls)
    } else {
        before::classify_function_ease_probe(calls)
    };
    result.map(|v| format!("{v:?}")).map_err(|e| e.to_string())
}

#[test]
fn inline_probe_names_keep_targets_coercions_errors_and_early_returns() {
    let lua = Lua::new();
    let mut values: Vec<Value> = [
        "x",
        "player.x",
        "y",
        "player.z",
        "rotationx",
        "rotationy",
        "rotationz",
        "skewx",
        "skewy",
        "zoom",
        "zoomx",
        "zoomy",
        "zoomz",
        "overlay.x",
        "other.x",
        "",
        "x\0",
        "player..x",
    ]
    .iter()
    .map(|s| Value::String(lua.create_string(s).unwrap()))
    .collect();
    values.extend([
        Value::Integer(17),
        Value::Number(1.5),
        Value::Boolean(false),
        Value::Table(lua.create_table().unwrap()),
        Value::String(lua.create_string(b"bad\xff").unwrap()),
    ]);
    for n in [31, 32, 33, 4096] {
        values.push(Value::String(lua.create_string("x".repeat(n)).unwrap()));
    }
    for left in &values {
        for right in &values {
            let calls = lua
                .create_sequence_from([left.clone(), right.clone()])
                .unwrap();
            assert_eq!(classify(false, &calls), classify(true, &calls));
        }
    }
    let sparse = lua.create_table().unwrap();
    sparse.raw_set(1, "player.x").unwrap();
    sparse
        .raw_set(3, lua.create_string(b"bad\xff").unwrap())
        .unwrap();
    assert_eq!(classify(false, &sparse), classify(true, &sparse));
    let empty = lua.create_table().unwrap();
    assert_eq!(classify(false, &empty), classify(true, &empty));
}

#[test]
fn inline_native_texture_tags_keep_identity_coercions_and_exact_errors() {
    let lua = Lua::new();
    let mut tags = vec![
        Value::Nil,
        Value::Boolean(false),
        Value::Integer(17),
        Value::Number(1.5),
        Value::Table(lua.create_table().unwrap()),
        Value::String(lua.create_string("RageTexture").unwrap()),
        Value::String(lua.create_string("ragetexture").unwrap()),
        Value::String(lua.create_string("RageTexture\0").unwrap()),
        Value::String(lua.create_string(b"bad\xff").unwrap()),
    ];
    for n in [31, 32, 33, 4096] {
        tags.push(Value::String(lua.create_string("x".repeat(n)).unwrap()));
    }
    for tag in tags {
        let texture = lua.create_table().unwrap();
        texture.raw_set("__songlua_native_type", tag).unwrap();
        texture.raw_set("__songlua_default_texture", true).unwrap();
        texture
            .set_metatable(Some(
                lua.load("return {__index=function() error('unexpected texture lookup') end}")
                    .eval::<Table>()
                    .unwrap(),
            ))
            .unwrap();
        let make = || {
            let actor = lua.create_table().unwrap();
            actor
                .raw_set("__songlua_actor_type", "ActorMultiVertex")
                .unwrap();
            actor
        };
        let old = make();
        let new = make();
        let value = Value::Table(texture);
        let a = before::set_actor_texture_from_value(&old, Some(&value), false)
            .map_err(|e| e.to_string());
        let b = super::set_actor_texture_from_value(&new, Some(&value), false)
            .map_err(|e| e.to_string());
        assert_eq!(a, b);
        let mut count = 0;
        old.for_each::<Value, Value>(|key, value| {
            assert_eq!(value, new.raw_get::<Value>(key)?);
            count += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(count, new.pairs::<Value, Value>().count());
    }
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_comparison_text() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    let _affinity = pin_cpu2();
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    };
    for (name, tag) in [
        ("tag_native", "RageTexture".to_owned()),
        ("tag_unknown", "OtherTexture".to_owned()),
        ("tag_long", "x".repeat(4096)),
        ("tag_empty", String::new()),
    ] {
        for current in order {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let lua = Lua::new();
                let actor = lua.create_table().unwrap();
                actor
                    .raw_set("__songlua_actor_type", "ActorMultiVertex")
                    .unwrap();
                let texture = lua.create_table().unwrap();
                texture
                    .raw_set("__songlua_native_type", tag.as_str())
                    .unwrap();
                texture.raw_set("__songlua_default_texture", true).unwrap();
                let value = Value::Table(texture);
                lua.gc_stop();
                let start = Instant::now();
                let begin = cycles();
                let (_, churn) = allocations::measure(|| {
                    for _ in 0..8192 {
                        black_box(if current {
                            super::set_actor_texture_from_value(&actor, Some(&value), false)
                        } else {
                            before::set_actor_texture_from_value(&actor, Some(&value), false)
                        })
                        .unwrap();
                    }
                });
                let used = cycles() - begin;
                let ns = start.elapsed().as_nanos();
                if sample > 0 {
                    samples.push(serde_json::json!({"ns":ns,"thread_cycles":used,"units":8192,"allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
                }
            }
            println!(
                "DRAW_CAPTURE_METRIC {}",
                serde_json::json!({"workload":name,"variant":if current{"after"}else{"before"},"unit":"tag writes","baseline_commit":"142996521","samples":samples})
            );
        }
    }
    for (name, n, kind) in [
        ("probe_empty", 0, 0),
        ("probe_one", 1, 0),
        ("probe_repeat64", 64, 0),
        ("probe_mixed12", 12, 1),
        ("probe_unknown_first", 64, 2),
        ("probe_long", 1, 3),
        ("probe_number", 1, 4),
    ] {
        for current in order {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let lua = Lua::new();
                let calls = lua.create_table().unwrap();
                for i in 0..n {
                    let value = match kind {
                        1 => Value::String(
                            lua.create_string(if i % 2 == 0 { "player.x" } else { "player.y" })
                                .unwrap(),
                        ),
                        2 if i == 0 => Value::String(lua.create_string("unknown.x").unwrap()),
                        3 => Value::String(lua.create_string("x".repeat(4096)).unwrap()),
                        4 => Value::Integer(17),
                        _ => Value::String(lua.create_string("player.x").unwrap()),
                    };
                    calls.raw_set(i + 1, value).unwrap();
                }
                lua.gc_stop();
                let start = Instant::now();
                let begin = cycles();
                let (_, churn) = allocations::measure(|| {
                    for _ in 0..8192 {
                        black_box(if current {
                            super::classify_function_ease_probe(&calls)
                        } else {
                            before::classify_function_ease_probe(&calls)
                        })
                        .unwrap();
                    }
                });
                let used = cycles() - begin;
                let ns = start.elapsed().as_nanos();
                if sample > 0 {
                    samples.push(serde_json::json!({"ns":ns,"thread_cycles":used,"units":8192,"allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
                }
            }
            println!(
                "DRAW_CAPTURE_METRIC {}",
                serde_json::json!({"workload":name,"variant":if current{"after"}else{"before"},"unit":"probe classifications","baseline_commit":"142996521","samples":samples})
            );
        }
    }
}

use super::*;

#[path = "native_function_before.rs"]
mod before;

#[test]
fn environments_classify_native_lua_main_bound_and_custom_environment_functions() {
    let lua = Lua::new();
    let native = lua.create_function(|_, ()| Ok(7)).unwrap();
    let builtin = lua
        .globals()
        .get::<Table>("math")
        .unwrap()
        .get::<mlua::Function>("sin")
        .unwrap();
    let closure = lua
        .load("return function() return 11 end")
        .eval::<mlua::Function>()
        .unwrap();
    let main = lua.load("return 13").into_function().unwrap();
    let bound = native.bind(()).unwrap();
    let custom = lua
        .load("return function() return custom end")
        .eval::<mlua::Function>()
        .unwrap();
    let environment = lua.create_table().unwrap();
    environment.raw_set("custom", 17).unwrap();
    assert!(custom.set_environment(environment).unwrap());
    for function in [native, builtin, closure, main, bound, custom] {
        assert_eq!(
            function.environment().is_none(),
            function.info().what == "C"
        );
    }
}

#[test]
fn binding_and_assignment_keep_function_identity_routing_and_utf8_errors() {
    fn fields(table: &Table) -> std::collections::BTreeMap<Vec<u8>, Value> {
        let mut fields = std::collections::BTreeMap::new();
        table
            .for_each::<mlua::String, Value>(|key, value| {
                if key.as_bytes().as_ref() != METHODS_KEY.as_bytes() {
                    fields.insert(key.as_bytes().to_vec(), value);
                }
                Ok(())
            })
            .unwrap();
        fields
    }
    let lua = Lua::new();
    let native = Value::Function(lua.create_function(|_, ()| Ok(7)).unwrap());
    let script = Value::Function(lua.load("return function() return 9 end").eval().unwrap());
    for invalid in [false, true] {
        for existing in [false, true] {
            let actors: [Table; 2] = std::array::from_fn(|_| {
                let actor = lua.create_table().unwrap();
                for (name, value) in [
                    ("native", &native),
                    ("script", &script),
                    ("CustomCommand", &native),
                    ("__songlua_private", &native),
                ] {
                    actor.raw_set(name, value.clone()).unwrap();
                }
                if existing {
                    actor
                        .raw_set(METHODS_KEY, lua.create_table().unwrap())
                        .unwrap();
                }
                if invalid {
                    actor
                        .raw_set(lua.create_string([0xff]).unwrap(), native.clone())
                        .unwrap();
                }
                actor
            });
            let old = before::bind(&lua, &actors[0]).map_err(|error| error.to_string());
            let new = bind(&lua, &actors[1]).map_err(|error| error.to_string());
            assert_eq!(new, old);
            assert_eq!(new.is_err(), invalid);
            assert_eq!(fields(&actors[0]), fields(&actors[1]));
            let old = actors[0].raw_get::<Option<Table>>(METHODS_KEY).unwrap();
            let new = actors[1].raw_get::<Option<Table>>(METHODS_KEY).unwrap();
            assert_eq!(new.as_ref().map(fields), old.as_ref().map(fields));
            for key in [
                lua.create_string("assigned").unwrap(),
                lua.create_string("MoreCommand").unwrap(),
                lua.create_string("__songlua_assigned").unwrap(),
                lua.create_string([0xff]).unwrap(),
            ] {
                for value in [native.clone(), script.clone(), Value::Integer(19)] {
                    let old = before::assign(
                        &lua,
                        (actors[0].clone(), Value::String(key.clone()), value.clone()),
                    )
                    .map_err(|e| e.to_string());
                    let new = assign(
                        &lua,
                        (actors[1].clone(), Value::String(key.clone()), value.clone()),
                    )
                    .map_err(|e| e.to_string());
                    assert_eq!(new, old);
                    assert_eq!(fields(&actors[0]), fields(&actors[1]));
                    let old = actors[0].raw_get::<Option<Table>>(METHODS_KEY).unwrap();
                    let new = actors[1].raw_get::<Option<Table>>(METHODS_KEY).unwrap();
                    assert_eq!(new.as_ref().map(fields), old.as_ref().map(fields));
                }
            }
        }
    }
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_native_binding() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    let _affinity = pin_cpu2();
    let variants = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    };
    for assignment in [false, true] {
        for current in variants {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let lua = Lua::new();
                let function = lua.create_function(|_, ()| Ok(7)).unwrap();
                let keys: Vec<_> = (0..128)
                    .map(|index| lua.create_string(format!("method{index}")).unwrap())
                    .collect();
                let actors: Vec<_> = (0..64)
                    .map(|_| {
                        let actor = lua.create_table().unwrap();
                        if assignment {
                            actor
                                .raw_set(METHODS_KEY, lua.create_table().unwrap())
                                .unwrap();
                        } else {
                            for key in &keys {
                                actor.raw_set(key.clone(), function.clone()).unwrap();
                            }
                        }
                        actor
                    })
                    .collect();
                let start = Instant::now();
                let begin = cycles();
                let (_, churn) = allocations::measure(|| {
                    for actor in &actors {
                        if assignment {
                            for key in &keys {
                                let args = (
                                    actor.clone(),
                                    Value::String(key.clone()),
                                    Value::Function(function.clone()),
                                );
                                if current {
                                    assign(&lua, args)
                                } else {
                                    before::assign(&lua, args)
                                }
                                .unwrap();
                            }
                        } else if current {
                            bind(&lua, actor).unwrap();
                        } else {
                            before::bind(&lua, actor).unwrap();
                        }
                        black_box(actor);
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
                serde_json::json!({"workload":if assignment {"assign128"} else {"bind128"},
                "variant":if current {"after"} else {"before"},"unit":"actors","baseline_commit":"31a7355f5","samples":samples})
            );
        }
    }
}

use super::*;

#[path = "vm_reads_registry_before.rs"]
mod before;

#[test]
fn registration_preserves_holes_duplicates_mutations_and_unusual_values() {
    fn fields(table: &Table) -> BTreeMap<i64, Value> {
        let mut out = BTreeMap::new();
        table
            .for_each::<i64, Value>(|key, value| {
                out.insert(key, value);
                Ok(())
            })
            .unwrap();
        out
    }
    struct Marker;
    impl mlua::UserData for Marker {}
    for live in [false, true] {
        let lua = Lua::new();
        if live {
            begin_live_runtime(&lua).unwrap();
        }
        lua.load("equal_calls = 0").exec().unwrap();
        let actor_meta = lua
            .load("return {__eq = function() equal_calls = equal_calls + 1; return true end}")
            .eval::<Table>()
            .unwrap();
        let actors: Vec<_> = (0..3)
            .map(|_| {
                let actor = lua.create_table().unwrap();
                actor.raw_set("__songlua_actor_type", "Actor").unwrap();
                actor.set_metatable(Some(actor_meta.clone())).unwrap();
                actor
            })
            .collect();
        let function = lua.create_function(|_, ()| Ok(7)).unwrap();
        let thread = lua
            .create_thread(lua.load("return 9").into_function().unwrap())
            .unwrap();
        let marker = lua.create_userdata(Marker).unwrap();
        let meta = lua.load("return {__index = function() error('registry index hook') end, __newindex = function() error('registry write hook') end}").eval::<Table>().unwrap();
        for case in 0..5 {
            let registries: [Table; 2] = std::array::from_fn(|_| {
                let registry = lua.create_table().unwrap();
                match case {
                    1 => {
                        registry.raw_set(1, &actors[0]).unwrap();
                    }
                    2 => {
                        registry.raw_set(1, false).unwrap();
                        registry.raw_set(2, &actors[0]).unwrap();
                        registry
                            .raw_set(3, lua.create_string([0xff]).unwrap())
                            .unwrap();
                        registry.raw_set(4, &actors[1]).unwrap();
                        registry.raw_set(5, marker.clone()).unwrap();
                    }
                    3 => {
                        registry.raw_set(1, &actors[0]).unwrap();
                        registry.raw_set(3, &actors[1]).unwrap();
                    }
                    4 => {
                        registry.raw_set(1, &function).unwrap();
                        registry.raw_set(2, thread.clone()).unwrap();
                        registry.raw_set(3, marker.clone()).unwrap();
                        registry.raw_set(4, 17).unwrap();
                        registry.raw_set(5, &actors[0]).unwrap();
                    }
                    _ => {}
                }
                registry.set_metatable(Some(meta.clone())).unwrap();
                registry
            });
            for (step, index) in [1, 1, 2, 0].into_iter().enumerate() {
                lua.globals()
                    .raw_set("__songlua_actor_registry", &registries[0])
                    .unwrap();
                let old = before::register_song_lua_actor(&lua, &actors[index])
                    .map_err(|error| error.to_string());
                lua.globals()
                    .raw_set("__songlua_actor_registry", &registries[1])
                    .unwrap();
                let new = register_song_lua_actor(&lua, &actors[index])
                    .map_err(|error| error.to_string());
                assert_eq!(new, old);
                assert_eq!(
                    fields(&registries[1]),
                    fields(&registries[0]),
                    "live={live}, case={case}, step={step}"
                );
                for registry in &registries {
                    if step == 1 {
                        registry.raw_set(1, &actors[2]).unwrap();
                    }
                    if step == 2 {
                        registry.raw_set(2, Value::Nil).unwrap();
                    }
                }
            }
        }
        assert_eq!(lua.globals().get::<u32>("equal_calls").unwrap(), 0);
        for invalid in [
            Value::Boolean(false),
            Value::Integer(17),
            Value::String(lua.create_string("invalid").unwrap()),
        ] {
            lua.globals()
                .raw_set("__songlua_actor_registry", invalid.clone())
                .unwrap();
            let old = before::register_song_lua_actor(&lua, &actors[0])
                .map_err(|error| error.to_string());
            lua.globals()
                .raw_set("__songlua_actor_registry", invalid)
                .unwrap();
            let new = register_song_lua_actor(&lua, &actors[0]).map_err(|error| error.to_string());
            assert_eq!(new, old);
            assert!(new.is_err());
        }
    }
}

#[test]
fn narrow_upvalue_queries_match_full_debug_info_on_functions_and_coroutines() {
    let lua = Lua::new();
    let mut functions = vec![
        lua.create_function(|_, ()| Ok(7)).unwrap(),
        lua.globals()
            .get::<Table>("math")
            .unwrap()
            .get::<Function>("sin")
            .unwrap(),
        lua.load("return 11").into_function().unwrap(),
    ];
    functions.push(functions[0].bind((1, 2)).unwrap());
    for count in [0, 1, 8, 32, 60] {
        let source = if count == 0 {
            "return function() return 7 end".to_owned()
        } else {
            let names: Vec<_> = (0..count).map(|index| format!("v{index}")).collect();
            format!(
                "local {} = {}; return function() return {} end",
                names.join(","),
                vec!["1"; count].join(","),
                names.join("+")
            )
        };
        let function = lua.load(source).eval::<Function>().unwrap();
        assert_eq!(function.info().num_upvalues as usize, count);
        functions.push(function);
    }
    let roots = lua.create_sequence_from(functions.iter().cloned()).unwrap();
    let expected = lua
        .create_sequence_from(
            functions
                .iter()
                .map(|function| function.info().num_upvalues),
        )
        .unwrap();
    for function in functions {
        assert_eq!(
            function_upvalue_count(&lua, &function).unwrap(),
            function.info().num_upvalues
        );
    }
    lua.globals().raw_set("functions", roots).unwrap();
    lua.globals().raw_set("expected_counts", expected).unwrap();
    lua.globals()
        .raw_set(
            "count_upvalues",
            lua.create_function(|lua, function: Function| function_upvalue_count(lua, &function))
                .unwrap(),
        )
        .unwrap();
    lua.load(r#"
        local thread = coroutine.create(function()
            for index, fn in ipairs(functions) do assert(count_upvalues(fn) == expected_counts[index]) end
        end)
        local ok, err = coroutine.resume(thread)
        assert(ok, err)
    "#).exec().unwrap();
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_registry_and_upvalues() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    let _affinity = pin_cpu2();
    let variants = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    };
    for workload in ["register_new128", "register_tail1024", "upvalue_count32"] {
        for current in variants {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let lua = Lua::new();
                let registry = lua.create_table().unwrap();
                lua.globals()
                    .raw_set("__songlua_actor_registry", &registry)
                    .unwrap();
                let actors: Vec<_> = (0..128).map(|_| lua.create_table().unwrap()).collect();
                let target = lua.create_table().unwrap();
                if workload == "register_tail1024" {
                    for index in 1..1024 {
                        registry
                            .raw_set(index, lua.create_table().unwrap())
                            .unwrap();
                    }
                    registry.raw_set(1024, &target).unwrap();
                }
                let names: Vec<_> = (0..32).map(|index| format!("v{index}")).collect();
                let function = lua
                    .load(format!(
                        "local {} = {}; return function() return {} end",
                        names.join(","),
                        vec!["1"; 32].join(","),
                        names.join("+")
                    ))
                    .eval::<Function>()
                    .unwrap();
                let units = if workload == "upvalue_count32" {
                    8192
                } else {
                    128
                };
                let start = Instant::now();
                let begin = cycles();
                let (_, churn) = allocations::measure(|| {
                    if workload == "upvalue_count32" {
                        for _ in 0..units {
                            black_box(if current {
                                function_upvalue_count(&lua, &function).unwrap()
                            } else {
                                function.info().num_upvalues
                            });
                        }
                    } else {
                        for index in 0..units {
                            let actor = if workload == "register_tail1024" {
                                &target
                            } else {
                                &actors[index]
                            };
                            if current {
                                register_song_lua_actor(&lua, actor)
                            } else {
                                before::register_song_lua_actor(&lua, actor)
                            }
                            .unwrap();
                        }
                    }
                });
                let used = cycles() - begin;
                let nanos = start.elapsed().as_nanos();
                assert_eq!(
                    registry.raw_len(),
                    match workload {
                        "register_new128" => 128,
                        "register_tail1024" => 1024,
                        _ => 0,
                    }
                );
                if sample > 0 {
                    samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used,"units":units,
                        "allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
                }
            }
            println!(
                "DRAW_CAPTURE_METRIC {}",
                serde_json::json!({"workload":workload,"variant":if current {"after"} else {"before"},
                "unit":"operations","baseline_commit":"4277c1b63","samples":samples})
            );
        }
    }
}

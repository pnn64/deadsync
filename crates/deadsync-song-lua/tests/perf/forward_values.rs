use super::*;
use mlua::Function;

#[path = "forward_values_before.rs"]
pub(crate) mod before;
#[path = "forward_values_variants.rs"]
mod variants;

fn prepared_forward(lua: &Lua, mode: usize) -> Function {
    let class = lua.create_table().unwrap();
    match mode {
        0 => before::forward(lua, &class, "probe"),
        1 => variants::forward_inputs(lua, &class, "probe"),
        2 => variants::forward_outputs(lua, &class, "probe"),
        _ => super::forward(lua, &class, "probe"),
    }
    .unwrap();
    class.raw_get("probe").unwrap()
}

fn actor(lua: &Lua, semantic: Function) -> Table {
    let actor = lua.create_table().unwrap();
    let bound = lua.create_table().unwrap();
    bound.raw_set("probe", semantic).unwrap();
    actor.raw_set(METHODS_KEY, bound).unwrap();
    actor
}

#[test]
fn forwarding_keeps_values_arities_failures_coroutines_and_panics() {
    let mut pressure = Vec::new();
    for current in [false, true] {
        let lua = Lua::new();
        let semantic = lua
            .load("return function(...) return ... end")
            .eval::<Function>()
            .unwrap();
        let receiver = actor(&lua, semantic);
        lua.globals().raw_set("actor", &receiver).unwrap();
        lua.globals()
            .raw_set("FORWARD", prepared_forward(&lua, usize::from(current) * 3))
            .unwrap();
        lua.load(r#"
            local t,f,co,ud={},function()end,coroutine.create(function()end),newproxy(true)
            local args={actor,nil,false,-0.0,string.char(255,0),t,f,co,ud,nil}
            for n=1,10 do
                local count=select('#',FORWARD(unpack(args,1,n)))
                local values={FORWARD(unpack(args,1,n))}
                assert(count==n)
                for i=1,n do assert(rawequal(values[i],args[i])) end
            end
            local ok,a,b,c=coroutine.resume(coroutine.create(function() return FORWARD(actor,nil,ud) end))
            assert(ok and a==actor and b==nil and c==ud)
            assert(not pcall(FORWARD)); assert(not pcall(FORWARD,false))
        "#).exec().unwrap();
        for expression in [
            "return",
            "return nil",
            "return 17,nil,-0.0",
            "return nil,false,17,{},function()end,string.char(255,0),newproxy(true),nil",
        ] {
            let semantic = lua
                .load(format!("return function(...) {expression} end"))
                .eval::<Function>()
                .unwrap();
            receiver
                .raw_get::<Table>(METHODS_KEY)
                .unwrap()
                .raw_set("probe", &semantic)
                .unwrap();
            let expected = semantic.call::<MultiValue>(()).unwrap();
            let actual = prepared_forward(&lua, usize::from(current) * 3)
                .call::<MultiValue>(&receiver)
                .unwrap();
            assert_eq!(actual.len(), expected.len());
            // Identity checks for freshly-created values are covered by echo.
            for (left, right) in actual.iter().zip(&expected) {
                if let (Value::Number(left), Value::Number(right)) = (left, right) {
                    assert_eq!(left.to_bits(), right.to_bits());
                } else if !matches!(
                    left,
                    Value::Table(_) | Value::Function(_) | Value::UserData(_)
                ) {
                    assert_eq!(left, right);
                }
            }
        }
        let semantic = lua
            .load("return function(...) return ... end")
            .eval::<Function>()
            .unwrap();
        receiver
            .raw_get::<Table>(METHODS_KEY)
            .unwrap()
            .raw_set("probe", semantic)
            .unwrap();
        let held: Vec<_> = (0..5000).filter_map(|_| lua.create_table().ok()).collect();
        let mut args = MultiValue::with_capacity(4000);
        for _ in 0..4000 {
            args.push_back(Value::Table(receiver.clone()));
        }
        pressure.push(
            prepared_forward(&lua, usize::from(current) * 3)
                .call::<MultiValue>(args)
                .map(|values| values.len())
                .map_err(|e| e.to_string()),
        );
        drop(held);
        assert_eq!(
            prepared_forward(&lua, usize::from(current) * 3)
                .call::<MultiValue>(&receiver)
                .unwrap()
                .len(),
            1
        );
    }
    assert!(pressure.windows(2).all(|pair| pair[0] == pair[1]));
    #[cfg(panic = "unwind")]
    for current in [false, true] {
        for results in [false, true] {
            for position in [1, 2, 3, 4, 8] {
                let lua = Lua::new();
                let semantic = lua
                    .load("return function(...) return ... end")
                    .eval::<Function>()
                    .unwrap();
                let receiver = actor(&lua, semantic);
                lua.globals().raw_set("actor", &receiver).unwrap();
                lua.globals()
                    .raw_set("FORWARD", prepared_forward(&lua, usize::from(current) * 3))
                    .unwrap();
                lua.globals()
                    .raw_set(
                        "panic_arg",
                        lua.create_function(|_, ()| -> mlua::Result<()> {
                            panic!("forward value panic")
                        })
                        .unwrap(),
                    )
                    .unwrap();
                let items = (1..=8)
                    .map(|i| {
                        if i == position {
                            "p"
                        } else if i == 1 {
                            "actor"
                        } else {
                            "nil"
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                let script = if results {
                    format!(
                        "local _,p=pcall(panic_arg); actor.__songlua_bound_methods.probe=function() return {items} end; return FORWARD(actor)"
                    )
                } else {
                    format!("local _,p=pcall(panic_arg); return FORWARD({items})")
                };
                let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    lua.load(script).eval::<MultiValue>()
                }))
                .unwrap_err();
                assert_eq!(
                    panic.downcast_ref::<&str>().copied(),
                    Some("forward value panic")
                );
                assert_eq!(lua.load("return 6*7").eval::<i64>().unwrap(), 42);
            }
        }
    }
    println!(
        "Validated exact arity/order, nils, binary/reference identities, coroutine calls, zero/one/three/eight returns, bounded reference-pressure parity/recovery and argument/result panic propagation."
    );
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_forward_values() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    let _affinity = pin_cpu2();
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    };
    for (name, count, existing) in [
        ("bind0", 0, false),
        ("bind8", 8, false),
        ("bind128", 128, false),
        ("bind512", 512, false),
        ("bind128_existing", 128, true),
    ] {
        for current in order {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let lua = Lua::new();
                let function = lua.create_function(|_, ()| Ok(17)).unwrap();
                let keys: Vec<_> = (0..count)
                    .map(|i| lua.create_string(format!("method{i}")).unwrap())
                    .collect();
                let actors: Vec<_> = (0..64)
                    .map(|_| {
                        let actor = lua.create_table().unwrap();
                        for key in &keys {
                            actor.raw_set(key, &function).unwrap();
                        }
                        if existing {
                            actor
                                .raw_set(METHODS_KEY, lua.create_table().unwrap())
                                .unwrap();
                        }
                        actor
                    })
                    .collect();
                lua.gc_stop();
                let start = Instant::now();
                let begin = cycles();
                let (_, churn) = allocations::measure(|| {
                    for actor in &actors {
                        if current {
                            super::bind(&lua, actor)
                        } else {
                            before::bind(&lua, actor)
                        }
                        .unwrap();
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
                serde_json::json!({"workload":name,"variant":if current{"after"}else{"before"},
                "unit":"actors","baseline_commit":"3d2bb123f","samples":samples})
            );
        }
    }
    for (group, mode) in [("inputs", 1), ("results", 2), ("both", 3)] {
        for (name, arguments, body, count) in [
            ("receiver1_return1", "receiver", "return 17", 1),
            ("scalar2_return1", "receiver,17", "return (...) ", 1),
            ("size3_return1", "receiver,17,23", "return (...) ", 1),
            (
                "extra8_return1",
                "receiver,17,nil,false,other,other,other,nil",
                "return (...) ",
                1,
            ),
            ("scalar2_return0", "receiver,17", "return", 0),
            ("scalar2_return3", "receiver,17", "return 17,nil,-0.0", 3),
            (
                "extra8_return8",
                "receiver,17,nil,false,other,other,other,nil",
                "return ...",
                8,
            ),
        ] {
            for current in order {
                let mut samples = Vec::new();
                for sample in 0..10 {
                    let lua = Lua::new();
                    let semantic = lua
                        .load(format!("return function(...) {body} end"))
                        .eval::<Function>()
                        .unwrap();
                    let receiver = actor(&lua, semantic);
                    let runner=lua.load(format!("local call,receiver=...; local other={{}}; return function() local n=0; for i=1,8192 do n=n+select('#',call({arguments})) end; return n end"))
                        .call::<Function>((prepared_forward(&lua,if current{mode}else{0}),receiver)).unwrap();
                    assert_eq!(runner.call::<usize>(()).unwrap(), 8192 * count);
                    lua.gc_stop();
                    let start = Instant::now();
                    let begin = cycles();
                    let (_, churn) = allocations::measure(|| {
                        assert_eq!(runner.call::<usize>(()).unwrap(), 8192 * count)
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
                    serde_json::json!({"workload":format!("{group}_{name}"),"variant":if current{"after"}else{"before"},
                    "unit":"calls","baseline_commit":"3d2bb123f","samples":samples})
                );
            }
        }
    }
}

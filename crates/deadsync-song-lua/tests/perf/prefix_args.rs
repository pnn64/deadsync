use super::*;

#[path = "prefix_args_before.rs"]
mod before;

fn method(lua: &Lua, actor: &Table, current: bool, kind: &str) -> Function {
    match (current, kind) {
        (true, "wrap") => make_actor_wrap_width_method(lua, actor),
        (false, "wrap") => before::make_actor_wrap_width_method(lua, actor),
        (true, "size") => make_actor_set_size_method(lua, actor),
        (false, "size") => before::make_actor_set_size_method(lua, actor),
        (true, "tween") => make_actor_tween_method(lua, actor, Some("linear")),
        (false, "tween") => before::make_actor_tween_method(lua, actor, Some("linear")),
        _ => unreachable!(),
    }
    .unwrap()
}

fn actor(lua: &Lua) -> Table {
    lua.load("{__songlua_actor_type='Quad'}").eval().unwrap()
}

fn compare_value(left: Value, right: Value) {
    match (left, right) {
        (Value::Table(left), Value::Table(right)) => {
            let entries = left
                .pairs::<Value, Value>()
                .collect::<mlua::Result<Vec<_>>>()
                .unwrap();
            assert_eq!(entries.len(), right.pairs::<Value, Value>().count());
            for (key, value) in entries {
                compare_value(value, right.raw_get(key).unwrap());
            }
        }
        (Value::Number(left), Value::Number(right)) => assert_eq!(left.to_bits(), right.to_bits()),
        (left, right) => assert_eq!(left, right),
    }
}

#[test]
fn numeric_factories_keep_coercion_receiver_rules_state_and_extra_inputs() {
    for kind in ["wrap", "size", "tween"] {
        for expression in [
            "",
            "actor",
            "nil",
            "false",
            "17",
            "17,23",
            "actor,17,23",
            "actor,'17.25','23'",
            "actor,-17,-23",
            "actor,0/0,math.huge",
            "actor,string.char(255,0),23",
            "actor,nil,23",
            "actor,17,nil",
            "actor,17,23,{},function()end,coroutine.create(function()end),newproxy(true),nil",
            "{},17,23",
            "false,17,23",
            "nil,17,23",
        ] {
            let lua = Lua::new();
            let old = actor(&lua);
            let new = actor(&lua);
            lua.globals().set("actor", &old).unwrap();
            let inputs = lua
                .load(format!("return {expression}"))
                .eval::<MultiValue>()
                .unwrap();
            let previous = method(&lua, &old, false, kind)
                .call::<Value>(inputs.clone())
                .unwrap();
            let current = method(&lua, &new, true, kind)
                .call::<Value>(inputs)
                .unwrap();
            assert_eq!(previous, Value::Table(old.clone()));
            assert_eq!(current, Value::Table(new.clone()));
            compare_value(Value::Table(old), Value::Table(new));
        }
    }
}

#[test]
fn numeric_factories_keep_coroutines_checked_extras_and_panic_propagation() {
    for kind in ["wrap", "size", "tween"] {
        for current in [false, true] {
            let lua = Lua::new();
            let actor = actor(&lua);
            lua.globals().set("actor", &actor).unwrap();
            lua.globals()
                .set("method", method(&lua, &actor, current, kind))
                .unwrap();
            lua.load("local ok,value=coroutine.resume(coroutine.create(function() return method(actor,17,23,{}) end)); assert(ok and value==actor)").exec().unwrap();
            for position in [1, 2, 3, 4, 8] {
                lua.globals()
                    .set(
                        "panic_arg",
                        lua.create_function(|_, ()| -> mlua::Result<()> {
                            panic!("numeric callback argument panic")
                        })
                        .unwrap(),
                    )
                    .unwrap();
                let inputs = (1..=8)
                    .map(|i| if i == position { "p" } else { "nil" })
                    .collect::<Vec<_>>()
                    .join(",");
                let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    lua.load(format!(
                        "local _,p=pcall(panic_arg); return method({inputs})"
                    ))
                    .eval::<Value>()
                }))
                .unwrap_err();
                assert_eq!(
                    panic.downcast_ref::<&str>().copied(),
                    Some("numeric callback argument panic")
                );
                assert_eq!(lua.load("return 6*7").eval::<i64>().unwrap(), 42);
            }
        }
    }
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_numeric_factories() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::time::Instant;
    let _affinity = pin_cpu2();
    let variants = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    };
    for kind in ["wrap", "size", "tween"] {
        for (shape, arguments) in [
            ("empty0", ""),
            ("scalar2", "receiver,17"),
            ("size3", "receiver,17,23"),
            ("extra8", "receiver,17,23,{},false,'unused',receiver,nil"),
        ] {
            for current in variants {
                let mut samples = Vec::new();
                for sample in 0..10 {
                    let lua = Lua::new();
                    let receiver = actor(&lua);
                    // Keep tween measurement on its bounded zero-duration path.
                    // Every call still enters the actual tween factory/helper.
                    let arguments = if kind == "tween" {
                        arguments.replace("17", "0")
                    } else {
                        arguments.to_owned()
                    };
                    let runner=lua.load(format!("local set,receiver=...; return function() local count=0; for i=1,8192 do if set({arguments})==receiver then count=count+1 end end; return count end"))
                        .call::<Function>((method(&lua,&receiver,current,kind),receiver)).unwrap();
                    assert_eq!(runner.call::<i64>(()).unwrap(), 8192);
                    lua.gc_stop();
                    let start = Instant::now();
                    let begin = cycles();
                    let (_, churn) =
                        allocations::measure(|| assert_eq!(runner.call::<i64>(()).unwrap(), 8192));
                    let used = cycles() - begin;
                    let nanos = start.elapsed().as_nanos();
                    if sample > 0 {
                        samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used,"units":8192,
                        "allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
                    }
                }
                println!(
                    "DRAW_CAPTURE_METRIC {}",
                    serde_json::json!({"workload":format!("numeric_{kind}_{shape}"),
                    "variant":if current{"after"}else{"before"},"unit":"calls","baseline_commit":"d95fdfe52","samples":samples})
                );
            }
        }
    }
}

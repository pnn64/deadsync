use super::*;
use mlua::Function;

pub(crate) fn argument_cases(lua: &Lua, receiver: &mlua::Table) -> Vec<MultiValue> {
    vec![
        MultiValue::new(),
        MultiValue::from_vec(vec![Value::Table(receiver.clone())]),
        MultiValue::from_vec(vec![Value::Nil]),
        MultiValue::from_vec(vec![
            Value::Table(receiver.clone()),
            Value::String(lua.create_string([255, 0]).unwrap()),
        ]),
        MultiValue::from_vec(vec![
            Value::Table(receiver.clone()),
            Value::Nil,
            Value::Boolean(false),
            Value::Integer(17),
            Value::Number(-0.0),
            Value::Table(lua.create_table().unwrap()),
            Value::Error(Box::new(mlua::Error::runtime("unused argument error"))),
            Value::Nil,
        ]),
    ]
}

#[cfg(windows)]
pub(crate) fn measure_calls(name: &str, prepare: impl Fn(&Lua, bool) -> (Function, mlua::Table)) {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::time::Instant;
    let _affinity = pin_cpu2();
    let variants = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    };
    for (shape, arguments) in [
        ("empty0", ""),
        ("receiver1", "receiver"),
        (
            "extra8",
            "receiver,nil,false,17,'unused',receiver,nil,receiver",
        ),
    ] {
        for current in variants {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let lua = Lua::new();
                let (getter, receiver) = prepare(&lua, current);
                let runner=lua.load(format!("local get,receiver=...; return function() local count=0; for i=1,8192 do if get({arguments})~=nil then count=count+1 end end; return count end"))
                    .call::<Function>((getter,receiver)).unwrap();
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
                serde_json::json!({"workload":format!("{name}_{shape}"),"variant":if current{"after"}else{"before"},
                "unit":"calls","baseline_commit":"1c744eebf","samples":samples})
            );
        }
    }
}

fn reader(lua: &Lua, current: bool) -> Function {
    if current {
        lua.create_function(|_, _args: IgnoredArgs| Ok(17)).unwrap()
    } else {
        lua.create_function(|_, _args: MultiValue| Ok(17)).unwrap()
    }
}

#[test]
fn checked_unused_arguments_match_values_panics_and_reference_pressure() {
    let mut pressure = Vec::new();
    for current in [false, true] {
        let lua = Lua::new();
        let capture = if current {
            lua.create_function(|lua, args: IgnoredArgs| {
                let args = match args {
                    IgnoredArgs::Empty => Vec::new(),
                    IgnoredArgs::One { _value } => vec![_value],
                    IgnoredArgs::Many { _values } => _values.into_vec(),
                };
                let count = args.len();
                Ok((count, lua.create_sequence_from(args)?))
            })
        } else {
            lua.create_function(|lua, args: MultiValue| {
                let count = args.len();
                Ok((count, lua.create_sequence_from(args)?))
            })
        }
        .unwrap();
        lua.globals().set("capture", capture).unwrap();
        lua.load(r#"
            local t, f, co, ud = {}, function() end, coroutine.create(function() end), newproxy(true)
            local args = {nil, false, 17, -0.0, string.char(255,0), t, f, co, ud, nil}
            for n=0,10 do
                local count, values = capture(unpack(args,1,n))
                assert(count==n)
                for i=1,n do assert(rawequal(values[i],args[i])) end
            end
            local ok, n, values = coroutine.resume(coroutine.create(function() return capture(t, nil, ud) end))
            assert(ok and n==3 and values[1]==t and values[2]==nil and values[3]==ud)
        "#).exec().unwrap();
        let target = lua.create_table().unwrap();
        let getter = reader(&lua, current);
        assert_eq!(getter.call::<i64>(&target).unwrap(), 17);
        let mut held = Vec::new();
        for _ in 0..5000 {
            if let Ok(table) = lua.create_table() {
                held.push(table);
            } else {
                break;
            }
        }
        let args = MultiValue::from_vec((0..4000).map(|_| Value::Table(target.clone())).collect());
        pressure.push(getter.call::<i64>(args).map_err(|error| error.to_string()));
        drop(held);
        assert_eq!(getter.call::<i64>(&target).unwrap(), 17);
    }
    assert_eq!(pressure[0], pressure[1]);
    // Lua's default pcall can catch a Rust panic payload. Ignored extra inputs
    // must still be converted so both variants resume that payload identically.
    #[cfg(panic = "unwind")]
    for current in [false, true] {
        let lua = Lua::new();
        lua.globals().set("read", reader(&lua, current)).unwrap();
        lua.globals()
            .set(
                "panic_arg",
                lua.create_function(|_, ()| -> mlua::Result<()> {
                    panic!("ignored callback argument panic")
                })
                .unwrap(),
            )
            .unwrap();
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            lua.load("local _, p=pcall(panic_arg); return read({},p)")
                .eval::<i64>()
        }))
        .unwrap_err();
        assert_eq!(
            panic.downcast_ref::<&str>().copied(),
            Some("ignored callback argument panic")
        );
        assert_eq!(lua.load("return 6*7").eval::<i64>().unwrap(), 42);
    }
    println!(
        "Validated values/order, nils, binary strings, coroutine inputs and reference-pressure parity/recovery (panic tests also run when built with unwinding)."
    );
}

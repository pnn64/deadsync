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



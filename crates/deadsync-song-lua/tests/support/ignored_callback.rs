//! Argument shapes and explicit owned-forwarding baseline for current callbacks.
#![allow(dead_code)]
use super::api;
use deadlib_lua::{Value, Vm};
pub fn argument_cases(vm: &mut Vm, receiver: Value) -> Vec<Vec<Value>> {
    let binary = api::string(vm, [255, 0]);
    let table = api::table(vm);
    // Typed mlua Error values do not exist in this VM. An opaque userdata still
    // checks that ignored extras are accepted without invoking their behavior.
    let error = vm.try_new_userdata(0).unwrap();
    vm.pin(error).unwrap();
    vec![
        vec![],
        vec![receiver],
        vec![Value::Nil],
        vec![receiver, binary],
        vec![
            receiver,
            Value::Nil,
            Value::Boolean(false),
            Value::Number(17.),
            Value::Number(-0.),
            table,
            error,
            Value::Nil,
        ],
    ]
}
pub fn owned_forward(vm: &mut Vm, getter: Value) -> Value {
    let f = vm
        .new_host_function(
            |call| {
                let args = call.arguments().to_vec();
                let target = call.captures()[0];
                call.call_and_return(target, &args)
            },
            &[getter],
        )
        .unwrap();
    vm.pin(f).unwrap();
    f
}
#[cfg(windows)]
pub fn measure_calls(
    name: &str,
    prepare: impl Fn(bool) -> (deadsync_song_lua::SongRuntime, Value, Value),
) {
    use super::{
        allocations,
        bench_clock::{cycles, pin_cpu2},
    };
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
                let (mut host, getter, receiver) = prepare(current);
                let getter = if current {
                    getter
                } else {
                    owned_forward(host.vm_mut(), getter)
                };
                let source = format!(
                    "local get,receiver=...; return function() local count=0; for i=1,8192 do if get({arguments})~=nil then count=count+1 end end; return count end"
                );
                let factory = api::load(host.vm_mut(), source);
                let runner = api::call_one(host.vm_mut(), factory, &[getter, receiver]);
                assert_eq!(api::call_number(host.vm_mut(), runner, &[]), 8192.);
                api::eval(host.vm_mut(), "collectgarbage('stop')");
                let start = Instant::now();
                let begin = cycles();
                let (_, churn) = allocations::measure(|| {
                    assert_eq!(api::call_number(host.vm_mut(), runner, &[]), 8192.)
                });
                let used = cycles() - begin;
                let nanos = start.elapsed().as_nanos();
                if sample > 0 {
                    samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used,"units":8192,"allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
                }
            }
            // Both paths call the current Rust host. The explicit owned-forwarding
            // baseline includes a nested native call; it is not a native-Lua comparison.
            println!(
                "DRAW_CAPTURE_METRIC {}",
                serde_json::json!({"workload":format!("{name}_{shape}"),"variant":if current{"native_borrowed"}else{"native_owned_forward"},"unit":"calls","baseline":"owned_forwarding","samples":samples})
            );
        }
    }
}

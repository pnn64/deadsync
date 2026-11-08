use super::*;
use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
use std::{hint::black_box, time::Instant};

fn measure<Work: FnMut()>(name: &str, current: bool, mut prepare: impl FnMut() -> Work) {
    let mut samples = Vec::new();
    for sample in 0..10 {
        let mut work = prepare();
        let start = Instant::now();
        let begin = cycles();
        let (_, churn) = allocations::measure(|| {
            for _ in 0..32 {
                work();
            }
        });
        let used = cycles() - begin;
        let nanos = start.elapsed().as_nanos();
        if sample > 0 {
            samples.push(
                serde_json::json!({"ns": nanos, "thread_cycles": used, "units": 32,
                "allocations": churn.allocs, "reallocations": churn.reallocs,
                "allocated_bytes": churn.allocated_bytes, "frees": churn.frees}),
            );
        }
    }
    println!(
        "DRAW_CAPTURE_METRIC {}",
        serde_json::json!({"workload": name,
        "variant": if current { "after" } else { "before" }, "unit": "captures",
        "baseline_commit": "82decba51", "samples": samples})
    );
}

#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_message_and_action_inputs() {
    let _affinity = pin_cpu2();
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    };
    for (name, count, touched) in [
        ("action_inputs_empty", 0, false),
        ("action_inputs_one", 1, true),
        ("action_inputs_128_untouched", 128, false),
        ("action_inputs_128_all", 128, true),
    ] {
        for current in order {
            measure(name, current, || {
                let (lua, actors, prepared) = prepare(count, 0, false, false);
                let function = if touched {
                    prepared
                } else {
                    lua.create_function(|_, ()| Ok(())).unwrap()
                };
                let work_lua = lua.clone();
                let work = move || {
                    let result = if current {
                        super::super::capture_overlay_compile_actor_function_action_blocks(
                            &work_lua,
                            &actors,
                            &[],
                            &function,
                            9.0,
                        )
                    } else {
                        before::capture_overlay_compile_actor_function_action_blocks(
                            &work_lua,
                            &actors,
                            &[],
                            &function,
                            9.0,
                        )
                    };
                    black_box(result.unwrap());
                };
                work();
                lua.gc_collect().unwrap();
                lua.gc_stop();
                work
            });
        }
    }
    for (name, count, messages) in [
        ("message_inputs_empty", 0, 0),
        ("message_inputs_one", 1, 1),
        ("message_inputs_32_one", 32, 1),
        ("message_inputs_32_four", 32, 4),
    ] {
        for current in order {
            measure(name, current, || {
                let (lua, mut actors, _) = prepare(count, messages, false, false);
                let work_lua = lua.clone();
                let mut work = move || {
                    let result = if current {
                        super::super::capture_stable_cross_actor_message_commands(
                            &work_lua,
                            &mut actors,
                            |v| {
                                black_box(v);
                            },
                        )
                    } else {
                        before::capture_stable_cross_actor_message_commands(
                            &work_lua,
                            &mut actors,
                            |v| {
                                black_box(v);
                            },
                        )
                    };
                    black_box(result.unwrap());
                    for actor in &mut actors {
                        actor.actor.message_commands.clear();
                    }
                };
                work();
                lua.gc_collect().unwrap();
                lua.gc_stop();
                work
            });
        }
    }
}

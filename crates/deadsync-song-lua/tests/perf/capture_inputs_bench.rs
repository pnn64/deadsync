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
            for _ in 0..64 {
                work();
            }
        });
        let used = cycles() - begin;
        let nanos = start.elapsed().as_nanos();
        if sample > 0 {
            samples.push(
                serde_json::json!({"ns":nanos,"thread_cycles":used,"units":64,
                "allocations":churn.allocs,"reallocations":churn.reallocs,
                "allocated_bytes":churn.allocated_bytes,"frees":churn.frees}),
            );
        }
    }
    println!(
        "DRAW_CAPTURE_METRIC {}",
        serde_json::json!({"workload":name,
        "variant":if current {"after"} else {"before"},"unit":"captures",
        "baseline_commit":"d418006b7","samples":samples})
    );
}

#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_capture_input_borrows() {
    let _affinity = pin_cpu2();
    let order = if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [true, false]
    } else {
        [false, true]
    };
    for (name, mode, count, touched) in [
        ("indexed_empty", 0, 0, 0),
        ("indexed_one", 0, 1, 1),
        ("indexed_128_untouched", 0, 128, 0),
        ("indexed_128_all", 0, 128, 128),
        ("touched_empty", 1, 0, 0),
        ("touched_one", 1, 1, 1),
        ("touched_128_one", 1, 128, 1),
        ("touched_128_all", 1, 128, 128),
        ("action_empty", 2, 0, 0),
        ("action_128_one", 2, 128, 1),
        ("action_128_all", 2, 128, 128),
    ] {
        for current in order {
            measure(name, current, || {
                let (lua, actors, function) = prepare_capture(count, touched, false, false);
                let work_lua = lua.clone();
                let mut work = move || match (current, mode) {
                    (false, 0) => {
                        black_box(
                            before::capture_indexed_actor_function_blocks(
                                &work_lua,
                                &actors,
                                &function,
                                Some(12.0),
                                Some(9.0),
                            )
                            .unwrap(),
                        );
                    }
                    (true, 0) => {
                        black_box(
                            super::super::capture_indexed_actor_function_blocks(
                                &work_lua,
                                &actors,
                                &function,
                                Some(12.0),
                                Some(9.0),
                            )
                            .unwrap(),
                        );
                    }
                    (false, 1) => {
                        black_box(
                            before::capture_touched_blocks(
                                &work_lua,
                                &actors,
                                &function,
                                Some(12.0),
                                Some(9.0),
                            )
                            .unwrap(),
                        );
                    }
                    (true, 1) => {
                        black_box(
                            super::super::capture_touched_blocks(
                                &work_lua,
                                &actors,
                                &function,
                                Some(12.0),
                                Some(9.0),
                            )
                            .unwrap(),
                        );
                    }
                    (false, _) => {
                        black_box(
                            before::capture_function_action_blocks(
                                &work_lua,
                                &actors,
                                &[],
                                &function,
                                9.0,
                            )
                            .unwrap(),
                        );
                    }
                    (true, _) => {
                        black_box(
                            super::super::capture_function_action_blocks(
                                &work_lua,
                                &actors,
                                &[],
                                &function,
                                9.0,
                            )
                            .unwrap(),
                        );
                    }
                };
                work();
                lua.gc_collect().unwrap();
                lua.gc_stop();
                work
            });
        }
    }
    for (name, count, extra_bytes) in [
        ("sound_none", 0, 0),
        ("sound_one", 1, 0),
        ("sound_128", 128, 0),
        ("sound_long_128", 128, 4096),
    ] {
        for current in order {
            measure(name, current, || {
                let (lua, function, _) = prepare_sounds(count, extra_bytes, false);
                let work_lua = lua.clone();
                let mut messages = Vec::new();
                let mut events = Vec::new();
                let mut counter = 0;
                let mut work = move || {
                    let handled = if current {
                        super::super::compile_overlay_compile_actor_function_action::<()>(
                            &work_lua,
                            &mut [],
                            &mut [],
                            &function,
                            4.5,
                            true,
                            &mut counter,
                            &mut messages,
                            &mut events,
                        )
                    } else {
                        before::compile_overlay_compile_actor_function_action::<()>(
                            &work_lua,
                            &mut [],
                            &mut [],
                            &function,
                            4.5,
                            true,
                            &mut counter,
                            &mut messages,
                            &mut events,
                        )
                    }
                    .unwrap();
                    black_box(handled);
                    events.clear();
                    messages.clear();
                };
                work();
                lua.gc_collect().unwrap();
                lua.gc_stop();
                work
            });
        }
    }
}

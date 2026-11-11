use super::*;

#[test]
fn event_rate_summary_preserves_caps_and_numeric_boundaries() {
    for (latest, maximum, expected) in [
        (0, 1, "0 Hz latest / 1 Hz max"),
        (999, 1000, "999 Hz latest / 1000 Hz max"),
        (1000, 999, "1000 Hz latest / 999 Hz max"),
        (1001, 1000, ">1000 Hz latest / 1000 Hz max"),
        (u32::MAX, u32::MAX, ">1000 Hz latest / >1000 Hz max"),
    ] {
        assert_eq!(format_event_rate_summary(latest, maximum), expected);
    }
}

#[cfg(windows)]
fn evaluation_cycles() -> u64 {
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn QueryThreadCycleTime(thread: *mut std::ffi::c_void, cycles: *mut u64) -> i32;
    }
    let mut cycles = 0u64;
    // SAFETY: The current-thread pseudo-handle is valid and `cycles` is live output storage.
    assert_ne!(
        unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut cycles) },
        0
    );
    cycles
}

#[cfg(windows)]
#[test]
#[ignore = "manual release-build Evaluation Snek actor cycle and allocation benchmark"]
fn benchmark_evaluation_snek_rendering() {
    use crate::perf::measure;
    use std::hint::black_box;

    let mut state = State::default();
    state.snek.connected = true;
    let mut actors = Vec::with_capacity(96);
    let title: Arc<str> = Arc::from("Evaluation");
    let instructions: Arc<str> = Arc::from("Press the panel");

    for (label, iterations) in [("pad", 15_000), ("panel", 15_000), ("double", 15_000)] {
        let mut times = [0u64; 7];
        let mut churn = crate::perf::Churn::default();
        for cycles in &mut times {
            let start = evaluation_cycles();
            let (_, counts) = measure(|| {
                for _ in 0..iterations {
                    actors.clear();
                    match label {
                        "pad" => push_evaluation_pad(
                            &mut actors,
                            &state,
                            GameFlag::Dance,
                            PlayerSlot::P1,
                            100.0,
                            200.0,
                            0.75,
                        ),
                        "panel" => push_evaluation_panel(
                            &mut actors,
                            &state,
                            GameFlag::Dance,
                            PlayerSlot::P1,
                            50.0,
                            70.0,
                            1.0,
                            "miso",
                            Arc::clone(&title),
                            "miso",
                            Arc::clone(&instructions),
                        ),
                        _ => {
                            push_evaluation_pad(
                                &mut actors,
                                &state,
                                GameFlag::Dance,
                                PlayerSlot::P1,
                                100.0,
                                200.0,
                                0.75,
                            );
                            push_evaluation_pad(
                                &mut actors,
                                &state,
                                GameFlag::Dance,
                                PlayerSlot::P2,
                                250.0,
                                200.0,
                                0.75,
                            );
                        }
                    }
                    black_box(actors.as_slice());
                }
            });
            *cycles = evaluation_cycles() - start;
            churn = counts;
        }
        times.sort_unstable();
        eprintln!(
            "EVAL_SNEK_BENCH {label} cycles={} iterations={iterations} allocs={} reallocs={} allocated_bytes={}",
            times[3], churn.allocs, churn.reallocs, churn.allocated_bytes
        );
    }
}

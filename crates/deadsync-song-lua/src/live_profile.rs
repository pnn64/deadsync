//! Opt-in phase measurements for headless live-Lua profiling.
//! Compiled only with the `performance` feature; never enabled in normal play.
use std::{cell::Cell, time::Instant};

#[derive(Clone, Copy)]
pub(crate) enum Phase {
    ActorUpdate,
    ActorDraw,
    StateRead,
    VisualPublication,
}

const NAMES: [&str; 4] = [
    "actor_update",
    "actor_draw",
    "state_read",
    "visual_publication",
];

#[derive(Clone, Copy, Default)]
struct Sample {
    calls: u64,
    cycles: u64,
    nanos: u128,
}

thread_local! {
    static TOTALS: Cell<[Sample; 4]> = const { Cell::new([Sample { calls: 0, cycles: 0, nanos: 0 }; 4]) };
}

#[cfg(windows)]
fn cycles() -> u64 {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn QueryThreadCycleTime(thread: *mut std::ffi::c_void, cycles: *mut u64) -> i32;
    }
    let mut value = 0;
    // SAFETY: the pseudo-handle identifies this live thread and the output
    // pointer remains valid for the synchronous call.
    assert_ne!(
        unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut value) },
        0
    );
    value
}

#[cfg(not(windows))]
fn cycles() -> u64 {
    0
}

pub(crate) struct Scope {
    phase: Phase,
    cycles: u64,
    started: Instant,
}

pub(crate) fn scope(phase: Phase) -> Scope {
    Scope {
        phase,
        cycles: cycles(),
        started: Instant::now(),
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        let nanos = self.started.elapsed().as_nanos();
        let used = cycles() - self.cycles;
        TOTALS.with(|totals| {
            let mut values = totals.get();
            let sample = &mut values[self.phase as usize];
            sample.calls += 1;
            sample.cycles += used;
            sample.nanos += nanos;
            totals.set(values);
        });
    }
}

/// Clear this thread's counters outside any measured phase, after warmup.
pub fn reset_live_profile() {
    TOTALS.with(|totals| totals.set([Sample::default(); 4]));
}

/// Read aggregate phase costs. These include timer overhead, exclude allocation
/// accounting, and diagnose costs rather than proving end-to-end speedups.
pub fn live_profile_report() -> serde_json::Value {
    TOTALS.with(|totals| {
        let values = totals.get();
        NAMES
            .into_iter()
            .zip(values)
            .map(|(name, value)| {
                (
                    name,
                    serde_json::json!({"calls":value.calls,
                "thread_cpu_cycles":cfg!(windows).then_some(value.cycles),
                "wall_seconds":value.nanos as f64 / 1e9}),
                )
            })
            .collect::<serde_json::Value>()
    })
}

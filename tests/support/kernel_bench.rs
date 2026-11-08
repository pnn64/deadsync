//! Paired CPU-cycle and throughput measurements; fixture setup stays outside timing.
use std::time::Instant;

#[cfg(windows)]
pub fn cycles() -> u64 {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn QueryThreadCycleTime(thread: *mut std::ffi::c_void, cycles: *mut u64) -> i32;
    }
    let mut cycles = 0;
    // SAFETY: the current-thread pseudo-handle is valid and the output lives through the call.
    assert_ne!(
        unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut cycles) },
        0
    );
    cycles
}

#[cfg(not(windows))]
pub fn cycles() -> u64 {
    0
}

pub fn pin_thread() {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetCurrentThread() -> *mut std::ffi::c_void;
            fn SetThreadAffinityMask(thread: *mut std::ffi::c_void, mask: usize) -> usize;
        }
        // SAFETY: changes only the current benchmark thread's affinity.
        assert_ne!(
            unsafe { SetThreadAffinityMask(GetCurrentThread(), 1 << 2) },
            0
        );
    }
}

#[allow(dead_code)]
pub fn measure(label: &str, iterations: usize, mut work: impl FnMut()) {
    pin_thread();
    let mut samples = Vec::new();
    for round in 0..12 {
        let start_cycles = cycles();
        let start = Instant::now();
        for _ in 0..iterations {
            work();
        }
        let ns = start.elapsed().as_nanos() as f64 / iterations as f64;
        let cpu = (cycles() - start_cycles) as f64 / iterations as f64;
        if round > 0 {
            samples.push((cpu, ns));
        }
    }
    samples.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let (cpu, ns) = samples[5];
    println!("{label}: cycles/op {cpu:.2}; ns/op {ns:.2}");
}

#[allow(dead_code)]
pub fn compare(label: &str, iterations: usize, mut work: impl FnMut(bool)) {
    pin_thread();
    let mut samples = [Vec::new(), Vec::new()];
    for round in 0..12 {
        for variant in [round % 2, 1 - round % 2] {
            let start_cycles = cycles();
            let start = Instant::now();
            for _ in 0..iterations {
                work(std::hint::black_box(variant == 1));
            }
            let ns = start.elapsed().as_nanos() as f64 / iterations as f64;
            let cpu = (cycles() - start_cycles) as f64 / iterations as f64;
            if round > 0 {
                samples[variant].push((cpu, ns));
            }
        }
    }
    for values in &mut samples {
        values.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    }
    let [(old_cycles, old_ns), (new_cycles, new_ns)] = [samples[0][5], samples[1][5]];
    println!(
        "{label}: cycles/op {old_cycles:.2} -> {new_cycles:.2}; ns/op {old_ns:.2} -> {new_ns:.2}; CPU throughput {:.3}x",
        old_cycles / new_cycles
    );
}

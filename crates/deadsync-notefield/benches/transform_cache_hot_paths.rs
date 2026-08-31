use deadsync_notefield::edit_bar_geometry_bench_support::{
    edit_bar_geometry_new, edit_bar_geometry_old,
};
use deadsync_notefield::hold_geometry_bench_support::{
    endpoint_appearance_new, endpoint_appearance_old, endpoint_sample_new, endpoint_sample_old,
    mesh_cap_pose_new, mesh_cap_pose_old, mesh_slice_new, mesh_slice_old, segment_uv_new,
    segment_uv_old, slice_uv_reuse_new, slice_uv_reuse_old, strip_normal_new, strip_normal_old,
};
use deadsync_notefield::lane_invariant_cache_bench_support::{
    bumpy_lane_new, bumpy_lane_old, bumpy_new, bumpy_old, move_new, move_old, tiny_new, tiny_old,
};
use deadsync_notefield::note_metadata_bench_support::{
    beat_fraction_new, beat_fraction_old, part_phase_new, part_phase_old, single_bucket_vivid_new,
    single_bucket_vivid_old, uv_translation_new, uv_translation_old, vivid_wrap_new,
    vivid_wrap_old,
};
use deadsync_notefield::note_projection_bench_support::{
    lane_offset_new, lane_offset_old, random_speed_row_new, random_speed_row_old,
};
use deadsync_notefield::transform_cache_bench_support::{
    appearance_new, appearance_old, blink_only_appearance_new, blink_only_appearance_old,
    boomerang_only_new, boomerang_only_old, boost_brake_new, boost_brake_old, boost_only_new,
    boost_only_old, bounded_dizzy_new, bounded_dizzy_old, brake_only_new, brake_only_old,
    expand_new, expand_old, expand_only_new, expand_only_old, hidden_blink_appearance_new,
    hidden_blink_appearance_old, hidden_only_appearance_new, hidden_only_appearance_old,
    hidden_stealth_appearance_new, hidden_stealth_appearance_old, hidden_sudden_appearance_new,
    hidden_sudden_appearance_old, hidden_sudden_blink_appearance_new,
    hidden_sudden_blink_appearance_old, hidden_sudden_stealth_appearance_new,
    hidden_sudden_stealth_appearance_old, inner_pulse_new, inner_pulse_old, pulse_new, pulse_old,
    rotation_new, rotation_old, stealth_blink_appearance_new, stealth_blink_appearance_old,
    stealth_only_appearance_new, stealth_only_appearance_old, sudden_blink_appearance_new,
    sudden_blink_appearance_old, sudden_only_appearance_new, sudden_only_appearance_old,
    sudden_stealth_appearance_new, sudden_stealth_appearance_old, tornado_new, tornado_old,
    wave_only_new, wave_only_old,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const EVALUATIONS: usize = 8_192;
const OPERATIONS_PER_SAMPLE: usize = 64;
const SAMPLES: usize = 21;
const WARMUPS: usize = 3;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    alloc_bytes: AtomicU64,
    realloc_bytes: AtomicU64,
    free_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            alloc_bytes: AtomicU64::new(0),
            realloc_bytes: AtomicU64::new(0),
            free_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            alloc_bytes: self.alloc_bytes.load(Ordering::Relaxed),
            realloc_bytes: self.realloc_bytes.load(Ordering::Relaxed),
            free_bytes: self.free_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every allocation operation delegates unchanged to `System`; relaxed
// counters only observe successful calls while the single-threaded gate is on.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplied `layout`.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.alloc_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.frees.fetch_add(1, Ordering::Relaxed);
            self.free_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: this pointer-layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.realloc_bytes
                .fetch_add((old.size() + new_size) as u64, Ordering::Relaxed);
        }
        out
    }
}

#[derive(Clone, Copy, Default)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    frees: u64,
    alloc_bytes: u64,
    realloc_bytes: u64,
    free_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            alloc_bytes: self.alloc_bytes - before.alloc_bytes,
            realloc_bytes: self.realloc_bytes - before.realloc_bytes,
            free_bytes: self.free_bytes - before.free_bytes,
        }
    }

    const fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

struct BenchResult {
    median_ns: f64,
    p95_ns: f64,
    median_cycles: Option<f64>,
    evaluations_per_second: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure(mut operation: impl FnMut(usize) -> u64) -> BenchResult {
    for _ in 0..WARMUPS {
        black_box(operation(EVALUATIONS));
    }

    let mut sample_ns = Vec::with_capacity(SAMPLES);
    let mut sample_cycles = Vec::with_capacity(SAMPLES);
    let mut checksum = 0_u64;
    let evaluations_per_sample = (EVALUATIONS * OPERATIONS_PER_SAMPLE) as f64;
    for _ in 0..SAMPLES {
        let cycle_start = cycle_counter();
        let started = Instant::now();
        for _ in 0..OPERATIONS_PER_SAMPLE {
            checksum = checksum.wrapping_add(black_box(operation(EVALUATIONS)));
        }
        let elapsed_ns = started.elapsed().as_secs_f64() * 1e9;
        let cycle_end = cycle_counter();
        sample_ns.push(elapsed_ns / evaluations_per_sample);
        if let Some(cycles) = cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / evaluations_per_sample)
        {
            sample_cycles.push(cycles);
        }
    }
    sample_ns.sort_unstable_by(f64::total_cmp);
    sample_cycles.sort_unstable_by(f64::total_cmp);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let allocation_checksum = black_box(operation(EVALUATIONS));
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);
    black_box(allocation_checksum);

    let median_ns = sample_ns[SAMPLES / 2];
    BenchResult {
        median_ns,
        p95_ns: sample_ns[SAMPLES * 95 / 100],
        median_cycles: (!sample_cycles.is_empty()).then(|| sample_cycles[sample_cycles.len() / 2]),
        evaluations_per_second: 1e9 / median_ns,
        allocated,
        checksum,
    }
}

fn run(title: &str, old_operation: fn(usize) -> u64, new_operation: fn(usize) -> u64) {
    assert_eq!(
        old_operation(EVALUATIONS),
        new_operation(EVALUATIONS),
        "{title} behavior diverged before measurement"
    );
    let old = measure(old_operation);
    let new = measure(new_operation);
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert_zero_alloc(&old);
    assert_zero_alloc(&new);

    println!("\n{title}");
    print_result("old", &old);
    print_result("new", &new);
    println!(
        "  change: {:>7.2}% median  {:>7.2}% cycles  {:>7.2}% throughput  \
         {:>7.2}% p95  {:>7.2}% churn",
        percent_change(old.median_ns, new.median_ns),
        percent_change(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN),
        ),
        percent_change(old.evaluations_per_second, new.evaluations_per_second),
        percent_change(old.p95_ns, new.p95_ns),
        percent_change(
            old.allocated.churn_bytes() as f64,
            new.allocated.churn_bytes() as f64,
        ),
    );

    assert!(
        new.median_ns < old.median_ns,
        "{title} median latency did not improve"
    );
    if let (Some(old_cycles), Some(new_cycles)) = (old.median_cycles, new.median_cycles) {
        assert!(
            new_cycles < old_cycles,
            "{title} median CPU cycles did not improve"
        );
    }
}

fn print_result(label: &str, result: &BenchResult) {
    println!(
        "  {label:<3} {:>9.3} ns/eval  {:>9.3} cycles/eval  {:>9.3} p95 ns  \
         {:>8.2} Meval/s  {:>3} alloc  {:>3} realloc  {:>3} free  {:>6} churn B",
        result.median_ns,
        result.median_cycles.unwrap_or(f64::NAN),
        result.p95_ns,
        result.evaluations_per_second / 1_000_000.0,
        result.allocated.allocs,
        result.allocated.reallocs,
        result.allocated.frees,
        result.allocated.churn_bytes(),
    );
}

fn assert_zero_alloc(result: &BenchResult) {
    assert_eq!(result.allocated.allocs, 0);
    assert_eq!(result.allocated.reallocs, 0);
    assert_eq!(result.allocated.frees, 0);
    assert_eq!(result.allocated.churn_bytes(), 0);
}

fn percent_change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> Option<u64> {
    None
}

fn main() {
    run(
        "notefield Hidden+Blink appearance",
        hidden_blink_appearance_old,
        hidden_blink_appearance_new,
    );
    run(
        "notefield Sudden+Blink appearance",
        sudden_blink_appearance_old,
        sudden_blink_appearance_new,
    );
    run(
        "notefield Hidden+Sudden+Blink appearance",
        hidden_sudden_blink_appearance_old,
        hidden_sudden_blink_appearance_new,
    );
    run(
        "notefield Hidden+Stealth appearance",
        hidden_stealth_appearance_old,
        hidden_stealth_appearance_new,
    );
    run(
        "notefield Sudden+Stealth appearance",
        sudden_stealth_appearance_old,
        sudden_stealth_appearance_new,
    );
    run(
        "notefield Hidden+Sudden+Stealth appearance",
        hidden_sudden_stealth_appearance_old,
        hidden_sudden_stealth_appearance_new,
    );
    run(
        "notefield Blink-only appearance",
        blink_only_appearance_old,
        blink_only_appearance_new,
    );
    run(
        "notefield Hidden+Sudden appearance",
        hidden_sudden_appearance_old,
        hidden_sudden_appearance_new,
    );
    run(
        "notefield Stealth+Blink appearance",
        stealth_blink_appearance_old,
        stealth_blink_appearance_new,
    );
    run(
        "notefield Hidden-only appearance",
        hidden_only_appearance_old,
        hidden_only_appearance_new,
    );
    run(
        "notefield Sudden-only appearance",
        sudden_only_appearance_old,
        sudden_only_appearance_new,
    );
    run(
        "notefield Stealth-only appearance",
        stealth_only_appearance_old,
        stealth_only_appearance_new,
    );
    run(
        "notefield Wave-only acceleration",
        wave_only_old,
        wave_only_new,
    );
    run(
        "notefield Boomerang-only acceleration",
        boomerang_only_old,
        boomerang_only_new,
    );
    run(
        "notefield Boost+Brake acceleration",
        boost_brake_old,
        boost_brake_new,
    );
    run(
        "notefield Boost-only acceleration",
        boost_only_old,
        boost_only_new,
    );
    run(
        "notefield Brake-only acceleration",
        brake_only_old,
        brake_only_new,
    );
    run(
        "notefield Expand-only acceleration",
        expand_only_old,
        expand_only_new,
    );
    run(
        "notefield cached Bumpy lane classification",
        bumpy_lane_old,
        bumpy_lane_new,
    );
    run(
        "notefield inner-only Pulse constant zoom",
        inner_pulse_old,
        inner_pulse_new,
    );
    run(
        "notefield bounded Dizzy rotation",
        bounded_dizzy_old,
        bounded_dizzy_new,
    );
    run(
        "notefield vivid note floor fraction",
        beat_fraction_old,
        beat_fraction_new,
    );
    run(
        "notefield single-bucket vivid phase",
        single_bucket_vivid_old,
        single_bucket_vivid_new,
    );
    run(
        "notefield bounded vivid phase wrap",
        vivid_wrap_old,
        vivid_wrap_new,
    );
    run(
        "notefield hold endpoint appearance reuse",
        endpoint_appearance_old,
        endpoint_appearance_new,
    );
    run(
        "notefield hold segment reciprocal UV",
        segment_uv_old,
        segment_uv_new,
    );
    run(
        "notefield hold slice UV endpoint reuse",
        slice_uv_reuse_old,
        slice_uv_reuse_new,
    );
    run(
        "notefield hold endpoint sample reuse",
        endpoint_sample_old,
        endpoint_sample_new,
    );
    run(
        "notefield hold strip single-division normal",
        strip_normal_old,
        strip_normal_new,
    );
    run(
        "notefield mesh cap pose elision",
        mesh_cap_pose_old,
        mesh_cap_pose_new,
    );
    run(
        "notefield cached note animation phase",
        part_phase_old,
        part_phase_new,
    );
    run(
        "notefield preclassified UV quantization",
        uv_translation_old,
        uv_translation_new,
    );
    run(
        "notefield hold mesh slice pose",
        mesh_slice_old,
        mesh_slice_new,
    );
    run(
        "notefield edit-bar segment geometry",
        edit_bar_geometry_old,
        edit_bar_geometry_new,
    );
    run(
        "notefield RandomSpeed preindexed rows",
        random_speed_row_old,
        random_speed_row_new,
    );
    run(
        "notefield hold lane offset reuse",
        lane_offset_old,
        lane_offset_new,
    );
    run("notefield Expand frame scale", expand_old, expand_new);
    run(
        "notefield appearance fade evaluation",
        appearance_old,
        appearance_new,
    );
    run("notefield Tornado lane angles", tornado_old, tornado_new);
    run("notefield Tiny frame scale", tiny_old, tiny_new);
    run("notefield Bumpy frame geometry", bumpy_old, bumpy_new);
    run("notefield Move column offsets", move_old, move_new);
    run("notefield Pulse frame geometry", pulse_old, pulse_new);
    run(
        "notefield dynamic rotation base",
        rotation_old,
        rotation_new,
    );
}

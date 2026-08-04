use deadsync_gameplay::{
    ActiveComboMilestone, COMBO_MILESTONE_CAPACITY, ComboMilestoneKind,
    queue_pending_missed_hold_resolution, trigger_combo_milestone,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const EVENTS: usize = 50_000;
const HOLD_COUNT: usize = 64;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every allocation operation delegates unchanged to `System`; the
// relaxed counters only observe successful calls while measurement is active.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes from the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: this pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            if new_size > old.size() {
                self.bytes
                    .fetch_add((new_size - old.size()) as u64, Ordering::Relaxed);
            }
        }
        out
    }
}

#[derive(Clone, Copy)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            bytes: self.bytes - before.bytes,
        }
    }
}

struct BenchResult {
    ns_per_event: f64,
    cycles_per_event: Option<f64>,
    allocated: AllocSnapshot,
    checksum: usize,
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

fn measure<T>(
    mut warmup: Vec<T>,
    mut timed: Vec<T>,
    mut allocated: Vec<T>,
    mut event: impl FnMut(&mut T) -> usize,
) -> BenchResult {
    for item in &mut warmup {
        black_box(event(item));
    }

    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0usize;
    for item in &mut timed {
        checksum = checksum.wrapping_add(black_box(event(item)));
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut allocation_checksum = 0usize;
    for item in &mut allocated {
        allocation_checksum = allocation_checksum.wrapping_add(black_box(event(item)));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocation_delta = ALLOC.snapshot().delta(before);
    black_box(allocation_checksum);

    BenchResult {
        ns_per_event: elapsed.as_secs_f64() * 1_000_000_000.0 / EVENTS as f64,
        cycles_per_event: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / EVENTS as f64),
        allocated: allocation_delta,
        checksum,
    }
}

fn measure_combo(prewarmed: bool) -> BenchResult {
    let make = || {
        (0..EVENTS)
            .map(|_| {
                if prewarmed {
                    Vec::with_capacity(COMBO_MILESTONE_CAPACITY)
                } else {
                    Vec::new()
                }
            })
            .collect()
    };
    measure(
        make(),
        make(),
        make(),
        |milestones: &mut Vec<ActiveComboMilestone>| {
            trigger_combo_milestone(milestones, ComboMilestoneKind::Hundred);
            milestones.len()
        },
    )
}

struct PendingHoldCase {
    resolution: [bool; 1],
    indices: Vec<usize>,
}

fn measure_pending_hold(prewarmed: bool) -> BenchResult {
    let make = || {
        (0..EVENTS)
            .map(|_| PendingHoldCase {
                resolution: [false],
                indices: if prewarmed {
                    Vec::with_capacity(HOLD_COUNT)
                } else {
                    Vec::new()
                },
            })
            .collect()
    };
    measure(make(), make(), make(), |case: &mut PendingHoldCase| {
        usize::from(queue_pending_missed_hold_resolution(
            &mut case.resolution,
            &mut case.indices,
            0,
        )) + case.indices.len()
    })
}

fn print_result(label: &str, result: &BenchResult) {
    let events = EVENTS as f64;
    println!(
        "{label:<18} {:>9.2} ns/event  {:>9.2} cycles/event  {:>8.2} Mevent/s  \
         {:>5.2} allocs/event  {:>7.1} bytes/event  {:>5.2} reallocs/event",
        result.ns_per_event,
        result.cycles_per_event.unwrap_or(f64::NAN),
        1_000.0 / result.ns_per_event,
        result.allocated.allocs as f64 / events,
        result.allocated.bytes as f64 / events,
        result.allocated.reallocs as f64 / events,
    );
}

fn main() {
    let cold_combo = measure_combo(false);
    let prewarmed_combo = measure_combo(true);
    let cold_hold = measure_pending_hold(false);
    let prewarmed_hold = measure_pending_hold(true);

    assert_eq!(cold_combo.checksum, prewarmed_combo.checksum);
    assert_eq!(cold_hold.checksum, prewarmed_hold.checksum);

    println!("Gameplay first-event buffer prewarm ({EVENTS} independent songs)");
    println!("combo milestone (bounded to {COMBO_MILESTONE_CAPACITY} live kinds)");
    print_result("cold Vec", &cold_combo);
    print_result("song-prewarmed", &prewarmed_combo);
    println!("pending missed hold ({HOLD_COUNT} chart holds/rolls)");
    print_result("cold Vec", &cold_hold);
    print_result("song-prewarmed", &prewarmed_hold);
}

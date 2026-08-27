use deadsync_song_lua::actor_pointers_touch_actor;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

struct CountingAlloc {
    enabled: AtomicBool,
    calls: AtomicU64,
    frees: AtomicU64,
    churn: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            calls: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            churn: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            calls: self.calls.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            churn: self.churn.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: allocator calls delegate unchanged to `System`; relaxed counters
// only observe successful calls while this single-threaded benchmark is gated.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplied a valid allocation layout.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.churn
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.frees.fetch_add(1, Ordering::Relaxed);
            self.churn
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: this pointer-layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.churn
                .fetch_add((old.size() + new_size) as u64, Ordering::Relaxed);
        }
        out
    }
}

#[derive(Clone, Copy, Default)]
struct AllocSnapshot {
    calls: u64,
    frees: u64,
    churn: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            calls: self.calls - before.calls,
            frees: self.frees - before.frees,
            churn: self.churn - before.churn,
        }
    }
}

struct ResultRow {
    ns: f64,
    cycles: Option<f64>,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn reference(len: usize, pointer_at: impl Fn(usize) -> usize, actor_ptrs: &[usize]) -> bool {
    !actor_ptrs.is_empty() && (0..len).any(|index| actor_ptrs.contains(&pointer_at(index)))
}

fn measure(iterations: usize, mut probe: impl FnMut() -> bool) -> ResultRow {
    for _ in 0..100 {
        black_box(probe());
    }
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iterations {
        checksum += u64::from(black_box(probe()));
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    for _ in 0..iterations {
        black_box(probe());
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    ResultRow {
        ns: elapsed.as_secs_f64() * 1e9 / iterations as f64,
        cycles: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / iterations as f64),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn run(title: &str, actor_ptrs: &[usize], iterations: usize) {
    const ACTORS: usize = 513;
    let old = measure(iterations, || {
        reference(ACTORS, |index| 0x1000 + index * 16, actor_ptrs)
    });
    let new = measure(iterations, || {
        actor_pointers_touch_actor(ACTORS, |index| 0x1000 + index * 16, actor_ptrs)
    });
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert_eq!(
        (old.alloc.calls, old.alloc.frees, old.alloc.churn),
        (0, 0, 0)
    );
    assert_eq!(
        (new.alloc.calls, new.alloc.frees, new.alloc.churn),
        (0, 0, 0)
    );
    println!("\n{title}");
    print_row("old", iterations, &old);
    print_row("new", iterations, &new);
    println!(
        "  change: {:+.2}% latency  {:+.2}% cycles  {:+.2}% throughput  {:+.2}% churn",
        change(old.ns, new.ns),
        change(
            old.cycles.unwrap_or(f64::NAN),
            new.cycles.unwrap_or(f64::NAN)
        ),
        change(1.0 / old.ns, 1.0 / new.ns),
        change(old.alloc.churn as f64, new.alloc.churn as f64),
    );
}

fn print_row(label: &str, iterations: usize, row: &ResultRow) {
    println!(
        "  {label:<3} {:>10.2} ns/probe  {:>10.2} cycles/probe  {:>8.1} probe/s  \
         {:>5.2} alloc/probe  {:>5.2} free/probe  {:>8.1} churn B/probe",
        row.ns,
        row.cycles.unwrap_or(f64::NAN),
        1e9 / row.ns,
        row.alloc.calls as f64 / iterations as f64,
        row.alloc.frees as f64 / iterations as f64,
        row.alloc.churn as f64 / iterations as f64,
    );
}

fn change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return 0.0;
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
    let dense_miss = (0..192)
        .map(|index| 0x1008 + ((index * 73) % 192) * 16)
        .collect::<Vec<_>>();
    run(
        "actor-pointer match (513 actors / 192-pointer miss)",
        &dense_miss,
        20_000,
    );

    let mut late_hit = dense_miss;
    late_hit[191] = 0x1000 + 500 * 16;
    run(
        "actor-pointer match (513 actors / 192-pointer late hit)",
        &late_hit,
        20_000,
    );

    let largest_inline_miss = (0..512)
        .map(|index| 0x1008 + ((index * 73) % 512) * 16)
        .collect::<Vec<_>>();
    run(
        "actor-pointer match (513 actors / 512-pointer miss)",
        &largest_inline_miss,
        10_000,
    );

    let small_hit = [0x1000 + 7 * 16, 0xDEAD, 0xBEEF, 0xCAFE];
    run(
        "actor-pointer match (small early hit)",
        &small_hit,
        2_000_000,
    );
}

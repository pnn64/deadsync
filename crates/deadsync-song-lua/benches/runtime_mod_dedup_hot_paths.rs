use deadsync_song_lua::{
    RuntimeModEaseEntry, RuntimeOverlayCaptureKey, SongLuaTimeUnit,
    collect_unique_runtime_mod_entries, collect_unique_runtime_overlay_capture_keys,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    churn: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            churn: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
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
            self.allocs.fetch_add(1, Ordering::Relaxed);
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
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.churn
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
    churn: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            churn: self.churn - before.churn,
        }
    }
}

fn fixture(count: usize, unique: usize) -> Vec<RuntimeModEaseEntry> {
    (0..count)
        .map(|index| {
            let key = (index * 73) % unique.max(1);
            RuntimeModEaseEntry {
                unit: if key % 7 == 0 {
                    SongLuaTimeUnit::Second
                } else {
                    SongLuaTimeUnit::Beat
                },
                start: key as f32 * 0.125,
                limit: 0.25 + (key % 5) as f32 * 0.0625,
                easing: format!("ease{}", key % 11),
                to: key as f32 * -0.75,
                target: format!("mod{}", key % 37),
                start_val: (key % 3 == 0).then_some(key as f32),
                opt1: (key % 4 == 0).then_some(key as f32 * 0.5),
                opt2: (key % 6 == 0).then_some(key as f32 * -0.25),
                player: Some((key % 2 + 1) as u8),
                add: key % 5 == 0,
            }
        })
        .collect()
}

fn entries_equal(left: &RuntimeModEaseEntry, right: &RuntimeModEaseEntry) -> bool {
    left.unit == right.unit
        && left.start.to_bits() == right.start.to_bits()
        && left.limit.to_bits() == right.limit.to_bits()
        && left.to.to_bits() == right.to.to_bits()
        && left.target == right.target
        && left.easing == right.easing
        && left.start_val.map(f32::to_bits) == right.start_val.map(f32::to_bits)
        && left.opt1.map(f32::to_bits) == right.opt1.map(f32::to_bits)
        && left.opt2.map(f32::to_bits) == right.opt2.map(f32::to_bits)
        && left.player == right.player
        && left.add == right.add
}

fn reference(source: &[RuntimeModEaseEntry]) -> Vec<RuntimeModEaseEntry> {
    let mut entries = Vec::new();
    for entry in source.iter().cloned() {
        if !entries.iter().any(|other| entries_equal(other, &entry)) {
            entries.push(entry);
        }
    }
    entries
}

fn checksum(entries: &[RuntimeModEaseEntry]) -> u64 {
    entries.iter().fold(entries.len() as u64, |sum, entry| {
        let sum = entry.target.bytes().fold(sum, |sum, byte| {
            sum.wrapping_mul(16_777_619).wrapping_add(byte as u64)
        });
        sum.rotate_left(11)
            ^ u64::from(entry.start.to_bits())
            ^ u64::from(entry.to.to_bits()).rotate_left(29)
    })
}

fn overlay_fixture(count: usize, unique: usize) -> Vec<RuntimeOverlayCaptureKey> {
    (0..count)
        .map(|index| {
            let key = (index * 73) % unique.max(1);
            RuntimeOverlayCaptureKey {
                function: 0x1000 + key % 31,
                unit: if key % 7 == 0 {
                    SongLuaTimeUnit::Second
                } else {
                    SongLuaTimeUnit::Beat
                },
                start: (key as f32 * 0.125).to_bits(),
                limit: (0.25 + (key % 5) as f32 * 0.0625).to_bits(),
                easing: format!("ease{}", key % 11),
                target: format!("node{}", key % 37),
                from: (key as f32 * 0.5).to_bits(),
                to: (key as f32 * -0.75).to_bits(),
                opt1: (key % 4 == 0).then(|| (key as f32 * 0.5).to_bits()),
                opt2: (key % 6 == 0).then(|| (key as f32 * -0.25).to_bits()),
            }
        })
        .collect()
}

fn overlay_reference(source: &[RuntimeOverlayCaptureKey]) -> Vec<RuntimeOverlayCaptureKey> {
    let mut keys = Vec::new();
    for key in source.iter().cloned() {
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys
}

fn overlay_checksum(keys: &[RuntimeOverlayCaptureKey]) -> u64 {
    keys.iter().fold(keys.len() as u64, |sum, key| {
        let sum = key.target.bytes().fold(sum, |sum, byte| {
            sum.wrapping_mul(16_777_619).wrapping_add(byte as u64)
        });
        sum.rotate_left(11) ^ key.function as u64 ^ u64::from(key.start).rotate_left(29)
    })
}

struct ResultRow {
    ns: f64,
    cycles: Option<f64>,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn measure(iterations: usize, mut compile: impl FnMut() -> u64) -> ResultRow {
    for _ in 0..10 {
        black_box(compile());
    }
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iterations {
        checksum = checksum.wrapping_add(black_box(compile()));
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    for _ in 0..iterations {
        black_box(compile());
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

fn run(title: &str, count: usize, unique: usize, iterations: usize) {
    let source = fixture(count, unique);
    let old = measure(iterations, || checksum(&reference(&source)));
    let new = measure(iterations, || {
        let entries = collect_unique_runtime_mod_entries(source.iter().cloned(), source.len());
        checksum(&entries)
    });
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert_eq!(new.alloc.reallocs, 0, "{title} unexpectedly reallocated");
    assert!(
        new.alloc.churn <= old.alloc.churn,
        "{title} increased allocation churn"
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

fn run_overlay(title: &str, count: usize, unique: usize, iterations: usize) {
    let source = overlay_fixture(count, unique);
    let old = measure(iterations, || overlay_checksum(&overlay_reference(&source)));
    let new = measure(iterations, || {
        let keys =
            collect_unique_runtime_overlay_capture_keys(source.iter().cloned(), source.len());
        overlay_checksum(&keys)
    });
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert_eq!(new.alloc.reallocs, 0, "{title} unexpectedly reallocated");
    assert!(
        new.alloc.churn <= old.alloc.churn,
        "{title} increased allocation churn"
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
    let runs = iterations as f64;
    println!(
        "  {label:<3} {:>11.2} ns/compile  {:>11.2} cycles/compile  {:>8.1} compile/s  \
         {:>6.2} alloc/compile  {:>5.2} realloc/compile  {:>6.2} free/compile  \
         {:>9.1} churn B/compile",
        row.ns,
        row.cycles.unwrap_or(f64::NAN),
        1e9 / row.ns,
        row.alloc.allocs as f64 / runs,
        row.alloc.reallocs as f64 / runs,
        row.alloc.frees as f64 / runs,
        row.alloc.churn as f64 / runs,
    );
}

fn change(old: f64, new: f64) -> f64 {
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
    run("runtime-mod dedup (192 unique entries)", 192, 192, 2_000);
    run("runtime-mod dedup (1,024 / 256 unique)", 1_024, 256, 500);
    run_overlay("overlay capture dedup (192 unique keys)", 192, 192, 2_000);
    run_overlay(
        "overlay capture dedup (1,024 / 256 unique)",
        1_024,
        256,
        500,
    );
}

use deadsync_gameplay::{SongLuaEase, song_lua_ease_factor_reference};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

const EVALUATIONS: usize = 16_384;
const SAMPLES: usize = 21;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

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

// SAFETY: every request is forwarded unchanged to `System`; relaxed counters
// only observe this single-threaded benchmark while measurement is enabled.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is supplied by the allocator caller.
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

    const fn churn(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

struct Row {
    median_ns: f64,
    p95_ns: f64,
    median_cycles: Option<f64>,
    alloc: AllocSnapshot,
    checksum: f64,
}

fn measure(iterations: usize, mut operation: impl FnMut() -> f64) -> Row {
    for _ in 0..3 {
        black_box(operation());
    }

    let mut times = Vec::with_capacity(SAMPLES);
    let mut cycles = Vec::with_capacity(SAMPLES);
    let mut checksum = 0.0_f64;
    for _ in 0..SAMPLES {
        let cycle_start = cycle_counter();
        let started = Instant::now();
        for _ in 0..iterations {
            checksum += black_box(operation());
        }
        times.push(started.elapsed().as_secs_f64() * 1e9 / iterations as f64);
        if let Some(elapsed) = cycle_start
            .zip(cycle_counter())
            .map(|(start, end)| end.wrapping_sub(start) as f64 / iterations as f64)
        {
            cycles.push(elapsed);
        }
    }
    times.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    black_box(operation());
    ALLOC.enabled.store(false, Ordering::Relaxed);

    Row {
        median_ns: times[SAMPLES / 2],
        p95_ns: times[SAMPLES * 95 / 100],
        median_cycles: (!cycles.is_empty()).then(|| cycles[cycles.len() / 2]),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_pair(title: &str, units: usize, old: &Row, new: &Row) {
    let checksum_scale = old.checksum.abs().max(new.checksum.abs()).max(1.0);
    assert!(
        (old.checksum - new.checksum).abs() <= checksum_scale * 2.0e-6,
        "{title} behavior diverged: old={}, new={}",
        old.checksum,
        new.checksum
    );
    assert_eq!(new.alloc.allocs, 0, "{title} new path allocated");
    assert_eq!(new.alloc.reallocs, 0, "{title} new path reallocated");
    assert_eq!(new.alloc.frees, 0, "{title} new path freed allocations");
    assert_eq!(new.alloc.churn(), 0, "{title} new path churned bytes");
    assert!(
        new.median_ns < old.median_ns,
        "{title} median runtime did not improve"
    );
    println!("\n{title} ({units} evaluations/op)");
    print_row("old", units, old);
    print_row("new", units, new);
    println!(
        "  change: {:>7.2}% median  {:>7.2}% cycles  {:>7.2}% throughput  \
         {:>7.2}% p95  {:>7.2}% allocs  {:>7.2}% reallocs  {:>7.2}% churn",
        change(old.median_ns, new.median_ns),
        change(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN),
        ),
        change(throughput(old, units), throughput(new, units)),
        change(old.p95_ns, new.p95_ns),
        change(old.alloc.allocs as f64, new.alloc.allocs as f64),
        change(old.alloc.reallocs as f64, new.alloc.reallocs as f64),
        change(old.alloc.churn() as f64, new.alloc.churn() as f64),
    );
}

fn print_row(label: &str, units: usize, row: &Row) {
    println!(
        "  {label:<3} {:>11.0} ns/op  {:>11.0} cycles/op  {:>11.0} p95 ns  \
         {:>8.2} Meval/s  {:>5} alloc  {:>5} realloc  {:>5} free  {:>10} churn B",
        row.median_ns,
        row.median_cycles.unwrap_or(f64::NAN),
        row.p95_ns,
        throughput(row, units) / 1e6,
        row.alloc.allocs,
        row.alloc.reallocs,
        row.alloc.frees,
        row.alloc.churn(),
    );
}

fn throughput(row: &Row, units: usize) -> f64 {
    units as f64 * 1e9 / row.median_ns
}

fn change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

const POLYNOMIALS: [SongLuaEase; 16] = [
    SongLuaEase::InQuad,
    SongLuaEase::OutQuad,
    SongLuaEase::InOutQuad,
    SongLuaEase::OutInQuad,
    SongLuaEase::InCubic,
    SongLuaEase::OutCubic,
    SongLuaEase::InOutCubic,
    SongLuaEase::OutInCubic,
    SongLuaEase::InQuart,
    SongLuaEase::OutQuart,
    SongLuaEase::InOutQuart,
    SongLuaEase::OutInQuart,
    SongLuaEase::InQuint,
    SongLuaEase::OutQuint,
    SongLuaEase::InOutQuint,
    SongLuaEase::OutInQuint,
];

fn polynomial_checksum(old: bool) -> f64 {
    let mut checksum = 0.0_f64;
    for index in 0..EVALUATIONS {
        let easing = black_box(POLYNOMIALS[index & (POLYNOMIALS.len() - 1)]);
        let t = black_box((index % 4_097) as f32 / 4_096.0);
        let value = if old {
            song_lua_ease_factor_reference(easing, t, None, None)
        } else {
            easing.factor(t, None, None)
        };
        checksum += f64::from(value) * ((index & 7) + 1) as f64;
    }
    checksum
}

const ELASTIC: [SongLuaEase; 3] = [
    SongLuaEase::InElastic,
    SongLuaEase::OutElastic,
    SongLuaEase::InOutElastic,
];

fn elastic_checksum(old: bool) -> f64 {
    let mut checksum = 0.0_f64;
    for index in 0..EVALUATIONS {
        let easing = black_box(ELASTIC[index % ELASTIC.len()]);
        let t = black_box((index % 4_097) as f32 / 4_096.0);
        let value = if old {
            song_lua_ease_factor_reference(easing, t, None, None)
        } else {
            easing.factor(t, None, None)
        };
        checksum += f64::from(value) * ((index & 7) + 1) as f64;
    }
    checksum
}

fn composite_checksum(old: bool) -> f64 {
    let mut checksum = 0.0_f64;
    for index in 0..EVALUATIONS {
        let easing = if index & 1 == 0 {
            SongLuaEase::OutInElastic
        } else {
            SongLuaEase::OutInBack
        };
        let opt1 = if index & 1 == 0 {
            Some(0.55)
        } else {
            Some(1.25)
        };
        let t = black_box((index % 4_097) as f32 / 4_096.0);
        let value = if old {
            song_lua_ease_factor_reference(black_box(easing), t, opt1, None)
        } else {
            black_box(easing).factor(t, opt1, None)
        };
        checksum += f64::from(value) * ((index & 7) + 1) as f64;
    }
    checksum
}

fn main() {
    print_pair(
        "Song-Lua fixed-degree polynomial easing",
        EVALUATIONS,
        &measure(32, || polynomial_checksum(true)),
        &measure(32, || polynomial_checksum(false)),
    );
    print_pair(
        "Song-Lua default elastic parameters",
        EVALUATIONS,
        &measure(24, || elastic_checksum(true)),
        &measure(24, || elastic_checksum(false)),
    );
    print_pair(
        "Song-Lua out-in composite dispatch",
        EVALUATIONS,
        &measure(24, || composite_checksum(true)),
        &measure(24, || composite_checksum(false)),
    );
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: LFENCE/RDTSC serialize and read the timestamp counter only.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: LFENCE/RDTSC serialize and read the timestamp counter only.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> Option<u64> {
    None
}

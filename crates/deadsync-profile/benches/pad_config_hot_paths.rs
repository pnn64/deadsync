use deadsync_profile::pad_config::{
    PadConfigProfile, parse, parse_owned_for_bench, serialize, serialize_joined_defaults_for_bench,
    serialize_unreserved_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 21;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    allocated_bytes: AtomicU64,
    freed_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            allocated_bytes: AtomicU64::new(0),
            freed_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            allocated_bytes: self.allocated_bytes.load(Ordering::Relaxed),
            freed_bytes: self.freed_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every operation delegates unchanged to `System`; relaxed counters
// only observe this single-threaded benchmark while their gate is enabled.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes from the allocator caller.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.allocated_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.frees.fetch_add(1, Ordering::Relaxed);
            self.freed_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: this pointer-layout pair came from the delegated allocator.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let output = unsafe { System.realloc(pointer, old, new_size) };
        if !output.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            if new_size > old.size() {
                self.allocated_bytes
                    .fetch_add((new_size - old.size()) as u64, Ordering::Relaxed);
            } else {
                self.freed_bytes
                    .fetch_add((old.size() - new_size) as u64, Ordering::Relaxed);
            }
        }
        output
    }
}

#[derive(Clone, Copy)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    frees: u64,
    allocated_bytes: u64,
    freed_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            allocated_bytes: self.allocated_bytes - before.allocated_bytes,
            freed_bytes: self.freed_bytes - before.freed_bytes,
        }
    }

    const fn churn_bytes(self) -> u64 {
        self.allocated_bytes + self.freed_bytes
    }
}

struct BenchResult {
    median_ns: f64,
    p95_ns: f64,
    median_cycles: Option<f64>,
    allocated: AllocSnapshot,
    checksum: u64,
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

fn hash_text(mut hash: u64, value: &str) -> u64 {
    for byte in value.bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash ^ value.len() as u64
}

fn profiles_checksum(profiles: &[PadConfigProfile]) -> u64 {
    profiles
        .iter()
        .fold(profiles.len() as u64, |mut hash, profile| {
            hash = hash_text(hash, &profile.name);
            hash = hash_text(hash, &profile.backend);
            hash = hash_text(hash, profile.pad_type.as_deref().unwrap_or(""));
            hash = hash_text(hash, profile.serial.as_deref().unwrap_or(""));
            for serial in &profile.default_for_serials {
                hash = hash_text(hash.rotate_left(5), serial);
            }
            for (key, value) in &profile.settings {
                hash = hash_text(hash.rotate_left(7), key);
                hash = hash_text(hash, value);
            }
            hash ^ u64::from(profile.global_default)
        })
}

fn string_fixture() -> Vec<PadConfigProfile> {
    (0..48)
        .map(|index| PadConfigProfile {
            name: format!("Pad Profile {index:02}"),
            backend: if index % 3 == 0 { "fsrio" } else { "smx" }.to_owned(),
            pad_type: Some(if index % 2 == 0 { "fsr" } else { "loadcell" }.to_owned()),
            serial: Some(format!("PAD-{index:04}")),
            default_for_serials: (0..4)
                .map(|serial| format!("USB-{index:02}-{serial:02}"))
                .collect(),
            global_default: index == 0,
            settings: (0..12)
                .map(|setting| {
                    (
                        format!("Panel{}.Setting{setting:02}", setting % 4),
                        format!("{} {} {} {}", index + setting, index + 2, setting + 3, 255),
                    )
                })
                .collect(),
        })
        .collect()
}

fn run_serialize(
    profiles: &[PadConfigProfile],
    repeats: usize,
    mut operation: impl FnMut(&[PadConfigProfile]) -> String,
) -> u64 {
    let mut checksum = 0_u64;
    for _ in 0..repeats {
        let content = operation(black_box(profiles));
        checksum = hash_text(checksum.rotate_left(9), black_box(&content));
    }
    checksum
}

fn run_parse(
    content: &str,
    repeats: usize,
    mut operation: impl FnMut(&str) -> Vec<PadConfigProfile>,
) -> u64 {
    let mut checksum = 0_u64;
    for _ in 0..repeats {
        let profiles = operation(black_box(content));
        checksum = checksum
            .rotate_left(11)
            .wrapping_add(profiles_checksum(black_box(&profiles)));
    }
    checksum
}

fn percentile(sorted: &[f64], percentile: usize) -> f64 {
    let index = (sorted.len() * percentile).div_ceil(100).saturating_sub(1);
    sorted[index.min(sorted.len() - 1)]
}

fn run_timed(operation: &mut impl FnMut() -> u64) -> (f64, Option<f64>, u64) {
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let checksum = black_box(operation());
    let elapsed_ns = started.elapsed().as_secs_f64() * 1_000_000_000.0;
    let elapsed_cycles = cycle_start
        .zip(cycle_counter())
        .map(|(start, end)| end.wrapping_sub(start) as f64);
    (elapsed_ns, elapsed_cycles, checksum)
}

fn measure(mut operation: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..4 {
        black_box(operation());
    }
    let mut times = Vec::with_capacity(SAMPLES);
    let mut cycles = Vec::with_capacity(SAMPLES);
    let mut checksum = 0_u64;
    for _ in 0..SAMPLES {
        let (elapsed, elapsed_cycles, value) = run_timed(&mut operation);
        times.push(elapsed);
        cycles.extend(elapsed_cycles);
        checksum ^= value;
    }
    times.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let allocation_checksum = black_box(operation());
    ALLOC.enabled.store(false, Ordering::Relaxed);

    BenchResult {
        median_ns: percentile(&times, 50),
        p95_ns: percentile(&times, 95),
        median_cycles: (!cycles.is_empty()).then(|| percentile(&cycles, 50)),
        allocated: ALLOC.snapshot().delta(before),
        checksum: checksum ^ allocation_checksum,
    }
}

fn reduction(old: u64, new: u64) -> f64 {
    if old == 0 {
        0.0
    } else {
        100.0 * (1.0 - new as f64 / old as f64)
    }
}

fn print_result(label: &str, result: &BenchResult, calls: usize) {
    println!(
        "{label:<4} {:>10.1} ns median  {:>10.1} ns p95  {:>10.1} cycles  \
         {:>8.2} Kops/s  {:>6} alloc  {:>5} realloc  {:>6} free  \
         {:>10} B alloc  {:>10} B churn",
        result.median_ns / calls as f64,
        result.p95_ns / calls as f64,
        result.median_cycles.unwrap_or(f64::NAN) / calls as f64,
        calls as f64 * 1_000_000.0 / result.median_ns,
        result.allocated.allocs,
        result.allocated.reallocs,
        result.allocated.frees,
        result.allocated.allocated_bytes,
        result.allocated.churn_bytes(),
    );
    println!("     allocation counters cover {calls} calls");
}

fn benchmark_pair(
    title: &str,
    calls: usize,
    mut old_operation: impl FnMut() -> u64,
    mut new_operation: impl FnMut() -> u64,
) {
    assert_eq!(
        old_operation(),
        new_operation(),
        "{title}: behavior diverged"
    );
    let old = measure(&mut old_operation);
    let new = measure(&mut new_operation);
    assert_eq!(old.checksum, new.checksum, "{title}: checksum diverged");

    println!("\n{title}");
    print_result("old", &old, calls);
    print_result("new", &new, calls);
    println!(
        "gain {:>7.2}x throughput  {:>7.2}% median  {:>7.2}% p95  \
         {:>7.2}% cycles  {:>7.2}% allocs  {:>7.2}% bytes  {:>7.2}% churn",
        old.median_ns / new.median_ns,
        100.0 * (1.0 - new.median_ns / old.median_ns),
        100.0 * (1.0 - new.p95_ns / old.p95_ns),
        100.0
            * (1.0 - new.median_cycles.unwrap_or(f64::NAN) / old.median_cycles.unwrap_or(f64::NAN)),
        reduction(old.allocated.allocs, new.allocated.allocs),
        reduction(old.allocated.allocated_bytes, new.allocated.allocated_bytes,),
        reduction(old.allocated.churn_bytes(), new.allocated.churn_bytes()),
    );

    assert!(new.median_ns < old.median_ns, "{title}: median regressed");
    if let (Some(old_cycles), Some(new_cycles)) = (old.median_cycles, new.median_cycles) {
        assert!(new_cycles < old_cycles, "{title}: cycles regressed");
    }
    assert!(
        new.allocated.allocated_bytes < old.allocated.allocated_bytes,
        "{title}: allocated bytes did not improve"
    );
    assert!(
        new.allocated.churn_bytes() < old.allocated.churn_bytes(),
        "{title}: allocation churn did not improve"
    );
}

fn main() {
    let profiles = string_fixture();
    let content = serialize(&profiles);

    println!("pad config hot paths ({SAMPLES} samples)");
    benchmark_pair(
        "serialization: formatted growth vs exact direct assembly",
        64,
        || run_serialize(&profiles, 64, serialize_unreserved_for_bench),
        || run_serialize(&profiles, 64, serialize),
    );
    benchmark_pair(
        "default serials: joined temporary vs direct output",
        64,
        || run_serialize(&profiles, 64, serialize_joined_defaults_for_bench),
        || run_serialize(&profiles, 64, serialize),
    );
    benchmark_pair(
        "parsing: eager owned fields vs delayed ownership",
        32,
        || run_parse(&content, 32, parse_owned_for_bench),
        || run_parse(&content, 32, parse),
    );
}

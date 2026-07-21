use deadsync_profile::runtime_local_score_profile_sources;
use deadsync_score::{
    CachedScore, Grade, runtime_ensure_machine_local_score_cache_loaded,
    runtime_machine_record_local, runtime_machine_record_local_lazy,
    runtime_update_machine_cache_if_loaded,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const PROFILES: usize = 64;
const WARMUP_LOOKUPS: usize = 32;
const MEASURE_LOOKUPS: usize = 1_024;
const CHART_HASH: &str = "select-music-machine-record-benchmark";

struct CountingAlloc {
    allocs: AtomicU64,
    reallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
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

// SAFETY: allocation operations are forwarded unchanged to `System`; the
// independent atomics only observe successful allocation operations.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is forwarded unchanged from the allocator caller.
        let output = unsafe { System.alloc(layout) };
        if !output.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        output
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller supplies the allocation's original layout.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees `ptr` and `old` identify a live allocation.
        let output = unsafe { System.realloc(ptr, old, new_size) };
        if !output.is_null() {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.bytes.fetch_add(
                new_size.saturating_sub(old.size()) as u64,
                Ordering::Relaxed,
            );
        }
        output
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
    elapsed: Duration,
    cycles: u64,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn main() {
    let fixture = ProfileFixture::new();
    let profiles = scan_profiles(fixture.root());
    let load = runtime_ensure_machine_local_score_cache_loaded(&profiles);
    assert!(
        load.load_report.is_some(),
        "benchmark must perform initial load"
    );

    let expected = CachedScore {
        grade: Grade::Tier03,
        score_percent: 0.9876,
        lamp_index: Some(4),
        lamp_judge_count: Some(7),
    };
    runtime_update_machine_cache_if_loaded(CHART_HASH, expected, "CPU");

    let eager = measure(|| eager_lookup(fixture.root()));
    let lazy = measure(|| lazy_lookup(fixture.root()));
    let expected_checksum = record_checksum(Some(("CPU".to_owned(), expected)));
    assert_eq!(
        record_checksum(eager_lookup(fixture.root())),
        expected_checksum,
        "eager lookup changed the cached machine record"
    );
    assert_eq!(
        record_checksum(lazy_lookup(fixture.root())),
        expected_checksum,
        "lazy lookup changed the cached machine record"
    );
    assert_eq!(eager.checksum, lazy.checksum, "old/new output mismatch");

    println!(
        "Select Music machine-record steady state ({PROFILES} profiles x {MEASURE_LOOKUPS} lookups)"
    );
    print_result("before: eager profile scan", &eager);
    print_result("after: lazy cached lookup", &lazy);
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | allocation-op reduction {:.1}% | byte reduction {:.1}%",
        eager.elapsed.as_secs_f64() / lazy.elapsed.as_secs_f64(),
        reduction(eager.cycles, lazy.cycles),
        reduction(
            eager.alloc.allocs + eager.alloc.reallocs,
            lazy.alloc.allocs + lazy.alloc.reallocs
        ),
        reduction(eager.alloc.bytes, lazy.alloc.bytes),
    );
}

fn scan_profiles(root: &Path) -> Vec<deadsync_score::LocalScoreProfileSource> {
    runtime_local_score_profile_sources(root, |_, _, _, _| {})
}

fn eager_lookup(root: &Path) -> Option<(String, CachedScore)> {
    let profiles = scan_profiles(root);
    runtime_machine_record_local(CHART_HASH, &profiles).0
}

fn lazy_lookup(root: &Path) -> Option<(String, CachedScore)> {
    runtime_machine_record_local_lazy(CHART_HASH, || scan_profiles(root)).0
}

fn measure(mut lookup: impl FnMut() -> Option<(String, CachedScore)>) -> BenchResult {
    for _ in 0..WARMUP_LOOKUPS {
        black_box(record_checksum(lookup()));
    }

    let before_alloc = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for lookup_index in 0..MEASURE_LOOKUPS {
        checksum =
            checksum.rotate_left(7) ^ black_box(record_checksum(lookup())) ^ lookup_index as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        alloc: ALLOC.snapshot().delta(before_alloc),
        checksum,
    }
}

fn record_checksum(record: Option<(String, CachedScore)>) -> u64 {
    record.map_or(0, |(initials, score)| {
        initials.len() as u64
            ^ ((score.grade as u64) << 8)
            ^ (score.score_percent.to_bits().rotate_left(17))
            ^ ((score.lamp_index.unwrap_or_default() as u64) << 40)
            ^ ((score.lamp_judge_count.unwrap_or_default() as u64) << 48)
    })
}

fn print_result(label: &str, result: &BenchResult) {
    let lookups = MEASURE_LOOKUPS as f64;
    println!(
        "  {label:<28} {:>10.1} us/lookup {:>11.0} cycles/lookup {:>9.1} lookups/s",
        result.elapsed.as_secs_f64() * 1.0e6 / lookups,
        result.cycles as f64 / lookups,
        lookups / result.elapsed.as_secs_f64(),
    );
    println!(
        "  {:<28} allocs={:.1} reallocs={:.1} bytes={:.0} per lookup",
        "memory",
        result.alloc.allocs as f64 / lookups,
        result.alloc.reallocs as f64 / lookups,
        result.alloc.bytes as f64 / lookups,
    );
}

fn reduction(old: u64, new: u64) -> f64 {
    if old == 0 {
        0.0
    } else {
        100.0 * (1.0 - new as f64 / old as f64)
    }
}

struct ProfileFixture {
    root: PathBuf,
}

impl ProfileFixture {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must follow the Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "deadsync-machine-record-bench-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create benchmark profile root");

        for index in 0..PROFILES {
            let profile_dir = root.join(format!("Player {index:02}"));
            fs::create_dir_all(profile_dir.join("scores").join("local"))
                .expect("create benchmark profile directory");
            let guid = format!("00000000-0000-4000-8000-{index:012x}");
            let initials = format!("P{:02}", index % 100);
            let ini = format!(
                "[UserProfile]\nGuid={guid}\nDisplayName=Player {index:02}\nPlayerInitials={initials}\n"
            );
            fs::write(profile_dir.join("profile.ini"), ini).expect("write benchmark profile.ini");
            fs::write(profile_dir.join("avatar.png"), b"benchmark-avatar")
                .expect("write benchmark avatar");
        }

        Self { root }
    }

    fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for ProfileFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: fences and timestamp reads only serialize measurement.
    unsafe {
        core::arch::x86_64::_mm_lfence();
        let cycles = core::arch::x86_64::_rdtsc();
        core::arch::x86_64::_mm_lfence();
        cycles
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn read_cycles() -> u64 {
    0
}

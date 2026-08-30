use deadsync_chart::{SongData, SongPack, SyncPref};
use deadsync_import::resolver::bench_support;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const PACKS: usize = 32;
const SONGS_PER_PACK: usize = 128;
const ITERATIONS: usize = 90;
const SAMPLES: usize = 30;

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

// SAFETY: allocator requests are delegated unchanged to `System`; relaxed
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
    ns_per_op: f64,
    p95_ns: f64,
    cycles_per_op: Option<f64>,
    items_per_second: f64,
    allocated: AllocSnapshot,
    allocation_runs: usize,
    checksum: u64,
}

fn measure(items_per_op: usize, mut operation: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..3 {
        black_box(operation());
    }

    let batch = (ITERATIONS / SAMPLES).max(1);
    let mut sample_ns = Vec::with_capacity(SAMPLES);
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..SAMPLES {
        let sample_started = Instant::now();
        for _ in 0..batch {
            checksum = checksum.wrapping_add(black_box(operation()));
        }
        sample_ns.push(sample_started.elapsed().as_secs_f64() * 1e9 / batch as f64);
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();
    sample_ns.sort_unstable_by(f64::total_cmp);

    let allocation_runs = ITERATIONS / 15;
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut allocation_checksum = 0u64;
    for _ in 0..allocation_runs {
        allocation_checksum = allocation_checksum.wrapping_add(black_box(operation()));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);
    black_box(allocation_checksum);

    let measured_runs = batch * SAMPLES;
    let seconds = elapsed.as_secs_f64();
    BenchResult {
        ns_per_op: seconds * 1e9 / measured_runs as f64,
        p95_ns: sample_ns[(sample_ns.len() * 95 / 100).min(sample_ns.len() - 1)],
        cycles_per_op: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / measured_runs as f64),
        items_per_second: measured_runs as f64 * items_per_op as f64 / seconds,
        allocated,
        allocation_runs,
        checksum,
    }
}

fn print_pair(title: &str, old: &BenchResult, new: &BenchResult) {
    println!("\n{title}");
    print_result("old", old);
    print_result("new", new);
    println!(
        "  change: {:>7.2}% latency  {:>7.2}% cycles  {:>7.2}% throughput  \
         {:>7.2}% p95  {:>7.2}% churn",
        percent_change(old.ns_per_op, new.ns_per_op),
        percent_change(
            old.cycles_per_op.unwrap_or(f64::NAN),
            new.cycles_per_op.unwrap_or(f64::NAN),
        ),
        percent_change(old.items_per_second, new.items_per_second),
        percent_change(old.p95_ns, new.p95_ns),
        percent_change(
            old.allocated.churn_bytes() as f64,
            new.allocated.churn_bytes() as f64,
        ),
    );

    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert!(new.ns_per_op < old.ns_per_op, "{title} latency regressed");
    assert!(
        new.items_per_second > old.items_per_second,
        "{title} throughput regressed"
    );
    if let (Some(old_cycles), Some(new_cycles)) = (old.cycles_per_op, new.cycles_per_op) {
        assert!(new_cycles < old_cycles, "{title} CPU cycles regressed");
    }
    assert!(
        new.allocated.allocs < old.allocated.allocs,
        "{title} allocation count did not improve"
    );
    assert!(
        new.allocated.frees < old.allocated.frees,
        "{title} free count did not improve"
    );
    assert!(
        new.allocated.churn_bytes() < old.allocated.churn_bytes(),
        "{title} memory churn did not improve"
    );
}

fn print_result(label: &str, result: &BenchResult) {
    let runs = result.allocation_runs as f64;
    println!(
        "  {label:<3} {:>11.2} ns/op  {:>11.2} cycles/op  {:>11.2} p95 ns  \
         {:>7.2} Mentry/s  {:>8.1} alloc/op  {:>6.1} realloc/op  \
         {:>8.1} free/op  {:>11.1} churn B/op",
        result.ns_per_op,
        result.cycles_per_op.unwrap_or(f64::NAN),
        result.p95_ns,
        result.items_per_second / 1e6,
        result.allocated.allocs as f64 / runs,
        result.allocated.reallocs as f64 / runs,
        result.allocated.frees as f64 / runs,
        result.allocated.churn_bytes() as f64 / runs,
    );
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

fn song(pack_index: usize, song_index: usize) -> Arc<SongData> {
    Arc::new(SongData {
        simfile_path: PathBuf::from(format!(
            "Songs/Folder Pack {pack_index:02}/Song Folder {song_index:04}/chart.ssc"
        )),
        title: format!("Benchmark Song {pack_index:02}-{song_index:04}"),
        subtitle: String::new(),
        translit_title: String::new(),
        translit_subtitle: String::new(),
        artist: String::new(),
        translit_artist: String::new(),
        genre: String::new(),
        banner_path: None,
        background_path: None,
        background_changes: Vec::new(),
        background_layer2_changes: Vec::new(),
        foreground_changes: Vec::new(),
        background_lua_changes: Vec::new(),
        foreground_lua_changes: Vec::new(),
        has_lua: false,
        cdtitle_path: None,
        music_path: None,
        display_bpm: String::new(),
        offset: 0.0,
        sample_start: None,
        sample_length: None,
        min_bpm: 120.0,
        max_bpm: 120.0,
        normalized_bpms: String::new(),
        music_length_seconds: 0.0,
        first_second: 0.0,
        total_length_seconds: 0,
        precise_last_second_seconds: 0.0,
        charts: Vec::new(),
    })
}

fn packs() -> Vec<SongPack> {
    (0..PACKS)
        .map(|pack_index| SongPack {
            group_name: format!("Display Pack {pack_index:02}"),
            name: format!("Display Pack {pack_index:02}"),
            sort_title: format!("Display Pack {pack_index:02}"),
            translit_title: String::new(),
            series: String::new(),
            folder_series: String::new(),
            year: 0,
            sync_pref: SyncPref::Default,
            directory: PathBuf::from(format!("Songs/Folder Pack {pack_index:02}")),
            banner_path: None,
            songs: (0..SONGS_PER_PACK)
                .map(|song_index| song(pack_index, song_index))
                .collect(),
        })
        .collect()
}

fn main() {
    let packs = packs();
    let entries = bench_support::entry_count(&packs);
    assert_eq!(entries, PACKS * SONGS_PER_PACK * 2);
    println!(
        "resolver key storage: {} -> {} -> {} bytes ({:.2}% smaller overall)",
        bench_support::owned_key_bytes(),
        bench_support::borrowed_pack_key_bytes(),
        bench_support::borrowed_key_bytes(),
        (1.0 - bench_support::borrowed_key_bytes() as f64
            / bench_support::owned_key_bytes() as f64)
            * 100.0
    );

    print_pair(
        "exact resolver map capacity",
        &measure(entries, || {
            bench_support::build_unreserved_owned(black_box(&packs))
        }),
        &measure(entries, || {
            bench_support::build_reserved_owned(black_box(&packs))
        }),
    );
    print_pair(
        "borrowed immutable pack aliases",
        &measure(entries, || {
            bench_support::build_reserved_owned(black_box(&packs))
        }),
        &measure(entries, || {
            bench_support::build_reserved_borrowed_pack(black_box(&packs))
        }),
    );
    print_pair(
        "borrowed immutable song folders",
        &measure(entries, || {
            bench_support::build_reserved_borrowed_pack(black_box(&packs))
        }),
        &measure(entries, || {
            bench_support::build_reserved_borrowed_all(black_box(&packs))
        }),
    );
}

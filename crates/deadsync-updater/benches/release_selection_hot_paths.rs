use deadsync_updater::bench_support;
use deadsync_updater::{HostTarget, ReleaseAsset, ReleaseInfo, expected_asset_name};
use semver::Version;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

const SAMPLES: usize = 21;
const WINDOWS_X64: HostTarget = HostTarget {
    arch: "x86_64",
    os: "windows",
    ext: "zip",
};

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

// SAFETY: requests are delegated unchanged to `System`; relaxed counters only
// observe this single-threaded benchmark while measurement is enabled.
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
    checksum: u64,
}

fn measure(operations: usize, mut op: impl FnMut() -> u64) -> Row {
    for _ in 0..operations.min(4_096) {
        black_box(op());
    }

    let mut times = Vec::with_capacity(SAMPLES);
    let mut cycles = Vec::with_capacity(SAMPLES);
    let mut checksum = 0_u64;
    for _ in 0..SAMPLES {
        let cycle_start = cycle_counter();
        let started = Instant::now();
        for _ in 0..operations {
            checksum = checksum.wrapping_add(black_box(op()));
        }
        times.push(started.elapsed().as_secs_f64() * 1e9 / operations as f64);
        if let Some(elapsed) = cycle_start
            .zip(cycle_counter())
            .map(|(start, end)| end.wrapping_sub(start) as f64 / operations as f64)
        {
            cycles.push(elapsed);
        }
    }
    times.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    black_box(op());
    ALLOC.enabled.store(false, Ordering::Relaxed);

    Row {
        median_ns: times[SAMPLES / 2],
        p95_ns: times[SAMPLES * 95 / 100],
        median_cycles: (!cycles.is_empty()).then(|| cycles[cycles.len() / 2]),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn string_checksum(text: String) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        hash.wrapping_mul(0x100_0000_01b3) ^ u64::from(byte)
    })
}

fn assert_faster(title: &str, old: &Row, new: &Row) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert!(
        new.median_ns < old.median_ns,
        "{title} median latency regressed"
    );
    if let (Some(old_cycles), Some(new_cycles)) = (old.median_cycles, new.median_cycles) {
        assert!(new_cycles < old_cycles, "{title} cycle count regressed");
    }
}

fn print_pair(title: &str, old: &Row, new: &Row) {
    println!("\n{title}");
    print_row("old", old);
    print_row("new", new);
    println!(
        "  change: {:>7.2}% median  {:>7.2}% cycles  {:>7.2}% throughput  \
         {:>7.2}% p95  {:>7.2}% allocs  {:>7.2}% reallocs  {:>7.2}% churn",
        change(old.median_ns, new.median_ns),
        change(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN),
        ),
        change(throughput(old), throughput(new)),
        change(old.p95_ns, new.p95_ns),
        change(old.alloc.allocs as f64, new.alloc.allocs as f64),
        change(old.alloc.reallocs as f64, new.alloc.reallocs as f64),
        change(old.alloc.churn() as f64, new.alloc.churn() as f64),
    );
    assert_faster(title, old, new);
}

fn print_row(label: &str, row: &Row) {
    println!(
        "  {label:<3} {:>10.2} ns/op  {:>10.2} cycles/op  {:>10.2} p95 ns  \
         {:>9.3} Mop/s  {:>5} alloc  {:>5} realloc  {:>5} free  {:>10} churn B",
        row.median_ns,
        row.median_cycles.unwrap_or(f64::NAN),
        row.p95_ns,
        throughput(row) / 1e6,
        row.alloc.allocs,
        row.alloc.reallocs,
        row.alloc.frees,
        row.alloc.churn(),
    );
}

fn throughput(row: &Row) -> f64 {
    1e9 / row.median_ns
}

fn change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

fn asset(name: String) -> ReleaseAsset {
    ReleaseAsset {
        browser_download_url: format!("https://example.invalid/{name}"),
        name,
        size: 48_000_000,
        digest: None,
    }
}

fn asset_fixture() -> Vec<ReleaseAsset> {
    [
        "deadsync-v1.2.345-arm64-linux.tar.gz",
        "deadsync-v1.2.345-x86_64-linux.tar.gz",
        "deadsync-v1.2.345-arm64-macos.tar.gz",
        "deadsync-v1.2.345-x86_64-macos.tar.gz",
        "deadsync-v1.2.345-i686-win7.zip",
        "deadsync-v1.2.345-x86_64-win7.zip",
        "deadsync-v1.2.345-x86_64-freebsd.tar.gz",
        "deadsync-v1.2.345-x86_64-windows.zip",
    ]
    .into_iter()
    .map(|name| asset(name.to_owned()))
    .collect()
}

fn release_fixture() -> Vec<ReleaseInfo> {
    (1..=128)
        .rev()
        .map(|patch| {
            let version = Version::new(1, 0, patch);
            let tag = format!("v{version}");
            let name = expected_asset_name(&tag, WINDOWS_X64);
            ReleaseInfo {
                html_url: format!("https://example.invalid/releases/{tag}"),
                body: String::new(),
                published_at: None,
                assets: vec![asset(name)],
                tag,
                version,
            }
        })
        .collect()
}

fn main() {
    let digest = std::array::from_fn(|index| (index as u8).wrapping_mul(37).wrapping_add(11));
    let old = measure(250_000, || {
        string_checksum(bench_support::sha256_hex_old(black_box(&digest)))
    });
    let new = measure(250_000, || {
        string_checksum(bench_support::sha256_hex_new(black_box(&digest)))
    });
    assert_eq!(new.alloc.allocs, old.alloc.allocs);
    assert_eq!(new.alloc.reallocs, 0);
    print_pair("SHA-256 lowercase hex encoding", &old, &new);

    let assets = asset_fixture();
    let old = measure(250_000, || {
        bench_support::pick_asset_old(black_box(&assets), "v1.2.345", WINDOWS_X64)
            .unwrap_or(usize::MAX) as u64
    });
    let new = measure(250_000, || {
        bench_support::pick_asset_new(black_box(&assets), "v1.2.345", WINDOWS_X64)
            .unwrap_or(usize::MAX) as u64
    });
    assert_eq!(new.alloc.allocs, 0, "asset matching allocated");
    assert_eq!(new.alloc.churn(), 0, "asset matching churned memory");
    print_pair("host release-asset matching", &old, &new);

    let releases = release_fixture();
    let current = Version::new(1, 1, 0);
    let old = measure(512, || {
        bench_support::rollback_old_checksum(black_box(&releases), black_box(&current), WINDOWS_X64)
    });
    let new = measure(512, || {
        bench_support::rollback_new_checksum(black_box(&releases), black_box(&current), WINDOWS_X64)
    });
    assert!(
        new.alloc.allocs < old.alloc.allocs,
        "bounded rollback selection did not reduce allocations"
    );
    assert_eq!(new.alloc.reallocs, 0, "bounded selection reallocated");
    assert!(
        new.alloc.churn() < old.alloc.churn(),
        "bounded rollback selection did not reduce churn"
    );
    print_pair("bounded rollback candidate selection", &old, &new);
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

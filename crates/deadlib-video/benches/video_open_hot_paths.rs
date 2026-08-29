use deadlib_video::bench_support;
use serde::Deserialize;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

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

// SAFETY: all requests are delegated unchanged to `System`; relaxed counters
// observe only this single-threaded benchmark while their gate is enabled.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes from the allocator caller.
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
    units: usize,
}

fn measure(operations: usize, units: usize, mut op: impl FnMut() -> u64) -> Row {
    for _ in 0..4 {
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
    let allocation_checksum = black_box(op());
    ALLOC.enabled.store(false, Ordering::Relaxed);
    black_box(allocation_checksum);

    Row {
        median_ns: times[SAMPLES / 2],
        p95_ns: times[SAMPLES * 95 / 100],
        median_cycles: (!cycles.is_empty()).then(|| cycles[cycles.len() / 2]),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
        units,
    }
}

fn print_pair(title: &str, old: &Row, new: &Row) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    println!("\n{title} ({} units/op)", old.units);
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
}

fn print_row(label: &str, row: &Row) {
    println!(
        "  {label:<3} {:>10.1} ns/op  {:>10.1} cycles/op  {:>10.1} p95 ns  \
         {:>8.2} Munit/s  {:>4} alloc  {:>4} realloc  {:>4} free  {:>8} churn B",
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
    row.units as f64 * 1e9 / row.median_ns
}

fn change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

#[derive(Deserialize)]
struct LegacyProbeOutput {
    streams: Vec<LegacyProbeStream>,
    format: Option<LegacyProbeFormat>,
}

#[derive(Deserialize)]
struct LegacyProbeStream {
    width: Option<u32>,
    height: Option<u32>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
    duration: Option<String>,
    color_space: Option<String>,
    color_range: Option<String>,
}

#[derive(Deserialize)]
struct LegacyProbeFormat {
    duration: Option<String>,
}

fn legacy_probe_json_checksum(raw: &[u8]) -> u64 {
    let parsed: LegacyProbeOutput = serde_json::from_slice(raw).unwrap();
    let mut checksum = parsed.streams.len() as u64;
    for stream in &parsed.streams {
        checksum = checksum
            .wrapping_mul(131)
            .wrapping_add(u64::from(stream.width.unwrap_or_default()))
            .wrapping_add(u64::from(stream.height.unwrap_or_default()) << 32);
        for value in [
            stream.avg_frame_rate.as_deref(),
            stream.r_frame_rate.as_deref(),
            stream.duration.as_deref(),
            stream.color_space.as_deref(),
            stream.color_range.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            checksum = value.bytes().fold(checksum, |sum, byte| {
                sum.wrapping_mul(33).wrapping_add(u64::from(byte))
            });
        }
    }
    if let Some(duration) = parsed.format.and_then(|format| format.duration) {
        checksum = duration.bytes().fold(checksum, |sum, byte| {
            sum.wrapping_mul(33).wrapping_add(u64::from(byte))
        });
    }
    checksum
}

fn legacy_rate_bits(raw: &str) -> u32 {
    let Some((num, den)) = raw.split_once('/') else {
        return u32::MAX;
    };
    let Some(num) = num.parse::<f64>().ok() else {
        return u32::MAX;
    };
    let Some(den) = den.parse::<f64>().ok() else {
        return u32::MAX;
    };
    if !num.is_finite() || !den.is_finite() || den <= 0.0 {
        return u32::MAX;
    }
    let fps = (num / den) as f32;
    if fps.is_finite() && fps > 0.0 {
        fps.to_bits()
    } else {
        u32::MAX
    }
}

#[cfg(windows)]
fn legacy_primary_path(dir: &Path, name: &str) -> PathBuf {
    let candidates = [format!("{name}.exe"), name.to_string()];
    dir.join(&candidates[0])
}

#[cfg(not(windows))]
fn legacy_primary_path(dir: &Path, name: &str) -> PathBuf {
    let candidates = [name.to_string(), format!("{name}.exe")];
    dir.join(&candidates[0])
}

fn path_checksum(path: PathBuf) -> u64 {
    path.to_string_lossy().bytes().fold(0_u64, |sum, byte| {
        sum.wrapping_mul(131).wrapping_add(u64::from(byte))
    })
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions have no memory operands.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions have no memory operands.
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
    const PROBE_OPS: usize = 1_024;
    let probe_json = br#"{
        "streams": [{
            "width": 1920,
            "height": 1080,
            "avg_frame_rate": "60000/1001",
            "r_frame_rate": "60/1",
            "duration": "183.516667",
            "color_space": "bt709",
            "color_range": "tv"
        }],
        "format": { "duration": "183.534000" }
    }"#;
    let old = measure(PROBE_OPS, 6, || {
        legacy_probe_json_checksum(black_box(probe_json))
    });
    let new = measure(PROBE_OPS, 6, || {
        bench_support::probe_json_checksum(black_box(probe_json))
    });
    print_pair("borrowed ffprobe metadata", &old, &new);

    const RATE_OPS: usize = 32_768;
    let rates = [
        "24000/1001",
        "24/1",
        "25/1",
        "30000/1001",
        "30/1",
        "50/1",
        "60000/1001",
        "60/1",
    ];
    let old = measure(RATE_OPS, rates.len(), || {
        rates.iter().fold(0_u64, |checksum, rate| {
            checksum
                .wrapping_mul(131)
                .wrapping_add(u64::from(legacy_rate_bits(black_box(rate))))
        })
    });
    let new = measure(RATE_OPS, rates.len(), || {
        rates.iter().fold(0_u64, |checksum, rate| {
            checksum
                .wrapping_mul(131)
                .wrapping_add(u64::from(bench_support::rate_bits(black_box(rate))))
        })
    });
    print_pair("integer ffprobe frame-rate parsing", &old, &new);

    const PATH_OPS: usize = 16_384;
    let dir = Path::new("runtime/bin");
    let names = ["ffmpeg", "ffprobe"];
    let old = measure(PATH_OPS, names.len(), || {
        names.iter().fold(0_u64, |checksum, name| {
            checksum
                .wrapping_mul(131)
                .wrapping_add(path_checksum(legacy_primary_path(
                    black_box(dir),
                    black_box(name),
                )))
        })
    });
    let new = measure(PATH_OPS, names.len(), || {
        names.iter().fold(0_u64, |checksum, name| {
            checksum
                .wrapping_mul(131)
                .wrapping_add(path_checksum(bench_support::primary_path(
                    black_box(dir),
                    black_box(name),
                )))
        })
    });
    print_pair("lazy bundled-tool candidate path", &old, &new);
}

use deadsync_simfile::tags::{extract_named_tag_values_baseline, named_tag_values};
use rssp::parse::{bgchanges_values, decode_bytes, extract_bgchanges_values, unescape_tag};
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
    deallocs: AtomicU64,
    allocated_bytes: AtomicU64,
    freed_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            deallocs: AtomicU64::new(0),
            allocated_bytes: AtomicU64::new(0),
            freed_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            deallocs: self.deallocs.load(Ordering::Relaxed),
            allocated_bytes: self.allocated_bytes.load(Ordering::Relaxed),
            freed_bytes: self.freed_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: calls delegate unchanged to `System`; relaxed counters only observe
// successful operations while this single-threaded benchmark enables them.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is supplied by the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.allocated_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.deallocs.fetch_add(1, Ordering::Relaxed);
            self.freed_bytes
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
            if new_size > old.size() {
                self.allocated_bytes
                    .fetch_add((new_size - old.size()) as u64, Ordering::Relaxed);
            } else {
                self.freed_bytes
                    .fetch_add((old.size() - new_size) as u64, Ordering::Relaxed);
            }
        }
        out
    }
}

#[derive(Clone, Copy)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    deallocs: u64,
    allocated_bytes: u64,
    freed_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            deallocs: self.deallocs - before.deallocs,
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
    median_cycles: f64,
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

fn percentile(sorted: &[f64], percentile: f64) -> f64 {
    let index = ((sorted.len() as f64 * percentile).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len() - 1);
    sorted[index]
}

fn measure(ops_per_sample: usize, mut op: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..4 {
        black_box(op());
    }

    let mut times = Vec::with_capacity(SAMPLES);
    let mut cycles = Vec::with_capacity(SAMPLES);
    let mut checksum = 0u64;
    for _ in 0..SAMPLES {
        let cycle_start = cycle_counter();
        let started = Instant::now();
        for _ in 0..ops_per_sample {
            checksum = checksum.wrapping_add(black_box(op()));
        }
        let elapsed = started.elapsed();
        let cycle_end = cycle_counter();
        times.push(elapsed.as_secs_f64() * 1_000_000_000.0 / ops_per_sample as f64);
        cycles.push(cycle_start.zip(cycle_end).map_or(f64::NAN, |(start, end)| {
            end.wrapping_sub(start) as f64 / ops_per_sample as f64
        }));
    }
    times.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let allocation_checksum = black_box(op());
    ALLOC.enabled.store(false, Ordering::Relaxed);

    BenchResult {
        median_ns: percentile(&times, 0.5),
        p95_ns: percentile(&times, 0.95),
        median_cycles: percentile(&cycles, 0.5),
        allocated: ALLOC.snapshot().delta(before),
        checksum: checksum.wrapping_add(allocation_checksum),
    }
}

fn values_checksum<'a>(values: impl IntoIterator<Item = &'a [u8]>) -> u64 {
    values
        .into_iter()
        .enumerate()
        .fold(0u64, |sum, (index, value)| {
            let edge = value.first().copied().unwrap_or_default()
                ^ value.last().copied().unwrap_or_default();
            sum.wrapping_add((value.len() as u64).rotate_left((index % 63) as u32))
                .wrapping_add(u64::from(edge))
        })
}

fn text_checksum(text: &str) -> u64 {
    let bytes = text.as_bytes();
    (bytes.len() as u64)
        ^ u64::from(bytes.first().copied().unwrap_or_default()).rotate_left(17)
        ^ u64::from(bytes.last().copied().unwrap_or_default()).rotate_left(41)
}

fn decode_owned_checksum(samples: &[&[u8]]) -> u64 {
    samples.iter().fold(0u64, |sum, raw| {
        let text = unescape_tag(decode_bytes(raw).as_ref()).into_owned();
        black_box(&text);
        sum.wrapping_add(text_checksum(&text))
    })
}

fn decode_borrowed_checksum(samples: &[&[u8]]) -> u64 {
    samples.iter().fold(0u64, |sum, raw| {
        let decoded = decode_bytes(raw);
        let text = unescape_tag(decoded.as_ref());
        black_box(text.as_ref());
        sum.wrapping_add(text_checksum(text.as_ref()))
    })
}

fn print_result(label: &str, result: &BenchResult) {
    println!(
        "{label:<9} {:>10.1} ns median  {:>10.1} ns p95  {:>10.1} cycles  \
         {:>9.1} Kops/s  {:>4} alloc  {:>3} realloc  {:>4} free  {:>7} B alloc  {:>7} B churn",
        result.median_ns,
        result.p95_ns,
        result.median_cycles,
        1_000_000.0 / result.median_ns,
        result.allocated.allocs,
        result.allocated.reallocs,
        result.allocated.deallocs,
        result.allocated.allocated_bytes,
        result.allocated.churn_bytes(),
    );
}

fn percent_reduction(new: u64, old: u64) -> f64 {
    if old == 0 {
        0.0
    } else {
        100.0 * (1.0 - new as f64 / old as f64)
    }
}

fn benchmark_pair(
    title: &str,
    ops: usize,
    mut old_op: impl FnMut() -> u64,
    mut new_op: impl FnMut() -> u64,
) {
    assert_eq!(old_op(), new_op(), "{title} behavior diverged");
    let old = measure(ops, &mut old_op);
    let new = measure(ops, &mut new_op);
    assert_eq!(old.checksum, new.checksum, "{title} checksum diverged");

    println!("\n{title}");
    print_result("old", &old);
    print_result("new", &new);
    println!(
        "change    {:>7.2}x throughput  {:>7.2}% median  {:>7.2}% p95  \
         {:>7.2}% cycles  {:>7.2}% bytes  {:>7.2}% churn",
        old.median_ns / new.median_ns,
        100.0 * (1.0 - new.median_ns / old.median_ns),
        100.0 * (1.0 - new.p95_ns / old.p95_ns),
        100.0 * (1.0 - new.median_cycles / old.median_cycles),
        percent_reduction(new.allocated.allocated_bytes, old.allocated.allocated_bytes),
        percent_reduction(new.allocated.churn_bytes(), old.allocated.churn_bytes()),
    );
}

fn fixture() -> String {
    let mut data = String::with_capacity(32 * 1024);
    for index in 0..96 {
        data.push_str("#TITLE:Tag Pipeline;#ARTIST:DeadSync;#BPMS:0.000=150.000;");
        if index % 3 == 0 {
            data.push_str(
                "#BGCHANGES:0.000=background.png=1.000=0=0=1,16.000=movie.mp4=1.000=0=0=1;",
            );
        }
        if index % 4 == 0 {
            data.push_str("#FGCHANGES:4.000=visuals\\;main=1.000=0=0=0=0;");
        }
        if index % 6 == 0 {
            data.push_str("#BGCHANGES2:8.000=overlay.png=1.000=0=0=1;");
        }
        data.push_str("#NOTES:dance-single::Hard:9:0,0,0,0,0:0000\n1000\n0000\n0000;");
    }
    data
}

fn main() {
    let fixture = fixture();
    let data = fixture.as_bytes();
    let named_tags = [b"#FGCHANGES:".as_slice(), b"#BGCHANGES2:".as_slice()];
    let decode_samples: &[&[u8]] = &[
        b"0.000=background.png=1.000=0=0=1",
        b"4.000=visuals\\;main=1.000=0=0=0=0",
        "8.000=日本語.png=1.000=0=0=1".as_bytes(),
        b"12.000=legacy-\x96-name.avi=1.000=0=0=1",
    ];

    println!(
        "tag value hot paths ({} KiB fixture, {} samples)",
        data.len().div_ceil(1024),
        SAMPLES
    );

    benchmark_pair(
        "RSSP BGCHANGES: eager Vec vs lazy iterator",
        1_200,
        || {
            let values = extract_bgchanges_values(black_box(data));
            values_checksum(black_box(&values).iter().copied())
        },
        || values_checksum(bgchanges_values(black_box(data))),
    );
    benchmark_pair(
        "named tags: eager Vec vs lazy iterator",
        1_200,
        || {
            let values = extract_named_tag_values_baseline(black_box(data), black_box(&named_tags));
            values_checksum(black_box(&values).iter().copied())
        },
        || values_checksum(named_tag_values(black_box(data), black_box(&named_tags))),
    );
    benchmark_pair(
        "decoded tag text: forced String vs retained Cow",
        80_000,
        || decode_owned_checksum(black_box(decode_samples)),
        || decode_borrowed_checksum(black_box(decode_samples)),
    );
}

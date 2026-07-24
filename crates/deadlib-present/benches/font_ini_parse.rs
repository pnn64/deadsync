use deadlib_present::font::{
    parse_font_ini_workload_for_bench, parse_font_ini_workload_legacy_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::fmt::Write;
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SECTIONS: usize = 16;
const VALUES_PER_SECTION: usize = 64;
const RAW_LINES_PER_SECTION: usize = 8;
const RUNS: usize = 1_000;

type Workload = fn(&str) -> u64;

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
// independent atomics only observe successful operations.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.bytes.fetch_add(
                new_size.saturating_sub(old.size()) as u64,
                Ordering::Relaxed,
            );
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
    elapsed: Duration,
    cycles: u64,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn main() {
    let text = fixture_ini();
    let lines = text.lines().count();
    assert_eq!(
        parse_font_ini_workload_legacy_for_bench(&text),
        parse_font_ini_workload_for_bench(&text),
    );

    let old = measure(&text, parse_font_ini_workload_legacy_for_bench);
    let new = measure(&text, parse_font_ini_workload_for_bench);
    assert_eq!(old.checksum, new.checksum);

    println!("font INI parsing ({lines} lines, {SECTIONS} sections, {RUNS} runs)");
    print_result("old", &old, lines);
    print_result("new", &new, lines);
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | allocation-op reduction {:.1}% | byte reduction {:.1}%",
        old.elapsed.as_secs_f64() / new.elapsed.as_secs_f64(),
        reduction(old.cycles, new.cycles),
        reduction(
            old.alloc.allocs + old.alloc.reallocs,
            new.alloc.allocs + new.alloc.reallocs,
        ),
        reduction(old.alloc.bytes, new.alloc.bytes),
    );
}

fn fixture_ini() -> String {
    let mut text = String::with_capacity(SECTIONS * VALUES_PER_SECTION * 24);
    text.push_str("DefaultStrokeColor = 0,0,0,0\n");
    text.push_str("; representative StepMania bitmap font metadata\n");
    for section in 0..SECTIONS {
        writeln!(text, "[Page {section:02}]").unwrap();
        writeln!(text, "TextureHints = {}", section % 3).unwrap();
        for value in 0..VALUES_PER_SECTION {
            if value < RAW_LINES_PER_SECTION {
                writeln!(
                    text,
                    "LiNe {value} =  ABCDEFGHIJKLMNOPQRSTUVWXYZ {section:02}-{value:02}  "
                )
                .unwrap();
            } else if value % 5 == 0 {
                writeln!(text, "Map U+{:04X} = {value}", 0x2500 + value).unwrap();
            } else {
                writeln!(text, "{value} = {}", 8 + (value % 12)).unwrap();
            }
        }
        writeln!(text, "[PAGE {section:02}]").unwrap();
        writeln!(text, "DefaultWidth = {}", 12 + section % 4).unwrap();
        text.push_str("// repeated sections and comments preserve override behavior\n");
    }
    text
}

fn measure(text: &str, workload: Workload) -> BenchResult {
    for _ in 0..3 {
        black_box(workload(text));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..RUNS {
        checksum = checksum.rotate_left(7) ^ black_box(workload(black_box(text))) ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult, lines: usize) {
    let runs = RUNS as f64;
    let parsed_lines = (lines * RUNS) as f64;
    println!(
        "  {label:<4} {:>7.1} ns/line {:>8.1} cycles/line {:>7.2} Mlines/s",
        result.elapsed.as_secs_f64() * 1.0e9 / parsed_lines,
        result.cycles as f64 / parsed_lines,
        parsed_lines / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.1}/{:.1} per run, {:.1} KiB/run",
        result.alloc.allocs as f64 / runs,
        result.alloc.reallocs as f64 / runs,
        result.alloc.bytes as f64 / runs / 1024.0,
    );
}

fn reduction(old: u64, new: u64) -> f64 {
    if old == 0 {
        return 0.0;
    }
    100.0 * (1.0 - new as f64 / old as f64)
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: timestamp reads and fences do not access memory.
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

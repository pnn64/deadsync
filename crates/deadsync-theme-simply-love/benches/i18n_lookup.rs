use deadsync_theme_simply_love::i18n::{
    self, format_translation_template_for_bench, format_translation_template_legacy_for_bench, tr,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const BATCHES: usize = 200_000;
const FORMAT_TEMPLATE: &str =
    "Player {name} cleared {songs} songs in {time}; {name} earned {points} points.";
const FORMAT_ARGS: [(&str, &str); 4] = [
    ("name", "ALICE"),
    ("songs", "128"),
    ("time", "01:23:45"),
    ("points", "987654"),
];
type TemplateFormatter = fn(&str, &[(&str, &str)]) -> Arc<str>;
const SELECT_MUSIC_KEYS: [(&str, &str); 34] = [
    ("ScreenTitles", "SelectMusic"),
    ("SelectMusic", "PressStartForOptions"),
    ("SelectMusic", "EnteringOptions"),
    ("SelectMusic", "ExitGamePrompt"),
    ("SelectMusic", "KeepPlayingInfo"),
    ("SelectMusic", "FinishedInfo"),
    ("SelectMusic", "RecentlyPlayed"),
    ("SelectMusic", "MostPopular"),
    ("SelectMusic", "ArtistLabel"),
    ("SelectMusic", "BPMLabel"),
    ("SelectMusic", "LengthLabel"),
    ("SelectMusic", "StepsLabel"),
    ("SelectMusic", "ExScore"),
    ("SelectMusic", "ItgScore"),
    ("SelectMusic", "OptionsMenuLabel"),
    ("SelectMusic", "SortBy"),
    ("SelectMusic", "Genre"),
    ("SelectMusic", "MachineTopScores"),
    ("SelectMusic", "P1MostPlayed"),
    ("SelectMusic", "P2MostPlayed"),
    ("SelectMusic", "P1RecentSongs"),
    ("SelectMusic", "P2RecentSongs"),
    ("SelectMusic", "ChangeStyleTo"),
    ("SelectMusic", "TestInputPrompt"),
    ("SelectMusic", "SongSearchPrompt"),
    ("SelectMusic", "ReloadPrompt"),
    ("SelectMusic", "Favorites"),
    ("SelectMusic", "Unplayed"),
    ("SelectMusic", "UnknownGenre"),
    ("SelectMusic", "NotAvailable"),
    ("SelectMusic", "TotalLabel"),
    ("SelectMusic", "MusicRateSuffix"),
    ("Common", "Yes"),
    ("Common", "No"),
];

fn main() {
    i18n::init(deadsync_assets::language::load_for_tests("en"));
    for _ in 0..1_000 {
        black_box(lookup_batch());
    }

    let started = Instant::now();
    let mut checksum = 0usize;
    for _ in 0..BATCHES {
        checksum = checksum.wrapping_add(black_box(lookup_batch()));
    }
    let elapsed = started.elapsed();
    let lookups = BATCHES * SELECT_MUSIC_KEYS.len();
    let ns_per_lookup = elapsed.as_secs_f64() * 1.0e9 / lookups as f64;

    println!("translation lookup microbenchmark");
    println!(
        "{lookups} Select Music translation hits in {:.3}s",
        elapsed.as_secs_f64()
    );
    println!(
        "{ns_per_lookup:>10.2} ns/lookup  {:>10.2} Mlookups/s  checksum={checksum}",
        lookups as f64 / elapsed.as_secs_f64() / 1.0e6,
    );

    benchmark_formatting();
}

#[inline(never)]
fn lookup_batch() -> usize {
    let mut checksum = 0usize;
    for &(section, key) in &SELECT_MUSIC_KEYS {
        let value = black_box(tr(black_box(section), black_box(key)));
        checksum = checksum.wrapping_add(value.len());
    }
    checksum
}

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
// independent atomics only observe successful allocations.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is forwarded unchanged from the allocator caller.
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller supplies the allocation's original layout.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees `ptr` and `old` identify a live allocation.
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

struct FormatResult {
    elapsed_ns: f64,
    cycles_per_format: f64,
    allocs_per_format: f64,
    reallocs_per_format: f64,
    bytes_per_format: f64,
    checksum: u64,
}

fn benchmark_formatting() {
    let legacy = format_translation_template_legacy_for_bench(FORMAT_TEMPLATE, &FORMAT_ARGS);
    let formatted = format_translation_template_for_bench(FORMAT_TEMPLATE, &FORMAT_ARGS);
    assert_eq!(legacy, formatted, "translation formatting changed");

    let old = measure_formatting(format_translation_template_legacy_for_bench);
    let new = measure_formatting(format_translation_template_for_bench);
    assert_eq!(old.checksum, new.checksum);
    println!("\nnamed translation formatting ({BATCHES} formats)");
    print_format_result("old", &old);
    print_format_result("new", &new);
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | allocations reduction {:.1}% | bytes reduction {:.1}%",
        old.elapsed_ns / new.elapsed_ns,
        100.0 * (1.0 - new.cycles_per_format / old.cycles_per_format),
        100.0
            * (1.0
                - (new.allocs_per_format + new.reallocs_per_format)
                    / (old.allocs_per_format + old.reallocs_per_format)),
        100.0 * (1.0 - new.bytes_per_format / old.bytes_per_format),
    );
}

fn measure_formatting(format: TemplateFormatter) -> FormatResult {
    for _ in 0..1_000 {
        black_box(format(FORMAT_TEMPLATE, &FORMAT_ARGS));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for iteration in 0..BATCHES {
        let formatted = black_box(format(black_box(FORMAT_TEMPLATE), black_box(&FORMAT_ARGS)));
        checksum = formatted
            .as_bytes()
            .iter()
            .fold(checksum.rotate_left(7) ^ iteration as u64, |sum, byte| {
                sum.rotate_left(3) ^ u64::from(*byte)
            });
    }
    let elapsed = started.elapsed();
    let cycles = read_cycles().saturating_sub(cycles_before);
    let alloc = ALLOC.snapshot().delta(before);
    FormatResult {
        elapsed_ns: elapsed.as_secs_f64() * 1.0e9 / BATCHES as f64,
        cycles_per_format: cycles as f64 / BATCHES as f64,
        allocs_per_format: alloc.allocs as f64 / BATCHES as f64,
        reallocs_per_format: alloc.reallocs as f64 / BATCHES as f64,
        bytes_per_format: alloc.bytes as f64 / BATCHES as f64,
        checksum,
    }
}

fn print_format_result(label: &str, result: &FormatResult) {
    println!(
        "  {label:<4} {:>8.1} ns/format {:>8.1} cycles/format {:>4.1}/{:>3.1} alloc/realloc {:>6.1} B/format",
        result.elapsed_ns,
        result.cycles_per_format,
        result.allocs_per_format,
        result.reallocs_per_format,
        result.bytes_per_format,
    );
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: fences and timestamp reads do not access memory; they serialize
    // this thread's measurement interval.
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

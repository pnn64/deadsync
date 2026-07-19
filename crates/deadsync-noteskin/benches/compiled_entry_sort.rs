use deadsync_noteskin::compiled::CompiledLoaderEntry;
use deadsync_noteskin::compiler::{
    sort_compiled_loader_entries_for_bench, sort_compiled_loader_entries_legacy_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const RUNS: usize = 300;
const ENTRY_COUNT: usize = 256;
type Sorter = fn(&mut [CompiledLoaderEntry]);

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

struct BenchResult {
    elapsed: Duration,
    cycles: u64,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn main() {
    let mut legacy = fixture_entries();
    let mut current = legacy.clone();
    sort_compiled_loader_entries_legacy_for_bench(&mut legacy);
    sort_compiled_loader_entries_for_bench(&mut current);
    assert_eq!(legacy, current);

    let old = measure(
        fixture_batches(),
        sort_compiled_loader_entries_legacy_for_bench,
    );
    let new = measure(fixture_batches(), sort_compiled_loader_entries_for_bench);
    assert_eq!(old.checksum, new.checksum);

    println!("compiled noteskin loader ordering ({ENTRY_COUNT} entries x {RUNS} runs)");
    print_result("old", &old);
    print_result("new", &new);
    print_reduction(&old, &new);
}

fn fixture_batches() -> Vec<Vec<CompiledLoaderEntry>> {
    (0..RUNS).map(|_| fixture_entries()).collect()
}

fn fixture_entries() -> Vec<CompiledLoaderEntry> {
    (0..ENTRY_COUNT)
        .map(|index| {
            let source = index * 73 % ENTRY_COUNT;
            let button_index = source % 16;
            let element_index = source / 16;
            let button = if source % 3 == 0 {
                format!("BUTTON-{button_index:02}")
            } else {
                format!("Button-{button_index:02}")
            };
            let element = if source % 5 == 0 {
                format!("ELEMENT-{element_index:02}")
            } else {
                format!("Element-{element_index:02}")
            };
            CompiledLoaderEntry {
                button,
                element,
                load_button: String::new(),
                load_element: String::new(),
                blank: false,
                rotation_z: None,
                init_command: None,
            }
        })
        .collect()
}

fn measure(mut batches: Vec<Vec<CompiledLoaderEntry>>, sort: Sorter) -> BenchResult {
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for entries in &mut batches {
        sort(black_box(entries));
        checksum = checksum.rotate_left(7) ^ black_box(entries_checksum(entries));
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn entries_checksum(entries: &[CompiledLoaderEntry]) -> u64 {
    entries
        .iter()
        .fold(entries.len() as u64, |checksum, entry| {
            checksum.rotate_left(3) ^ entry.button.len() as u64 ^ entry.element.len() as u64
        })
}

fn print_result(label: &str, result: &BenchResult) {
    let operations = (ENTRY_COUNT * RUNS) as f64;
    println!(
        "  {label:<4} {:>7.2} ns/entry {:>7.2} cycles/entry {:>7.1} Mentries/s",
        result.elapsed.as_secs_f64() * 1.0e9 / operations,
        result.cycles as f64 / operations,
        operations / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.2}/{:.2} per entry, {:.1} bytes/entry",
        result.alloc.allocs as f64 / operations,
        result.alloc.reallocs as f64 / operations,
        result.alloc.bytes as f64 / operations,
    );
}

fn print_reduction(old: &BenchResult, new: &BenchResult) {
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

fn reduction(old: u64, new: u64) -> f64 {
    if old == 0 {
        0.0
    } else {
        100.0 * (1.0 - new as f64 / old as f64)
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

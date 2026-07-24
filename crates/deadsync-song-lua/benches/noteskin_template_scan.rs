use deadsync_song_lua::{
    normalize_noteskin_template_path_for_bench, normalize_noteskin_template_path_legacy_for_bench,
    noteskin_get_path_args_for_bench, noteskin_get_path_args_legacy_for_bench,
    noteskin_model_field_for_bench, noteskin_model_field_legacy_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::borrow::Cow;
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const RUNS: usize = 20_000;
const SCRIPT_BYTES: usize = 8_192;

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
    let get_path_script = padded_script("return NOTESKIN:GetPath('Down', 'Hold Head Active')\n");
    assert_eq!(
        noteskin_get_path_args_legacy_for_bench(&get_path_script),
        noteskin_get_path_args_for_bench(&get_path_script)
    );
    let old_get_path = measure(RUNS, || {
        string_pair_checksum(noteskin_get_path_args_legacy_for_bench(black_box(
            &get_path_script,
        )))
    });
    let new_get_path = measure(RUNS, || {
        string_pair_checksum(noteskin_get_path_args_for_bench(black_box(
            &get_path_script,
        )))
    });
    assert_eq!(old_get_path.checksum, new_get_path.checksum);
    print_comparison(
        "noteskin GetPath extraction",
        RUNS,
        &old_get_path,
        &new_get_path,
    );

    let field_script = padded_script("return Def.Model { BoNeS = 'arrow model.txt' }\n");
    assert_eq!(
        noteskin_model_field_legacy_for_bench(&field_script),
        noteskin_model_field_for_bench(&field_script)
    );
    let old_field = measure(RUNS, || {
        string_checksum(noteskin_model_field_legacy_for_bench(black_box(
            &field_script,
        )))
    });
    let new_field = measure(RUNS, || {
        string_checksum(noteskin_model_field_for_bench(black_box(&field_script)))
    });
    assert_eq!(old_field.checksum, new_field.checksum);
    print_comparison(
        "noteskin model-field extraction",
        RUNS,
        &old_field,
        &new_field,
    );

    let template_paths = [
        "arrow model.txt",
        "models/arrow model.txt",
        "assets/noteskins/default/Down Tap Note model.txt",
        r"fallback\common\Down Hold Head Active model.txt",
        r"nested\models\materials\arrow model.txt",
        r"bones\player\arrow model.txt",
    ];
    for path in template_paths {
        assert_eq!(
            normalize_noteskin_template_path_legacy_for_bench(path),
            normalize_noteskin_template_path_for_bench(path)
        );
    }
    let old_paths = measure(RUNS * template_paths.len(), || {
        path_checksum(
            &template_paths,
            normalize_noteskin_template_path_legacy_for_bench,
        )
    });
    let new_paths = measure(RUNS * template_paths.len(), || {
        path_checksum(&template_paths, normalize_noteskin_template_path_for_bench)
    });
    assert_eq!(old_paths.checksum, new_paths.checksum);
    print_comparison(
        "noteskin template-path normalization",
        RUNS * template_paths.len(),
        &old_paths,
        &new_paths,
    );
}

fn padded_script(tail: &str) -> String {
    const NOISE: &str = "local texture = 'assets/noteskins/default/Down Tap Note.png'\n";
    let mut script = String::with_capacity(SCRIPT_BYTES + tail.len());
    while script.len() + NOISE.len() + tail.len() <= SCRIPT_BYTES {
        script.push_str(NOISE);
    }
    while script.len() + tail.len() < SCRIPT_BYTES {
        script.push('-');
    }
    script.push_str(tail);
    script
}

fn path_checksum(paths: &[&str], normalize: for<'a> fn(&'a str) -> Cow<'a, str>) -> u64 {
    paths.iter().fold(0_u64, |checksum, path| {
        let normalized = normalize(black_box(path));
        normalized.bytes().fold(checksum, |checksum, byte| {
            checksum.rotate_left(3) ^ u64::from(byte)
        })
    })
}

fn string_pair_checksum(value: Option<(String, String)>) -> u64 {
    value.map_or(0, |(left, right)| {
        (left.len() as u64).rotate_left(7) ^ right.len() as u64
    })
}

fn string_checksum(value: Option<String>) -> u64 {
    value.map_or(0, |value| {
        value.bytes().fold(value.len() as u64, |checksum, byte| {
            checksum.rotate_left(5) ^ u64::from(byte)
        })
    })
}

fn measure(operations_per_batch: usize, mut operation: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..100 {
        black_box(operation());
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = operations_per_batch as u64;
    for run in 0..RUNS {
        checksum = checksum.rotate_left(7) ^ black_box(operation()) ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_comparison(title: &str, operations: usize, old: &BenchResult, new: &BenchResult) {
    println!("{title} ({operations} operations)");
    print_result("old", old, operations);
    print_result("new", new, operations);
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

fn print_result(label: &str, result: &BenchResult, operations: usize) {
    let operations = operations as f64;
    println!(
        "  {label:<4} {:>8.2} ns/op {:>8.2} cycles/op {:>7.2} Mops/s",
        result.elapsed.as_secs_f64() * 1.0e9 / operations,
        result.cycles as f64 / operations,
        operations / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.3}/{:.3} per op, {:.1} bytes/op",
        result.alloc.allocs as f64 / operations,
        result.alloc.reallocs as f64 / operations,
        result.alloc.bytes as f64 / operations,
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

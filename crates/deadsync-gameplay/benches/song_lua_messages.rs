use deadsync_gameplay::{
    build_song_lua_message_command_indices, song_lua_message_command_index,
    song_lua_message_command_index_legacy_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const COMMANDS: usize = 64;
const MESSAGES: usize = 8_192;
const RUNS: usize = 256;

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
    let command_names = (0..COMMANDS)
        .map(|index| format!("command{index:03}"))
        .collect::<Vec<_>>();
    let indices = build_song_lua_message_command_indices(
        command_names
            .iter()
            .enumerate()
            .map(|(index, command)| (index, command.as_str())),
    );
    let messages = (0..MESSAGES)
        .map(|index| {
            let command = &command_names[index % command_names.len()];
            if index.is_multiple_of(2) {
                command.to_ascii_uppercase()
            } else {
                command.clone()
            }
        })
        .collect::<Vec<_>>();

    for message in &messages {
        assert_eq!(
            song_lua_message_command_index_legacy_for_bench(&indices, message),
            song_lua_message_command_index(&indices, message),
            "message lookup changed for {message:?}"
        );
    }

    let old = measure(|| {
        lookup_checksum(&messages, |message| {
            song_lua_message_command_index_legacy_for_bench(&indices, message)
        })
    });
    let new = measure(|| {
        lookup_checksum(&messages, |message| {
            song_lua_message_command_index(&indices, message)
        })
    });
    assert_eq!(old.checksum, new.checksum);

    println!("Song Lua message lookup ({MESSAGES} messages x {RUNS} runs)");
    print_result("old", &old);
    print_result("new", &new);
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}% | allocations reduction {:.1}% | bytes reduction {:.1}%",
        old.elapsed.as_secs_f64() / new.elapsed.as_secs_f64(),
        reduction(old.cycles, new.cycles),
        reduction(old.alloc.allocs, new.alloc.allocs),
        reduction(old.alloc.bytes, new.alloc.bytes),
    );
}

fn lookup_checksum(messages: &[String], mut lookup: impl FnMut(&str) -> Option<usize>) -> u64 {
    messages.iter().fold(0_u64, |checksum, message| {
        checksum.rotate_left(7) ^ lookup(black_box(message)).unwrap_or_default() as u64
    })
}

fn measure(mut work: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..4 {
        black_box(work());
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..RUNS {
        checksum = checksum.rotate_left(11) ^ black_box(work()) ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let lookups = (MESSAGES * RUNS) as f64;
    println!(
        "  {label:<4} {:>7.2} ns/lookup {:>7.2} cycles/lookup {:>8.1} Mlookups/s",
        result.elapsed.as_secs_f64() * 1.0e9 / lookups,
        result.cycles as f64 / lookups,
        lookups / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.2}/{:.2} per lookup, {:.1} bytes/lookup",
        result.alloc.allocs as f64 / lookups,
        result.alloc.reallocs as f64 / lookups,
        result.alloc.bytes as f64 / lookups,
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

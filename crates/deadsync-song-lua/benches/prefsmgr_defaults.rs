use deadsync_song_lua::{prefsmgr_default_value, prefsmgr_default_value_legacy_for_bench};
use mlua::{Lua, Value};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const RUNS: usize = 250_000;
const KEYS: [&str; 10] = [
    "globaloffsetseconds",
    "GlobalOffsetSeconds",
    "DISPLAYASPECTRATIO",
    "displaywidth",
    "DisplayHeight",
    "TimingWindowSecondsW1",
    "BGBrightness",
    "SongsPerPlay",
    "EventMode",
    "UnknownPreference",
];

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

type Lookup = fn(&Lua, &str, f32, f32, i32, i32) -> mlua::Result<Value>;

fn main() {
    let lua = Lua::new();
    for key in KEYS {
        let old = prefsmgr_default_value_legacy_for_bench(&lua, key, 0.02, 16.0 / 9.0, 1280, 720)
            .unwrap();
        let new = prefsmgr_default_value(&lua, key, 0.02, 16.0 / 9.0, 1280, 720).unwrap();
        assert_eq!(
            value_checksum(&old),
            value_checksum(&new),
            "value changed for {key:?}"
        );
    }

    let old = measure(&lua, prefsmgr_default_value_legacy_for_bench);
    let new = measure(&lua, prefsmgr_default_value);
    assert_eq!(old.checksum, new.checksum);

    println!(
        "Song Lua preference defaults ({} keys x {RUNS} runs)",
        KEYS.len()
    );
    print_result("old", &old);
    print_result("new", &new);
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

fn measure(lua: &Lua, lookup: Lookup) -> BenchResult {
    for _ in 0..1_000 {
        black_box(lookup_checksum(lua, lookup));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..RUNS {
        checksum = checksum.rotate_left(7)
            ^ black_box(lookup_checksum(black_box(lua), lookup))
            ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn lookup_checksum(lua: &Lua, lookup: Lookup) -> u64 {
    KEYS.iter().fold(0_u64, |checksum, key| {
        let value = lookup(lua, black_box(key), 0.02, 16.0 / 9.0, 1280, 720).unwrap();
        checksum.rotate_left(5) ^ value_checksum(&value)
    })
}

fn value_checksum(value: &Value) -> u64 {
    match value {
        Value::Nil => 1,
        Value::Boolean(value) => 2 ^ u64::from(*value),
        Value::Integer(value) => 3 ^ *value as u64,
        Value::Number(value) => 4 ^ value.to_bits(),
        Value::String(value) => value.as_bytes().iter().fold(5_u64, |checksum, byte| {
            checksum.rotate_left(3) ^ u64::from(*byte)
        }),
        _ => 6,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let lookups = (KEYS.len() * RUNS) as f64;
    println!(
        "  {label:<4} {:>7.2} ns/key {:>7.2} cycles/key {:>7.1} Mkeys/s",
        result.elapsed.as_secs_f64() * 1.0e9 / lookups,
        result.cycles as f64 / lookups,
        lookups / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.2}/{:.2} per key, {:.1} bytes/key",
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

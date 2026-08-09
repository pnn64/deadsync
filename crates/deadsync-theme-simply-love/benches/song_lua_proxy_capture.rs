use deadsync_theme_simply_love::screens::gameplay::{
    bench_field_proxy_direct, bench_field_proxy_materialized, bench_player_proxy_direct,
    bench_player_proxy_materialized, bench_song_lua_proxy_capture_cycles,
    bench_song_lua_proxy_capture_cycles_legacy, bench_song_lua_proxy_capture_cycles_screen_reuse,
    bench_song_lua_proxy_capture_cycles_single_bank,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const CYCLES: usize = 20_000;

struct CountingAlloc {
    allocs: AtomicU64,
    reallocs: AtomicU64,
    deallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            deallocs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            deallocs: self.deallocs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: all requests are forwarded unchanged to `System`; the atomics only
// observe successful allocation activity.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes directly from the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.deallocs.fetch_add(1, Ordering::Relaxed);
        // SAFETY: the caller supplies the live allocation and original layout.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller supplies the live pointer and its original layout.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            if new_size > old.size() {
                self.bytes
                    .fetch_add((new_size - old.size()) as u64, Ordering::Relaxed);
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
    bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            deallocs: self.deallocs - before.deallocs,
            bytes: self.bytes - before.bytes,
        }
    }

    fn saturating_sub(self, other: Self) -> Self {
        Self {
            allocs: self.allocs.saturating_sub(other.allocs),
            reallocs: self.reallocs.saturating_sub(other.reallocs),
            deallocs: self.deallocs.saturating_sub(other.deallocs),
            bytes: self.bytes.saturating_sub(other.bytes),
        }
    }
}

struct BenchResult {
    elapsed: Duration,
    cycles: u64,
    alloc: AllocSnapshot,
    checksum: usize,
}

fn main() {
    for players in [1, 2] {
        let old = measure(bench_song_lua_proxy_capture_cycles_legacy, players);
        let screen = measure(bench_song_lua_proxy_capture_cycles_screen_reuse, players);
        let single = measure(bench_song_lua_proxy_capture_cycles_single_bank, players);
        let new = measure(bench_song_lua_proxy_capture_cycles, players);
        assert_eq!(old.checksum, screen.checksum);
        assert_eq!(old.checksum, single.checksum);
        assert_eq!(old.checksum, new.checksum);
        println!("Song-Lua proxy capture: {players} player(s), {CYCLES} frames");
        print_result("old", &old);
        print_result("screen reuse", &screen);
        print_result("single bank", &single);
        print_result("double bank", &new);
        println!(
            "  screen speedup={:.2}x alloc reduction={:.2}% | double-bank speedup={:.2}x alloc reduction={:.2}%\n",
            old.elapsed.as_secs_f64() / screen.elapsed.as_secs_f64(),
            percent_reduction(old.alloc.allocs, screen.alloc.allocs),
            single.elapsed.as_secs_f64() / new.elapsed.as_secs_f64(),
            percent_reduction(single.alloc.allocs, new.alloc.allocs),
        );

        let materialized = measure(bench_player_proxy_materialized, players);
        let direct = measure(bench_player_proxy_direct, players);
        assert_eq!(materialized.checksum, direct.checksum);
        println!("Player proxy transfer: {players} player(s), {CYCLES} frames");
        print_result("materialized", &materialized);
        print_result("direct", &direct);
        println!(
            "  speedup={:.2}x cycle reduction={:.2}%\n",
            materialized.elapsed.as_secs_f64() / direct.elapsed.as_secs_f64(),
            percent_reduction(materialized.cycles, direct.cycles),
        );

        let materialized = measure(bench_field_proxy_materialized, players);
        let direct = measure(bench_field_proxy_direct, players);
        assert_eq!(materialized.checksum, direct.checksum);
        println!("Field proxy handoff: {players} player(s), {CYCLES} frames");
        print_result("materialized", &materialized);
        print_result("direct", &direct);
        println!(
            "  speedup={:.2}x cycle reduction={:.2}%\n",
            materialized.elapsed.as_secs_f64() / direct.elapsed.as_secs_f64(),
            percent_reduction(materialized.cycles, direct.cycles),
        );
    }
}

fn measure(run: fn(usize, usize) -> usize, players: usize) -> BenchResult {
    black_box(run(players, 32));
    let before_setup = ALLOC.snapshot();
    black_box(run(players, 0));
    let setup = ALLOC.snapshot().delta(before_setup);
    let before = ALLOC.snapshot();
    let started = Instant::now();
    let before_cycles = read_cycles();
    let checksum = black_box(run(players, CYCLES));
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        alloc: ALLOC.snapshot().delta(before).saturating_sub(setup),
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    println!(
        "  {label:<3} {:>8.2} us/frame {:>10.0} frames/s {:>9.0} cycles/frame  alloc/realloc/dealloc={:.2}/{:.2}/{:.2} per frame  bytes={:.0}/frame",
        result.elapsed.as_secs_f64() * 1.0e6 / CYCLES as f64,
        CYCLES as f64 / result.elapsed.as_secs_f64(),
        result.cycles as f64 / CYCLES as f64,
        result.alloc.allocs as f64 / CYCLES as f64,
        result.alloc.reallocs as f64 / CYCLES as f64,
        result.alloc.deallocs as f64 / CYCLES as f64,
        result.alloc.bytes as f64 / CYCLES as f64,
    );
}

fn percent_reduction(old: u64, new: u64) -> f64 {
    if old == 0 {
        0.0
    } else {
        old.saturating_sub(new) as f64 * 100.0 / old as f64
    }
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: `_rdtsc` only reads the processor timestamp counter.
    unsafe { core::arch::x86_64::_rdtsc() }
}

#[cfg(not(target_arch = "x86_64"))]
fn read_cycles() -> u64 {
    0
}

use deadsync_gameplay::{
    SongLuaColumnOffsetWindowRuntime, SongLuaEaseMaskTarget, SongLuaEaseMaskWindow,
    SongLuaOverlayEaseWindowRuntime, group_song_lua_overlay_eases,
    group_song_lua_overlay_eases_legacy_for_bench, song_lua_extend_column_offset_tails,
    song_lua_extend_column_offset_tails_legacy_for_bench, song_lua_extend_ease_tails,
    song_lua_extend_ease_tails_legacy_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const TAIL_WINDOWS: usize = 256;
const TAIL_RUNS: usize = 2_000;
const OVERLAY_WINDOWS: usize = 512;
const OVERLAY_COUNT: usize = 32;
const OVERLAY_RUNS: usize = 10_000;

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
    let ease_windows = ease_windows();
    assert_ease_parity(&ease_windows);
    let old_eases = measure(TAIL_RUNS, || legacy_ease_batch(&ease_windows));
    let new_eases = measure(TAIL_RUNS, || current_ease_batch(&ease_windows));
    assert_eq!(old_eases.checksum, new_eases.checksum);
    print_comparison(
        "Song Lua ease-tail resolution",
        TAIL_WINDOWS,
        TAIL_RUNS,
        &old_eases,
        &new_eases,
    );

    let column_windows = column_windows();
    assert_column_parity(&column_windows);
    let old_columns = measure(TAIL_RUNS, || legacy_column_batch(&column_windows));
    let new_columns = measure(TAIL_RUNS, || current_column_batch(&column_windows));
    assert_eq!(old_columns.checksum, new_columns.checksum);
    print_comparison(
        "Song Lua column-tail resolution",
        TAIL_WINDOWS,
        TAIL_RUNS,
        &old_columns,
        &new_columns,
    );

    let overlay_windows = overlay_windows();
    assert_eq!(
        group_song_lua_overlay_eases_legacy_for_bench(OVERLAY_COUNT, overlay_windows.clone()),
        group_song_lua_overlay_eases(OVERLAY_COUNT, overlay_windows.clone())
    );
    let old_overlays = measure(OVERLAY_RUNS, || legacy_overlay_batch(&overlay_windows));
    let new_overlays = measure(OVERLAY_RUNS, || current_overlay_batch(&overlay_windows));
    assert_eq!(old_overlays.checksum, new_overlays.checksum);
    print_comparison(
        "Song Lua overlay grouping",
        OVERLAY_WINDOWS,
        OVERLAY_RUNS,
        &old_overlays,
        &new_overlays,
    );
}

fn ease_windows() -> Vec<SongLuaEaseMaskWindow> {
    (0..TAIL_WINDOWS)
        .map(|index| {
            let slot = (index * 73) % TAIL_WINDOWS;
            let start =
                (slot / 2) as f32 * 0.25 + if slot.is_multiple_of(2) { 0.0 } else { 0.0005 };
            let end = start + 0.125 + (index % 3) as f32 * 0.05;
            let target = match index % 8 {
                0 => SongLuaEaseMaskTarget::AppearanceStealth,
                1 => SongLuaEaseMaskTarget::VisualDrunk,
                2 => SongLuaEaseMaskTarget::VisualBumpyColumn(index % 16),
                3 => SongLuaEaseMaskTarget::VisualMoveXColumn(index % 16),
                4 => SongLuaEaseMaskTarget::ScrollReverse,
                5 => SongLuaEaseMaskTarget::MiniPercent,
                6 => SongLuaEaseMaskTarget::PlayerX,
                _ => SongLuaEaseMaskTarget::PlayerRotationZ,
            };
            SongLuaEaseMaskWindow {
                start_second: start,
                end_second: end,
                sustain_end_second: if index.is_multiple_of(5) {
                    end + 3.0
                } else {
                    end
                },
                target,
                from: index as f32,
                to: index as f32 + 1.0,
                easing: None,
                opt1: None,
                opt2: None,
            }
        })
        .collect()
}

fn column_windows() -> Vec<SongLuaColumnOffsetWindowRuntime> {
    (0..TAIL_WINDOWS)
        .map(|index| {
            let slot = (index * 61) % TAIL_WINDOWS;
            let start =
                (slot / 2) as f32 * 0.125 + if slot.is_multiple_of(2) { 0.0 } else { 0.0005 };
            let end = start + 0.25;
            SongLuaColumnOffsetWindowRuntime {
                column: index % 16,
                start_second: start,
                end_second: end,
                sustain_end_second: if index.is_multiple_of(7) {
                    end + 2.0
                } else {
                    end
                },
                from_y: index as f32,
                to_y: index as f32 + 64.0,
                easing: None,
                opt1: None,
                opt2: None,
            }
        })
        .collect()
}

fn overlay_windows() -> Vec<SongLuaOverlayEaseWindowRuntime<u128>> {
    (0..OVERLAY_WINDOWS)
        .map(|index| {
            let slot = (index * 43) % OVERLAY_WINDOWS;
            let start = (slot / 3) as f32 * 0.25;
            SongLuaOverlayEaseWindowRuntime {
                overlay_index: index % (OVERLAY_COUNT + 4),
                start_second: start,
                end_second: start + (index % 5 + 1) as f32 * 0.1,
                sustain_end_second: start + (index % 7 + 1) as f32 * 0.2,
                cutoff_second: index.is_multiple_of(3).then_some(start + 1.0),
                from: 1_u128 << (index % 120),
                to: 1_u128 << ((index + 1) % 120),
                easing: None,
                opt1: None,
                opt2: None,
            }
        })
        .collect()
}

fn assert_ease_parity(fixture: &[SongLuaEaseMaskWindow]) {
    let mut old = fixture.to_vec();
    let mut new = fixture.to_vec();
    song_lua_extend_ease_tails_legacy_for_bench(&mut old, &[]);
    song_lua_extend_ease_tails(&mut new, &[]);
    for (old, new) in old.iter().zip(&new) {
        assert_eq!(
            old.sustain_end_second.to_bits(),
            new.sustain_end_second.to_bits()
        );
    }
}

fn assert_column_parity(fixture: &[SongLuaColumnOffsetWindowRuntime]) {
    let mut old = fixture.to_vec();
    let mut new = fixture.to_vec();
    song_lua_extend_column_offset_tails_legacy_for_bench(&mut old);
    song_lua_extend_column_offset_tails(&mut new);
    for (old, new) in old.iter().zip(&new) {
        assert_eq!(
            old.sustain_end_second.to_bits(),
            new.sustain_end_second.to_bits()
        );
    }
}

fn legacy_ease_batch(fixture: &[SongLuaEaseMaskWindow]) -> u64 {
    let mut windows = fixture.to_vec();
    song_lua_extend_ease_tails_legacy_for_bench(&mut windows, &[]);
    tail_checksum(windows.iter().map(|window| window.sustain_end_second))
}

fn current_ease_batch(fixture: &[SongLuaEaseMaskWindow]) -> u64 {
    let mut windows = fixture.to_vec();
    song_lua_extend_ease_tails(&mut windows, &[]);
    tail_checksum(windows.iter().map(|window| window.sustain_end_second))
}

fn legacy_column_batch(fixture: &[SongLuaColumnOffsetWindowRuntime]) -> u64 {
    let mut windows = fixture.to_vec();
    song_lua_extend_column_offset_tails_legacy_for_bench(&mut windows);
    tail_checksum(windows.iter().map(|window| window.sustain_end_second))
}

fn current_column_batch(fixture: &[SongLuaColumnOffsetWindowRuntime]) -> u64 {
    let mut windows = fixture.to_vec();
    song_lua_extend_column_offset_tails(&mut windows);
    tail_checksum(windows.iter().map(|window| window.sustain_end_second))
}

fn tail_checksum(values: impl IntoIterator<Item = f32>) -> u64 {
    values.into_iter().fold(0_u64, |checksum, value| {
        checksum.rotate_left(5) ^ u64::from(value.to_bits())
    })
}

fn legacy_overlay_batch(fixture: &[SongLuaOverlayEaseWindowRuntime<u128>]) -> u64 {
    let (windows, ranges) =
        group_song_lua_overlay_eases_legacy_for_bench(OVERLAY_COUNT, fixture.to_vec());
    overlay_checksum(&windows, &ranges)
}

fn current_overlay_batch(fixture: &[SongLuaOverlayEaseWindowRuntime<u128>]) -> u64 {
    let (windows, ranges) = group_song_lua_overlay_eases(OVERLAY_COUNT, fixture.to_vec());
    overlay_checksum(&windows, &ranges)
}

fn overlay_checksum(
    windows: &[SongLuaOverlayEaseWindowRuntime<u128>],
    ranges: &[std::ops::Range<usize>],
) -> u64 {
    let window_checksum = windows.iter().fold(0_u64, |checksum, window| {
        checksum.rotate_left(5)
            ^ window.overlay_index as u64
            ^ u64::from(window.start_second.to_bits())
            ^ window.from as u64
    });
    ranges.iter().fold(window_checksum, |checksum, range| {
        checksum.rotate_left(7) ^ range.start as u64 ^ ((range.end as u64) << 32)
    })
}

fn measure(runs: usize, mut operation: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..20 {
        black_box(operation());
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..runs {
        checksum = checksum.rotate_left(7) ^ black_box(operation()) ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_comparison(
    title: &str,
    windows: usize,
    runs: usize,
    old: &BenchResult,
    new: &BenchResult,
) {
    let operations = (windows * runs) as f64;
    println!("{title} ({windows} windows x {runs} runs)");
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

fn print_result(label: &str, result: &BenchResult, operations: f64) {
    println!(
        "  {label:<4} {:>8.2} ns/window {:>8.2} cycles/window {:>7.2} Mwindows/s",
        result.elapsed.as_secs_f64() * 1.0e9 / operations,
        result.cycles as f64 / operations,
        operations / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.4}/{:.4} per window, {:.1} bytes/window",
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

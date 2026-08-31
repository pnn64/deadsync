use deadsync_profile_gameplay::{
    build_song_lua_column_offset_windows_for_player,
    build_song_lua_column_offset_windows_for_player_reference,
    build_song_lua_constant_windows_for_player,
    build_song_lua_constant_windows_for_player_reference, build_song_lua_ease_windows_for_player,
    build_song_lua_ease_windows_for_player_reference,
};
use deadsync_rules::timing::{TimingData, TimingSegments};
use deadsync_song_lua::{
    CompiledSongLua, SongLuaColumnOffsetWindow, SongLuaColumnTransformTarget, SongLuaEaseTarget,
    SongLuaEaseWindow, SongLuaModWindow, SongLuaSpanMode, SongLuaTimeUnit,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

const WINDOWS: usize = 512;
const SAMPLES: usize = 21;
const OPS_PER_SAMPLE: usize = 8;

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

// SAFETY: allocation operations delegate unchanged to `System`; relaxed
// counters only observe this single-threaded benchmark while enabled.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` was supplied by the allocator caller.
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
        let ptr = unsafe { System.realloc(ptr, old, new_size) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.realloc_bytes
                .fetch_add((old.size() + new_size) as u64, Ordering::Relaxed);
        }
        ptr
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

    const fn churn_calls(self) -> u64 {
        self.allocs + self.reallocs + self.frees
    }

    const fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

struct Fixture {
    compiled: CompiledSongLua<()>,
    timing: TimingData,
}

struct Row {
    median_ns: f64,
    p95_ns: f64,
    median_cycles: Option<f64>,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn measure(mut operation: impl FnMut() -> u64) -> Row {
    for _ in 0..3 {
        black_box(operation());
    }

    let mut times = Vec::with_capacity(SAMPLES);
    let mut cycles = Vec::with_capacity(SAMPLES);
    let mut checksum = 0_u64;
    for _ in 0..SAMPLES {
        let cycle_start = cycle_counter();
        let started = Instant::now();
        for _ in 0..OPS_PER_SAMPLE {
            checksum = checksum.wrapping_add(black_box(operation()));
        }
        times.push(started.elapsed().as_secs_f64() * 1e9 / OPS_PER_SAMPLE as f64);
        if let Some(elapsed) = cycle_start
            .zip(cycle_counter())
            .map(|(start, end)| end.wrapping_sub(start) as f64 / OPS_PER_SAMPLE as f64)
        {
            cycles.push(elapsed);
        }
    }
    times.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let alloc_checksum = black_box(operation());
    ALLOC.enabled.store(false, Ordering::Relaxed);
    black_box(alloc_checksum);

    Row {
        median_ns: times[SAMPLES / 2],
        p95_ns: times[SAMPLES * 95 / 100],
        median_cycles: (!cycles.is_empty()).then(|| cycles[cycles.len() / 2]),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_pair(title: &str, items_per_op: usize, old: &Row, new: &Row) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert!(
        new.alloc.allocs < old.alloc.allocs,
        "{title} did not reduce allocations"
    );
    assert!(
        new.alloc.churn_calls() < old.alloc.churn_calls(),
        "{title} did not reduce allocation churn calls"
    );
    assert!(
        new.alloc.churn_bytes() < old.alloc.churn_bytes(),
        "{title} did not reduce allocation churn bytes"
    );
    assert!(
        new.median_ns < old.median_ns,
        "{title} did not improve median throughput"
    );
    assert!(
        new.p95_ns <= old.p95_ns * 1.10,
        "{title} regressed p95 latency by more than 10%"
    );
    if let (Some(old_cycles), Some(new_cycles)) = (old.median_cycles, new.median_cycles) {
        assert!(
            new_cycles < old_cycles,
            "{title} did not reduce median CPU cycles"
        );
    }

    println!("\n{title}");
    print_row("old", items_per_op, old);
    print_row("new", items_per_op, new);
    println!(
        "  change: {:>7.2}% median  {:>7.2}% cycles  {:>7.2}% throughput  \
         {:>7.2}% p95  {:>7.2}% allocs  {:>7.2}% churn calls  {:>7.2}% churn bytes",
        change(old.median_ns, new.median_ns),
        change(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN),
        ),
        change(throughput(items_per_op, old), throughput(items_per_op, new)),
        change(old.p95_ns, new.p95_ns),
        change(old.alloc.allocs as f64, new.alloc.allocs as f64),
        change(
            old.alloc.churn_calls() as f64,
            new.alloc.churn_calls() as f64,
        ),
        change(
            old.alloc.churn_bytes() as f64,
            new.alloc.churn_bytes() as f64,
        ),
    );
}

fn print_row(label: &str, items_per_op: usize, row: &Row) {
    println!(
        "  {label:<3} {:>11.0} ns/op  {:>11.0} cycles/op  {:>11.0} p95 ns  \
         {:>8.2} Mwindow/s  {:>5}/{:>3}/{:>5} a/r/f  {:>10} allocated B  {:>10} churn B",
        row.median_ns,
        row.median_cycles.unwrap_or(f64::NAN),
        row.p95_ns,
        throughput(items_per_op, row) / 1e6,
        row.alloc.allocs,
        row.alloc.reallocs,
        row.alloc.frees,
        row.alloc.alloc_bytes,
        row.alloc.churn_bytes(),
    );
}

fn throughput(items_per_op: usize, row: &Row) -> f64 {
    items_per_op as f64 * 1e9 / row.median_ns
}

fn change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

fn fixture() -> Fixture {
    let time_mods = (0..WINDOWS)
        .map(|index| mod_window(index, SongLuaTimeUnit::Second))
        .collect();
    let beat_mods = (0..WINDOWS)
        .map(|index| mod_window(index, SongLuaTimeUnit::Beat))
        .collect();
    let eases = (0..WINDOWS)
        .map(|index| SongLuaEaseWindow {
            player: target_player(index),
            unit: SongLuaTimeUnit::Second,
            start: index as f32 * 0.125,
            limit: 0.1,
            span_mode: SongLuaSpanMode::Len,
            target: match index % 4 {
                0 => SongLuaEaseTarget::Mod("drunk".to_string()),
                1 => SongLuaEaseTarget::Mod("confusionoffset".to_string()),
                2 => SongLuaEaseTarget::PlayerX,
                _ => SongLuaEaseTarget::Function,
            },
            from: index as f32 % 100.0,
            to: 100.0 - index as f32 % 100.0,
            easing: Some("inOutQuad".to_string()),
            sustain: Some(0.05),
            opt1: None,
            opt2: None,
        })
        .collect();
    let column_offsets = (0..WINDOWS)
        .map(|index| SongLuaColumnOffsetWindow {
            player: index % 2,
            column: index % 8,
            target: match index % 4 {
                0 => SongLuaColumnTransformTarget::OffsetX,
                1 => SongLuaColumnTransformTarget::OffsetY,
                2 => SongLuaColumnTransformTarget::Zoom,
                _ => SongLuaColumnTransformTarget::RotationZ,
            },
            unit: SongLuaTimeUnit::Second,
            start: index as f32 * 0.125,
            limit: 0.1,
            span_mode: SongLuaSpanMode::Len,
            from_y: index as f32,
            to_y: index as f32 + 8.0,
            easing: Some("inOutSine".to_string()),
            sustain: Some(0.05),
            opt1: None,
            opt2: None,
        })
        .collect();
    let compiled = CompiledSongLua {
        time_mods,
        beat_mods,
        eases,
        column_offsets,
        ..Default::default()
    };
    let rows = (0..=(WINDOWS * 6))
        .map(|row| row as f32 / 48.0)
        .collect::<Vec<_>>();
    let segments = TimingSegments {
        bpms: vec![(0.0, 120.0)],
        ..TimingSegments::default()
    };
    let timing = TimingData::from_segments(0.0, 0.0, &segments, &rows);
    Fixture { compiled, timing }
}

fn mod_window(index: usize, unit: SongLuaTimeUnit) -> SongLuaModWindow {
    SongLuaModWindow {
        unit,
        start: index as f32 * 0.125,
        limit: 0.1,
        span_mode: SongLuaSpanMode::Len,
        mods: "*100 25 drunk, *100 40 tipsy, *100 60 dark, *100 -25 flip".to_string(),
        player: target_player(index),
    }
}

fn target_player(index: usize) -> Option<u8> {
    match index % 3 {
        0 => None,
        1 => Some(1),
        _ => Some(2),
    }
}

fn constant_checksum(windows: &[deadsync_gameplay::AttackMaskWindow]) -> u64 {
    windows.iter().fold(windows.len() as u64, |sum, window| {
        sum.rotate_left(7)
            ^ u64::from(window.start_second.to_bits())
            ^ (u64::from(window.end_second.to_bits()) << 32)
    })
}

fn ease_checksum(windows: &[deadsync_gameplay::SongLuaEaseMaskWindow], unsupported: usize) -> u64 {
    windows.iter().fold(unsupported as u64, |sum, window| {
        sum.rotate_left(11)
            ^ u64::from(window.start_second.to_bits())
            ^ u64::from(window.end_second.to_bits()).rotate_left(29)
            ^ u64::from(window.to.to_bits())
    })
}

fn column_checksum(windows: &[deadsync_gameplay::SongLuaColumnOffsetWindowRuntime]) -> u64 {
    windows.iter().fold(windows.len() as u64, |sum, window| {
        sum.rotate_left(13)
            ^ window.column as u64
            ^ u64::from(window.start_second.to_bits())
            ^ u64::from(window.to_y.to_bits())
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
    let fixture = fixture();

    let old_constants = build_song_lua_constant_windows_for_player_reference(
        &fixture.compiled,
        &fixture.timing,
        0,
        0.025,
    );
    let new_constants =
        build_song_lua_constant_windows_for_player(&fixture.compiled, &fixture.timing, 0, 0.025);
    assert_eq!(old_constants, new_constants);
    let old = measure(|| {
        constant_checksum(&build_song_lua_constant_windows_for_player_reference(
            black_box(&fixture.compiled),
            black_box(&fixture.timing),
            0,
            0.025,
        ))
    });
    let new = measure(|| {
        constant_checksum(&build_song_lua_constant_windows_for_player(
            black_box(&fixture.compiled),
            black_box(&fixture.timing),
            0,
            0.025,
        ))
    });
    print_pair(
        "1. borrowed constant-window bridge",
        WINDOWS * 2,
        &old,
        &new,
    );

    let old_eases = build_song_lua_ease_windows_for_player_reference(
        &fixture.compiled,
        &fixture.timing,
        0,
        0.025,
        &[],
    );
    let new_eases =
        build_song_lua_ease_windows_for_player(&fixture.compiled, &fixture.timing, 0, 0.025, &[]);
    assert_eq!(old_eases, new_eases);
    let old = measure(|| {
        let (windows, unsupported) = build_song_lua_ease_windows_for_player_reference(
            black_box(&fixture.compiled),
            black_box(&fixture.timing),
            0,
            0.025,
            &[],
        );
        ease_checksum(&windows, unsupported)
    });
    let new = measure(|| {
        let (windows, unsupported) = build_song_lua_ease_windows_for_player(
            black_box(&fixture.compiled),
            black_box(&fixture.timing),
            0,
            0.025,
            &[],
        );
        ease_checksum(&windows, unsupported)
    });
    print_pair("2. borrowed ease-window bridge", WINDOWS, &old, &new);

    let old_columns = build_song_lua_column_offset_windows_for_player_reference(
        &fixture.compiled,
        &fixture.timing,
        0,
        0.025,
    );
    let new_columns = build_song_lua_column_offset_windows_for_player(
        &fixture.compiled,
        &fixture.timing,
        0,
        0.025,
    );
    assert_eq!(old_columns, new_columns);
    let old = measure(|| {
        column_checksum(&build_song_lua_column_offset_windows_for_player_reference(
            black_box(&fixture.compiled),
            black_box(&fixture.timing),
            0,
            0.025,
        ))
    });
    let new = measure(|| {
        column_checksum(&build_song_lua_column_offset_windows_for_player(
            black_box(&fixture.compiled),
            black_box(&fixture.timing),
            0,
            0.025,
        ))
    });
    print_pair("3. borrowed column-offset bridge", WINDOWS, &old, &new);
}

use deadsync_core::input::MAX_PLAYERS;
use deadsync_core::song_time::{SongTimeNs, song_time_ns_add_seconds};
use deadsync_core::timing::ROWS_PER_BEAT;
use deadsync_gameplay::{
    GameplayTimeToBeatCaches, assist_lookahead_future_row, assist_row_no_offset_for_timing,
    lane_search_rows_for_timing, missed_note_cutoff_rows_for_players, visible_notefield_time_ns,
};
use deadsync_rules::timing::{
    BeatInfoCache, DelaySegment, StopSegment, TimingData, TimingProfile, TimingSegments,
    WarpSegment,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::mem::size_of;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const FRAMES: usize = 8_192;
const CHORDS: usize = 4_096;

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

// SAFETY: every operation delegates to `System` with the caller's original
// pointer and layout; independent atomics only observe allocation churn.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is forwarded unchanged from the allocator caller.
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
        // SAFETY: the caller guarantees that `ptr` and `layout` identify a live
        // allocation made through this allocator, which delegates to `System`.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees the old allocation is live and valid;
        // the request is delegated unchanged to `System`.
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
}

struct BenchResult {
    elapsed: Duration,
    cycles: Option<u64>,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn main() {
    println!("gameplay time-to-beat cache microbenchmark");
    run_suite("moderate chart", 16, false);
    run_suite("timing-event stress chart", 1_024, true);
    println!(
        "fixed cache state: {} bytes (old authoritative cursor: {} bytes)",
        size_of::<GameplayTimeToBeatCaches>(),
        size_of::<BeatInfoCache>(),
    );
}

fn run_suite(label: &str, bpm_segments: usize, include_assist: bool) {
    let timing = eventful_timing(0.008, 1.0, bpm_segments);
    let player_two = eventful_timing(-0.013, 1.075, bpm_segments);
    let players = [&timing, &player_two];
    let times: Vec<_> = (0..FRAMES)
        .map(|frame| timing.get_time_for_beat_ns(frame as f32 * 0.25))
        .collect();

    println!("\n{label}: {FRAMES} frames, {bpm_segments} BPM segments plus stops/delays/warps");
    run_frame_pair(&timing, &players, &times, 1, false);
    run_frame_pair(&timing, &players, &times, 2, false);
    if include_assist {
        run_frame_pair(&timing, &players, &times, 1, true);
    }
    run_input_pair(&timing, &players, &times[..CHORDS]);
}

fn run_frame_pair(
    timing: &TimingData,
    players: &[&TimingData; MAX_PLAYERS],
    times: &[SongTimeNs],
    num_players: usize,
    assist_enabled: bool,
) {
    let old = run_old_frames(timing, players, times, num_players, assist_enabled);
    let new = run_cached_frames(timing, players, times, num_players, assist_enabled);
    assert_eq!(old.checksum, new.checksum);
    let label = if assist_enabled {
        "1P frame, assist on"
    } else if num_players == 1 {
        "1P frame, assist off"
    } else {
        "2P frame, assist off"
    };
    print_pair(label, times.len(), &old, &new);
}

fn run_old_frames(
    timing: &TimingData,
    players: &[&TimingData; MAX_PLAYERS],
    times: &[SongTimeNs],
    num_players: usize,
    assist_enabled: bool,
) -> BenchResult {
    let profile = TimingProfile::default_itg_with_fa_plus();
    let mut song_cache = BeatInfoCache::new(timing);
    measure(|| {
        let mut checksum = 0_u64;
        for &time_ns in times {
            let info = timing.get_beat_info_from_time_ns_cached(time_ns, &mut song_cache);
            mix_beat_info(
                &mut checksum,
                info.beat,
                info.is_in_freeze,
                info.is_in_delay,
            );
            let display_time_ns = time_ns.saturating_sub(7_000_000);
            mix_f32(&mut checksum, timing.get_beat_for_time_ns(display_time_ns));
            let song_row = assist_row_no_offset_for_timing(timing, 0.008, time_ns);
            mix_usize(&mut checksum, song_row.max(0) as usize);
            let future_row = black_box(assist_lookahead_future_row(
                timing, 0.008, 0.030, time_ns, 1.0, song_row,
            ));
            if assist_enabled {
                mix_usize(&mut checksum, future_row.max(0) as usize);
            }
            for (player, timing_player) in players.iter().take(num_players).enumerate() {
                let visible_time_ns =
                    visible_notefield_time_ns(time_ns, 0.011 * (player as f32 + 1.0));
                mix_f32(
                    &mut checksum,
                    timing_player.get_beat_for_time_ns(visible_time_ns),
                );
            }
            for _ in 0..2 {
                let rows = missed_note_cutoff_rows_for_players(
                    &profile,
                    players,
                    1.0,
                    time_ns,
                    num_players,
                );
                for &row in rows.iter().take(num_players) {
                    mix_usize(&mut checksum, row);
                }
            }
        }
        checksum
    })
}

fn run_cached_frames(
    timing: &TimingData,
    players: &[&TimingData; MAX_PLAYERS],
    times: &[SongTimeNs],
    num_players: usize,
    assist_enabled: bool,
) -> BenchResult {
    let profile = TimingProfile::default_itg_with_fa_plus();
    let mut cache = GameplayTimeToBeatCaches::new(timing, players);
    measure(|| {
        let mut checksum = 0_u64;
        for &time_ns in times {
            let info = cache.song_info(timing, time_ns);
            mix_beat_info(
                &mut checksum,
                info.beat,
                info.is_in_freeze,
                info.is_in_delay,
            );
            let display_time_ns = time_ns.saturating_sub(7_000_000);
            mix_f32(&mut checksum, cache.display_beat(timing, display_time_ns));
            let song_row = cache.assist_row_no_offset(timing, 0.008, time_ns);
            mix_usize(&mut checksum, song_row.max(0) as usize);
            if assist_enabled {
                let future_row =
                    cache.assist_future_row(timing, 0.008, 0.030, time_ns, 1.0, song_row);
                mix_usize(&mut checksum, future_row.max(0) as usize);
            }
            for (player, timing_player) in players.iter().take(num_players).enumerate() {
                let visible_time_ns =
                    visible_notefield_time_ns(time_ns, 0.011 * (player as f32 + 1.0));
                mix_f32(
                    &mut checksum,
                    cache.visible_beat(player, timing_player, visible_time_ns),
                );
            }
            for _ in 0..2 {
                let rows =
                    cache.missed_note_cutoff_rows(&profile, players, 1.0, time_ns, num_players);
                for &row in rows.iter().take(num_players) {
                    mix_usize(&mut checksum, row);
                }
            }
        }
        checksum
    })
}

fn run_input_pair(timing: &TimingData, players: &[&TimingData; MAX_PLAYERS], times: &[SongTimeNs]) {
    let old = measure(|| {
        let mut checksum = 0_u64;
        for &press_time_ns in times {
            let release_time_ns = song_time_ns_add_seconds(press_time_ns, 0.025);
            for time_ns in [press_time_ns, release_time_ns] {
                for _ in 0..4 {
                    let rows = lane_search_rows_for_timing(timing, time_ns);
                    mix_usize(&mut checksum, rows.current);
                    mix_usize(&mut checksum, rows.start);
                    mix_usize(&mut checksum, rows.end);
                }
            }
        }
        checksum
    });
    let mut cache = GameplayTimeToBeatCaches::new(timing, players);
    let new = measure(|| {
        let mut checksum = 0_u64;
        for &press_time_ns in times {
            let release_time_ns = song_time_ns_add_seconds(press_time_ns, 0.025);
            for time_ns in [press_time_ns, release_time_ns] {
                for _ in 0..4 {
                    let rows = cache.lane_search_rows(0, timing, time_ns);
                    mix_usize(&mut checksum, rows.current);
                    mix_usize(&mut checksum, rows.start);
                    mix_usize(&mut checksum, rows.end);
                }
            }
        }
        checksum
    });
    assert_eq!(old.checksum, new.checksum);
    print_pair("4-panel chord press/release", times.len(), &old, &new);
}

fn measure(work: impl FnOnce() -> u64) -> BenchResult {
    let alloc_before = ALLOC.snapshot();
    let cycles_before = thread_cycles();
    let started = Instant::now();
    let checksum = black_box(work());
    let elapsed = started.elapsed();
    let cycles = cycles_before
        .zip(thread_cycles())
        .map(|(before, after)| after - before);
    BenchResult {
        elapsed,
        cycles,
        alloc: ALLOC.snapshot().delta(alloc_before),
        checksum,
    }
}

fn print_pair(label: &str, operations: usize, old: &BenchResult, new: &BenchResult) {
    println!("\n{label} ({operations} operations)");
    print_result("uncached baseline", operations, old);
    print_result("cached streams", operations, new);
    println!(
        "  speedup {:>7.2}x, elapsed reduction {:>6.2}%",
        old.elapsed.as_secs_f64() / new.elapsed.as_secs_f64(),
        100.0 * (1.0 - new.elapsed.as_secs_f64() / old.elapsed.as_secs_f64()),
    );
}

fn print_result(label: &str, operations: usize, result: &BenchResult) {
    let ops = operations as f64;
    let cycles = result
        .cycles
        .map(|cycles| format!("{:>10.1} cycles/op", cycles as f64 / ops))
        .unwrap_or_else(|| "cycles unavailable".to_owned());
    println!(
        "  {label:<18} {:>10.1} ns/op  {:>11.0} op/s  {cycles}  \
         alloc/realloc/free={}/{}/{} bytes={}",
        result.elapsed.as_secs_f64() * 1.0e9 / ops,
        ops / result.elapsed.as_secs_f64(),
        result.alloc.allocs,
        result.alloc.reallocs,
        result.alloc.deallocs,
        result.alloc.bytes,
    );
}

fn eventful_timing(global_offset: f32, bpm_scale: f32, bpm_segments: usize) -> TimingData {
    let beat_count = FRAMES / 4;
    let bpm_step = beat_count / bpm_segments.max(1);
    let bpms = (0..bpm_segments)
        .map(|index| {
            let bpm = 90.0 + (index % 11) as f32 * 13.0;
            (index.saturating_mul(bpm_step) as f32, bpm * bpm_scale)
        })
        .collect();
    let stops = (1..beat_count / 64)
        .map(|index| StopSegment {
            beat: index as f32 * 64.0 - 8.0,
            duration: 0.015 + (index % 3) as f32 * 0.005,
        })
        .collect();
    let delays = (1..beat_count / 96)
        .map(|index| DelaySegment {
            beat: index as f32 * 96.0 - 12.0,
            duration: 0.010 + (index % 4) as f32 * 0.004,
        })
        .collect();
    let warps = (1..beat_count / 160)
        .map(|index| WarpSegment {
            beat: index as f32 * 160.0 - 20.0,
            length: 0.5,
        })
        .collect();
    let row_to_beat: Vec<_> = (0..=beat_count * ROWS_PER_BEAT as usize)
        .map(|row| row as f32 / ROWS_PER_BEAT as f32)
        .collect();
    TimingData::from_segments(
        0.0,
        global_offset,
        &TimingSegments {
            bpms,
            stops,
            delays,
            warps,
            ..TimingSegments::default()
        },
        &row_to_beat,
    )
}

#[inline(always)]
fn mix_f32(checksum: &mut u64, value: f32) {
    *checksum = checksum.rotate_left(7) ^ u64::from(value.to_bits());
}

#[inline(always)]
fn mix_usize(checksum: &mut u64, value: usize) {
    *checksum = checksum.rotate_left(7) ^ value as u64;
}

#[inline(always)]
fn mix_beat_info(checksum: &mut u64, beat: f32, freeze: bool, delay: bool) {
    mix_f32(checksum, beat);
    *checksum ^= u64::from(freeze) << 62 | u64::from(delay) << 63;
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentThread() -> isize;
    fn QueryThreadCycleTime(thread: isize, cycles: *mut u64) -> i32;
}

#[cfg(windows)]
fn thread_cycles() -> Option<u64> {
    let mut cycles = 0_u64;
    // SAFETY: `GetCurrentThread` returns a valid pseudo-handle for the calling
    // thread, and `cycles` is a valid writable `u64` for the duration of the call.
    let ok = unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut cycles) };
    (ok != 0).then_some(cycles)
}

#[cfg(not(windows))]
fn thread_cycles() -> Option<u64> {
    None
}

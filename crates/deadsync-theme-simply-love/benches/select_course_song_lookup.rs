use deadsync_chart::{ChartData, SongData, SongPack, SyncPref};
use deadsync_theme_simply_love::screens::select_course::{
    select_course_song_lookup_workload_for_bench,
    select_course_song_lookup_workload_legacy_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const PACKS: usize = 64;
const SONGS_PER_PACK: usize = 32;
const CHARTS_PER_SONG: usize = 5;
const PLAYED_CHARTS: usize = 512;
const RUNS: usize = 200;

type Workload = fn(&[SongPack], &[(String, u32)]) -> u64;

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
    let packs = fixture_packs();
    let played = fixture_played_counts(&packs);
    assert_eq!(
        select_course_song_lookup_workload_legacy_for_bench(&packs, &played),
        select_course_song_lookup_workload_for_bench(&packs, &played),
    );

    let old = measure(
        &packs,
        &played,
        select_course_song_lookup_workload_legacy_for_bench,
    );
    let new = measure(
        &packs,
        &played,
        select_course_song_lookup_workload_for_bench,
    );
    assert_eq!(old.checksum, new.checksum);

    println!(
        "Select Course song lookup ({PACKS} packs x {SONGS_PER_PACK} songs x {CHARTS_PER_SONG} charts, {} play records x {RUNS} runs)",
        played.len()
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

fn fixture_packs() -> Vec<SongPack> {
    (0..PACKS)
        .map(|pack_index| {
            let songs = (0..SONGS_PER_PACK)
                .map(|song_index| {
                    let charts = (0..CHARTS_PER_SONG)
                        .map(|chart_index| {
                            let hash = if chart_index == 0 && song_index % 16 == 0 {
                                format!("shared-{song_index:02}")
                            } else {
                                format!("chart-{pack_index:02}-{song_index:02}-{chart_index:02}")
                            };
                            chart(hash)
                        })
                        .collect();
                    Arc::new(song(
                        format!("Songs/Pack {pack_index:02}/Song {song_index:02}/chart.ssc"),
                        charts,
                    ))
                })
                .collect();
            SongPack {
                group_name: format!("Pack {pack_index:02}"),
                name: format!("Pack {pack_index:02}"),
                sort_title: String::new(),
                translit_title: String::new(),
                series: String::new(),
                year: 0,
                sync_pref: SyncPref::Default,
                directory: PathBuf::from(format!("Songs/Pack {pack_index:02}")),
                banner_path: None,
                songs,
            }
        })
        .collect()
}

fn fixture_played_counts(packs: &[SongPack]) -> Vec<(String, u32)> {
    let mut played = Vec::with_capacity(PLAYED_CHARTS + PLAYED_CHARTS / 31 + 64);
    let total_songs = PACKS * SONGS_PER_PACK;
    for index in 0..PLAYED_CHARTS {
        let flat_song = index * 977 % total_songs;
        let pack = flat_song / SONGS_PER_PACK;
        let song = flat_song % SONGS_PER_PACK;
        let chart = index * 3 % CHARTS_PER_SONG;
        let hash = packs[pack].songs[song].charts[chart].short_hash.clone();
        played.push((hash.clone(), (index as u32 % 19) + 1));
        if index % 31 == 0 {
            played.push((hash, 7));
        }
    }
    for index in 0..64 {
        played.push((format!("missing-chart-{index:02}"), 99));
    }
    played
}

fn chart(short_hash: String) -> ChartData {
    ChartData {
        chart_type: "dance-single".to_owned(),
        difficulty: "Hard".to_owned(),
        description: String::new(),
        chart_name: String::new(),
        meter: 10,
        step_artist: String::new(),
        music_path: None,
        short_hash,
        stats: Default::default(),
        tech_counts: Default::default(),
        mines_nonfake: 0,
        stamina_counts: Default::default(),
        total_streams: 0,
        matrix_rating: 0.0,
        max_nps: 0.0,
        sn_detailed_breakdown: String::new(),
        sn_partial_breakdown: String::new(),
        sn_simple_breakdown: String::new(),
        detailed_breakdown: String::new(),
        partial_breakdown: String::new(),
        simple_breakdown: String::new(),
        total_measures: 0,
        measure_nps_vec: Vec::new(),
        measure_seconds_vec: Vec::new(),
        first_second: 0.0,
        has_note_data: true,
        has_chart_attacks: false,
        possible_grade_points: 0,
        holds_total: 0,
        rolls_total: 0,
        mines_total: 0,
        display_bpm: None,
        min_bpm: 120.0,
        max_bpm: 120.0,
    }
}

fn song(simfile_path: String, charts: Vec<ChartData>) -> SongData {
    SongData {
        simfile_path: PathBuf::from(simfile_path),
        title: String::new(),
        subtitle: String::new(),
        translit_title: String::new(),
        translit_subtitle: String::new(),
        artist: String::new(),
        genre: String::new(),
        banner_path: None,
        background_path: None,
        background_changes: Vec::new(),
        background_layer2_changes: Vec::new(),
        foreground_changes: Vec::new(),
        background_lua_changes: Vec::new(),
        foreground_lua_changes: Vec::new(),
        has_lua: false,
        cdtitle_path: None,
        music_path: None,
        display_bpm: String::new(),
        offset: 0.0,
        sample_start: None,
        sample_length: None,
        min_bpm: 0.0,
        max_bpm: 0.0,
        normalized_bpms: String::new(),
        music_length_seconds: 0.0,
        first_second: 0.0,
        total_length_seconds: 0,
        precise_last_second_seconds: 0.0,
        charts,
    }
}

fn measure(packs: &[SongPack], played: &[(String, u32)], workload: Workload) -> BenchResult {
    for _ in 0..3 {
        black_box(workload(packs, played));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..RUNS {
        checksum = checksum.rotate_left(7) ^ black_box(workload(packs, played)) ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let runs = RUNS as f64;
    let charts = (PACKS * SONGS_PER_PACK * CHARTS_PER_SONG * RUNS) as f64;
    println!(
        "  {label:<4} {:>7.2} ms/run {:>9.0} cycles/run {:>7.1} Mcharts/s",
        result.elapsed.as_secs_f64() * 1.0e3 / runs,
        result.cycles as f64 / runs,
        charts / result.elapsed.as_secs_f64() / 1.0e6,
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

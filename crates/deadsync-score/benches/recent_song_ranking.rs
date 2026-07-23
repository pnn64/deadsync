use deadsync_chart::{ArrowStats, ChartData, SongData, StaminaCounts, TechCounts};
use deadsync_score::select_music::{
    ranked_recent_song_indices_for_bench, ranked_recent_song_indices_legacy_for_bench,
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

const SONGS: usize = 8_000;
const CHARTS_PER_SONG: usize = 4;
const RECENT_LIMIT: usize = 20;
const RUNS: usize = 100;

type Ranker = fn(&[Arc<SongData>], &[&str], usize) -> Vec<usize>;

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
    let songs = fixture_songs();
    let recent = recent_hashes(&songs);
    assert_eq!(
        ranked_recent_song_indices_legacy_for_bench(&songs, &recent, RECENT_LIMIT),
        ranked_recent_song_indices_for_bench(&songs, &recent, RECENT_LIMIT)
    );

    let old = measure(&songs, &recent, ranked_recent_song_indices_legacy_for_bench);
    let new = measure(&songs, &recent, ranked_recent_song_indices_for_bench);
    assert_eq!(old.checksum, new.checksum);

    println!(
        "recent-song ranking ({SONGS} songs x {CHARTS_PER_SONG} charts, {} probes, limit {RECENT_LIMIT}, {RUNS} runs)",
        recent.len()
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

fn fixture_songs() -> Vec<Arc<SongData>> {
    (0..SONGS)
        .map(|song_index| {
            let charts = (0..CHARTS_PER_SONG)
                .map(|chart_index| chart(format!("hash-{song_index:05}-{chart_index}")))
                .collect();
            Arc::new(song(charts))
        })
        .collect()
}

fn recent_hashes(songs: &[Arc<SongData>]) -> Vec<&str> {
    let mut recent = Vec::with_capacity(64);
    recent.push("missing-chart-hash");
    for song in songs.iter().rev().take(32) {
        recent.push(song.charts[1].short_hash.as_str());
        recent.push(song.charts[0].short_hash.as_str());
    }
    recent
}

fn chart(short_hash: String) -> ChartData {
    ChartData {
        chart_type: "dance-single".to_string(),
        difficulty: "Hard".to_string(),
        description: String::new(),
        chart_name: String::new(),
        meter: 9,
        step_artist: String::new(),
        music_path: None,
        short_hash,
        stats: ArrowStats::default(),
        tech_counts: TechCounts::default(),
        mines_nonfake: 0,
        stamina_counts: StaminaCounts::default(),
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

fn song(charts: Vec<ChartData>) -> SongData {
    SongData {
        simfile_path: PathBuf::new(),
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

fn measure(songs: &[Arc<SongData>], recent: &[&str], rank: Ranker) -> BenchResult {
    for _ in 0..5 {
        black_box(rank(songs, recent, RECENT_LIMIT));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..RUNS {
        let ranked = rank(black_box(songs), black_box(recent), RECENT_LIMIT);
        checksum = checksum.rotate_left(7)
            ^ black_box(ranked.len() as u64)
            ^ ranked.first().copied().unwrap_or(0) as u64
            ^ (ranked.last().copied().unwrap_or(0) as u64) << 32
            ^ run as u64;
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
    let charts = (SONGS * CHARTS_PER_SONG * RUNS) as f64;
    println!(
        "  {label:<4} {:>7.2} ms/run {:>9.0} cycles/run {:>7.1} Mcharts/s",
        result.elapsed.as_secs_f64() * 1.0e3 / runs,
        result.cycles as f64 / runs,
        charts / result.elapsed.as_secs_f64() / 1.0e6,
    );
    println!(
        "       alloc/realloc={:.2}/{:.2} per run, {:.1} KiB/run",
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

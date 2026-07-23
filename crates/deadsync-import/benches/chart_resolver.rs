use deadsync_chart::{
    ArrowStats, ChartData, SongData, SongPack, StaminaCounts, SyncPref, TechCounts,
};
use deadsync_import::resolver::{
    chart_resolver_workload_for_bench, chart_resolver_workload_legacy_for_bench,
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

const PACKS: usize = 48;
const SONGS_PER_PACK: usize = 24;
const PASSES: usize = 10;
const RUNS: usize = 200;

type Query<'a> = (&'a str, &'a str, &'a str, &'a str);
type Resolver = fn(&[SongPack], &[Query<'_>], usize) -> u64;

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
    let query_paths = fixture_query_paths();
    let queries = fixture_queries(&query_paths);
    assert_eq!(
        chart_resolver_workload_legacy_for_bench(&packs, &queries, PASSES),
        chart_resolver_workload_for_bench(&packs, &queries, PASSES)
    );

    let old = measure(&packs, &queries, chart_resolver_workload_legacy_for_bench);
    let new = measure(&packs, &queries, chart_resolver_workload_for_bench);
    assert_eq!(old.checksum, new.checksum);

    println!(
        "ITG chart resolver ({PACKS} packs x {SONGS_PER_PACK} songs, {} queries x {PASSES} passes x {RUNS} runs)",
        queries.len()
    );
    print_result("old", &old, queries.len());
    print_result("new", &new, queries.len());
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
                    Arc::new(song(
                        format!("Songs/Directory {pack_index:02}/Song {song_index:02}/chart.ssc"),
                        vec![
                            chart("Hard", "", format!("hard-{pack_index:02}-{song_index:02}")),
                            chart(
                                "Challenge",
                                "",
                                format!("challenge-{pack_index:02}-{song_index:02}"),
                            ),
                            chart(
                                "Edit",
                                "Edit Alpha",
                                format!("edit-a-{pack_index:02}-{song_index:02}"),
                            ),
                            chart(
                                "Edit",
                                "Edit Beta",
                                format!("edit-b-{pack_index:02}-{song_index:02}"),
                            ),
                        ],
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
                directory: PathBuf::from(format!("Songs/Directory {pack_index:02}")),
                banner_path: None,
                songs,
            }
        })
        .collect()
}

fn fixture_query_paths() -> Vec<String> {
    let mut paths = Vec::with_capacity(65);
    for index in 0..64 {
        let pack = index * 17 % PACKS;
        let song = index * 11 % SONGS_PER_PACK;
        let pack_name = if index % 2 == 0 {
            format!("PACK {pack:02}")
        } else {
            format!("directory {pack:02}")
        };
        paths.push(format!("Songs/{pack_name}/Song {song:02}/"));
    }
    paths.push("Songs/Missing Pack/Ghost Song/".to_string());
    paths
}

fn fixture_queries(paths: &[String]) -> Vec<Query<'_>> {
    paths
        .iter()
        .enumerate()
        .map(|(index, path)| match index % 4 {
            0 => (path.as_str(), "dance-single", "Hard", ""),
            1 => (path.as_str(), "DANCE-SINGLE", "challenge", ""),
            2 => (path.as_str(), "dance-single", "Edit", "edit alpha"),
            _ => (path.as_str(), "dance-single", "Edit", "missing edit"),
        })
        .collect()
}

fn chart(difficulty: &str, description: &str, short_hash: String) -> ChartData {
    ChartData {
        chart_type: "dance-single".to_string(),
        difficulty: difficulty.to_string(),
        description: description.to_string(),
        chart_name: String::new(),
        meter: 10,
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

fn measure(packs: &[SongPack], queries: &[Query<'_>], resolve: Resolver) -> BenchResult {
    for _ in 0..5 {
        black_box(resolve(packs, queries, PASSES));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..RUNS {
        checksum =
            checksum.rotate_left(7) ^ black_box(resolve(packs, queries, PASSES)) ^ run as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult, query_count: usize) {
    let runs = RUNS as f64;
    let lookups = (query_count * PASSES * RUNS) as f64;
    println!(
        "  {label:<4} {:>7.2} ms/run {:>9.0} cycles/run {:>7.1} Mlookups/s",
        result.elapsed.as_secs_f64() * 1.0e3 / runs,
        result.cycles as f64 / runs,
        lookups / result.elapsed.as_secs_f64() / 1.0e6,
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

use deadsync_chart::{ArrowStats, ChartData, SongData, StaminaCounts, TechCounts};
use deadsync_simfile::song_search::{
    SongSearchCandidate, SongSearchCatalogEntry, build_song_search_candidates,
    build_song_search_candidates_legacy,
};
use deadsync_simfile::song_sort::{GroupedSongs, title_grouped_songs, title_grouped_songs_legacy};
use deadsync_simfile::tags::{latest_simfile_tag_value_legacy, latest_simfile_tag_values};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SONGS: usize = 8_192;
const TAG_BYTES: usize = 512 * 1024;
const TAG_ITERATIONS: usize = 256;
const SEARCH_ITERATIONS: usize = 64;
const SORT_ITERATIONS: usize = 12;

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

// SAFETY: every operation delegates to `System` with the caller-provided
// pointer and layout; the independent atomics only observe successful calls.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is forwarded unchanged to the system allocator.
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.deallocs.fetch_add(1, Ordering::Relaxed);
        // SAFETY: the caller provides the allocation's original layout.
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
    cycles: u64,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn main() {
    let simfile = benchmark_simfile();
    benchmark_tag_batch(&simfile);

    let songs: Vec<_> = (0..SONGS).map(benchmark_song).collect();
    benchmark_search(&songs);
    benchmark_sort(&songs);
}

fn benchmark_tag_batch(simfile: &[u8]) {
    let old_values = [
        latest_simfile_tag_value_legacy(simfile, b"#CDIMAGE:"),
        latest_simfile_tag_value_legacy(simfile, b"#DISCIMAGE:"),
    ];
    let new_values = latest_simfile_tag_values(
        simfile,
        [b"#CDIMAGE:".as_slice(), b"#DISCIMAGE:".as_slice()],
    );
    assert_eq!(old_values, new_values, "tag extraction output changed");

    let old = measure(TAG_ITERATIONS, || {
        let cdimage = latest_simfile_tag_value_legacy(black_box(simfile), b"#CDIMAGE:");
        let discimage = latest_simfile_tag_value_legacy(black_box(simfile), b"#DISCIMAGE:");
        text_checksum(&cdimage) ^ text_checksum(&discimage).rotate_left(17)
    });
    let new = measure(TAG_ITERATIONS, || {
        let [cdimage, discimage] = latest_simfile_tag_values(
            black_box(simfile),
            [b"#CDIMAGE:".as_slice(), b"#DISCIMAGE:".as_slice()],
        );
        text_checksum(&cdimage) ^ text_checksum(&discimage).rotate_left(17)
    });
    assert_eq!(old.checksum, new.checksum);
    print_pair(
        "batched artwork tags",
        simfile.len(),
        TAG_ITERATIONS,
        "bytes",
        &old,
        &new,
    );
}

fn benchmark_search(songs: &[Arc<SongData>]) {
    const QUERY: &str = "PERFORMANCE PACK/song 04";
    let old_candidates =
        build_song_search_candidates_legacy(search_entries(songs), QUERY, "dance-single");
    let new_candidates = build_song_search_candidates(search_entries(songs), QUERY, "dance-single");
    assert_eq!(
        candidate_paths(&old_candidates),
        candidate_paths(&new_candidates),
        "search candidates changed"
    );

    let old = measure(SEARCH_ITERATIONS, || {
        candidate_checksum(&build_song_search_candidates_legacy(
            search_entries(black_box(songs)),
            QUERY,
            "dance-single",
        ))
    });
    let new = measure(SEARCH_ITERATIONS, || {
        candidate_checksum(&build_song_search_candidates(
            search_entries(black_box(songs)),
            QUERY,
            "dance-single",
        ))
    });
    assert_eq!(old.checksum, new.checksum);
    print_pair(
        "allocation-free search matching",
        songs.len(),
        SEARCH_ITERATIONS,
        "songs",
        &old,
        &new,
    );
}

fn benchmark_sort(songs: &[Arc<SongData>]) {
    let old_groups = title_grouped_songs_legacy(songs.to_vec());
    let new_groups = title_grouped_songs(songs.to_vec());
    assert_eq!(
        grouped_paths(&old_groups),
        grouped_paths(&new_groups),
        "title sort order changed"
    );

    let old = measure(SORT_ITERATIONS, || {
        grouped_checksum(&title_grouped_songs_legacy(black_box(songs).to_vec()))
    });
    let new = measure(SORT_ITERATIONS, || {
        grouped_checksum(&title_grouped_songs(black_box(songs).to_vec()))
    });
    assert_eq!(old.checksum, new.checksum);
    print_pair(
        "allocation-free title sort",
        songs.len(),
        SORT_ITERATIONS,
        "songs",
        &old,
        &new,
    );
}

fn search_entries(songs: &[Arc<SongData>]) -> impl Iterator<Item = SongSearchCatalogEntry<'_>> {
    std::iter::once(SongSearchCatalogEntry::PackHeader("Performance Pack"))
        .chain(songs.iter().map(SongSearchCatalogEntry::Song))
}

fn measure(iterations: usize, mut work: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..2 {
        black_box(work());
    }
    let alloc_before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for iteration in 0..iterations {
        checksum = checksum.rotate_left(7) ^ black_box(work()) ^ iteration as u64;
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(cycles_before),
        alloc: ALLOC.snapshot().delta(alloc_before),
        checksum,
    }
}

fn print_pair(
    name: &str,
    units_per_iteration: usize,
    iterations: usize,
    unit: &str,
    old: &BenchResult,
    new: &BenchResult,
) {
    println!("\n{name} ({units_per_iteration} {unit}/operation, {iterations} operations)");
    print_result("old", units_per_iteration, iterations, unit, old);
    print_result("new", units_per_iteration, iterations, unit, new);
    println!(
        "  speedup {:>6.2}x | cycles reduction {:>6.1}% | allocations reduction {:>6.1}% | bytes reduction {:>6.1}%",
        old.elapsed.as_secs_f64() / new.elapsed.as_secs_f64(),
        reduction(old.cycles, new.cycles),
        reduction(old.alloc.allocs, new.alloc.allocs),
        reduction(old.alloc.bytes, new.alloc.bytes),
    );
}

fn print_result(
    label: &str,
    units_per_iteration: usize,
    iterations: usize,
    unit: &str,
    result: &BenchResult,
) {
    let operations = iterations as f64;
    let units = units_per_iteration as f64 * operations;
    println!(
        "  {label:<4} {:>9.2} ms/op {:>12.0} {unit}/s {:>9.1} cycles/{unit}",
        result.elapsed.as_secs_f64() * 1_000.0 / operations,
        units / result.elapsed.as_secs_f64(),
        result.cycles as f64 / units,
    );
    println!(
        "       alloc/realloc/free={:.1}/{:.1}/{:.1} per op, {:.1} KiB allocated/op",
        result.alloc.allocs as f64 / operations,
        result.alloc.reallocs as f64 / operations,
        result.alloc.deallocs as f64 / operations,
        result.alloc.bytes as f64 / operations / 1024.0,
    );
}

fn reduction(old: u64, new: u64) -> f64 {
    if old == 0 {
        return 0.0;
    }
    100.0 * (1.0 - new as f64 / old as f64)
}

fn candidate_paths(candidates: &[SongSearchCandidate]) -> Vec<&std::path::Path> {
    candidates
        .iter()
        .map(|candidate| candidate.song.simfile_path.as_path())
        .collect()
}

fn grouped_paths(groups: &[GroupedSongs]) -> Vec<&std::path::Path> {
    groups
        .iter()
        .flat_map(|group| &group.songs)
        .map(|song| song.simfile_path.as_path())
        .collect()
}

fn candidate_checksum(candidates: &[SongSearchCandidate]) -> u64 {
    candidates.iter().fold(0_u64, |checksum, candidate| {
        checksum.rotate_left(9)
            ^ text_checksum(&candidate.pack_name)
            ^ text_checksum(&candidate.song.title).rotate_left(23)
    })
}

fn grouped_checksum(groups: &[GroupedSongs]) -> u64 {
    groups.iter().fold(0_u64, |checksum, group| {
        group
            .songs
            .iter()
            .fold(checksum.rotate_left(3), |sum, song| {
                sum.rotate_left(11) ^ text_checksum(&song.title)
            })
    })
}

fn text_checksum(text: &str) -> u64 {
    text.bytes().fold(text.len() as u64, |checksum, byte| {
        checksum.rotate_left(5) ^ u64::from(byte)
    })
}

fn benchmark_simfile() -> Vec<u8> {
    let mut data = Vec::with_capacity(TAG_BYTES + 128);
    data.extend_from_slice(b"#TITLE:Benchmark;#CDIMAGE:old.png;\n");
    while data.len() < TAG_BYTES / 2 {
        data.extend_from_slice(b"00000000\n00000000\n00001000\n00000000\n");
    }
    data.extend_from_slice(b"#DISCIMAGE:disc.png;\n");
    while data.len() < TAG_BYTES {
        data.extend_from_slice(b"00000000\n00000000\n00000000\n00000000\n");
    }
    data.extend_from_slice(b"#CDIMAGE:new\\;image.png;");
    data
}

fn benchmark_song(index: usize) -> Arc<SongData> {
    let shuffled = index.wrapping_mul(2_654_435_761usize) % SONGS;
    let title = if index.is_multiple_of(17) {
        format!("SONG {shuffled:05}")
    } else {
        format!("Song {shuffled:05}")
    };
    let translit_title = if index.is_multiple_of(7) {
        format!("Translit {shuffled:05}")
    } else {
        String::new()
    };
    Arc::new(SongData {
        simfile_path: PathBuf::from(format!("Songs/Performance/Song{index:05}/song.ssc")),
        title,
        subtitle: "Tournament Mix".to_string(),
        translit_title,
        translit_subtitle: String::new(),
        artist: format!("Artist {:03}", index % 257),
        genre: format!("Genre {}", index % 12),
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
        display_bpm: "120:180".to_string(),
        offset: 0.0,
        sample_start: None,
        sample_length: None,
        min_bpm: 120.0,
        max_bpm: 180.0,
        normalized_bpms: "0=120,64=180".to_string(),
        music_length_seconds: 120.0 + (index % 240) as f32,
        first_second: 0.0,
        total_length_seconds: 120 + (index % 240) as i32,
        precise_last_second_seconds: 120.0,
        charts: vec![benchmark_chart(index)],
    })
}

fn benchmark_chart(index: usize) -> ChartData {
    ChartData {
        chart_type: "dance-single".to_string(),
        difficulty: "Challenge".to_string(),
        description: String::new(),
        chart_name: String::new(),
        meter: 8 + (index % 20) as u32,
        step_artist: String::new(),
        music_path: None,
        short_hash: format!("bench-{index:05}"),
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
        max_bpm: 180.0,
    }
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: fences and the timestamp read do not dereference memory and only
    // serialize this thread's measured interval.
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

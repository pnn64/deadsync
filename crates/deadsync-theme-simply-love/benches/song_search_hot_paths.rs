use deadsync_chart::{ArrowStats, ChartData, SongData, StaminaCounts, TechCounts};
use deadsync_simfile::song_search::{
    SongSearchLiveQuery, parse_song_search_live, parse_song_search_live_reference,
    song_passes_search_filters, song_passes_search_filters_reference,
    song_search_difficulties_text, song_search_difficulties_text_reference,
};
use deadsync_theme_simply_love::MusicWheelEntry;
use deadsync_theme_simply_love::screens::components::select_music::select_music_menu::{
    SongSearchMatch, build_pack_matches, build_pack_matches_reference, build_song_matches,
    build_song_matches_reference, build_song_search_index, build_song_search_index_reference,
    song_search_index_checksum,
};
use deadsync_theme_simply_love::screens::components::shared::fuzzy::{
    best_match_score, best_match_score_reference, prepare_query, prepare_query_reference,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 21;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    deallocs: AtomicU64,
    allocated_bytes: AtomicU64,
    freed_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            deallocs: AtomicU64::new(0),
            allocated_bytes: AtomicU64::new(0),
            freed_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            deallocs: self.deallocs.load(Ordering::Relaxed),
            allocated_bytes: self.allocated_bytes.load(Ordering::Relaxed),
            freed_bytes: self.freed_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: allocation is delegated unchanged to `System`; relaxed counters only
// observe successful calls while the single-threaded benchmark enables them.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes from the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.allocated_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.deallocs.fetch_add(1, Ordering::Relaxed);
            self.freed_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: this pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            if new_size > old.size() {
                self.allocated_bytes
                    .fetch_add((new_size - old.size()) as u64, Ordering::Relaxed);
            } else {
                self.freed_bytes
                    .fetch_add((old.size() - new_size) as u64, Ordering::Relaxed);
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
    allocated_bytes: u64,
    freed_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            deallocs: self.deallocs - before.deallocs,
            allocated_bytes: self.allocated_bytes - before.allocated_bytes,
            freed_bytes: self.freed_bytes - before.freed_bytes,
        }
    }

    const fn churn_bytes(self) -> u64 {
        self.allocated_bytes + self.freed_bytes
    }
}

struct BenchResult {
    median_ns: f64,
    p95_ns: f64,
    median_cycles: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> Option<u64> {
    None
}

fn percentile(sorted: &[f64], percentile: f64) -> f64 {
    let index = ((sorted.len() as f64 * percentile).ceil() as usize)
        .saturating_sub(1)
        .min(sorted.len() - 1);
    sorted[index]
}

fn measure(ops_per_sample: usize, mut op: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..3 {
        black_box(op());
    }

    let mut ns = Vec::with_capacity(SAMPLES);
    let mut cycles = Vec::with_capacity(SAMPLES);
    let mut checksum = 0u64;
    for _ in 0..SAMPLES {
        let cycle_start = cycle_counter();
        let started = Instant::now();
        for _ in 0..ops_per_sample {
            checksum = checksum.wrapping_add(black_box(op()));
        }
        let elapsed = started.elapsed();
        let cycle_end = cycle_counter();
        ns.push(elapsed.as_secs_f64() * 1_000_000_000.0 / ops_per_sample as f64);
        cycles.push(cycle_start.zip(cycle_end).map_or(f64::NAN, |(start, end)| {
            end.wrapping_sub(start) as f64 / ops_per_sample as f64
        }));
    }
    ns.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let allocation_checksum = black_box(op());
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);
    checksum = checksum.wrapping_add(allocation_checksum);

    BenchResult {
        median_ns: percentile(&ns, 0.5),
        p95_ns: percentile(&ns, 0.95),
        median_cycles: percentile(&cycles, 0.5),
        allocated,
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    println!(
        "{label:<9} {:>11.1} ns median  {:>11.1} ns p95  {:>11.1} cycles  \
         {:>9.1} query/s  {:>5} alloc  {:>5} realloc  {:>5} free  {:>9} B alloc  {:>9} B churn",
        result.median_ns,
        result.p95_ns,
        result.median_cycles,
        1_000_000_000.0 / result.median_ns,
        result.allocated.allocs,
        result.allocated.reallocs,
        result.allocated.deallocs,
        result.allocated.allocated_bytes,
        result.allocated.churn_bytes(),
    );
}

fn print_pair(title: &str, old: &BenchResult, new: &BenchResult) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    println!("\n{title}");
    print_result("old", old);
    print_result("new", new);
    let cycle_reduction = if old.median_cycles.is_finite()
        && new.median_cycles.is_finite()
        && old.median_cycles != 0.0
    {
        100.0 * (1.0 - new.median_cycles / old.median_cycles)
    } else {
        0.0
    };
    println!(
        "change    {:>8.2}x throughput  {:>7.2}% median  {:>7.2}% p95  \
         {:>7.2}% cycles  {:>7.2}% bytes  {:>7.2}% churn",
        old.median_ns / new.median_ns,
        100.0 * (1.0 - new.median_ns / old.median_ns),
        100.0 * (1.0 - new.p95_ns / old.p95_ns),
        cycle_reduction,
        if old.allocated.allocated_bytes == 0 {
            0.0
        } else {
            100.0
                * (1.0
                    - new.allocated.allocated_bytes as f64 / old.allocated.allocated_bytes as f64)
        },
        if old.allocated.churn_bytes() == 0 {
            0.0
        } else {
            100.0 * (1.0 - new.allocated.churn_bytes() as f64 / old.allocated.churn_bytes() as f64)
        },
    );
}

fn assert_strict_improvement(title: &str, old: &BenchResult, new: &BenchResult) {
    assert!(
        new.median_ns < old.median_ns,
        "{title}: median latency did not improve"
    );
    assert!(
        new.p95_ns < old.p95_ns,
        "{title}: p95 latency did not improve"
    );
    if old.median_cycles.is_finite() && new.median_cycles.is_finite() {
        assert!(
            new.median_cycles < old.median_cycles,
            "{title}: CPU cycles did not improve"
        );
    }
    assert!(
        new.allocated.allocs < old.allocated.allocs,
        "{title}: allocation count did not improve"
    );
    assert!(
        new.allocated.reallocs <= old.allocated.reallocs,
        "{title}: reallocation count regressed"
    );
    assert!(
        new.allocated.deallocs < old.allocated.deallocs,
        "{title}: free count did not improve"
    );
    assert!(
        new.allocated.allocated_bytes < old.allocated.allocated_bytes,
        "{title}: allocated bytes did not improve"
    );
    assert!(
        new.allocated.churn_bytes() < old.allocated.churn_bytes(),
        "{title}: memory churn did not improve"
    );
}

fn assert_allocation_improvement(title: &str, old: &BenchResult, new: &BenchResult) {
    assert!(
        new.allocated.allocs < old.allocated.allocs,
        "{title}: allocation count did not improve"
    );
    assert!(
        new.allocated.reallocs < old.allocated.reallocs,
        "{title}: reallocation count did not improve"
    );
    assert!(
        new.allocated.deallocs < old.allocated.deallocs,
        "{title}: free count did not improve"
    );
    assert!(
        new.allocated.allocated_bytes < old.allocated.allocated_bytes,
        "{title}: allocated bytes did not improve"
    );
    assert!(
        new.allocated.churn_bytes() < old.allocated.churn_bytes(),
        "{title}: memory churn did not improve"
    );
}

fn chart(meter: u32) -> ChartData {
    ChartData {
        chart_type: "dance-single".to_string(),
        difficulty: if meter == 12 { "Challenge" } else { "Hard" }.to_string(),
        description: String::new(),
        chart_name: String::new(),
        meter,
        step_artist: String::new(),
        music_path: None,
        short_hash: format!("chart-{meter}"),
        stats: ArrowStats::default(),
        tech_counts: TechCounts::default(),
        mines_nonfake: 0,
        stamina_counts: StaminaCounts::default(),
        total_streams: 0,
        matrix_rating: 0.0,
        matrix_profile: Box::default(),
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
        min_bpm: 170.0,
        max_bpm: 170.0,
    }
}

fn song(index: usize) -> Arc<SongData> {
    Arc::new(SongData {
        simfile_path: PathBuf::from(format!("song-{index}.sm")),
        title: format!("Catalog Song {:04} Remix {}", index % 997, index % 31),
        subtitle: String::new(),
        translit_title: String::new(),
        translit_subtitle: String::new(),
        artist: String::new(),
        translit_artist: String::new(),
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
        display_bpm: "170".to_string(),
        offset: 0.0,
        sample_start: None,
        sample_length: None,
        min_bpm: 170.0,
        max_bpm: 170.0,
        normalized_bpms: "170".to_string(),
        music_length_seconds: 120.0,
        first_second: 0.0,
        total_length_seconds: 120,
        precise_last_second_seconds: 120.0,
        charts: (5..=12).map(chart).collect(),
    })
}

fn catalog(pack_count: usize, songs_per_pack: usize) -> (Vec<MusicWheelEntry>, Vec<Arc<SongData>>) {
    let mut wheel = Vec::with_capacity(pack_count * (songs_per_pack + 1));
    let mut songs = Vec::with_capacity(pack_count * songs_per_pack);
    for pack in 0..pack_count {
        wheel.push(MusicWheelEntry::PackHeader {
            name: Arc::from(format!("Collection Pack {:04}", pack % 509)),
            original_index: pack,
            banner_path: None,
            song_count: songs_per_pack,
            pack_key: None,
            parent_series: None,
        });
        for offset in 0..songs_per_pack {
            let song = song(pack * songs_per_pack + offset);
            wheel.push(MusicWheelEntry::Song(Arc::clone(&song)));
            songs.push(song);
        }
    }
    (wheel, songs)
}

fn translit_catalog(pack_count: usize, songs_per_pack: usize) -> Vec<MusicWheelEntry> {
    let (mut wheel, _) = catalog(pack_count, songs_per_pack);
    for entry in &mut wheel {
        if let MusicWheelEntry::Song(song) = entry {
            let song = Arc::make_mut(song);
            song.translit_title = format!("[12] Catalogue Caf\u{e9} {:04}", song.title.len());
        }
    }
    wheel
}

fn text_hash(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        hash.wrapping_mul(0x100_0000_01b3) ^ u64::from(byte)
    })
}

fn query_checksum(query: SongSearchLiveQuery) -> u64 {
    text_hash(&query.text)
        ^ u64::from(query.difficulty.unwrap_or(u8::MAX)).rotate_left(17)
        ^ (query.bpm_tier.unwrap_or(i32::MIN) as u64).rotate_left(31)
}

fn match_checksum(matches: &[SongSearchMatch]) -> u64 {
    matches.iter().fold(matches.len() as u64, |checksum, item| {
        let (text, score, count) = match item {
            SongSearchMatch::Song { candidate, score } => (
                candidate.title.as_ref(),
                *score,
                candidate.song.charts.len(),
            ),
            SongSearchMatch::Pack {
                name,
                song_count,
                score,
            } => (name.as_ref(), *score, *song_count),
        };
        checksum.rotate_left(7)
            ^ text_hash(text)
            ^ (score as u64).rotate_left(17)
            ^ (count as u64).rotate_left(31)
    })
}

fn filter_checksum(songs: &[Arc<SongData>], reference: bool) -> u64 {
    const DIFFICULTIES: [Option<u8>; 8] = [
        Some(5),
        Some(6),
        Some(7),
        Some(8),
        Some(9),
        Some(10),
        Some(11),
        Some(12),
    ];
    let mut checksum = 0u64;
    for song in songs {
        for difficulty in DIFFICULTIES {
            let passed = if reference {
                song_passes_search_filters_reference(song, "dance-single", difficulty, None)
            } else {
                song_passes_search_filters(song, "dance-single", difficulty, None)
            };
            checksum = checksum.rotate_left(1) ^ u64::from(passed);
        }
    }
    checksum
}

fn fuzzy_score_checksum(songs: &[Arc<SongData>], query_text: &str, reference: bool) -> u64 {
    let mut checksum = 0u64;
    if reference {
        let query = prepare_query_reference(query_text);
        for song in songs {
            let score = best_match_score_reference(&query, &song.title, &[]);
            checksum = checksum.rotate_left(3) ^ score.map_or(u64::MAX, |value| value as u64);
        }
    } else {
        let query = prepare_query(query_text);
        for song in songs {
            let score = best_match_score(&query, &song.title, &[]);
            checksum = checksum.rotate_left(3) ^ score.map_or(u64::MAX, |value| value as u64);
        }
    }
    checksum
}

fn main() {
    let (song_wheel, songs) = catalog(128, 24);
    let (pack_wheel, _) = catalog(512, 1);
    let translit_wheel = translit_catalog(128, 24);

    let old = measure(8, || {
        song_search_index_checksum(&build_song_search_index_reference(black_box(&song_wheel)))
    });
    let new = measure(8, || {
        song_search_index_checksum(&build_song_search_index(black_box(&song_wheel)))
    });
    let title = "ASCII song index construction (3,072 songs)";
    print_pair(title, &old, &new);
    assert_allocation_improvement(title, &old, &new);

    let old = measure(40, || {
        song_search_index_checksum(&build_song_search_index_reference(black_box(&pack_wheel)))
    });
    let new = measure(40, || {
        song_search_index_checksum(&build_song_search_index(black_box(&pack_wheel)))
    });
    let title = "pack key index construction (512 packs)";
    print_pair(title, &old, &new);
    assert_allocation_improvement(title, &old, &new);

    let old = measure(5, || {
        song_search_index_checksum(&build_song_search_index_reference(black_box(
            &translit_wheel,
        )))
    });
    let new = measure(5, || {
        song_search_index_checksum(&build_song_search_index(black_box(&translit_wheel)))
    });
    let title = "cleaned translit index construction (3,072 songs)";
    print_pair(title, &old, &new);
    assert_allocation_improvement(title, &old, &new);

    let song_index = build_song_search_index(&song_wheel);
    let pack_index = build_song_search_index(&pack_wheel);

    let old = measure(50_000, || {
        query_checksum(parse_song_search_live_reference(black_box(
            "  Finale [12] Song [180] Mix  ",
        )))
    });
    let new = measure(50_000, || {
        query_checksum(parse_song_search_live(black_box(
            "  Finale [12] Song [180] Mix  ",
        )))
    });
    print_pair("live query parsing", &old, &new);

    let detail_song = &songs[0];
    let old = measure(20_000, || {
        text_hash(&song_search_difficulties_text_reference(
            black_box(detail_song),
            black_box("dance-single"),
        ))
    });
    let new = measure(20_000, || {
        text_hash(&song_search_difficulties_text(
            black_box(detail_song),
            black_box("dance-single"),
        ))
    });
    print_pair("standard difficulty detail formatting", &old, &new);

    let old = measure(5, || {
        fuzzy_score_checksum(black_box(&songs), black_box("song"), true)
    });
    let new = measure(5, || {
        fuzzy_score_checksum(black_box(&songs), black_box("song"), false)
    });
    let title = "ASCII subsequence scoring (3,072 songs)";
    print_pair(title, &old, &new);
    assert_strict_improvement(title, &old, &new);

    let old = measure(2, || {
        fuzzy_score_checksum(black_box(&songs), black_box("sonf"), true)
    });
    let new = measure(2, || {
        fuzzy_score_checksum(black_box(&songs), black_box("sonf"), false)
    });
    let title = "typo fallback (3,072 songs, one-edit query)";
    print_pair(title, &old, &new);
    assert_strict_improvement(title, &old, &new);

    let old = measure(5, || {
        match_checksum(&build_song_matches_reference(
            black_box(&song_index),
            black_box("song"),
            "dance-single",
        ))
    });
    let new = measure(5, || {
        match_checksum(&build_song_matches(
            black_box(&song_index),
            black_box("song"),
            "dance-single",
        ))
    });
    print_pair("song fuzzy ranking (3,072 songs -> 9 rows)", &old, &new);

    let old = measure(20, || {
        match_checksum(&build_pack_matches_reference(
            black_box(&pack_index),
            black_box("pack"),
        ))
    });
    let new = measure(20, || {
        match_checksum(&build_pack_matches(
            black_box(&pack_index),
            black_box("pack"),
        ))
    });
    print_pair("pack fuzzy ranking (512 packs -> 9 rows)", &old, &new);

    let old = measure(4, || filter_checksum(black_box(&songs), true));
    let new = measure(4, || filter_checksum(black_box(&songs), false));
    print_pair("difficulty filtering (3,072 songs x 8 filters)", &old, &new);
}

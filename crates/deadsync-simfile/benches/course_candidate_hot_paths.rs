use deadsync_chart::{ArrowStats, ChartData, SongData, StaminaCounts, TechCounts};
use deadsync_simfile::course::{
    CourseEntry, CourseGradeCounts, CourseSong, Difficulty, SongSort, StepsSpec,
    benchmark_course_candidate_clone, benchmark_course_candidate_move,
    benchmark_course_chart_indices, benchmark_course_chart_indices_reference,
    benchmark_course_pick_cached_sort, benchmark_course_pick_repeated_lookups,
    benchmark_course_pick_selected, benchmark_course_repeats, benchmark_course_repeats_reference,
    benchmark_course_sort, benchmark_course_sort_reference, song_unique_key,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashMap;
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SONGS: usize = 2_048;
const SAMPLES: usize = 21;
const TAKE_BATCH: usize = 4_096;

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

// SAFETY: allocation requests are delegated unchanged to `System`; relaxed
// counters only observe this single-threaded benchmark while its gate is set.
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
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.realloc_bytes
                .fetch_add((old.size() + new_size) as u64, Ordering::Relaxed);
        }
        out
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

    const fn churn(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

struct Row {
    median_ns: f64,
    p95_ns: f64,
    median_cycles: Option<f64>,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn measure(mut operation: impl FnMut() -> u64) -> Row {
    for _ in 0..5 {
        black_box(operation());
    }

    let mut times = Vec::with_capacity(SAMPLES);
    let mut cycles = Vec::with_capacity(SAMPLES);
    let mut checksum = 0u64;
    for _ in 0..SAMPLES {
        let cycle_start = cycle_counter();
        let started = Instant::now();
        checksum ^= black_box(operation());
        times.push(started.elapsed().as_secs_f64() * 1e9);
        if let Some(elapsed) = cycle_start
            .zip(cycle_counter())
            .map(|(start, end)| end.wrapping_sub(start) as f64)
        {
            cycles.push(elapsed);
        }
    }
    times.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    black_box(operation());
    ALLOC.enabled.store(false, Ordering::Relaxed);

    Row {
        median_ns: times[SAMPLES / 2],
        p95_ns: times[SAMPLES * 95 / 100],
        median_cycles: (!cycles.is_empty()).then(|| cycles[cycles.len() / 2]),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_pair(title: &str, items: usize, old: &Row, new: &Row, require_less_churn: bool) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert!(
        new.median_ns < old.median_ns,
        "{title} latency regressed: old={}ns new={}ns",
        old.median_ns,
        new.median_ns
    );
    if let (Some(old_cycles), Some(new_cycles)) = (old.median_cycles, new.median_cycles) {
        assert!(
            new_cycles < old_cycles,
            "{title} cycles regressed: old={old_cycles} new={new_cycles}"
        );
    }
    assert!(
        new.alloc.allocs <= old.alloc.allocs,
        "{title} allocs regressed"
    );
    assert!(
        new.alloc.reallocs <= old.alloc.reallocs,
        "{title} reallocs regressed"
    );
    assert!(
        new.alloc.frees <= old.alloc.frees,
        "{title} frees regressed"
    );
    if require_less_churn {
        assert!(
            new.alloc.churn() < old.alloc.churn(),
            "{title} churn did not improve: old={}B new={}B",
            old.alloc.churn(),
            new.alloc.churn()
        );
    } else {
        assert!(
            new.alloc.churn() <= old.alloc.churn(),
            "{title} churn regressed: old={}B new={}B",
            old.alloc.churn(),
            new.alloc.churn()
        );
    }

    println!("\n{title} ({items} candidates)");
    print_row("old", items, old);
    print_row("new", items, new);
    println!(
        "  change: {:>7.2}% median  {:>7.2}% cycles  {:>7.2}% throughput  \
         {:>7.2}% p95  {:>7.2}% allocs  {:>7.2}% churn",
        change(old.median_ns, new.median_ns),
        change(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN),
        ),
        change(throughput(items, old), throughput(items, new)),
        change(old.p95_ns, new.p95_ns),
        change(old.alloc.allocs as f64, new.alloc.allocs as f64),
        change(old.alloc.churn() as f64, new.alloc.churn() as f64),
    );
}

fn print_row(label: &str, items: usize, row: &Row) {
    println!(
        "  {label:<3} {:>11.0} ns  {:>11.0} cycles  {:>11.0} p95 ns  \
         {:>8.3} Mcandidate/s  {:>6} allocs  {:>5} reallocs  {:>6} frees  {:>10} churn B",
        row.median_ns,
        row.median_cycles.unwrap_or(f64::NAN),
        row.p95_ns,
        throughput(items, row) / 1e6,
        row.alloc.allocs,
        row.alloc.reallocs,
        row.alloc.frees,
        row.alloc.churn(),
    );
}

fn throughput(items: usize, row: &Row) -> f64 {
    items as f64 * 1e9 / row.median_ns
}

fn change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

fn main() {
    let songs = fixture_songs();
    let entry = fixture_entry();
    let plays: HashMap<_, _> = songs
        .iter()
        .enumerate()
        .map(|(index, song)| {
            (
                song_unique_key(song),
                ((index * 2_654_435_761usize) >> 7) as u32,
            )
        })
        .collect();
    let grades: HashMap<_, _> = songs
        .iter()
        .enumerate()
        .map(|(index, song)| {
            let mut counts = CourseGradeCounts::default();
            for (grade, count) in counts.iter_mut().enumerate() {
                *count = ((index.wrapping_mul(97) + grade * 31) % 23) as u32;
            }
            (song_unique_key(song), counts)
        })
        .collect();
    let selected: Vec<_> = songs.iter().map(|song| song_unique_key(song)).collect();

    let old = measure(|| {
        benchmark_course_chart_indices_reference(black_box(&songs), &entry, "dance-single")
    });
    let new = measure(|| benchmark_course_chart_indices(black_box(&songs), &entry, "dance-single"));
    print_pair("shared matching-chart storage", SONGS, &old, &new, false);

    let old = measure(|| {
        benchmark_course_sort_reference(
            black_box(&songs),
            &entry,
            "dance-single",
            SongSort::MostPlays,
            black_box(&plays),
            &grades,
        )
    });
    let new = measure(|| {
        benchmark_course_sort(
            black_box(&songs),
            &entry,
            "dance-single",
            SongSort::MostPlays,
            black_box(&plays),
            &grades,
        )
    });
    print_pair("cached course-song sort keys", SONGS, &old, &new, true);

    let old = measure(|| {
        benchmark_course_repeats_reference(
            black_box(&songs),
            &entry,
            "dance-single",
            black_box(&selected),
        )
    });
    let new = measure(|| {
        benchmark_course_repeats(
            black_box(&songs),
            &entry,
            "dance-single",
            black_box(&selected),
        )
    });
    print_pair(
        "clone-free endless-course rollover",
        SONGS,
        &old,
        &new,
        true,
    );

    let pick = 0;
    let old = measure(|| {
        benchmark_course_pick_repeated_lookups(
            black_box(&songs),
            &entry,
            "dance-single",
            SongSort::MostPlays,
            pick,
            black_box(&plays),
            &grades,
        )
    });
    let new = measure(|| {
        benchmark_course_pick_cached_sort(
            black_box(&songs),
            &entry,
            "dance-single",
            SongSort::MostPlays,
            pick,
            black_box(&plays),
            &grades,
        )
    });
    print_pair("precomputed compact play ranks", SONGS, &old, &new, false);

    let old = measure(|| {
        benchmark_course_pick_repeated_lookups(
            black_box(&songs),
            &entry,
            "dance-single",
            SongSort::TopGrades,
            pick,
            &plays,
            black_box(&grades),
        )
    });
    let new = measure(|| {
        benchmark_course_pick_cached_sort(
            black_box(&songs),
            &entry,
            "dance-single",
            SongSort::TopGrades,
            pick,
            &plays,
            black_box(&grades),
        )
    });
    print_pair("precomputed compact grade ranks", SONGS, &old, &new, false);

    let old = measure(|| {
        benchmark_course_pick_cached_sort(
            black_box(&songs),
            &entry,
            "dance-single",
            SongSort::MostPlays,
            pick,
            black_box(&plays),
            &grades,
        )
    });
    let new = measure(|| {
        benchmark_course_pick_selected(
            black_box(&songs),
            &entry,
            "dance-single",
            SongSort::MostPlays,
            pick,
            black_box(&plays),
            &grades,
        )
    });
    print_pair("partial ranked course selection", SONGS, &old, &new, false);

    let one_song = &songs[..1];
    let old = measure(|| {
        benchmark_course_candidate_clone(black_box(one_song), &entry, "dance-single", TAKE_BATCH)
    });
    let new = measure(|| {
        benchmark_course_candidate_move(black_box(one_song), &entry, "dance-single", TAKE_BATCH)
    });
    print_pair(
        "move selected course candidate",
        TAKE_BATCH,
        &old,
        &new,
        true,
    );
}

fn fixture_entry() -> CourseEntry {
    CourseEntry {
        song: CourseSong::RandomAny,
        steps: StepsSpec::Difficulty(Difficulty::Hard),
        modifiers: String::new(),
        secret: false,
        no_difficult: false,
        gain_seconds: 0.0,
        gain_lives: -1,
    }
}

fn fixture_songs() -> Vec<Arc<SongData>> {
    (0..SONGS)
        .map(|index| {
            let mut song = empty_song();
            song.simfile_path = PathBuf::from(format!(
                "Packs/Benchmark Pack/Very Long Course Song {index:04}/song.ssc"
            ));
            song.charts = (0..4)
                .map(|chart| test_chart(7 + chart, &format!("{index:04}-{chart}")))
                .collect();
            Arc::new(song)
        })
        .collect()
}

fn empty_song() -> SongData {
    SongData {
        simfile_path: PathBuf::new(),
        title: String::new(),
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
        display_bpm: String::new(),
        offset: 0.0,
        sample_start: None,
        sample_length: None,
        min_bpm: 120.0,
        max_bpm: 120.0,
        normalized_bpms: String::new(),
        music_length_seconds: 120.0,
        first_second: 0.0,
        total_length_seconds: 120,
        precise_last_second_seconds: 120.0,
        charts: Vec::new(),
    }
}

fn test_chart(meter: u32, hash: &str) -> ChartData {
    ChartData {
        chart_type: "dance-single".to_string(),
        difficulty: "Hard".to_string(),
        description: String::new(),
        chart_name: String::new(),
        meter,
        step_artist: String::new(),
        music_path: None,
        short_hash: hash.to_string(),
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
        min_bpm: 120.0,
        max_bpm: 120.0,
    }
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

use deadsync_chart::{
    ArrowStats, ChartData, SongData, SongPack, StaminaCounts, SyncPref, TechCounts,
};
use deadsync_import::itg::{ItgSongScores, ItgSource, ItgStepsScores};
use deadsync_import::pipeline::bench_support;
use deadsync_import::resolver::ChartResolver;
use deadsync_score::ImportedHighScore;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const PACKS: usize = 8;
const SONGS_PER_PACK: usize = 64;
const CHARTS_PER_SONG: usize = 3;
const SCORES_PER_CHART: usize = 8;
const ITERATIONS: usize = 60;
const SAMPLES: usize = 20;

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

// SAFETY: allocator requests are delegated unchanged to `System`; relaxed
// counters only observe successful calls while the single-threaded gate is on.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplied `layout`.
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

    const fn operations(self) -> u64 {
        self.allocs + self.reallocs + self.frees
    }

    const fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

struct BenchResult {
    ns_per_op: f64,
    p95_ns: f64,
    cycles_per_op: Option<f64>,
    items_per_second: f64,
    allocated: AllocSnapshot,
    allocation_runs: usize,
    checksum: u64,
}

fn measure(items_per_op: usize, mut operation: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..2 {
        black_box(operation());
    }

    let batch = (ITERATIONS / SAMPLES).max(1);
    let mut sample_ns = Vec::with_capacity(SAMPLES);
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..SAMPLES {
        let sample_started = Instant::now();
        for _ in 0..batch {
            checksum = checksum.wrapping_add(black_box(operation()));
        }
        sample_ns.push(sample_started.elapsed().as_secs_f64() * 1e9 / batch as f64);
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();
    sample_ns.sort_unstable_by(f64::total_cmp);

    let allocation_runs = ITERATIONS / 15;
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut allocation_checksum = 0u64;
    for _ in 0..allocation_runs {
        allocation_checksum = allocation_checksum.wrapping_add(black_box(operation()));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);
    black_box(allocation_checksum);

    let measured_runs = batch * SAMPLES;
    let seconds = elapsed.as_secs_f64();
    BenchResult {
        ns_per_op: seconds * 1e9 / measured_runs as f64,
        p95_ns: sample_ns[(sample_ns.len() * 95 / 100).min(sample_ns.len() - 1)],
        cycles_per_op: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / measured_runs as f64),
        items_per_second: measured_runs as f64 * items_per_op as f64 / seconds,
        allocated,
        allocation_runs,
        checksum,
    }
}

#[derive(Clone, Copy)]
enum AllocationGuard {
    ReallocationsDrop,
    Stable,
    OperationsDrop,
}

fn print_pair(title: &str, old: &BenchResult, new: &BenchResult, guard: AllocationGuard) {
    println!("\n{title}");
    print_result("old", old);
    print_result("new", new);
    println!(
        "  change: {:>7.2}% latency  {:>7.2}% cycles  {:>7.2}% throughput  \
         {:>7.2}% p95  {:>7.2}% churn",
        percent_change(old.ns_per_op, new.ns_per_op),
        percent_change(
            old.cycles_per_op.unwrap_or(f64::NAN),
            new.cycles_per_op.unwrap_or(f64::NAN),
        ),
        percent_change(old.items_per_second, new.items_per_second),
        percent_change(old.p95_ns, new.p95_ns),
        percent_change(
            old.allocated.churn_bytes() as f64,
            new.allocated.churn_bytes() as f64,
        ),
    );

    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert!(new.ns_per_op < old.ns_per_op, "{title} latency regressed");
    assert!(
        new.items_per_second > old.items_per_second,
        "{title} throughput regressed"
    );
    if let (Some(old_cycles), Some(new_cycles)) = (old.cycles_per_op, new.cycles_per_op) {
        assert!(new_cycles < old_cycles, "{title} CPU cycles regressed");
    }
    match guard {
        AllocationGuard::ReallocationsDrop => {
            assert!(
                new.allocated.reallocs < old.allocated.reallocs,
                "{title} reallocation count did not improve"
            );
            assert!(
                new.allocated.churn_bytes() < old.allocated.churn_bytes(),
                "{title} memory churn did not improve"
            );
        }
        AllocationGuard::Stable => {
            assert_eq!(new.allocated.allocs, old.allocated.allocs);
            assert_eq!(new.allocated.reallocs, old.allocated.reallocs);
            assert_eq!(new.allocated.frees, old.allocated.frees);
            assert_eq!(new.allocated.churn_bytes(), old.allocated.churn_bytes());
        }
        AllocationGuard::OperationsDrop => {
            assert!(
                new.allocated.operations() < old.allocated.operations(),
                "{title} allocation operations did not improve"
            );
            assert!(
                new.allocated.churn_bytes() < old.allocated.churn_bytes(),
                "{title} memory churn did not improve"
            );
        }
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let runs = result.allocation_runs as f64;
    println!(
        "  {label:<3} {:>11.2} ns/op  {:>11.2} cycles/op  {:>11.2} p95 ns  \
         {:>7.2} Mitem/s  {:>8.1} alloc/op  {:>6.1} realloc/op  \
         {:>8.1} free/op  {:>11.1} churn B/op",
        result.ns_per_op,
        result.cycles_per_op.unwrap_or(f64::NAN),
        result.p95_ns,
        result.items_per_second / 1e6,
        result.allocated.allocs as f64 / runs,
        result.allocated.reallocs as f64 / runs,
        result.allocated.frees as f64 / runs,
        result.allocated.churn_bytes() as f64 / runs,
    );
}

fn percent_change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
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

fn chart(pack: usize, song: usize, chart: usize) -> ChartData {
    ChartData {
        chart_type: "dance-single".to_owned(),
        difficulty: format!("Benchmark-{chart}"),
        description: String::new(),
        chart_name: String::new(),
        meter: 10,
        step_artist: String::new(),
        music_path: None,
        short_hash: format!("{pack:04x}{song:04x}{chart:08x}"),
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

fn song(pack: usize, song: usize, charts: Vec<ChartData>) -> SongData {
    SongData {
        simfile_path: PathBuf::from(format!(
            "Songs/Benchmark Pack {pack:02}/Song {song:04}/chart.ssc"
        )),
        title: format!("Benchmark Song {pack:02}-{song:04}"),
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
        music_length_seconds: 0.0,
        first_second: 0.0,
        total_length_seconds: 0,
        precise_last_second_seconds: 0.0,
        charts,
    }
}

fn high_score(index: usize) -> ImportedHighScore {
    ImportedHighScore {
        grade: "Tier03".to_owned(),
        percent_dp: 0.95,
        w1: index as u32,
        ..Default::default()
    }
}

fn fixtures() -> (Vec<SongPack>, ItgSource) {
    let mut packs = Vec::with_capacity(PACKS);
    let mut source = ItgSource::default();
    source.songs.reserve(PACKS * SONGS_PER_PACK);
    source.favorites.reserve(PACKS * SONGS_PER_PACK);
    for pack_index in 0..PACKS {
        let pack_name = format!("Benchmark Pack {pack_index:02}");
        let mut songs = Vec::with_capacity(SONGS_PER_PACK);
        for song_index in 0..SONGS_PER_PACK {
            let charts = (0..CHARTS_PER_SONG)
                .map(|chart_index| chart(pack_index, song_index, chart_index))
                .collect::<Vec<_>>();
            let steps = (0..CHARTS_PER_SONG)
                .map(|chart_index| ItgStepsScores {
                    steps_type: "dance-single".to_owned(),
                    difficulty: format!("Benchmark-{chart_index}"),
                    high_scores: (0..SCORES_PER_CHART).map(high_score).collect(),
                    ..Default::default()
                })
                .collect();
            source.songs.push(ItgSongScores {
                dir: format!("Songs/{pack_name}/Song {song_index:04}/"),
                steps,
            });
            source
                .favorites
                .push(format!("{pack_name}/Song {song_index:04}"));
            songs.push(Arc::new(song(pack_index, song_index, charts)));
        }
        packs.push(SongPack {
            group_name: pack_name.clone(),
            name: pack_name.clone(),
            sort_title: pack_name.clone(),
            translit_title: String::new(),
            series: String::new(),
            folder_series: String::new(),
            year: 0,
            sync_pref: SyncPref::Default,
            directory: PathBuf::from("Songs").join(pack_name),
            banner_path: None,
            songs,
        });
    }
    (packs, source)
}

fn main() {
    let (packs, source) = fixtures();
    let resolver = ChartResolver::build(&packs);
    let score_items = source.total_high_scores();
    let favorite_items = PACKS * SONGS_PER_PACK * CHARTS_PER_SONG;
    assert_eq!(
        score_items,
        PACKS * SONGS_PER_PACK * CHARTS_PER_SONG * SCORES_PER_CHART
    );

    print_pair(
        "exact score output capacity",
        &measure(score_items, || {
            bench_support::scores_unreserved(black_box(&source), black_box(&resolver))
        }),
        &measure(score_items, || {
            bench_support::scores_reserved(black_box(&source), black_box(&resolver))
        }),
        AllocationGuard::ReallocationsDrop,
    );
    print_pair(
        "one chart resolution per Steps batch",
        &measure(score_items, || {
            bench_support::scores_reserved(black_box(&source), black_box(&resolver))
        }),
        &measure(score_items, || {
            bench_support::scores_resolved_per_step(black_box(&source), black_box(&resolver))
        }),
        AllocationGuard::Stable,
    );
    print_pair(
        "batched favorite hash-set capacity",
        &measure(favorite_items, || {
            bench_support::favorites_unreserved(black_box(&source), black_box(&resolver))
        }),
        &measure(favorite_items, || {
            bench_support::favorites_reserved(black_box(&source), black_box(&resolver))
        }),
        AllocationGuard::OperationsDrop,
    );
}

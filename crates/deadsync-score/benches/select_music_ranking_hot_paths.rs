use deadsync_chart::{ArrowStats, ChartData, SongData, StaminaCounts, TechCounts};
use deadsync_score::select_music::{
    benchmark_ranked_popular_songs_cached, ranked_popular_songs, ranked_recent_songs,
};
use deadsync_simfile::song_sort::{song_title_cmp, song_title_sort_key};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SONGS: usize = 2_048;
const CHARTS_PER_SONG: usize = 4;
const ITERATIONS: usize = 60;
const SAMPLES: usize = 30;

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

// SAFETY: every operation delegates unchanged to `System`; relaxed counters
// only observe successful calls while the single-threaded benchmark gate is on.
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
    for _ in 0..4 {
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

    let allocation_runs = ITERATIONS / 6;
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

fn print_pair(title: &str, old: &BenchResult, new: &BenchResult) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
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

struct Fixtures {
    songs: Vec<Arc<SongData>>,
    counts: Vec<(String, u32)>,
    recent: Vec<String>,
}

fn fixtures() -> Fixtures {
    let mut songs = Vec::with_capacity(SONGS);
    let mut counts = Vec::with_capacity(SONGS * CHARTS_PER_SONG);
    let mut hashes = Vec::with_capacity(SONGS * CHARTS_PER_SONG);
    for song_ix in 0..SONGS {
        let title_ix = song_ix.wrapping_mul(1_009) % SONGS;
        let mut charts = Vec::with_capacity(CHARTS_PER_SONG);
        for chart_ix in 0..CHARTS_PER_SONG {
            let hash = format!("{song_ix:08x}{chart_ix:08x}{:048x}", song_ix ^ chart_ix);
            counts.push((hash.clone(), (song_ix % 32 + chart_ix) as u32));
            hashes.push(hash.clone());
            charts.push(chart(hash));
        }
        songs.push(Arc::new(song(song_ix, title_ix, charts)));
    }

    let mut recent = Vec::with_capacity(180);
    for ix in 0usize..45 {
        recent.push(format!("missing-{ix}"));
        let song_ix = ix.wrapping_mul(137) % SONGS;
        for chart_ix in 0..3 {
            recent.push(hashes[song_ix * CHARTS_PER_SONG + chart_ix].clone());
        }
    }
    Fixtures {
        songs,
        counts,
        recent,
    }
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

fn song(song_ix: usize, title_ix: usize, charts: Vec<ChartData>) -> SongData {
    SongData {
        simfile_path: PathBuf::from(format!("Pack {song_ix:04}/Song {song_ix:04}/chart.ssc")),
        title: format!("A realistically sized song title {title_ix:04}"),
        subtitle: format!("subtitle group {}", song_ix % 17),
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
        charts,
    }
}

fn ranked_checksum(ranked: &[(Arc<SongData>, u32)]) -> u64 {
    ranked
        .iter()
        .fold(ranked.len() as u64, |checksum, (song, count)| {
            checksum
                .wrapping_mul(31)
                .wrapping_add(song.title.len() as u64)
                .wrapping_add(u64::from(*count))
        })
}

fn recent_checksum(ranked: &[Arc<SongData>]) -> u64 {
    ranked.iter().fold(ranked.len() as u64, |checksum, song| {
        checksum
            .wrapping_mul(31)
            .wrapping_add(song.simfile_path.as_os_str().len() as u64)
    })
}

fn main() {
    let fixture = fixtures();

    let owned_hashes = measure(fixture.counts.len(), || {
        ranked_checksum(&ranked_popular_songs(
            fixture.songs.clone(),
            fixture.counts.iter().cloned(),
            50,
            true,
            song_title_cmp,
        ))
    });
    let borrowed_hashes = measure(fixture.counts.len(), || {
        ranked_checksum(&ranked_popular_songs(
            fixture.songs.clone(),
            fixture
                .counts
                .iter()
                .map(|(hash, count)| (hash.as_str(), *count)),
            50,
            true,
            song_title_cmp,
        ))
    });
    print_pair(
        "popular history input: cloned String vs borrowed str",
        &owned_hashes,
        &borrowed_hashes,
    );

    let owned_recent = measure(fixture.recent.len(), || {
        recent_checksum(&ranked_recent_songs(
            fixture.songs.clone(),
            fixture.recent.iter().cloned(),
            30,
        ))
    });
    let borrowed_recent = measure(fixture.recent.len(), || {
        recent_checksum(&ranked_recent_songs(
            fixture.songs.clone(),
            fixture.recent.iter().map(String::as_str),
            30,
        ))
    });
    print_pair(
        "recent history input: cloned String vs borrowed str",
        &owned_recent,
        &borrowed_recent,
    );

    let cached_keys = measure(SONGS, || {
        ranked_checksum(&benchmark_ranked_popular_songs_cached(
            fixture.songs.clone(),
            fixture
                .counts
                .iter()
                .map(|(hash, count)| (hash.as_str(), *count)),
            50,
            true,
            song_title_sort_key,
        ))
    });
    let direct_cmp = measure(SONGS, || {
        ranked_checksum(&ranked_popular_songs(
            fixture.songs.clone(),
            fixture
                .counts
                .iter()
                .map(|(hash, count)| (hash.as_str(), *count)),
            50,
            true,
            song_title_cmp,
        ))
    });
    print_pair(
        "popular tie sort: allocated cached keys vs direct comparator",
        &cached_keys,
        &direct_cmp,
    );
}

use deadsync_chart::{ArrowStats, ChartData, SongData, StaminaCounts, TechCounts};
use deadsync_simfile::song_sort::{
    GroupedSongs, SongSortGroup, genre_grouped_songs, genre_grouped_songs_reference,
    meter_grouped_songs, meter_grouped_songs_reference, title_grouped_songs,
    title_grouped_songs_reference,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SONGS: usize = 2_048;
const SAMPLES: usize = 21;
const ALPHA_FIXTURE_GROUPS: usize = 28;

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

// SAFETY: all requests are delegated unchanged to `System`; relaxed counters
// only observe this single-threaded benchmark while the gate is enabled.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes from the allocator caller.
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

fn measure(mut op: impl FnMut() -> u64) -> Row {
    for _ in 0..3 {
        black_box(op());
    }

    let mut times = Vec::with_capacity(SAMPLES);
    let mut cycles = Vec::with_capacity(SAMPLES);
    let mut checksum = 0u64;
    for _ in 0..SAMPLES {
        let cycle_start = cycle_counter();
        let started = Instant::now();
        checksum ^= black_box(op());
        times.push(started.elapsed().as_secs_f64() * 1e9);
        if let Some(elapsed_cycles) = cycle_start
            .zip(cycle_counter())
            .map(|(start, end)| end.wrapping_sub(start) as f64)
        {
            cycles.push(elapsed_cycles);
        }
    }
    times.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    black_box(op());
    ALLOC.enabled.store(false, Ordering::Relaxed);

    Row {
        median_ns: times[SAMPLES / 2],
        p95_ns: times[SAMPLES * 95 / 100],
        median_cycles: (!cycles.is_empty()).then(|| cycles[cycles.len() / 2]),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_pair(title: &str, old: &Row, new: &Row) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    println!("\n{title} ({SONGS} songs)");
    print_row("old", old);
    print_row("new", new);
    println!(
        "  change: {:>7.2}% median  {:>7.2}% cycles  {:>7.2}% throughput  \
         {:>7.2}% p95  {:>7.2}% allocs  {:>7.2}% reallocs  {:>7.2}% churn",
        change(old.median_ns, new.median_ns),
        change(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN),
        ),
        change(throughput(old), throughput(new)),
        change(old.p95_ns, new.p95_ns),
        change(old.alloc.allocs as f64, new.alloc.allocs as f64),
        change(old.alloc.reallocs as f64, new.alloc.reallocs as f64),
        change(old.alloc.churn() as f64, new.alloc.churn() as f64),
    );
}

fn print_row(label: &str, row: &Row) {
    println!(
        "  {label:<3} {:>11.0} ns  {:>11.0} cycles  {:>11.0} p95 ns  \
         {:>7.2} Msong/s  {:>7} allocs  {:>5} reallocs  {:>7} frees  {:>10} churn B",
        row.median_ns,
        row.median_cycles.unwrap_or(f64::NAN),
        row.p95_ns,
        throughput(row) / 1e6,
        row.alloc.allocs,
        row.alloc.reallocs,
        row.alloc.frees,
        row.alloc.churn(),
    );
}

fn throughput(row: &Row) -> f64 {
    SONGS as f64 * 1e9 / row.median_ns
}

fn change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

fn grouped_checksum(groups: Vec<GroupedSongs>) -> u64 {
    let mut checksum = groups.len() as u64;
    for group in groups {
        checksum = checksum.wrapping_mul(131).wrapping_add(match &group.group {
            SongSortGroup::Title(bucket) => u64::from(*bucket),
            SongSortGroup::Artist(bucket) => 32 + u64::from(*bucket),
            SongSortGroup::Genre(Some(genre)) => genre.bytes().fold(64, |sum, byte| {
                sum.wrapping_mul(33).wrapping_add(u64::from(byte))
            }),
            SongSortGroup::Genre(None) => 96,
            SongSortGroup::Bpm { lo, hi } => ((*lo as u64) << 32) ^ *hi as u64,
            SongSortGroup::Length { lo, hi } => ((*lo as u64) << 32) ^ *hi as u64,
            SongSortGroup::Meter(Some(meter)) => 128 + u64::from(*meter),
            SongSortGroup::Meter(None) => 127,
        });
        for song in group.songs {
            checksum = checksum
                .wrapping_mul(1_099_511_628_211)
                .wrapping_add(song.total_length_seconds as u64);
        }
    }
    checksum
}

fn main() {
    let songs = fixtures();

    let old = measure(|| grouped_checksum(title_grouped_songs_reference(songs.clone())));
    let new = measure(|| grouped_checksum(title_grouped_songs(songs.clone())));
    print_pair("dense alpha-bucket grouping", &old, &new);

    let old = measure(|| {
        grouped_checksum(genre_grouped_songs_reference(
            songs.clone(),
            "Unknown Genre",
        ))
    });
    let new = measure(|| grouped_checksum(genre_grouped_songs(songs.clone(), "Unknown Genre")));
    print_pair("genre boundary ownership", &old, &new);

    let old =
        measure(|| grouped_checksum(meter_grouped_songs_reference(songs.clone(), "dance-single")));
    let new = measure(|| grouped_checksum(meter_grouped_songs(songs.clone(), "dance-single")));
    print_pair("meter scratch and bucket handoff", &old, &new);
}

fn fixtures() -> Vec<Arc<SongData>> {
    const GENRES: [&str; 12] = [
        "Pop",
        "pop",
        "Rock",
        "Electronic",
        "Anime",
        "Game",
        "Jazz",
        "Metal",
        "Classical",
        "",
        "   ",
        "Other",
    ];
    let mut songs = Vec::with_capacity(SONGS);
    for index in 0..SONGS {
        let shuffled = index.wrapping_mul(1_009) % SONGS;
        let mut charts = Vec::with_capacity(8);
        charts.push(chart("Edit", 18 + (index % 4) as u32, true));
        for chart_index in 0..6 {
            charts.push(chart(
                if chart_index == 5 {
                    "Edit"
                } else {
                    "Challenge"
                },
                7 + ((index + chart_index * 3) % 22) as u32,
                (index + chart_index) % 17 != 0,
            ));
        }
        charts.push(chart("Challenge", 10 + (index % 9) as u32, true));
        songs.push(Arc::new(song(
            index,
            shuffled,
            GENRES[index % GENRES.len()],
            charts,
        )));
    }
    songs
}

fn song(index: usize, shuffled: usize, genre: &str, charts: Vec<ChartData>) -> SongData {
    let title_prefix = match index % ALPHA_FIXTURE_GROUPS {
        0 => '!'.to_string(),
        1 => '7'.to_string(),
        bucket => ((b'A' + (bucket - 2) as u8) as char).to_string(),
    };
    SongData {
        simfile_path: PathBuf::from(format!(
            "Packs/Benchmark Pack {:02}/Song {shuffled:05}/chart.ssc",
            index % 32
        )),
        title: format!("{title_prefix} Song {shuffled:05}"),
        subtitle: String::new(),
        translit_title: String::new(),
        translit_subtitle: String::new(),
        artist: "Shared Artist".to_string(),
        translit_artist: String::new(),
        genre: genre.to_string(),
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
        max_bpm: 180.0,
        normalized_bpms: String::new(),
        music_length_seconds: 120.0,
        first_second: 0.0,
        total_length_seconds: index as i32 + 1,
        precise_last_second_seconds: 120.0,
        charts,
    }
}

fn chart(difficulty: &str, meter: u32, has_note_data: bool) -> ChartData {
    ChartData {
        chart_type: "dance-single".to_string(),
        difficulty: difficulty.to_string(),
        description: String::new(),
        chart_name: String::new(),
        meter,
        step_artist: String::new(),
        music_path: None,
        short_hash: String::new(),
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
        has_note_data,
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

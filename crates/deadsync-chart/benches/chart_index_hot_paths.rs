use deadsync_chart::song::bench_support;
use deadsync_chart::{
    ArrowStats, ChartData, STANDARD_DIFFICULTY_COUNT, SongData, StaminaCounts, TechCounts,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

const SLOT_OPERATIONS: usize = 20_000;
const HASH_OPERATIONS: usize = 10_000;
const EDIT_SORT_OPERATIONS: usize = 5_000;
const FIXTURE_CHARTS: usize = 101;
const SAMPLES: usize = 21;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

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

// SAFETY: allocation requests are forwarded unchanged to `System`; relaxed
// counters only observe this single-threaded benchmark while enabled.
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
    median_ns_per_item: f64,
    p95_ns_per_item: f64,
    median_cycles_per_item: Option<f64>,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn measure(
    operations: usize,
    items_per_operation: usize,
    mut operation: impl FnMut() -> u64,
) -> Row {
    for _ in 0..3 {
        black_box(operation());
    }

    let measured_items = (operations * items_per_operation) as f64;
    let mut times = Vec::with_capacity(SAMPLES);
    let mut cycles = Vec::with_capacity(SAMPLES);
    let mut checksum = 0_u64;
    for _ in 0..SAMPLES {
        let cycle_start = cycle_counter();
        let started = Instant::now();
        for _ in 0..operations {
            checksum = checksum.wrapping_add(black_box(operation()));
        }
        times.push(started.elapsed().as_secs_f64() * 1e9 / measured_items);
        if let Some(elapsed) = cycle_start
            .zip(cycle_counter())
            .map(|(start, end)| end.wrapping_sub(start) as f64 / measured_items)
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
        median_ns_per_item: times[SAMPLES / 2],
        p95_ns_per_item: times[SAMPLES * 95 / 100],
        median_cycles_per_item: (!cycles.is_empty()).then(|| cycles[cycles.len() / 2]),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_pair(title: &str, old: &Row, new: &Row, returns_vec: bool) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    if returns_vec {
        assert_eq!(new.alloc.allocs, 1, "{title} new path allocation changed");
        assert_eq!(new.alloc.reallocs, 0, "{title} new path reallocated");
        assert_eq!(new.alloc.frees, 1, "{title} new path free count changed");
        assert!(old.alloc.reallocs > 0, "{title} reference did not grow");
        assert!(
            new.alloc.churn() < old.alloc.churn(),
            "{title} new path did not reduce churn"
        );
    } else {
        assert_zero_churn(title, "old", old.alloc);
        assert_zero_churn(title, "new", new.alloc);
    }

    println!("\n{title}");
    print_row("old", old);
    print_row("new", new);
    println!(
        "  change: {:>7.2}% median  {:>7.2}% cycles  {:>7.2}% throughput  \
         {:>7.2}% p95  {:>7.2}% allocs  {:>7.2}% reallocs  {:>7.2}% churn",
        change(old.median_ns_per_item, new.median_ns_per_item),
        change(
            old.median_cycles_per_item.unwrap_or(f64::NAN),
            new.median_cycles_per_item.unwrap_or(f64::NAN),
        ),
        change(throughput(old), throughput(new)),
        change(old.p95_ns_per_item, new.p95_ns_per_item),
        change(old.alloc.allocs as f64, new.alloc.allocs as f64),
        change(old.alloc.reallocs as f64, new.alloc.reallocs as f64),
        change(old.alloc.churn() as f64, new.alloc.churn() as f64),
    );
}

fn assert_zero_churn(title: &str, label: &str, alloc: AllocSnapshot) {
    assert_eq!(alloc.allocs, 0, "{title} {label} path allocated");
    assert_eq!(alloc.reallocs, 0, "{title} {label} path reallocated");
    assert_eq!(alloc.frees, 0, "{title} {label} path freed");
    assert_eq!(alloc.churn(), 0, "{title} {label} path churned memory");
}

fn print_row(label: &str, row: &Row) {
    println!(
        "  {label:<3} {:>10.2} ns/item  {:>10.2} cycles/item  {:>10.2} p95 ns  \
         {:>8.2} Mitem/s  {:>5} alloc  {:>5} realloc  {:>5} free  {:>10} churn B",
        row.median_ns_per_item,
        row.median_cycles_per_item.unwrap_or(f64::NAN),
        row.p95_ns_per_item,
        throughput(row) / 1e6,
        row.alloc.allocs,
        row.alloc.reallocs,
        row.alloc.frees,
        row.alloc.churn(),
    );
}

fn throughput(row: &Row) -> f64 {
    1e9 / row.median_ns_per_item
}

fn change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

fn indices_checksum(indices: [Option<usize>; STANDARD_DIFFICULTY_COUNT]) -> u64 {
    indices.into_iter().fold(0_u64, |checksum, index| {
        checksum.rotate_left(9) ^ index.map_or(u64::MAX, |index| index as u64)
    })
}

fn edit_indices_checksum(indices: Vec<usize>) -> u64 {
    indices.into_iter().fold(0_u64, |checksum, index| {
        checksum.rotate_left(7) ^ index as u64
    })
}

fn chart(difficulty: &str, hash: String, seed: usize) -> ChartData {
    ChartData {
        chart_type: "dance-single".to_owned(),
        difficulty: difficulty.to_owned(),
        description: format!("Edit {:03}", seed.wrapping_mul(37) % 101),
        chart_name: String::new(),
        meter: 6 + (seed * 7 % 20) as u32,
        step_artist: String::new(),
        music_path: None,
        short_hash: hash,
        stats: ArrowStats {
            total_steps: (80 + seed * 37) as u32,
            ..ArrowStats::default()
        },
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

fn fixture_song() -> SongData {
    let mut charts = ["Beginner", "Easy", "Medium", "Hard", "Challenge"]
        .into_iter()
        .enumerate()
        .map(|(seed, difficulty)| chart(difficulty, format!("standard-{seed}"), seed))
        .collect::<Vec<_>>();
    for seed in 5..FIXTURE_CHARTS {
        let hash = if matches!(seed, 76 | 94) {
            "target-edit".to_owned()
        } else {
            format!("edit-{seed:03}")
        };
        charts.push(chart("Edit", hash, seed));
    }

    SongData {
        simfile_path: PathBuf::from("bench.ssc"),
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
        music_length_seconds: 0.0,
        first_second: 0.0,
        total_length_seconds: 0,
        precise_last_second_seconds: 0.0,
        charts,
    }
}

fn main() {
    let song = fixture_song();

    print_pair(
        "Edit chart index construction and sort",
        &measure(EDIT_SORT_OPERATIONS, FIXTURE_CHARTS, || {
            edit_indices_checksum(bench_support::edit_chart_indices_sorted_reference(
                black_box(&song),
                black_box("dance-single"),
            ))
        }),
        &measure(EDIT_SORT_OPERATIONS, FIXTURE_CHARTS, || {
            edit_indices_checksum(
                black_box(&song).edit_chart_indices_sorted(black_box("dance-single")),
            )
        }),
        true,
    );

    print_pair(
        "complete standard-chart slot construction",
        &measure(SLOT_OPERATIONS, FIXTURE_CHARTS, || {
            indices_checksum(bench_support::standard_chart_indices_reference(
                black_box(&song),
                black_box("dance-single"),
            ))
        }),
        &measure(SLOT_OPERATIONS, FIXTURE_CHARTS, || {
            indices_checksum(black_box(&song).standard_chart_indices(black_box("dance-single")))
        }),
        false,
    );

    print_pair(
        "duplicate Edit hash restoration",
        &measure(HASH_OPERATIONS, FIXTURE_CHARTS, || {
            bench_support::steps_index_for_chart_hash_reference(
                black_box(&song),
                black_box("dance-single"),
                black_box("target-edit"),
            )
            .map_or(u64::MAX, |index| index as u64)
        }),
        &measure(HASH_OPERATIONS, FIXTURE_CHARTS, || {
            black_box(&song)
                .steps_index_for_chart_hash(black_box("dance-single"), black_box("target-edit"))
                .map_or(u64::MAX, |index| index as u64)
        }),
        false,
    );
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: LFENCE/RDTSC serialize and read the timestamp counter only.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: LFENCE/RDTSC serialize and read the timestamp counter only.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> Option<u64> {
    None
}

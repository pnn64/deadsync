use deadsync_chart::{
    ArrowStats, ChartData, SongData, StaminaCounts, TechCounts,
    song::{
        edit_chart_best_steps_for_bench, edit_chart_best_steps_legacy_for_bench,
        edit_chart_hash_lookup_for_bench, edit_chart_hash_lookup_legacy_for_bench,
        edit_chart_steps_len_for_bench, edit_chart_steps_len_legacy_for_bench,
    },
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const EDITS: usize = 24;
const RUNS: usize = 10_000;

type Workload = fn(&SongData, &str) -> u64;

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
    let song = fixture_song();
    benchmark_pair(
        "edit-chart hash-to-index lookup",
        &song,
        edit_chart_hash_lookup_legacy_for_bench,
        edit_chart_hash_lookup_for_bench,
    );
    benchmark_pair(
        "edit-chart step count",
        &song,
        edit_chart_steps_len_legacy_for_bench,
        edit_chart_steps_len_for_bench,
    );
    benchmark_pair(
        "edit-chart fallback selection",
        &song,
        edit_chart_best_steps_legacy_for_bench,
        edit_chart_best_steps_for_bench,
    );
}

fn benchmark_pair(label: &str, song: &SongData, old_workload: Workload, new_workload: Workload) {
    assert_eq!(
        old_workload(song, "dance-single"),
        new_workload(song, "dance-single")
    );
    let old = measure(song, old_workload);
    let new = measure(song, new_workload);
    assert_eq!(old.checksum, new.checksum);
    println!("{label} ({EDITS} edits, {RUNS} runs)");
    print_comparison(&old, &new);
}

fn fixture_song() -> SongData {
    const DESCRIPTIONS: [&str; 8] = [
        "Technical",
        "challenge",
        "Äpfel",
        "ALPHA",
        "Stream",
        "İstanbul",
        "Straße",
        "Footswitch",
    ];

    let mut charts = Vec::with_capacity(EDITS + 6);
    for index in 0..EDITS {
        charts.push(chart(
            "dance-single",
            "Edit",
            &format!(
                "{} {:02}",
                DESCRIPTIONS[index % DESCRIPTIONS.len()],
                index % 5
            ),
            7 + (index * 11 % 18) as u32,
            &format!("edit-{index:02}"),
        ));
    }
    for index in 0..4 {
        charts.push(chart(
            "dance-double",
            "Edit",
            "Filtered",
            10 + index as u32,
            &format!("double-{index}"),
        ));
    }
    charts.push(chart("dance-single", "Couple", "Filtered", 12, "couple"));
    charts.push(chart("pump-single", "Challenge", "Filtered", 15, "pump"));

    SongData {
        simfile_path: PathBuf::from("Songs/Benchmark/Edit Navigation/song.ssc"),
        title: "Edit Navigation".to_string(),
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
        display_bpm: "160".to_string(),
        offset: 0.0,
        sample_start: None,
        sample_length: None,
        min_bpm: 160.0,
        max_bpm: 160.0,
        normalized_bpms: "160".to_string(),
        music_length_seconds: 120.0,
        first_second: 0.0,
        total_length_seconds: 120,
        precise_last_second_seconds: 120.0,
        charts,
    }
}

fn chart(
    chart_type: &str,
    difficulty: &str,
    description: &str,
    meter: u32,
    short_hash: &str,
) -> ChartData {
    ChartData {
        chart_type: chart_type.to_string(),
        difficulty: difficulty.to_string(),
        description: description.to_string(),
        chart_name: String::new(),
        meter,
        step_artist: String::new(),
        music_path: None,
        short_hash: short_hash.to_string(),
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
        min_bpm: 160.0,
        max_bpm: 160.0,
    }
}

fn measure(song: &SongData, workload: Workload) -> BenchResult {
    for _ in 0..10 {
        black_box(workload(song, "dance-single"));
    }
    let before = ALLOC.snapshot();
    let cycles_before = read_cycles();
    let started = Instant::now();
    let mut checksum = 0_u64;
    for run in 0..RUNS {
        checksum = checksum.rotate_left(7)
            ^ black_box(workload(black_box(song), black_box("dance-single")))
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
    println!(
        "  {label:<4} {:>7.2} us/run {:>10.0} cycles/run {:>8.1} Kworkloads/s",
        result.elapsed.as_secs_f64() * 1.0e6 / runs,
        result.cycles as f64 / runs,
        runs / result.elapsed.as_secs_f64() / 1.0e3,
    );
    println!(
        "       alloc/realloc={:.1}/{:.1} per run, {:.1} KiB/run",
        result.alloc.allocs as f64 / runs,
        result.alloc.reallocs as f64 / runs,
        result.alloc.bytes as f64 / runs / 1024.0,
    );
}

fn print_comparison(old: &BenchResult, new: &BenchResult) {
    print_result("old", old);
    print_result("new", new);
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

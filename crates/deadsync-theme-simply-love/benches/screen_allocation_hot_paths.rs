use deadsync_simfile::song_search::{SongSearchCandidate, song_search_difficulties_text};
use deadsync_theme_simply_love::screens::components::select_music::select_music_menu::{
    SongSearchResultsState, benchmark_song_search_frame_text,
};
use deadsync_theme_simply_love::screens::evaluation_summary::{
    benchmark_eval_numeric_text, benchmark_profile_name_changed,
};
use deadsync_theme_simply_love::screens::select_music::benchmark_wheel_song_meta;
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashSet;
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const PROFILE_OPS: usize = 500_000;
const NUMERIC_OPS: usize = 500_000;
const SEARCH_FRAME_OPS: usize = 100_000;
const SONG_SEARCH_WHEEL_SLOTS: usize = 12;
const SONG_SEARCH_WHEEL_FOCUS_SLOT: usize = SONG_SEARCH_WHEEL_SLOTS / 2 - 1;
const DETAIL_LABELS: [&str; 5] = ["Pack", "Song", "Subtitle", "BPMs", "Difficulties"];

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    deallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
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

// SAFETY: every operation delegates unchanged to `System`; relaxed counters
// only observe successful calls while measurement is enabled.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes from the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.deallocs.fetch_add(1, Ordering::Relaxed);
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
                self.bytes
                    .fetch_add((new_size - old.size()) as u64, Ordering::Relaxed);
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
    ns_per_op: f64,
    worst_sample_ns: f64,
    cycles_per_op: Option<f64>,
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

fn measure(iterations: usize, sample_ops: usize, mut op: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..iterations.min(2_000) {
        black_box(op());
    }

    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    let mut worst_sample_ns = 0.0f64;
    for _ in 0..(iterations / sample_ops) {
        let sample_started = Instant::now();
        for _ in 0..sample_ops {
            checksum = checksum.wrapping_add(black_box(op()));
        }
        worst_sample_ns = worst_sample_ns
            .max(sample_started.elapsed().as_secs_f64() * 1_000_000_000.0 / sample_ops as f64);
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut allocation_checksum = 0u64;
    for _ in 0..iterations {
        allocation_checksum = allocation_checksum.wrapping_add(black_box(op()));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);
    black_box(allocation_checksum);

    BenchResult {
        ns_per_op: elapsed.as_secs_f64() * 1_000_000_000.0 / iterations as f64,
        worst_sample_ns,
        cycles_per_op: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / iterations as f64),
        allocated,
        checksum,
    }
}

fn print_result(label: &str, iterations: usize, result: &BenchResult) {
    let ops = iterations as f64;
    let churn = result.allocated.allocs + result.allocated.reallocs + result.allocated.deallocs;
    println!(
        "{label:<12} {:>10.2} ns/op  {:>10.2} cycles/op  {:>10.2} worst ns  \
         {:>8.3} Mop/s  {:>7.2} alloc  {:>7.2} realloc  {:>7.2} free  \
         {:>7.2} churn  {:>10.1} B/op",
        result.ns_per_op,
        result.cycles_per_op.unwrap_or(f64::NAN),
        result.worst_sample_ns,
        1_000.0 / result.ns_per_op,
        result.allocated.allocs as f64 / ops,
        result.allocated.reallocs as f64 / ops,
        result.allocated.deallocs as f64 / ops,
        churn as f64 / ops,
        result.allocated.bytes as f64 / ops,
    );
}

fn print_pair(title: &str, iterations: usize, old: &BenchResult, new: &BenchResult) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert_eq!(new.allocated.allocs, 0, "{title} still allocates");
    assert_eq!(new.allocated.reallocs, 0, "{title} still reallocates");
    assert_eq!(new.allocated.deallocs, 0, "{title} still frees");
    assert_eq!(new.allocated.bytes, 0, "{title} still allocates bytes");
    println!("\n{title}");
    print_result("old", iterations, old);
    print_result("new", iterations, new);
    println!(
        "improvement  {:>8.2}x throughput  {:>8.2}% fewer allocated bytes",
        old.ns_per_op / new.ns_per_op,
        100.0 * (1.0 - new.allocated.bytes as f64 / old.allocated.bytes as f64),
    );
}

fn legacy_profile_name_changed(sides: [&[&str]; 2]) -> bool {
    sides
        .into_iter()
        .any(|names| names.iter().copied().collect::<HashSet<_>>().len() > 1)
}

fn legacy_eval_numeric_text(percent: f64, ex: f64, counts: &[u32; 8]) -> usize {
    let percent = format!("{percent:.2}");
    let ex = format!("{ex:.2}");
    let mut bytes = percent.len() + ex.len();
    for count in counts {
        bytes += count.to_string().len();
    }
    bytes
}

fn legacy_song_search_frame_text(
    results: &SongSearchResultsState,
    raw_query: &str,
    chart_type: &str,
) -> usize {
    let query = format!("\"{raw_query}\"");
    let result_count = format!("{} Results Found", results.candidates.len());
    let total_items = results.candidates.len() + 1;
    let mut bytes = query.len() + result_count.len();

    for slot_idx in 0..SONG_SEARCH_WHEEL_SLOTS {
        let offset = slot_idx as isize - SONG_SEARCH_WHEEL_FOCUS_SLOT as isize;
        let row_idx =
            ((results.selected_index as isize + offset).rem_euclid(total_items as isize)) as usize;
        let text = results.candidates.get(row_idx).map_or_else(
            || "Exit".to_string(),
            |candidate| candidate.song.display_title(false).to_string(),
        );
        bytes += text.len();
        black_box(text);
    }

    if let Some(candidate) = results.candidates.get(results.selected_index) {
        let details = [
            candidate.pack_name.to_string(),
            candidate.song.display_title(false).to_string(),
            candidate.song.display_subtitle(false).to_string(),
            candidate.song.formatted_chart_display_bpm(None),
            song_search_difficulties_text(candidate.song.as_ref(), chart_type),
        ];
        for (label, value) in DETAIL_LABELS.into_iter().zip(&details) {
            let label_text = format!("{label}:");
            let value_text = value.clone();
            bytes += label_text.len() + value_text.len();
            black_box(label_text);
            black_box(value_text);
        }
        black_box(details);
    }
    black_box(query);
    black_box(result_count);
    bytes
}

fn song_search_fixture() -> SongSearchResultsState {
    let songs = benchmark_wheel_song_meta(12);
    let pack_name: Arc<str> = Arc::from("Benchmark Pack");
    let candidates = songs
        .songs()
        .iter()
        .map(|song| SongSearchCandidate {
            pack_name: Arc::clone(&pack_name),
            title: Arc::from(song.display_title(false)),
            subtitle: Arc::from(song.display_subtitle(false)),
            bpm: Arc::from(song.formatted_chart_display_bpm(None)),
            difficulties: Arc::from(song_search_difficulties_text(song, "dance-single")),
            song: Arc::clone(song),
        })
        .collect();
    SongSearchResultsState {
        query_label: Arc::from("\"benchmark\""),
        result_count_label: Arc::from("12 Results Found"),
        candidates,
        selected_index: 4,
        prev_selected_index: 3,
        last_move_dir: 1,
        focus_anim_elapsed: 0.1,
        input_lock: 0.0,
    }
}

fn main() {
    let p1 = ["Player One"; 12];
    let p2 = ["Player Two"; 12];
    let sides = [&p1[..], &p2[..]];
    let old_profile = measure(PROFILE_OPS, 500, || {
        u64::from(legacy_profile_name_changed(black_box(sides)))
    });
    let new_profile = measure(PROFILE_OPS, 500, || {
        u64::from(benchmark_profile_name_changed(black_box(sides)))
    });
    print_pair(
        "1. evaluation profile-name scan",
        PROFILE_OPS,
        &old_profile,
        &new_profile,
    );

    let counts = [20, 1_024, 3_456, 789, 12, 3, 0, 19];
    let old_numeric = measure(NUMERIC_OPS, 500, || {
        legacy_eval_numeric_text(black_box(98.76), black_box(97.53), black_box(&counts)) as u64
    });
    let new_numeric = measure(NUMERIC_OPS, 500, || {
        benchmark_eval_numeric_text(black_box(98.76), black_box(97.53), black_box(&counts)) as u64
    });
    print_pair(
        "2. evaluation numeric text",
        NUMERIC_OPS,
        &old_numeric,
        &new_numeric,
    );

    let results = song_search_fixture();
    let expected = legacy_song_search_frame_text(&results, "benchmark", "dance-single");
    assert_eq!(expected, benchmark_song_search_frame_text(&results));
    let old_search = measure(SEARCH_FRAME_OPS, 100, || {
        legacy_song_search_frame_text(black_box(&results), "benchmark", "dance-single") as u64
    });
    let new_search = measure(SEARCH_FRAME_OPS, 100, || {
        benchmark_song_search_frame_text(black_box(&results)) as u64
    });
    print_pair(
        "3. song-search frame text",
        SEARCH_FRAME_OPS,
        &old_search,
        &new_search,
    );
}

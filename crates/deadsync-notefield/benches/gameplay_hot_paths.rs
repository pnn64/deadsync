use deadsync_gameplay::NoteCountStat;
use deadsync_notefield::{
    BrokenRunLookup, StreamProgressLookup, performance::find_first_displayed_beat,
};
use deadsync_rules::stream::StreamSegment;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const NOTE_SEARCHES: usize = 300_000;
const NOTE_STATS: usize = 8_192;
const HUD_FRAMES: usize = 2_000_000;
const HUD_SEGMENTS: usize = 512;
const WARMUP_DIVISOR: usize = 20;
const MAX_NOTES_AFTER: usize = 64;

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

// SAFETY: allocation calls delegate unchanged to `System`; relaxed atomics
// only count successful calls while a single benchmark thread measures.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplied `layout`.
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

#[derive(Clone, Copy, Default)]
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
    ns_per_item: f64,
    cycles_per_item: Option<f64>,
    items_per_second: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure(iterations: usize, mut operation: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..iterations / WARMUP_DIVISOR {
        black_box(operation());
    }
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..iterations {
        checksum = checksum.wrapping_add(black_box(operation()));
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let seconds = elapsed.as_secs_f64();
    BenchResult {
        ns_per_item: seconds * 1_000_000_000.0 / iterations as f64,
        cycles_per_item: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / iterations as f64),
        items_per_second: iterations as f64 / seconds,
        allocated: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_result(label: &str, result: &BenchResult) {
    println!(
        "{label:<24} {:>10.2} ns/item  {:>10.2} cycles/item  {:>9.2} Mitem/s  \
         {:>5} alloc  {:>5} realloc  {:>5} free  {:>8} bytes  {:016x}",
        result.ns_per_item,
        result.cycles_per_item.unwrap_or(f64::NAN),
        result.items_per_second / 1_000_000.0,
        result.allocated.allocs,
        result.allocated.reallocs,
        result.allocated.deallocs,
        result.allocated.bytes,
        result.checksum,
    );
}

fn print_change(old: &BenchResult, new: &BenchResult) {
    println!(
        "  change: {:>7.2}% latency, {:>7.2}% cycles, {:>7.2}% throughput",
        percent_change(old.ns_per_item, new.ns_per_item),
        percent_change(
            old.cycles_per_item.unwrap_or(f64::NAN),
            new.cycles_per_item.unwrap_or(f64::NAN),
        ),
        percent_change(old.items_per_second, new.items_per_second),
    );
}

fn percent_change(old: f64, new: f64) -> f64 {
    (new - old) * 100.0 / old
}

fn assert_zero_alloc(result: &BenchResult) {
    assert_eq!(result.allocated.allocs, 0);
    assert_eq!(result.allocated.reallocs, 0);
    assert_eq!(result.allocated.deallocs, 0);
    assert_eq!(result.allocated.bytes, 0);
}

fn note_count_at(stats: &[NoteCountStat], beat: f32) -> NoteCountStat {
    let index = stats
        .partition_point(|stat| stat.beat <= beat)
        .saturating_sub(1);
    stats.get(index).copied().unwrap_or(NoteCountStat {
        beat: 0.0,
        notes_lower: 0,
        notes_upper: 0,
    })
}

fn legacy_first_beat(
    current_beat: f32,
    draw_distance: f32,
    stats: &[NoteCountStat],
) -> Option<f32> {
    if !current_beat.is_finite() || !draw_distance.is_finite() {
        return None;
    }
    let mut high = current_beat.max(0.0);
    let high_count = (!stats.is_empty()).then(|| note_count_at(stats, current_beat));
    let mut low = if high_count.is_some() {
        0.0
    } else {
        high - 4.0
    };
    let mut first = low;
    for _ in 0..24 {
        let mid = (low + high) * 0.5;
        let too_many_notes = high_count.is_some_and(|high| {
            high.notes_upper
                .saturating_sub(note_count_at(stats, mid).notes_lower)
                > MAX_NOTES_AFTER
        });
        if (mid - current_beat) * 48.0 < -draw_distance || too_many_notes {
            first = mid;
            low = mid;
        } else {
            high = mid;
        }
    }
    Some(first)
}

fn note_search_benchmark() {
    let stats = (0..NOTE_STATS)
        .map(|index| NoteCountStat {
            beat: index as f32 * 0.125,
            notes_lower: index * 2,
            notes_upper: index * 2 + 2,
        })
        .collect::<Vec<_>>();
    for query in 0..4_096 {
        let beat = 80.0 + (query % (NOTE_STATS - 640)) as f32 * 0.125;
        let old = legacy_first_beat(beat, 768.0, &stats);
        let new =
            find_first_displayed_beat(beat, 768.0, &stats, |candidate| (candidate - beat) * 48.0);
        assert_eq!(old.map(f32::to_bits), new.map(f32::to_bits));
    }

    let mut old_query = 0usize;
    let old = measure(NOTE_SEARCHES, || {
        let beat = 80.0 + (old_query % (NOTE_STATS - 640)) as f32 * 0.125;
        old_query += 1;
        u64::from(
            legacy_first_beat(black_box(beat), 768.0, black_box(&stats))
                .unwrap_or_default()
                .to_bits(),
        )
    });
    let mut new_query = 0usize;
    let new = measure(NOTE_SEARCHES, || {
        let beat = 80.0 + (new_query % (NOTE_STATS - 640)) as f32 * 0.125;
        new_query += 1;
        u64::from(
            find_first_displayed_beat(black_box(beat), 768.0, black_box(&stats), |candidate| {
                (candidate - beat) * 48.0
            })
            .unwrap_or_default()
            .to_bits(),
        )
    });
    assert_eq!(old.checksum, new.checksum);
    assert_zero_alloc(&old);
    assert_zero_alloc(&new);

    println!("\nnotefield first-visible search ({NOTE_STATS} row stats, {NOTE_SEARCHES} frames)");
    print_result("old 25 beat searches", &old);
    print_result("new count cutoff", &new);
    print_change(&old, &new);
}

#[derive(Clone, Copy)]
struct ProgressEntry {
    start: f32,
    end: f32,
    stream_before: f64,
    is_break: bool,
}

struct BinaryProgressLookup {
    segments: Box<[ProgressEntry]>,
}

impl BinaryProgressLookup {
    fn new(segments: &[StreamSegment]) -> Self {
        let mut stream_before = 0.0;
        let entries = segments
            .iter()
            .map(|segment| {
                let start = segment.start as f32;
                let end = segment.end as f32;
                let entry = ProgressEntry {
                    start,
                    end,
                    stream_before,
                    is_break: segment.is_break,
                };
                if !segment.is_break {
                    stream_before += f64::from(end - start);
                }
                entry
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self { segments: entries }
    }

    #[inline(always)]
    fn completion_for_beat(&self, total_stream_measures: f64, beat_floor: f32) -> Option<f64> {
        if total_stream_measures <= 0.0 || self.segments.is_empty() {
            return None;
        }
        let current = if beat_floor.is_finite() {
            (beat_floor / 4.0).ceil().max(0.0)
        } else {
            0.0
        };
        let index = self
            .segments
            .partition_point(|segment| current >= segment.end);
        let Some(segment) = self.segments.get(index) else {
            let completed = self.segments.last().map_or(0.0, |last| {
                last.stream_before
                    + if last.is_break {
                        0.0
                    } else {
                        f64::from(last.end - last.start)
                    }
            });
            return Some((completed / total_stream_measures).clamp(0.0, 1.0));
        };
        let partial = if segment.is_break || current <= segment.start {
            0.0
        } else {
            f64::from(current.min(segment.end) - segment.start)
        };
        Some(((segment.stream_before + partial) / total_stream_measures).clamp(0.0, 1.0))
    }
}

fn stream_progress_benchmark() {
    let segments = (0..HUD_SEGMENTS)
        .map(|index| StreamSegment {
            start: index * 4,
            end: index * 4 + 4,
            is_break: index % 5 == 4,
        })
        .collect::<Vec<_>>();
    let total = segments
        .iter()
        .filter(|segment| !segment.is_break)
        .map(|segment| (segment.end - segment.start) as f64)
        .sum::<f64>();
    let old_lookup = BinaryProgressLookup::new(&segments);
    let new_lookup = StreamProgressLookup::new(&segments);
    let cycle_beats = HUD_SEGMENTS as f32 * 16.0;

    for beat in [0.0, 2048.0, 16.0, cycle_beats, 128.0, f32::INFINITY] {
        assert_eq!(
            old_lookup
                .completion_for_beat(total, beat)
                .map(f64::to_bits),
            new_lookup
                .completion_for_beat(total, beat)
                .map(f64::to_bits),
        );
    }

    let mut old_frame = 0usize;
    let old = measure(HUD_FRAMES, || {
        let beat = (old_frame as f32 * 0.02) % cycle_beats;
        old_frame += 1;
        old_lookup
            .completion_for_beat(black_box(total), black_box(beat))
            .unwrap_or_default()
            .to_bits()
    });
    let mut new_frame = 0usize;
    let new = measure(HUD_FRAMES, || {
        let beat = (new_frame as f32 * 0.02) % cycle_beats;
        new_frame += 1;
        new_lookup
            .completion_for_beat(black_box(total), black_box(beat))
            .unwrap_or_default()
            .to_bits()
    });
    assert_eq!(old.checksum, new.checksum);
    assert_zero_alloc(&old);
    assert_zero_alloc(&new);

    println!("\nstream-progress lookup ({HUD_SEGMENTS} segments, {HUD_FRAMES} forward frames)");
    print_result("old binary search", &old);
    print_result("new hinted cursor", &new);
    print_change(&old, &new);
    println!(
        "  retained entry storage: old={} B, new={} B (+{} B cursor)",
        old_lookup.segments.len() * std::mem::size_of::<ProgressEntry>(),
        new_lookup.storage_bytes(),
        std::mem::size_of::<usize>(),
    );
}

fn legacy_segment_indices(segments: &[StreamSegment], current_measure: f32) -> (usize, usize) {
    if current_measure.is_nan() {
        return (segments.len(), segments.len());
    }
    (
        segments.partition_point(|segment| current_measure >= segment.end as f32),
        segments.partition_point(|segment| current_measure > segment.end as f32),
    )
}

fn counter_hud_lookup_benchmark() {
    let segments = (0..HUD_SEGMENTS)
        .map(|index| StreamSegment {
            start: index * 4,
            end: index * 4 + 4,
            is_break: index % 5 == 4,
        })
        .collect::<Vec<_>>();
    let lookup = BrokenRunLookup::new(&segments);
    let cycle_measures = HUD_SEGMENTS as f32 * 4.0;
    for measure in [0.0, 128.0, 4.0, cycle_measures, 17.0, f32::NAN] {
        assert_eq!(
            legacy_segment_indices(&segments, measure),
            lookup.segment_indices(&segments, measure),
        );
    }

    let mut old_frame = 0usize;
    let old = measure(HUD_FRAMES, || {
        let current = (old_frame as f32 * 0.005) % cycle_measures;
        old_frame += 1;
        let (counter, timer) = legacy_segment_indices(black_box(&segments), black_box(current));
        counter as u64 | (timer as u64) << 32
    });
    let mut new_frame = 0usize;
    let new = measure(HUD_FRAMES, || {
        let current = (new_frame as f32 * 0.005) % cycle_measures;
        new_frame += 1;
        let (counter, timer) = lookup.segment_indices(black_box(&segments), black_box(current));
        counter as u64 | (timer as u64) << 32
    });
    assert_eq!(old.checksum, new.checksum);
    assert_zero_alloc(&old);
    assert_zero_alloc(&new);

    println!("\ncounter HUD boundaries ({HUD_SEGMENTS} segments, {HUD_FRAMES} forward frames)");
    print_result("old two binary searches", &old);
    print_result("new shared cursor", &new);
    print_change(&old, &new);
    println!(
        "  retained cursor storage: +{} B",
        std::mem::size_of::<usize>()
    );
}

fn main() {
    note_search_benchmark();
    stream_progress_benchmark();
    counter_hud_lookup_benchmark();
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

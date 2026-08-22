use deadsync_core::{song_time::song_time_ns_delta_seconds, timing::beat_to_note_row};
use deadsync_gameplay::NoteCountStat;
use deadsync_notefield::{
    BrokenRunLookup, StreamProgressLookup,
    performance::{
        EditBeatBarCursor, cue_segment_ranges, edit_beat_bar_info_for_row,
        find_first_displayed_beat, find_first_displayed_row, find_last_displayed_row,
        measure_cue_range_search_enabled,
    },
};
use deadsync_rules::{
    scroll::ScrollSpeedSetting,
    stream::StreamSegment,
    timing::{
        DelaySegment, ScrollSegment, StopSegment, TimeSignatureSegment, TimingData, TimingSegments,
    },
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const NOTE_SEARCHES: usize = 300_000;
const NOTE_STATS: usize = 8_192;
const CMOD_SEARCHES: usize = 50_000;
const CUE_FRAMES: usize = 500_000;
const CUE_SEGMENTS: usize = 4_096;
const EDIT_FRAMES: usize = 5_000;
const EDIT_SIGNATURES: usize = 128;
const EDIT_BARS_PER_SIDE: i32 = 48;
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

fn legacy_visible_rows(
    timing: &TimingData,
    current_time_ns: i64,
    current_beat: f32,
    draw_after: f32,
    draw_before: f32,
    stats: &[NoteCountStat],
) -> Option<(i32, i32)> {
    let first = find_first_displayed_beat(current_beat, draw_after, stats, |beat| {
        cmod_y(timing, current_time_ns, beat)
    });
    let last = legacy_last_beat(current_beat, draw_before, |beat| {
        (cmod_y(timing, current_time_ns, beat), true)
    });
    first.zip(last).map(|(first, last)| {
        let first_row = beat_to_note_row(first);
        let last_row = beat_to_note_row(last.max(first)).max(first_row);
        (first_row, last_row)
    })
}

#[inline(always)]
fn cmod_y(timing: &TimingData, current_time_ns: i64, beat: f32) -> f32 {
    song_time_ns_delta_seconds(timing.get_time_for_beat_ns(beat), current_time_ns) * 320.0
}

fn legacy_last_beat(
    current_beat: f32,
    draw_distance: f32,
    mut y_for_beat: impl FnMut(f32) -> (f32, bool),
) -> Option<f32> {
    if !current_beat.is_finite() || !draw_distance.is_finite() {
        return None;
    }
    let mut search_distance = 10.0;
    let mut last = current_beat + search_distance;
    for _ in 0..20 {
        let (y_offset, _) = y_for_beat(last);
        if y_offset > draw_distance {
            last -= search_distance;
        } else {
            last += search_distance;
        }
        search_distance *= 0.5;
    }
    Some(last)
}

fn row_precision_benchmark() {
    let timing = TimingData::from_segments(
        0.0,
        0.0,
        &TimingSegments {
            bpms: (0..64)
                .map(|index| (index as f32 * 16.0, 120.0 + (index % 5) as f32 * 15.0))
                .collect(),
            ..TimingSegments::default()
        },
        &[],
    );
    let stats = (0..NOTE_STATS)
        .map(|index| NoteCountStat {
            beat: index as f32 * 0.125,
            notes_lower: index * 2,
            notes_upper: index * 2 + 2,
        })
        .collect::<Vec<_>>();
    let queries = (0..4_096)
        .map(|query| {
            let beat = 80.0 + (query % (NOTE_STATS - 640)) as f32 * 0.125;
            (beat, timing.get_time_for_beat_ns(beat))
        })
        .collect::<Vec<_>>();
    for &(beat, current_time_ns) in &queries {
        let old = legacy_visible_rows(&timing, current_time_ns, beat, 768.0, 1_024.0, &stats);
        let first = find_first_displayed_row(beat, 768.0, &stats, |candidate| {
            cmod_y(&timing, current_time_ns, candidate)
        });
        let last = find_last_displayed_row(beat, 1_024.0, 1.0, false, |candidate| {
            (cmod_y(&timing, current_time_ns, candidate), true)
        });
        assert_eq!(old, first.zip(last).map(|(a, b)| (a, b.max(a))));
    }

    let mut old_query = 0usize;
    let old = measure(CMOD_SEARCHES, || {
        let (beat, current_time_ns) = queries[old_query % queries.len()];
        old_query += 1;
        let (first, last) = legacy_visible_rows(
            black_box(&timing),
            current_time_ns,
            black_box(beat),
            768.0,
            1_024.0,
            black_box(&stats),
        )
        .unwrap_or_default();
        first as u32 as u64 | (last as u32 as u64) << 32
    });
    let mut new_query = 0usize;
    let new = measure(CMOD_SEARCHES, || {
        let (beat, current_time_ns) = queries[new_query % queries.len()];
        new_query += 1;
        let first = find_first_displayed_row(beat, 768.0, black_box(&stats), |candidate| {
            cmod_y(black_box(&timing), current_time_ns, candidate)
        });
        let last = find_last_displayed_row(beat, 1_024.0, 1.0, false, |candidate| {
            (cmod_y(black_box(&timing), current_time_ns, candidate), true)
        });
        let (first, last) = first
            .zip(last)
            .map(|(a, b)| (a, b.max(a)))
            .unwrap_or_default();
        first as u32 as u64 | (last as u32 as u64) << 32
    });
    assert_eq!(old.checksum, new.checksum);
    assert_zero_alloc(&old);
    assert_zero_alloc(&new);

    println!("\nCMod visible row range (64 BPMs, {CMOD_SEARCHES} frames)");
    print_result("old full precision", &old);
    print_result("new row precision", &new);
    print_change(&old, &new);
}

fn cmod_cue_search_benchmark() {
    let timing = TimingData::from_segments(
        0.0,
        0.0,
        &TimingSegments {
            bpms: (0..64)
                .map(|index| (index as f32 * 16.0, 120.0 + (index % 5) as f32 * 15.0))
                .collect(),
            ..TimingSegments::default()
        },
        &[],
    );
    let stats = (0..NOTE_STATS)
        .map(|index| NoteCountStat {
            beat: index as f32 * 0.125,
            notes_lower: index * 2,
            notes_upper: index * 2 + 2,
        })
        .collect::<Vec<_>>();
    let queries = (0..4_096)
        .map(|query| {
            let beat = 80.0 + (query % (NOTE_STATS - 640)) as f32 * 0.125;
            (beat, timing.get_time_for_beat_ns(beat))
        })
        .collect::<Vec<_>>();
    assert!(!measure_cue_range_search_enabled(
        true,
        ScrollSpeedSetting::CMod(600.0),
        true,
    ));

    let mut old_query = 0usize;
    let old = measure(CMOD_SEARCHES, || {
        let (beat, current_time_ns) = queries[old_query % queries.len()];
        old_query += 1;
        black_box(legacy_visible_rows(
            black_box(&timing),
            current_time_ns,
            black_box(beat),
            768.0,
            1_424.0,
            black_box(&stats),
        ));
        0
    });
    let mut new_query = 0usize;
    let new = measure(CMOD_SEARCHES, || {
        let (beat, current_time_ns) = queries[new_query % queries.len()];
        new_query += 1;
        let range = measure_cue_range_search_enabled(
            black_box(true),
            black_box(ScrollSpeedSetting::CMod(600.0)),
            black_box(true),
        )
        .then(|| {
            legacy_visible_rows(
                black_box(&timing),
                current_time_ns,
                black_box(beat),
                768.0,
                1_424.0,
                black_box(&stats),
            )
        })
        .flatten();
        black_box(range);
        0
    });
    assert_eq!(old.checksum, new.checksum);
    assert_zero_alloc(&old);
    assert_zero_alloc(&new);

    println!("\nCMod measure-cue range planning ({CMOD_SEARCHES} frames)");
    print_result("old discarded search", &old);
    print_result("new mode gate", &new);
    print_change(&old, &new);
}

fn range_checksum(ranges: &deadsync_notefield::performance::CueSegmentRanges) -> u64 {
    ranges.scrolls.start as u64
        ^ (ranges.scrolls.end as u64).rotate_left(7)
        ^ (ranges.bpms.start as u64).rotate_left(13)
        ^ (ranges.bpms.end as u64).rotate_left(19)
        ^ (ranges.delays.start as u64).rotate_left(29)
        ^ (ranges.delays.end as u64).rotate_left(37)
        ^ (ranges.stops.start as u64).rotate_left(43)
        ^ (ranges.stops.end as u64).rotate_left(53)
}

fn cue_segment_window_benchmark() {
    let scrolls = (0..CUE_SEGMENTS)
        .map(|index| ScrollSegment {
            beat: index as f32 * 0.5,
            ratio: 0.5 + (index % 7) as f32 * 0.125,
        })
        .collect::<Vec<_>>();
    let bpms = (0..CUE_SEGMENTS)
        .map(|index| (index as f32 * 0.5, 90.0 + (index % 11) as f32 * 15.0))
        .collect::<Vec<_>>();
    let delays = (0..CUE_SEGMENTS)
        .map(|index| DelaySegment {
            beat: index as f32 * 0.5,
            duration: 0.025,
        })
        .collect::<Vec<_>>();
    let stops = (0..CUE_SEGMENTS)
        .map(|index| StopSegment {
            beat: index as f32 * 0.5,
            duration: 0.05,
        })
        .collect::<Vec<_>>();
    let cycle = CUE_SEGMENTS as f32 * 0.5 - 24.0;

    for query in 0..4_096 {
        let low = (query as f32 * 0.37) % cycle;
        let range = Some((low, low + 24.0));
        let first = cue_segment_ranges(&scrolls, &bpms, &delays, &stops, range);
        let second = cue_segment_ranges(&scrolls, &bpms, &delays, &stops, range);
        assert_eq!(range_checksum(&first), range_checksum(&second));
    }

    let mut old_frame = 0usize;
    let old = measure(CUE_FRAMES, || {
        let low = (old_frame as f32 * 0.37) % cycle;
        old_frame += 1;
        let range = Some((low, low + 24.0));
        let first = cue_segment_ranges(
            black_box(&scrolls),
            black_box(&bpms),
            black_box(&delays),
            black_box(&stops),
            black_box(range),
        );
        let second = cue_segment_ranges(
            black_box(&scrolls),
            black_box(&bpms),
            black_box(&delays),
            black_box(&stops),
            black_box(range),
        );
        range_checksum(&first).wrapping_add(range_checksum(&second))
    });
    let mut new_frame = 0usize;
    let new = measure(CUE_FRAMES, || {
        let low = (new_frame as f32 * 0.37) % cycle;
        new_frame += 1;
        let ranges = cue_segment_ranges(
            black_box(&scrolls),
            black_box(&bpms),
            black_box(&delays),
            black_box(&stops),
            black_box(Some((low, low + 24.0))),
        );
        range_checksum(&ranges).wrapping_mul(2)
    });
    assert_eq!(old.checksum, new.checksum);
    assert_zero_alloc(&old);
    assert_zero_alloc(&new);

    println!("\nmixed-direction cue windows ({CUE_SEGMENTS} segments/type, {CUE_FRAMES} frames)");
    print_result("old per-group windows", &old);
    print_result("new shared windows", &new);
    print_change(&old, &new);
}

fn edit_info_checksum(info: Option<deadsync_notefield::performance::EditBeatBarInfo>) -> u64 {
    info.map_or(u64::MAX, |info| {
        u64::from(info.frame) ^ (info.measure_index.unwrap_or(-1) as u64).rotate_left(17)
    })
}

fn edit_bar_cursor_benchmark() {
    let signatures = (0..EDIT_SIGNATURES)
        .map(|index| TimeSignatureSegment {
            beat: index as f32 * 16.0,
            numerator: [4, 3, 7, 5][index % 4],
            denominator: [4, 8, 8, 16][index % 4],
        })
        .collect::<Vec<_>>();
    let step_rows = 12;
    let cycle_rows = beat_to_note_row(16.0 * EDIT_SIGNATURES as f32 - 32.0);

    for frame in 0..4_096 {
        let center = (frame * 37 % cycle_rows as usize) as i32 + 96;
        let mut backward = EditBeatBarCursor::new(center, &signatures);
        let mut forward = backward;
        for offset in 0..=EDIT_BARS_PER_SIDE {
            let row = center - offset * step_rows;
            assert_eq!(
                backward.info_for_row(row),
                edit_beat_bar_info_for_row(row, &signatures),
            );
        }
        for offset in 1..=EDIT_BARS_PER_SIDE {
            let row = center + offset * step_rows;
            assert_eq!(
                forward.info_for_row(row),
                edit_beat_bar_info_for_row(row, &signatures),
            );
        }
    }

    let mut old_frame = 0usize;
    let old = measure(EDIT_FRAMES, || {
        let center = (old_frame * 37 % cycle_rows as usize) as i32 + 96;
        old_frame += 1;
        let mut checksum = 0u64;
        for offset in 0..=EDIT_BARS_PER_SIDE {
            checksum = checksum.wrapping_add(edit_info_checksum(edit_beat_bar_info_for_row(
                black_box(center - offset * step_rows),
                black_box(&signatures),
            )));
        }
        for offset in 1..=EDIT_BARS_PER_SIDE {
            checksum = checksum.wrapping_add(edit_info_checksum(edit_beat_bar_info_for_row(
                black_box(center + offset * step_rows),
                black_box(&signatures),
            )));
        }
        checksum
    });
    let mut new_frame = 0usize;
    let new = measure(EDIT_FRAMES, || {
        let center = (new_frame * 37 % cycle_rows as usize) as i32 + 96;
        new_frame += 1;
        let mut backward = EditBeatBarCursor::new(center, black_box(&signatures));
        let mut forward = backward;
        let mut checksum = 0u64;
        for offset in 0..=EDIT_BARS_PER_SIDE {
            checksum = checksum.wrapping_add(edit_info_checksum(
                backward.info_for_row(black_box(center - offset * step_rows)),
            ));
        }
        for offset in 1..=EDIT_BARS_PER_SIDE {
            checksum = checksum.wrapping_add(edit_info_checksum(
                forward.info_for_row(black_box(center + offset * step_rows)),
            ));
        }
        checksum
    });
    assert_eq!(old.checksum, new.checksum);
    assert_zero_alloc(&old);
    assert_zero_alloc(&new);

    println!(
        "\nedit beat-bar metadata ({EDIT_SIGNATURES} signatures, {} bars/frame, {EDIT_FRAMES} frames)",
        EDIT_BARS_PER_SIDE * 2 + 1,
    );
    print_result("old per-bar rescans", &old);
    print_result("new stack cursors", &new);
    print_change(&old, &new);
    println!(
        "  transient cursor storage: {} B",
        std::mem::size_of::<EditBeatBarCursor<'_>>() * 2,
    );
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
                let start = segment.start() as f32;
                let end = segment.end() as f32;
                let entry = ProgressEntry {
                    start,
                    end,
                    stream_before,
                    is_break: segment.is_break(),
                };
                if !segment.is_break() {
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
        .map(|index| StreamSegment::new((index * 4) as u32, (index * 4 + 4) as u32, index % 5 == 4))
        .collect::<Vec<_>>();
    let total = segments
        .iter()
        .filter(|segment| !segment.is_break())
        .map(|segment| (segment.end() - segment.start()) as f64)
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
        segments.partition_point(|segment| current_measure >= segment.end() as f32),
        segments.partition_point(|segment| current_measure > segment.end() as f32),
    )
}

fn counter_hud_lookup_benchmark() {
    let segments = (0..HUD_SEGMENTS)
        .map(|index| StreamSegment::new((index * 4) as u32, (index * 4 + 4) as u32, index % 5 == 4))
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
    row_precision_benchmark();
    cmod_cue_search_benchmark();
    cue_segment_window_benchmark();
    edit_bar_cursor_benchmark();
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

use deadsync_chart::GameplayChartData;
use deadsync_chart::notes::ParsedNote;
use deadsync_core::note::NoteType;
use deadsync_lights::CabinetLight;
use deadsync_lights::cabinet_chart::{
    CabinetLightEvent, CabinetLightPlan, build_cabinet_light_events_for_bench,
    build_cabinet_light_events_reference_for_bench,
};
use deadsync_rules::timing::{
    DelaySegment, FakeSegment, StopSegment, TimingData, TimingSegments, WarpSegment,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

const ROWS: usize = 4_096;
const OPERATIONS: usize = 24;
const SAMPLES: usize = 31;
const PACK_OFFSET_SECONDS: f32 = -0.137_125;

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

// SAFETY: all requests are delegated unchanged to `System`. The benchmark is
// single-threaded while counters are enabled, and relaxed atomics are metrics.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller supplied a valid allocation layout.
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

    const fn heap_calls(self) -> u64 {
        self.allocs + self.reallocs + self.frees
    }

    const fn churn(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }

    const fn allocated_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes
    }
}

struct Measurement {
    median_ns: f64,
    p95_ns: f64,
    median_cycles: Option<f64>,
    alloc: AllocSnapshot,
    checksum: u64,
    events: usize,
}

#[derive(Default)]
struct Samples {
    ns: Vec<f64>,
    cycles: Vec<f64>,
    checksum: u64,
    events: usize,
}

fn record_sample(samples: &mut Samples, build: &mut impl FnMut() -> Vec<CabinetLightEvent>) {
    let cycle_start = cycle_counter();
    let started = Instant::now();
    for _ in 0..OPERATIONS {
        let built = build();
        samples.events = built.len();
        samples.checksum = samples.checksum.wrapping_add(black_box(checksum(built)));
    }
    samples
        .ns
        .push(started.elapsed().as_secs_f64() * 1e9 / OPERATIONS as f64);
    if let Some(elapsed) = cycle_start
        .zip(cycle_counter())
        .map(|(start, end)| end.wrapping_sub(start) as f64 / OPERATIONS as f64)
    {
        samples.cycles.push(elapsed);
    }
}

fn allocation_sample(build: &mut impl FnMut() -> Vec<CabinetLightEvent>) -> AllocSnapshot {
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    black_box(checksum(build()));
    ALLOC.enabled.store(false, Ordering::Relaxed);
    ALLOC.snapshot().delta(before)
}

fn finish(mut samples: Samples, alloc: AllocSnapshot) -> Measurement {
    samples.ns.sort_by(f64::total_cmp);
    samples.cycles.sort_by(f64::total_cmp);
    Measurement {
        median_ns: samples.ns[SAMPLES / 2],
        p95_ns: samples.ns[SAMPLES * 95 / 100],
        median_cycles: (!samples.cycles.is_empty())
            .then(|| samples.cycles[samples.cycles.len() / 2]),
        alloc,
        checksum: samples.checksum,
        events: samples.events,
    }
}

fn measure_pair(
    mut old_build: impl FnMut() -> Vec<CabinetLightEvent>,
    mut new_build: impl FnMut() -> Vec<CabinetLightEvent>,
) -> (Measurement, Measurement) {
    for _ in 0..3 {
        black_box(checksum(old_build()));
        black_box(checksum(new_build()));
    }

    let mut old_samples = Samples {
        ns: Vec::with_capacity(SAMPLES),
        cycles: Vec::with_capacity(SAMPLES),
        ..Samples::default()
    };
    let mut new_samples = Samples {
        ns: Vec::with_capacity(SAMPLES),
        cycles: Vec::with_capacity(SAMPLES),
        ..Samples::default()
    };
    for sample in 0..SAMPLES {
        if sample.is_multiple_of(2) {
            record_sample(&mut old_samples, &mut old_build);
            record_sample(&mut new_samples, &mut new_build);
        } else {
            record_sample(&mut new_samples, &mut new_build);
            record_sample(&mut old_samples, &mut old_build);
        }
    }

    let old_alloc = allocation_sample(&mut old_build);
    let new_alloc = allocation_sample(&mut new_build);
    (
        finish(old_samples, old_alloc),
        finish(new_samples, new_alloc),
    )
}

fn checksum(events: Vec<CabinetLightEvent>) -> u64 {
    // Equal-time light events commute at runtime, so use an order-independent
    // fingerprint while the unit test separately proves exact event parity.
    let mut sum = events.len() as u64;
    let mut xor = 0_u64;
    for event in events {
        let light = match event.light {
            CabinetLight::MarqueeUpperLeft => 1,
            CabinetLight::MarqueeUpperRight => 2,
            CabinetLight::MarqueeLowerLeft => 3,
            CabinetLight::MarqueeLowerRight => 4,
            CabinetLight::BassLeft => 5,
            CabinetLight::BassRight => 6,
        };
        let value = (event.time_ns as u64)
            .wrapping_mul(0x9e37_79b9_7f4a_7c15)
            .wrapping_add((event.row_index as u64).rotate_left(17))
            .wrapping_add(light << 3)
            .wrapping_add(u64::from(event.simplify_bass_candidate));
        sum = sum.wrapping_add(value);
        xor ^= value.rotate_left((event.row_index & 63) as u32);
    }
    sum ^ xor
}

fn main() {
    let charts = fixture();
    let plan = CabinetLightPlan::Generated {
        marquee_ix: 0,
        marquee_hash: "challenge".to_owned(),
        bass_ix: 1,
        bass_hash: "hard".to_owned(),
    };

    let (old, new) = measure_pair(
        || {
            build_cabinet_light_events_reference_for_bench(
                black_box(&plan),
                black_box(&charts),
                black_box(PACK_OFFSET_SECONDS),
            )
        },
        || {
            build_cabinet_light_events_for_bench(
                black_box(&plan),
                black_box(&charts),
                black_box(PACK_OFFSET_SECONDS),
            )
        },
    );

    assert_eq!(old.events, new.events, "event count diverged");
    assert_eq!(old.checksum, new.checksum, "event behavior diverged");
    assert_eq!(
        new.alloc.allocs, 1,
        "new path must allocate only its result"
    );
    assert_eq!(new.alloc.reallocs, 0, "new result must not grow");
    assert!(new.median_ns < old.median_ns, "median latency regressed");
    assert!(new.p95_ns <= old.p95_ns * 1.05, "p95 latency regressed");
    if let (Some(old_cycles), Some(new_cycles)) = (old.median_cycles, new.median_cycles) {
        assert!(new_cycles < old_cycles, "CPU cycles regressed");
    }
    assert!(
        new.alloc.heap_calls() < old.alloc.heap_calls(),
        "heap calls did not improve"
    );
    assert!(
        new.alloc.allocated_bytes() < old.alloc.allocated_bytes(),
        "allocated bytes did not improve"
    );
    assert!(
        new.alloc.churn() < old.alloc.churn(),
        "memory churn did not improve"
    );

    println!("cabinet-light chart compilation ({} events)", new.events);
    print_row("old", &old);
    print_row("new", &new);
    println!(
        "  change: {:+.2}% median  {:+.2}% p95  {:+.2}% cycles  {:+.2}% throughput  \
         {:+.2}% heap calls  {:+.2}% allocated bytes  {:+.2}% churn",
        change(old.median_ns, new.median_ns),
        change(old.p95_ns, new.p95_ns),
        change(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN),
        ),
        change(throughput(&old), throughput(&new)),
        change(old.alloc.heap_calls() as f64, new.alloc.heap_calls() as f64),
        change(
            old.alloc.allocated_bytes() as f64,
            new.alloc.allocated_bytes() as f64,
        ),
        change(old.alloc.churn() as f64, new.alloc.churn() as f64),
    );
}

fn print_row(label: &str, row: &Measurement) {
    println!(
        "  {label:<3} {:>11.0} ns  {:>11.0} p95  {:>11.0} cycles  {:>8.2} Mevent/s  \
         {:>2} alloc {:>2} realloc {:>2} free  {:>9} alloc B  {:>9} churn B",
        row.median_ns,
        row.p95_ns,
        row.median_cycles.unwrap_or(f64::NAN),
        throughput(row) / 1e6,
        row.alloc.allocs,
        row.alloc.reallocs,
        row.alloc.frees,
        row.alloc.allocated_bytes(),
        row.alloc.churn(),
    );
}

fn throughput(row: &Measurement) -> f64 {
    row.events as f64 * 1e9 / row.median_ns
}

fn change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

fn fixture() -> [GameplayChartData; 2] {
    let rows_per_beat = deadsync_core::timing::ROWS_PER_BEAT as usize;
    let notes = (0..ROWS)
        .flat_map(|row| {
            let chart_row = row * (rows_per_beat / 4);
            let note_type = match row % 17 {
                0 => NoteType::Mine,
                1 => NoteType::Lift,
                2 => NoteType::Fake,
                3 => NoteType::Hold,
                4 => NoteType::Roll,
                _ => NoteType::Tap,
            };
            [
                ParsedNote {
                    row_index: chart_row,
                    column: row % 4,
                    note_type,
                    tail_row_index: None,
                },
                ParsedNote {
                    row_index: chart_row,
                    column: (row + 1) % 4,
                    note_type: NoteType::Tap,
                    tail_row_index: None,
                },
            ]
        })
        .collect::<Vec<_>>();
    [chart(notes.clone()), chart(notes)]
}

fn chart(parsed_notes: Vec<ParsedNote>) -> GameplayChartData {
    let max_row = parsed_notes.last().map_or(0, |note| note.row_index);
    let row_to_beat = (0..=max_row)
        .map(|row| row as f32 / deadsync_core::timing::ROWS_PER_BEAT as f32)
        .collect::<Vec<_>>();
    let timing_segments = TimingSegments {
        bpms: vec![(0.0, 120.0), (128.0, 180.0), (256.0, 90.0), (512.0, 210.0)],
        stops: vec![StopSegment {
            beat: 96.0,
            duration: 0.125,
        }],
        delays: vec![DelaySegment {
            beat: 320.0,
            duration: 0.075,
        }],
        warps: vec![WarpSegment {
            beat: 224.0,
            length: 2.0,
        }],
        fakes: vec![
            FakeSegment {
                beat: 400.0,
                length: 2.0,
            },
            FakeSegment {
                beat: 700.0,
                length: 1.0,
            },
        ],
        ..TimingSegments::default()
    };
    let timing = TimingData::from_segments(0.0, 0.0, &timing_segments, &row_to_beat);
    GameplayChartData {
        notes: Vec::new(),
        parsed_notes,
        row_to_beat,
        timing_segments,
        timing,
        chart_attacks: None,
    }
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

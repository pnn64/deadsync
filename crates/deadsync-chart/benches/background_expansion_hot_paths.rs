use deadsync_chart::background::{bench_support, expand_random_background_changes};
use deadsync_chart::{SongBackgroundChange, SongBackgroundChangeTarget, SongData};
use deadsync_core::timing::beat_to_note_row;
use deadsync_rules::timing::{
    TimeSignatureSegment, TimingData, TimingSegments, default_time_signature,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

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

// SAFETY: every request is delegated unchanged to `System`; relaxed counters
// only observe calls made by this single-threaded benchmark while enabled.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is supplied by the allocator caller.
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

fn measure(operations: usize, mut op: impl FnMut() -> u64) -> Row {
    for _ in 0..3 {
        black_box(op());
    }

    let mut times = Vec::with_capacity(SAMPLES);
    let mut cycles = Vec::with_capacity(SAMPLES);
    let mut checksum = 0_u64;
    for _ in 0..SAMPLES {
        let cycle_start = cycle_counter();
        let started = Instant::now();
        for _ in 0..operations {
            checksum = checksum.wrapping_add(black_box(op()));
        }
        times.push(started.elapsed().as_secs_f64() * 1e9 / operations as f64);
        if let Some(elapsed) = cycle_start
            .zip(cycle_counter())
            .map(|(start, end)| end.wrapping_sub(start) as f64 / operations as f64)
        {
            cycles.push(elapsed);
        }
    }
    times.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let alloc_checksum = black_box(op());
    ALLOC.enabled.store(false, Ordering::Relaxed);
    black_box(alloc_checksum);

    Row {
        median_ns: times[SAMPLES / 2],
        p95_ns: times[SAMPLES * 95 / 100],
        median_cycles: (!cycles.is_empty()).then(|| cycles[cycles.len() / 2]),
        alloc: ALLOC.snapshot().delta(before),
        checksum,
    }
}

fn print_pair(title: &str, units: usize, old: &Row, new: &Row) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    println!("\n{title} ({units} units/op)");
    print_row("old", units, old);
    print_row("new", units, new);
    println!(
        "  change: {:>7.2}% median  {:>7.2}% cycles  {:>7.2}% throughput  \
         {:>7.2}% p95  {:>7.2}% allocs  {:>7.2}% reallocs  {:>7.2}% churn",
        change(old.median_ns, new.median_ns),
        change(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN),
        ),
        change(throughput(old, units), throughput(new, units)),
        change(old.p95_ns, new.p95_ns),
        change(old.alloc.allocs as f64, new.alloc.allocs as f64),
        change(old.alloc.reallocs as f64, new.alloc.reallocs as f64),
        change(old.alloc.churn() as f64, new.alloc.churn() as f64),
    );
}

fn print_row(label: &str, units: usize, row: &Row) {
    println!(
        "  {label:<3} {:>11.0} ns/op  {:>11.0} cycles/op  {:>11.0} p95 ns  \
         {:>8.2} Munit/s  {:>5} alloc  {:>5} realloc  {:>5} free  {:>10} churn B",
        row.median_ns,
        row.median_cycles.unwrap_or(f64::NAN),
        row.p95_ns,
        throughput(row, units) / 1e6,
        row.alloc.allocs,
        row.alloc.reallocs,
        row.alloc.frees,
        row.alloc.churn(),
    );
}

fn throughput(row: &Row, units: usize) -> f64 {
    units as f64 * 1e9 / row.median_ns
}

fn change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

fn change_checksum(changes: Vec<SongBackgroundChange>) -> u64 {
    changes.into_iter().fold(0_u64, |checksum, change| {
        let target = match change.target {
            SongBackgroundChangeTarget::File(path) => {
                path.to_string_lossy().bytes().fold(0_u64, |sum, byte| {
                    sum.wrapping_mul(33).wrapping_add(u64::from(byte))
                })
            }
            SongBackgroundChangeTarget::Animation(name) => name.bytes().fold(1_u64, |sum, byte| {
                sum.wrapping_mul(33).wrapping_add(u64::from(byte))
            }),
            SongBackgroundChangeTarget::NoSongBg => 2,
            SongBackgroundChangeTarget::Random => 3,
        };
        checksum
            .wrapping_mul(1_099_511_628_211)
            .wrapping_add(u64::from(change.start_beat.to_bits()))
            .wrapping_add(target)
    })
}

fn row_checksum(rows: Vec<i32>) -> u64 {
    rows.into_iter().fold(0_u64, |checksum, row| {
        checksum
            .wrapping_mul(1_099_511_628_211)
            .wrapping_add(row as u64)
    })
}

fn legacy_unique_rows(rows: &[i32]) -> Vec<i32> {
    let mut out = Vec::with_capacity(rows.len());
    for &row in rows {
        if !out.contains(&row) {
            out.push(row);
        }
    }
    out
}

fn legacy_signature_checksum(segments: &TimingSegments, interval_count: usize) -> u64 {
    let mut checksum = 0_u64;
    for interval in 0..interval_count {
        let sigs = normalized_time_signatures(segments);
        for sig in sigs {
            checksum = checksum
                .wrapping_mul(1_099_511_628_211)
                .wrapping_add(u64::from(sig.beat.to_bits()))
                .wrapping_add(sig.numerator as u64)
                .wrapping_add((sig.denominator as u64) << 32)
                .wrapping_add(interval as u64);
        }
    }
    checksum
}

fn normalized_time_signatures(segments: &TimingSegments) -> Vec<TimeSignatureSegment> {
    let mut sigs = segments.time_signatures.clone();
    if sigs.is_empty() {
        sigs.push(default_time_signature());
    }
    sigs.sort_by(|left, right| left.beat.total_cmp(&right.beat));
    if sigs
        .first()
        .is_none_or(|sig| beat_to_note_row(sig.beat) > 0)
    {
        sigs.insert(0, default_time_signature());
    }
    sigs
}

fn legacy_expand_nonrandom(
    song: &SongData,
    timing: &TimingData,
    paths: Vec<PathBuf>,
    seed_text: &str,
) -> Vec<SongBackgroundChange> {
    if paths.is_empty() {
        return song.background_changes.clone();
    }
    let mut cycle = LegacyMovieCycle::new(paths, seed_text);
    let last_beat =
        timing.get_beat_for_time(song.precise_last_second().max(song.music_length_seconds));
    let mut out = Vec::with_capacity(song.background_changes.len());
    let mut expanded_random = false;
    for change in &song.background_changes {
        match change.target {
            SongBackgroundChangeTarget::Random => {
                let mut expanded = change.clone();
                expanded.start_beat = last_beat;
                if let Some(path) = cycle.next_path() {
                    expanded.target = SongBackgroundChangeTarget::File(path);
                    out.push(expanded);
                }
                expanded_random = true;
            }
            _ => out.push(change.clone()),
        }
    }
    if !expanded_random {
        return song.background_changes.clone();
    }
    out
}

struct LegacyMovieCycle {
    paths: Vec<PathBuf>,
    next: usize,
}

impl LegacyMovieCycle {
    fn new(mut paths: Vec<PathBuf>, seed_text: &str) -> Self {
        shuffle_paths(&mut paths, u64::from(crc32(seed_text.as_bytes())));
        paths.truncate(10);
        Self { paths, next: 0 }
    }

    fn next_path(&mut self) -> Option<PathBuf> {
        let path = self.paths.get(self.next)?.clone();
        self.next = (self.next + 1) % self.paths.len();
        Some(path)
    }
}

fn shuffle_paths(paths: &mut [PathBuf], seed: u64) {
    let mut rng = XorShift64::new(seed);
    for index in (1..paths.len()).rev() {
        let other = rng.gen_range(index + 1);
        paths.swap(index, other);
    }
}

struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    const fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
        }
    }

    const fn next_u32(&mut self) -> u32 {
        let mut value = self.state;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.state = value;
        (value >> 32) as u32
    }

    const fn gen_range(&mut self, upper_exclusive: usize) -> usize {
        if upper_exclusive <= 1 {
            0
        } else {
            self.next_u32() as usize % upper_exclusive
        }
    }
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 0 {
                crc >> 1
            } else {
                (crc >> 1) ^ 0xEDB8_8320
            };
        }
    }
    !crc
}

fn fixture_song() -> SongData {
    let background_changes = (0..128)
        .map(|index| {
            SongBackgroundChange::new(
                index as f32 * 4.0,
                SongBackgroundChangeTarget::File(PathBuf::from(format!(
                    "backgrounds/change-{index:03}.png"
                ))),
            )
        })
        .collect();
    SongData {
        simfile_path: PathBuf::from("Songs/Bench/Background/song.ssc"),
        title: "Background expansion benchmark".to_owned(),
        subtitle: String::new(),
        translit_title: String::new(),
        translit_subtitle: String::new(),
        artist: String::new(),
        translit_artist: String::new(),
        genre: String::new(),
        banner_path: None,
        background_path: None,
        background_changes,
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
        normalized_bpms: "0.000=120.000".to_owned(),
        music_length_seconds: 180.0,
        first_second: 0.0,
        total_length_seconds: 180,
        precise_last_second_seconds: 180.0,
        charts: Vec::new(),
    }
}

fn fixture_signatures() -> TimingSegments {
    TimingSegments {
        time_signatures: (0..64)
            .rev()
            .map(|index| TimeSignatureSegment {
                beat: index as f32 * 48.0,
                numerator: 3 + index % 5,
                denominator: [4, 8, 4, 16][index as usize % 4],
            })
            .collect(),
        ..TimingSegments::default()
    }
}

fn fixture_rows() -> Vec<i32> {
    let mut rows = Vec::with_capacity(6_144);
    for row in 0..2_048_i32 {
        rows.push(row);
        rows.push(row);
        if row % 2 == 0 {
            rows.push(row / 2);
        }
    }
    rows
}

#[cfg(target_arch = "x86")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions have no memory operands.
    unsafe {
        std::arch::x86::_mm_lfence();
        Some(std::arch::x86::_rdtsc())
    }
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions have no memory operands.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> Option<u64> {
    None
}

fn main() {
    let song = fixture_song();
    let timing_segments = TimingSegments {
        bpms: vec![(0.0, 120.0)],
        ..TimingSegments::default()
    };
    let timing = TimingData::from_segments(0.0, 0.0, &timing_segments, &[]);
    let paths = (0..10)
        .map(|index| PathBuf::from(format!("RandomMovies/movie-{index}.mp4")))
        .collect::<Vec<_>>();
    let old = measure(200, || {
        change_checksum(legacy_expand_nonrandom(
            black_box(&song),
            &timing,
            paths.clone(),
            "Bench Song",
        ))
    });
    let new = measure(200, || {
        change_checksum(expand_random_background_changes(
            black_box(&song),
            &timing,
            &timing_segments,
            paths.clone(),
            "Bench Song",
        ))
    });
    print_pair(
        "non-random boundary exit",
        song.background_changes.len(),
        &old,
        &new,
    );

    let signatures = fixture_signatures();
    let interval_count = 64;
    let old = measure(20, || {
        legacy_signature_checksum(black_box(&signatures), interval_count)
    });
    let new = measure(20, || {
        bench_support::reused_signature_checksum(black_box(&signatures), interval_count)
    });
    print_pair(
        "time-signature normalization reuse",
        signatures.time_signatures.len() * interval_count,
        &old,
        &new,
    );

    let rows = fixture_rows();
    let old = measure(10, || row_checksum(legacy_unique_rows(black_box(&rows))));
    let new = measure(10, || {
        row_checksum(bench_support::unique_rows(black_box(&rows)))
    });
    print_pair("random-row duplicate rejection", rows.len(), &old, &new);
}

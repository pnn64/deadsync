use deadsync_chart::SongData;
use deadsync_simfile::playlist::bench_support::{
    normalize_song_path_ascii_lowercase_current, normalize_song_path_ascii_lowercase_reference,
    playlist_entries_from_text_reference, playlist_lookup_current_checksum,
    playlist_lookup_reference_checksum,
};
use deadsync_simfile::playlist::{
    PlaylistEntry, PlaylistSongSource, build_playlist_song_lookup, playlist_entries_from_text,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::fmt::Write as _;
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 31;
const NORMALIZE_REPEATS: usize = 512;
const PARSE_REPEATS: usize = 16;
const LOOKUP_SONGS: usize = 1_024;
const PLAYLIST_LINES: usize = 512;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    frees: AtomicU64,
    allocated_bytes: AtomicU64,
    freed_bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            frees: AtomicU64::new(0),
            allocated_bytes: AtomicU64::new(0),
            freed_bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            frees: self.frees.load(Ordering::Relaxed),
            allocated_bytes: self.allocated_bytes.load(Ordering::Relaxed),
            freed_bytes: self.freed_bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: allocation operations are delegated unchanged to `System`; the
// relaxed counters only observe this single-threaded benchmark while enabled.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` is supplied by the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.allocated_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.frees.fetch_add(1, Ordering::Relaxed);
            self.freed_bytes
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
            if new_size > old.size() {
                self.allocated_bytes
                    .fetch_add((new_size - old.size()) as u64, Ordering::Relaxed);
            } else {
                self.freed_bytes
                    .fetch_add((old.size() - new_size) as u64, Ordering::Relaxed);
            }
        }
        out
    }
}

#[derive(Clone, Copy)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    frees: u64,
    allocated_bytes: u64,
    freed_bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            allocated_bytes: self.allocated_bytes - before.allocated_bytes,
            freed_bytes: self.freed_bytes - before.freed_bytes,
        }
    }

    const fn churn_bytes(self) -> u64 {
        self.allocated_bytes + self.freed_bytes
    }
}

struct BenchResult {
    median_ns: f64,
    p95_ns: f64,
    median_cycles: Option<f64>,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure_pair(
    mut old_op: impl FnMut() -> u64,
    mut new_op: impl FnMut() -> u64,
) -> (BenchResult, BenchResult) {
    for _ in 0..3 {
        black_box(old_op());
        black_box(new_op());
    }

    let mut old_times = Vec::with_capacity(SAMPLES);
    let mut new_times = Vec::with_capacity(SAMPLES);
    let mut old_cycles = Vec::with_capacity(SAMPLES);
    let mut new_cycles = Vec::with_capacity(SAMPLES);
    let mut old_checksum = 0;
    let mut new_checksum = 0;
    for sample in 0..SAMPLES {
        let record = |op: &mut dyn FnMut() -> u64,
                      times: &mut Vec<f64>,
                      cycles: &mut Vec<f64>,
                      checksum: &mut u64| {
            let cycle_start = cycle_counter();
            let started = Instant::now();
            let value = black_box(op());
            times.push(started.elapsed().as_secs_f64() * 1_000_000_000.0);
            if let Some((start, end)) = cycle_start.zip(cycle_counter()) {
                cycles.push(end.wrapping_sub(start) as f64);
            }
            *checksum ^= value;
        };
        if sample % 2 == 0 {
            record(
                &mut old_op,
                &mut old_times,
                &mut old_cycles,
                &mut old_checksum,
            );
            record(
                &mut new_op,
                &mut new_times,
                &mut new_cycles,
                &mut new_checksum,
            );
        } else {
            record(
                &mut new_op,
                &mut new_times,
                &mut new_cycles,
                &mut new_checksum,
            );
            record(
                &mut old_op,
                &mut old_times,
                &mut old_cycles,
                &mut old_checksum,
            );
        }
    }

    old_times.sort_by(f64::total_cmp);
    new_times.sort_by(f64::total_cmp);
    old_cycles.sort_by(f64::total_cmp);
    new_cycles.sort_by(f64::total_cmp);
    let old_allocated = measure_allocations(&mut old_op);
    let new_allocated = measure_allocations(&mut new_op);
    let row = |times: &[f64], cycles: &[f64], allocated, checksum| BenchResult {
        median_ns: percentile(times, 50),
        p95_ns: percentile(times, 95),
        median_cycles: (!cycles.is_empty()).then(|| percentile(cycles, 50)),
        allocated,
        checksum,
    };
    (
        row(&old_times, &old_cycles, old_allocated, old_checksum),
        row(&new_times, &new_cycles, new_allocated, new_checksum),
    )
}

fn measure_allocations(op: &mut impl FnMut() -> u64) -> AllocSnapshot {
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    black_box(op());
    ALLOC.enabled.store(false, Ordering::Relaxed);
    ALLOC.snapshot().delta(before)
}

fn percentile(sorted: &[f64], percentile: usize) -> f64 {
    let index = (sorted.len() * percentile).div_ceil(100).saturating_sub(1);
    sorted[index.min(sorted.len() - 1)]
}

fn text_checksum(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn entries_checksum(entries: &[PlaylistEntry]) -> u64 {
    entries
        .iter()
        .enumerate()
        .fold(0, |checksum, (index, entry)| {
            let value = match entry {
                PlaylistEntry::Header { name, song_count } => {
                    text_checksum(name) ^ (*song_count as u64).rotate_left(17)
                }
                PlaylistEntry::Song(song) => text_checksum(&song.title).rotate_left(31),
            };
            checksum.wrapping_add(value.rotate_left((index % 63) as u32 + 1))
        })
}

fn run_normalization(fixture: &[&str], normalize: fn(&str) -> String) -> u64 {
    let mut checksum = 0u64;
    for _ in 0..NORMALIZE_REPEATS {
        for path in fixture {
            checksum = checksum
                .rotate_left(7)
                .wrapping_add(text_checksum(&normalize(black_box(path))));
        }
    }
    checksum
}

fn run_playlist_parse(
    text: &str,
    lookup: &deadsync_simfile::playlist::PlaylistSongLookup,
    reference: bool,
) -> u64 {
    let mut checksum = 0u64;
    for _ in 0..PARSE_REPEATS {
        let entries = if reference {
            playlist_entries_from_text_reference(text, "Benchmark", lookup)
        } else {
            playlist_entries_from_text(text, "Benchmark", lookup)
        };
        checksum = checksum
            .rotate_left(5)
            .wrapping_add(entries_checksum(&entries));
    }
    checksum
}

fn song(pack: &str, index: usize) -> Arc<SongData> {
    Arc::new(SongData {
        simfile_path: PathBuf::from(format!("Songs/{pack}/Song {index:05}/chart.ssc")),
        title: format!("Song {index:05}"),
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
        min_bpm: 0.0,
        max_bpm: 0.0,
        normalized_bpms: String::new(),
        music_length_seconds: 0.0,
        first_second: 0.0,
        total_length_seconds: 0,
        precise_last_second_seconds: 0.0,
        charts: Vec::new(),
    })
}

fn sources() -> Vec<PlaylistSongSource> {
    (0..LOOKUP_SONGS)
        .map(|index| {
            let folder_pack = format!("Pack {:03}", index % 32);
            let group = if index % 3 == 0 {
                format!("Series {:02}", index % 8)
            } else {
                folder_pack.clone()
            };
            PlaylistSongSource {
                group_name: Some(group),
                song: song(&folder_pack, index),
                lobby_path: Some(format!("{folder_pack}/Song {index:05}")),
            }
        })
        .collect()
}

fn playlist_text() -> String {
    let mut text = String::with_capacity(PLAYLIST_LINES * 32);
    for index in 0..PLAYLIST_LINES {
        if index % 64 == 0 {
            let _ = writeln!(text, "--- Section {}", index / 64);
        }
        if index % 17 == 0 {
            let _ = writeln!(text, "PACK {:03}/*", index % 32);
        } else {
            let song = index % LOOKUP_SONGS;
            let _ = writeln!(text, "\\Pack {:03}\\Song {song:05}\\", song % 32);
        }
    }
    text
}

fn print_pair(title: &str, old: &BenchResult, new: &BenchResult, calls: usize) {
    assert_eq!(old.checksum, new.checksum, "{title}: behavior diverged");
    let print = |label: &str, result: &BenchResult| {
        println!(
            "{label:<3} {:>10.1} ns median  {:>10.1} ns p95  {:>10.1} cycles  \
             {:>8.2} Mops/s  {:>7} alloc  {:>6} realloc  {:>7} free  \
             {:>10} B alloc  {:>10} B churn",
            result.median_ns,
            result.p95_ns,
            result.median_cycles.unwrap_or(f64::NAN),
            calls as f64 * 1_000.0 / result.median_ns,
            result.allocated.allocs,
            result.allocated.reallocs,
            result.allocated.frees,
            result.allocated.allocated_bytes,
            result.allocated.churn_bytes(),
        );
    };
    println!("\n{title} ({calls} operations/sample)");
    print("old", old);
    print("new", new);
    println!(
        "gain {:>7.2}x throughput  {:>7.2}% median  {:>7.2}% p95  \
         {:>7.2}% cycles  {:>7.2}% allocs  {:>7.2}% churn",
        old.median_ns / new.median_ns,
        reduction(old.median_ns, new.median_ns),
        reduction(old.p95_ns, new.p95_ns),
        reduction(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN),
        ),
        reduction(old.allocated.allocs as f64, new.allocated.allocs as f64),
        reduction(
            old.allocated.churn_bytes() as f64,
            new.allocated.churn_bytes() as f64,
        ),
    );
    assert!(new.median_ns < old.median_ns, "{title}: median regressed");
    if let (Some(old_cycles), Some(new_cycles)) = (old.median_cycles, new.median_cycles) {
        assert!(new_cycles < old_cycles, "{title}: cycles regressed");
    }
}

fn reduction(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::NEG_INFINITY };
    }
    (1.0 - new / old) * 100.0
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
    let paths = [
        " /Songs\\Pack//Song/ ",
        "MIXED/Ascii/Path.SSC",
        "Songs/Very Long Pack Name/Very Long Song Name/chart.ssc",
        " MÃ¼sic\\æ›²/Ä°STANBUL ",
        "\\\\Pack\\\\Song\\",
        "Pack/ Song Name /",
        "A/B/C/D/E/F/G/H/I/chart.sm",
        "already/lowercase/chart.ssc",
    ];
    let (old, new) = measure_pair(
        || run_normalization(&paths, normalize_song_path_ascii_lowercase_reference),
        || run_normalization(&paths, normalize_song_path_ascii_lowercase_current),
    );
    print_pair(
        "ASCII playlist-path normalization",
        &old,
        &new,
        paths.len() * NORMALIZE_REPEATS,
    );

    let sources = sources();
    let (old, new) = measure_pair(
        || playlist_lookup_reference_checksum(sources.iter().cloned()),
        || playlist_lookup_current_checksum(sources.iter().cloned()),
    );
    print_pair("playlist lookup construction", &old, &new, sources.len());

    let lookup = build_playlist_song_lookup(sources.iter().cloned());
    let text = playlist_text();
    let (old, new) = measure_pair(
        || run_playlist_parse(&text, &lookup, true),
        || run_playlist_parse(&text, &lookup, false),
    );
    print_pair(
        "playlist line resolution",
        &old,
        &new,
        PLAYLIST_LINES * PARSE_REPEATS,
    );
}

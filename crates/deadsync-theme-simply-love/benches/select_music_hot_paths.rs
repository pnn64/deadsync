use deadsync_chart::{STANDARD_DIFFICULTY_COUNT, STANDARD_DIFFICULTY_NAMES, SongData, SyncPref};
use deadsync_score::is_itl_unlocks_pack;
use deadsync_simfile::event_intro::is_srpg_event_song;
use deadsync_theme_simply_love::screens::select_music::{
    MusicWheelEntry, benchmark_chart_info_text_new, benchmark_chart_info_text_old,
    benchmark_fill_displayed_entries, benchmark_select_music_entries, benchmark_wheel_song_meta,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const PACK_COUNT: usize = 128;
const SONGS_PER_PACK: usize = 24;
const LOOKUP_OPS: usize = 200_000;
const SORT_OPS: usize = 2_000;
const REFILL_OPS: usize = 5_000;
const CHART_INFO_OPS: usize = 500_000;
const WHEEL_META_OPS: usize = 200_000;

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
    println!(
        "{label:<12} {:>10.2} ns/op  {:>10.2} cycles/op  {:>10.2} worst ns  \
         {:>8.3} Mop/s  {:>7.2} alloc  {:>7.2} realloc  {:>7.2} free  {:>10.1} B/op",
        result.ns_per_op,
        result.cycles_per_op.unwrap_or(f64::NAN),
        result.worst_sample_ns,
        1_000.0 / result.ns_per_op,
        result.allocated.allocs as f64 / ops,
        result.allocated.reallocs as f64 / ops,
        result.allocated.deallocs as f64 / ops,
        result.allocated.bytes as f64 / ops,
    );
}

fn print_pair(title: &str, iterations: usize, old: &BenchResult, new: &BenchResult) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    println!("\n{title}");
    print_result("old", iterations, old);
    print_result("new", iterations, new);
    println!(
        "improvement  {:>8.2}x throughput  {:>8.2}% fewer allocated bytes",
        old.ns_per_op / new.ns_per_op,
        if old.allocated.bytes == 0 {
            0.0
        } else {
            100.0 * (1.0 - new.allocated.bytes as f64 / old.allocated.bytes as f64)
        },
    );
}

#[derive(Clone)]
enum LegacyEntry {
    Header {
        name: String,
        original_index: usize,
        banner_path: Option<PathBuf>,
        song_count: usize,
        pack_key: Option<String>,
        parent_series: Option<String>,
    },
    Song(Arc<SongData>),
}

fn legacy_entries(entries: &[MusicWheelEntry]) -> Vec<LegacyEntry> {
    entries
        .iter()
        .map(|entry| match entry {
            MusicWheelEntry::PackHeader {
                name,
                original_index,
                banner_path,
                song_count,
                pack_key,
                parent_series,
                ..
            } => LegacyEntry::Header {
                name: name.to_string(),
                original_index: *original_index,
                banner_path: banner_path.as_deref().map(Path::to_path_buf),
                song_count: *song_count,
                pack_key: pack_key.as_deref().map(str::to_owned),
                parent_series: parent_series.as_deref().map(str::to_owned),
            },
            MusicWheelEntry::Song(song) => LegacyEntry::Song(Arc::clone(song)),
        })
        .collect()
}

fn new_entry_checksum(entry: &MusicWheelEntry) -> u64 {
    match entry {
        MusicWheelEntry::PackHeader {
            name,
            original_index,
            banner_path,
            song_count,
            pack_key,
            parent_series,
        } => {
            name.len() as u64
                ^ (*original_index as u64).rotate_left(7)
                ^ (*song_count as u64).rotate_left(13)
                ^ u64::from(banner_path.is_some()).rotate_left(19)
                ^ pack_key.as_deref().map_or(0, |key| key.len() as u64)
                ^ parent_series
                    .as_deref()
                    .map_or(0, |parent| (parent.len() as u64).rotate_left(23))
        }
        MusicWheelEntry::Song(song) => Arc::as_ptr(song) as usize as u64,
    }
}

fn legacy_entry_checksum(entry: &LegacyEntry) -> u64 {
    match entry {
        LegacyEntry::Header {
            name,
            original_index,
            banner_path,
            song_count,
            pack_key,
            parent_series,
        } => {
            name.len() as u64
                ^ (*original_index as u64).rotate_left(7)
                ^ (*song_count as u64).rotate_left(13)
                ^ u64::from(banner_path.is_some()).rotate_left(19)
                ^ pack_key.as_deref().map_or(0, |key| key.len() as u64)
                ^ parent_series
                    .as_deref()
                    .map_or(0, |parent| (parent.len() as u64).rotate_left(23))
        }
        LegacyEntry::Song(song) => Arc::as_ptr(song) as usize as u64,
    }
}

fn new_entries_checksum(entries: &[MusicWheelEntry]) -> u64 {
    entries
        .iter()
        .fold(entries.len() as u64, |checksum, entry| {
            checksum.wrapping_add(new_entry_checksum(entry))
        })
}

fn legacy_entries_checksum(entries: &[LegacyEntry]) -> u64 {
    entries
        .iter()
        .fold(entries.len() as u64, |checksum, entry| {
            checksum.wrapping_add(legacy_entry_checksum(entry))
        })
}

fn new_edge_checksum(entries: &[MusicWheelEntry]) -> u64 {
    entries.len() as u64
        ^ entries.first().map_or(0, new_entry_checksum)
        ^ entries.last().map_or(0, new_entry_checksum)
}

fn legacy_edge_checksum(entries: &[LegacyEntry]) -> u64 {
    entries.len() as u64
        ^ entries.first().map_or(0, legacy_entry_checksum)
        ^ entries.last().map_or(0, legacy_entry_checksum)
}

fn new_first_section(entries: &[MusicWheelEntry]) -> Arc<str> {
    entries
        .iter()
        .find_map(|entry| match entry {
            MusicWheelEntry::PackHeader { name, pack_key, .. } => {
                Some(Arc::clone(pack_key.as_ref().unwrap_or(name)))
            }
            MusicWheelEntry::Song(_) => None,
        })
        .expect("benchmark wheel should contain a header")
}

fn legacy_first_section(entries: &[LegacyEntry]) -> String {
    entries
        .iter()
        .find_map(|entry| match entry {
            LegacyEntry::Header { name, pack_key, .. } => {
                Some(pack_key.as_ref().unwrap_or(name).clone())
            }
            LegacyEntry::Song(_) => None,
        })
        .expect("benchmark wheel should contain a header")
}

fn legacy_fill_displayed_entries(
    entries: &[LegacyEntry],
    expanded_pack_name: &str,
) -> Vec<LegacyEntry> {
    let mut visible = Vec::with_capacity(entries.len());
    let mut current_pack_key: Option<&str> = None;
    for entry in entries {
        match entry {
            LegacyEntry::Header { name, pack_key, .. } => {
                current_pack_key = Some(pack_key.as_deref().unwrap_or(name));
                visible.push(entry.clone());
            }
            LegacyEntry::Song(_) if current_pack_key == Some(expanded_pack_name) => {
                visible.push(entry.clone());
            }
            LegacyEntry::Song(_) => {}
        }
    }
    visible
}

fn text_checksum(text: &str) -> u64 {
    text.bytes().fold(text.len() as u64, |checksum, byte| {
        checksum.rotate_left(5) ^ u64::from(byte)
    })
}

fn lookup_checksum<S: BuildHasher>(
    prefs: &HashMap<String, SyncPref, S>,
    new_packs: &HashSet<String, S>,
    edit_songs: &HashSet<usize, S>,
    pack_names: &[String],
    song_keys: &[usize],
) -> u64 {
    let mut checksum = 0u64;
    for index in (0..pack_names.len()).step_by(4) {
        checksum = checksum.wrapping_add(
            prefs
                .get(pack_names[index].as_str())
                .copied()
                .map_or(0, |pref| pref as u64 + 1),
        );
        checksum = checksum.wrapping_add(new_packs.contains(pack_names[index].as_str()) as u64);
        checksum = checksum.wrapping_add(edit_songs.contains(&song_keys[index]) as u64);
    }
    checksum
}

struct LegacyWheelMeta {
    preferred: FxHashMap<usize, [usize; STANDARD_DIFFICULTY_COUNT]>,
    edits: FxHashSet<usize>,
    pack_prefs: FxHashMap<String, SyncPref>,
}

fn legacy_preferred(song: &SongData) -> [usize; STANDARD_DIFFICULTY_COUNT] {
    let mut exact = [usize::MAX; STANDARD_DIFFICULTY_COUNT];
    let mut nearest = [usize::MAX; STANDARD_DIFFICULTY_COUNT];
    let mut distance = [usize::MAX; STANDARD_DIFFICULTY_COUNT];
    for (chart_index, chart) in song.charts.iter().enumerate() {
        if !chart.chart_type.eq_ignore_ascii_case("dance-single") {
            continue;
        }
        let Some(difficulty) = STANDARD_DIFFICULTY_NAMES
            .iter()
            .position(|name| chart.difficulty.eq_ignore_ascii_case(name))
        else {
            continue;
        };
        exact[difficulty] = exact[difficulty].min(chart_index);
        if !chart.has_note_data {
            continue;
        }
        for preferred in 0..STANDARD_DIFFICULTY_COUNT {
            let next = difficulty.abs_diff(preferred);
            if next < distance[preferred] {
                distance[preferred] = next;
                nearest[preferred] = chart_index;
            }
        }
    }
    std::array::from_fn(|index| {
        if exact[index] == usize::MAX {
            nearest[index]
        } else {
            exact[index]
        }
    })
}

fn legacy_wheel_meta(songs: &[Arc<SongData>]) -> LegacyWheelMeta {
    let mut preferred = FxHashMap::with_capacity_and_hasher(songs.len(), Default::default());
    let mut edits = FxHashSet::with_capacity_and_hasher(songs.len(), Default::default());
    let mut pack_prefs = FxHashMap::with_capacity_and_hasher(songs.len(), Default::default());
    for (index, song) in songs.iter().enumerate() {
        let ptr = Arc::as_ptr(song) as usize;
        preferred.insert(ptr, legacy_preferred(song));
        if song.charts.iter().any(|chart| {
            chart.chart_type.eq_ignore_ascii_case("dance-single")
                && chart.difficulty.eq_ignore_ascii_case("edit")
        }) {
            edits.insert(ptr);
        }
        let pack = song
            .simfile_path
            .parent()
            .and_then(Path::parent)
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .unwrap();
        pack_prefs.insert(
            pack.to_string(),
            if index % 2 == 0 {
                SyncPref::Itg
            } else {
                SyncPref::Null
            },
        );
    }
    LegacyWheelMeta {
        preferred,
        edits,
        pack_prefs,
    }
}

fn legacy_wheel_meta_checksum(songs: &[Arc<SongData>], meta: &LegacyWheelMeta) -> u64 {
    songs
        .iter()
        .enumerate()
        .fold(0u64, |checksum, (index, song)| {
            let ptr = Arc::as_ptr(song) as usize;
            let pack = song
                .simfile_path
                .parent()
                .and_then(Path::parent)
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .unwrap();
            let request_srpg = is_srpg_event_song(black_box(song));
            let push_srpg = is_srpg_event_song(black_box(song));
            checksum.wrapping_add(
                meta.preferred[&ptr][index % STANDARD_DIFFICULTY_COUNT] as u64
                    ^ u64::from(meta.edits.contains(&ptr)).rotate_left(8)
                    ^ u64::from(request_srpg && push_srpg).rotate_left(16)
                    ^ u64::from(is_itl_unlocks_pack(pack)).rotate_left(24)
                    ^ (meta.pack_prefs[pack] as u64).rotate_left(32),
            )
        })
}

fn main() {
    deadsync_theme_simply_love::i18n::init(deadsync_assets::language::load_for_tests("en"));
    let shared = benchmark_select_music_entries(PACK_COUNT, SONGS_PER_PACK);
    let legacy = legacy_entries(&shared);
    let pack_names = (0..PACK_COUNT)
        .map(|index| format!("Benchmark Pack {index:04}"))
        .collect::<Vec<_>>();
    let song_keys = (0..PACK_COUNT)
        .map(|index| index.wrapping_mul(4_099).wrapping_add(17))
        .collect::<Vec<_>>();

    let mut std_prefs = HashMap::with_capacity(PACK_COUNT);
    let mut std_new = HashSet::with_capacity(PACK_COUNT);
    let mut std_edits = HashSet::with_capacity(PACK_COUNT);
    let mut fx_prefs = FxHashMap::default();
    let mut fx_new = FxHashSet::default();
    let mut fx_edits = FxHashSet::default();
    fx_prefs.reserve(PACK_COUNT);
    fx_new.reserve(PACK_COUNT);
    fx_edits.reserve(PACK_COUNT);
    for (index, name) in pack_names.iter().enumerate() {
        let pref = [SyncPref::Default, SyncPref::Null, SyncPref::Itg][index % 3];
        std_prefs.insert(name.clone(), pref);
        fx_prefs.insert(name.clone(), pref);
        if index % 2 == 0 {
            std_new.insert(name.clone());
            fx_new.insert(name.clone());
        }
        if index % 3 == 0 {
            std_edits.insert(song_keys[index]);
            fx_edits.insert(song_keys[index]);
        }
    }

    let old_lookup = measure(LOOKUP_OPS, 500, || {
        lookup_checksum(&std_prefs, &std_new, &std_edits, &pack_names, &song_keys)
    });
    let new_lookup = measure(LOOKUP_OPS, 500, || {
        lookup_checksum(&fx_prefs, &fx_new, &fx_edits, &pack_names, &song_keys)
    });
    print_pair(
        "1. trusted internal wheel lookups",
        LOOKUP_OPS,
        &old_lookup,
        &new_lookup,
    );

    let old_sort = measure(SORT_OPS, 20, || {
        let sorted = black_box(&legacy).clone();
        let expanded = legacy_first_section(&sorted);
        let checksum = legacy_edge_checksum(&sorted) ^ (expanded.len() as u64).rotate_left(17);
        black_box(expanded);
        black_box(sorted);
        checksum
    });
    let new_sort = measure(SORT_OPS, 20, || {
        let sorted = Arc::clone(black_box(&shared));
        let expanded = new_first_section(&sorted);
        let checksum = new_edge_checksum(&sorted) ^ (expanded.len() as u64).rotate_left(17);
        black_box(expanded);
        black_box(sorted);
        checksum
    });
    print_pair(
        "2. switch cached sort and selection",
        SORT_OPS,
        &old_sort,
        &new_sort,
    );

    let expanded_pack = pack_names[PACK_COUNT / 2].as_str();
    let old_visible_once = legacy_fill_displayed_entries(&legacy, expanded_pack);
    let mut new_visible = Vec::with_capacity(shared.len());
    let cold_before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    benchmark_fill_displayed_entries(&mut new_visible, &shared, Some(expanded_pack));
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let cold_allocated = ALLOC.snapshot().delta(cold_before);
    assert_eq!(cold_allocated.allocs, 0);
    assert_eq!(cold_allocated.reallocs, 0);
    assert_eq!(cold_allocated.deallocs, 0);
    assert_eq!(cold_allocated.bytes, 0);
    assert_eq!(
        legacy_entries_checksum(&old_visible_once),
        new_entries_checksum(&new_visible),
        "visible-wheel fixtures must agree before measurement"
    );
    drop(old_visible_once);

    let old_refill = measure(REFILL_OPS, 50, || {
        let visible = legacy_fill_displayed_entries(&legacy, expanded_pack);
        let checksum = legacy_entries_checksum(&visible);
        black_box(visible);
        checksum
    });
    let new_refill = measure(REFILL_OPS, 50, || {
        benchmark_fill_displayed_entries(&mut new_visible, &shared, Some(expanded_pack));
        new_entries_checksum(&new_visible)
    });
    assert_eq!(new_refill.allocated.allocs, 0);
    assert_eq!(new_refill.allocated.reallocs, 0);
    assert_eq!(new_refill.allocated.deallocs, 0);
    assert_eq!(new_refill.allocated.bytes, 0);
    print_pair(
        "3. rebuild visible wheel",
        REFILL_OPS,
        &old_refill,
        &new_refill,
    );

    assert_eq!(
        benchmark_chart_info_text_old(),
        benchmark_chart_info_text_new(),
        "chart-info fixtures must agree before measurement"
    );
    let old_chart_info = measure(CHART_INFO_OPS, 500, || {
        text_checksum(benchmark_chart_info_text_old().as_ref())
    });
    let new_chart_info = measure(CHART_INFO_OPS, 500, || {
        text_checksum(benchmark_chart_info_text_new().as_ref())
    });
    print_pair(
        "4. cached chart-info text",
        CHART_INFO_OPS,
        &old_chart_info,
        &new_chart_info,
    );

    let wheel_meta = benchmark_wheel_song_meta(19);
    let legacy_meta = legacy_wheel_meta(wheel_meta.songs());
    assert_eq!(
        legacy_wheel_meta_checksum(wheel_meta.songs(), &legacy_meta),
        wheel_meta.prepared_checksum(),
    );
    let old_meta = measure(WHEEL_META_OPS, 500, || {
        legacy_wheel_meta_checksum(wheel_meta.songs(), &legacy_meta)
    });
    let new_meta = measure(WHEEL_META_OPS, 500, || wheel_meta.prepared_checksum());
    print_pair(
        "5. prepared visible-song metadata",
        WHEEL_META_OPS,
        &old_meta,
        &new_meta,
    );

    println!(
        "\nrepresentation sizes: legacy entry {} B, shared entry {} B, Vec handle {} B, Arc slice handle {} B",
        size_of::<LegacyEntry>(),
        size_of::<MusicWheelEntry>(),
        size_of::<Vec<LegacyEntry>>(),
        size_of::<Arc<[MusicWheelEntry]>>(),
    );
}

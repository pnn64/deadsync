use deadsync_simfile::app_runtime::{
    benchmark_parse_options_hoisted, benchmark_parse_options_per_song,
};
use deadsync_simfile::cache::{
    benchmark_cache_header_loads_baseline, benchmark_cache_header_loads_current,
    benchmark_cache_probes_current, benchmark_cache_probes_legacy,
    benchmark_chart_payload_encoding_baseline, benchmark_chart_payload_encoding_current,
    benchmark_chart_requests_current, benchmark_chart_requests_legacy,
    benchmark_runtime_debug_logs, benchmark_song_cache_paths_current,
    benchmark_song_cache_paths_legacy, benchmark_timing_handoff_baseline,
    benchmark_timing_handoff_current, write_song_cache_file,
};
use deadsync_simfile::media::{
    benchmark_genre_whitelist_current, benchmark_genre_whitelist_legacy,
};
use deadsync_simfile::scan::{
    benchmark_child_dirs_current, benchmark_child_dirs_legacy, benchmark_legacy_song_workers,
    benchmark_nested_membership_hash, benchmark_nested_membership_linear,
    benchmark_nested_membership_sorted_names, benchmark_nested_membership_sorted_paths,
    benchmark_nested_scan_current, benchmark_nested_scan_hash, benchmark_nested_scan_legacy,
    benchmark_nested_scan_linear, benchmark_pack_groups_current, benchmark_pack_groups_legacy,
    benchmark_pooled_song_workers, benchmark_scan_map_fixture, benchmark_scan_maps_current,
    benchmark_scan_maps_legacy, benchmark_song_slots_current, benchmark_song_slots_legacy,
};
use deadsync_simfile::song::{
    ParseSongOptions, benchmark_note_parse_current, benchmark_note_parse_legacy,
    benchmark_song_bytes_current, benchmark_song_bytes_previous, benchmark_song_parse_current,
    benchmark_song_parse_previous, parse_song_file,
};
use deadsync_simfile::timing::{benchmark_timing_tags_baseline, benchmark_timing_tags_current};
use std::alloc::{GlobalAlloc, Layout, System};
use std::fs;
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const JOB_COUNT: usize = 512;
const SONG_COUNT: usize = 2_805;
const DISCOVERY_DIRS: usize = 2_048;
const DISCOVERY_FILES: usize = 64;
const NESTED_SCAN_SONGS: usize = 512;
const MEMBERSHIP_ROUNDS: usize = 32;
const SCAN_MAP_PACKS: usize = 1_024;
const SCAN_MAP_GROUPS: usize = 512;
const SCAN_MAP_SONGS: usize = 512;
const SCAN_MAP_ROUNDS: usize = 64;
const CHART_REQUEST_ROUNDS: usize = 65_536;
const MEDIA_MAP_ROUNDS: usize = 2_048;
const CACHE_PATH_OPS: usize = 2_048;
const CACHE_PROBE_OPS: usize = 2_048;
const CACHE_PAYLOAD_ROUNDS: usize = 256;
const CACHE_HEADER_ROUNDS: usize = 512;
const SONG_PARSE_ROUNDS: usize = 64;
const NOTE_PARSE_ROUNDS: usize = 2_048;
const TIMING_TAG_ROUNDS: usize = 4_096;
const TIMING_HANDOFF_ROUNDS: usize = 2_048;
const SAMPLES: usize = 9;

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

// SAFETY: every allocator request is delegated unchanged to `System`; the
// atomic counters support observations from all benchmark worker threads.
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
        // SAFETY: the pointer-layout pair came from the delegated allocator.
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
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            frees: self.frees - before.frees,
            alloc_bytes: self.alloc_bytes - before.alloc_bytes,
            realloc_bytes: self.realloc_bytes - before.realloc_bytes,
            free_bytes: self.free_bytes - before.free_bytes,
        }
    }

    fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

struct Sample {
    ns: f64,
    cycles: Option<u64>,
}

struct BenchResult {
    median_ns: f64,
    max_ns: f64,
    cycles_per_item: Option<f64>,
    alloc: AllocSnapshot,
    checksum: u64,
}

fn measure(item_count: usize, mut op: impl FnMut() -> u64) -> BenchResult {
    black_box(op());
    let mut checksum = 0u64;
    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let (sample, sample_checksum) = timed_sample(&mut op);
        checksum ^= sample_checksum;
        samples.push(sample);
    }
    let (alloc, alloc_checksum) = allocation_sample(&mut op);
    checksum ^= alloc_checksum;
    bench_result(item_count, samples, alloc, checksum)
}

fn measure_pair(
    item_count: usize,
    mut old: impl FnMut() -> u64,
    mut new: impl FnMut() -> u64,
) -> (BenchResult, BenchResult) {
    black_box(old());
    black_box(new());
    let mut old_checksum = 0u64;
    let mut new_checksum = 0u64;
    let mut old_samples = Vec::with_capacity(SAMPLES);
    let mut new_samples = Vec::with_capacity(SAMPLES);
    for sample_index in 0..SAMPLES {
        if sample_index % 2 == 0 {
            let (sample, checksum) = timed_sample(&mut old);
            old_samples.push(sample);
            old_checksum ^= checksum;
            let (sample, checksum) = timed_sample(&mut new);
            new_samples.push(sample);
            new_checksum ^= checksum;
        } else {
            let (sample, checksum) = timed_sample(&mut new);
            new_samples.push(sample);
            new_checksum ^= checksum;
            let (sample, checksum) = timed_sample(&mut old);
            old_samples.push(sample);
            old_checksum ^= checksum;
        }
    }
    let (old_alloc, checksum) = allocation_sample(&mut old);
    old_checksum ^= checksum;
    let (new_alloc, checksum) = allocation_sample(&mut new);
    new_checksum ^= checksum;
    (
        bench_result(item_count, old_samples, old_alloc, old_checksum),
        bench_result(item_count, new_samples, new_alloc, new_checksum),
    )
}

fn timed_sample(op: &mut impl FnMut() -> u64) -> (Sample, u64) {
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let checksum = black_box(op());
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();
    (
        Sample {
            ns: elapsed.as_secs_f64() * 1e9,
            cycles: cycle_start
                .zip(cycle_end)
                .map(|(start, end)| end.wrapping_sub(start)),
        },
        checksum,
    )
}

fn allocation_sample(op: &mut impl FnMut() -> u64) -> (AllocSnapshot, u64) {
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let checksum = black_box(op());
    ALLOC.enabled.store(false, Ordering::Relaxed);
    (ALLOC.snapshot().delta(before), checksum)
}

fn bench_result(
    item_count: usize,
    mut samples: Vec<Sample>,
    alloc: AllocSnapshot,
    checksum: u64,
) -> BenchResult {
    let max_ns = samples.iter().map(|sample| sample.ns).fold(0.0, f64::max);
    samples.sort_by(|left, right| left.ns.total_cmp(&right.ns));
    let median = &samples[samples.len() / 2];
    BenchResult {
        median_ns: median.ns,
        max_ns,
        cycles_per_item: median
            .cycles
            .map(|cycles| cycles as f64 / item_count as f64),
        alloc,
        checksum,
    }
}

fn main() {
    if std::env::args().any(|arg| arg == "scan-maps") {
        bench_scan_maps();
        return;
    }
    if std::env::args().any(|arg| arg == "chart-requests") {
        bench_chart_requests();
        return;
    }
    if std::env::args().any(|arg| arg == "media-map") {
        bench_media_map();
        return;
    }
    if std::env::args().any(|arg| arg == "rssp-boundary") {
        bench_rssp_boundary();
        return;
    }
    let discovery_root = discovery_fixture();
    let old_checksum = benchmark_child_dirs_legacy(&discovery_root).unwrap();
    let new_checksum = benchmark_child_dirs_current(&discovery_root).unwrap();
    assert_eq!(old_checksum, new_checksum, "directory discovery diverged");
    let discovery_entries = DISCOVERY_DIRS + DISCOVERY_FILES + 1;
    let old = measure(discovery_entries, || {
        benchmark_child_dirs_legacy(&discovery_root).unwrap()
    });
    let new = measure(discovery_entries, || {
        benchmark_child_dirs_current(&discovery_root).unwrap()
    });
    black_box(old.checksum ^ new.checksum);
    println!("song directory discovery ({discovery_entries} entries)");
    print_result("old", discovery_entries, &old);
    print_result("new", discovery_entries, &new);
    print_change(&old, &new);

    let nested_root = nested_scan_fixture();
    let old_checksum = benchmark_nested_scan_legacy(&nested_root).unwrap();
    let new_checksum = benchmark_nested_scan_current(&nested_root).unwrap();
    let hash_checksum = benchmark_nested_scan_hash(&nested_root).unwrap();
    let linear_checksum = benchmark_nested_scan_linear(&nested_root).unwrap();
    assert_eq!(old_checksum, new_checksum, "nested-pack detection diverged");
    assert_eq!(old_checksum, hash_checksum, "hash membership diverged");
    assert_eq!(old_checksum, linear_checksum, "linear membership diverged");
    let old = measure(NESTED_SCAN_SONGS, || {
        benchmark_nested_scan_legacy(&nested_root).unwrap()
    });
    let hash = measure(NESTED_SCAN_SONGS, || {
        benchmark_nested_scan_hash(&nested_root).unwrap()
    });
    let new = measure(NESTED_SCAN_SONGS, || {
        benchmark_nested_scan_current(&nested_root).unwrap()
    });
    let linear = measure(NESTED_SCAN_SONGS, || {
        benchmark_nested_scan_linear(&nested_root).unwrap()
    });
    black_box(old.checksum ^ hash.checksum ^ new.checksum ^ linear.checksum);
    println!("nested-pack validation ({NESTED_SCAN_SONGS} direct songs)");
    print_result("old", NESTED_SCAN_SONGS, &old);
    print_result("hash", NESTED_SCAN_SONGS, &hash);
    print_result("new", NESTED_SCAN_SONGS, &new);
    print_result("linear", NESTED_SCAN_SONGS, &linear);
    println!("  legacy -> new");
    print_change(&old, &new);
    println!("  hash -> new");
    print_change(&hash, &new);

    let mut direct_dirs = (0..NESTED_SCAN_SONGS)
        .map(|index| nested_root.join(format!("Song {index:05}")))
        .collect::<Vec<_>>();
    let mut shuffle = 0x9e37_79b9u32;
    for index in (1..direct_dirs.len()).rev() {
        shuffle = shuffle.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        direct_dirs.swap(index, shuffle as usize % (index + 1));
    }
    let mut queries = direct_dirs.clone();
    queries.sort_unstable();
    let membership_items = NESTED_SCAN_SONGS * MEMBERSHIP_ROUNDS;
    let hash_checksum = benchmark_nested_membership_hash(&direct_dirs, &queries, MEMBERSHIP_ROUNDS);
    let sorted_path_checksum =
        benchmark_nested_membership_sorted_paths(&direct_dirs, &queries, MEMBERSHIP_ROUNDS);
    let sorted_name_checksum =
        benchmark_nested_membership_sorted_names(&direct_dirs, &queries, MEMBERSHIP_ROUNDS);
    let linear_checksum =
        benchmark_nested_membership_linear(&direct_dirs, &queries, MEMBERSHIP_ROUNDS);
    assert_eq!(hash_checksum, sorted_path_checksum);
    assert_eq!(hash_checksum, sorted_name_checksum);
    assert_eq!(hash_checksum, linear_checksum);
    let hash = measure(membership_items, || {
        benchmark_nested_membership_hash(&direct_dirs, &queries, MEMBERSHIP_ROUNDS)
    });
    let sorted_paths = measure(membership_items, || {
        benchmark_nested_membership_sorted_paths(&direct_dirs, &queries, MEMBERSHIP_ROUNDS)
    });
    let sorted_names = measure(membership_items, || {
        benchmark_nested_membership_sorted_names(&direct_dirs, &queries, MEMBERSHIP_ROUNDS)
    });
    let linear = measure(membership_items, || {
        benchmark_nested_membership_linear(&direct_dirs, &queries, MEMBERSHIP_ROUNDS)
    });
    black_box(hash.checksum ^ sorted_paths.checksum ^ sorted_names.checksum ^ linear.checksum);
    println!("nested-song membership ({membership_items} lookups including setup)");
    print_result("hash", membership_items, &hash);
    print_result("paths", membership_items, &sorted_paths);
    print_result("names", membership_items, &sorted_names);
    print_result("linear", membership_items, &linear);

    bench_scan_maps();
    bench_chart_requests();
    bench_media_map();
    bench_rssp_boundary();

    let simfile_path = nested_root.join("Song 00000").join("chart.sm");
    let cache_dir = nested_root.join("cache");
    let old_checksum =
        benchmark_song_cache_paths_legacy(&cache_dir, &simfile_path, CACHE_PATH_OPS).unwrap();
    let new_checksum =
        benchmark_song_cache_paths_current(&cache_dir, &simfile_path, CACHE_PATH_OPS).unwrap();
    assert_eq!(old_checksum, new_checksum, "song cache paths diverged");
    let old = measure(CACHE_PATH_OPS, || {
        benchmark_song_cache_paths_legacy(&cache_dir, &simfile_path, CACHE_PATH_OPS).unwrap()
    });
    let new = measure(CACHE_PATH_OPS, || {
        benchmark_song_cache_paths_current(&cache_dir, &simfile_path, CACHE_PATH_OPS).unwrap()
    });
    black_box(old.checksum ^ new.checksum);
    println!("canonical song cache path ({CACHE_PATH_OPS} paths)");
    print_result("old", CACHE_PATH_OPS, &old);
    print_result("new", CACHE_PATH_OPS, &new);
    print_change(&old, &new);

    let old_checksum = benchmark_cache_probes_legacy(&simfile_path, CACHE_PROBE_OPS).unwrap();
    let new_checksum = benchmark_cache_probes_current(&simfile_path, CACHE_PROBE_OPS).unwrap();
    assert_eq!(old_checksum, new_checksum, "cache probes diverged");
    let old = measure(CACHE_PROBE_OPS, || {
        benchmark_cache_probes_legacy(&simfile_path, CACHE_PROBE_OPS).unwrap()
    });
    let new = measure(CACHE_PROBE_OPS, || {
        benchmark_cache_probes_current(&simfile_path, CACHE_PROBE_OPS).unwrap()
    });
    black_box(old.checksum ^ new.checksum);
    println!("cache-file probe ({CACHE_PROBE_OPS} hits)");
    print_result("old", CACHE_PROBE_OPS, &old);
    print_result("new", CACHE_PROBE_OPS, &new);
    print_change(&old, &new);

    let available = std::thread::available_parallelism()
        .map(std::num::NonZero::get)
        .unwrap_or(1);
    let workers = available.min(JOB_COUNT);

    for (label, work_iterations) in [("cache-like", 64), ("parse-like", 16_384)] {
        let old_checksum = benchmark_legacy_song_workers(JOB_COUNT, workers, work_iterations);
        let new_checksum = benchmark_pooled_song_workers(JOB_COUNT, workers, work_iterations);
        assert_eq!(old_checksum, new_checksum, "worker results diverged");

        let old = measure(JOB_COUNT, || {
            benchmark_legacy_song_workers(JOB_COUNT, workers, work_iterations)
        });
        let new = measure(JOB_COUNT, || {
            benchmark_pooled_song_workers(JOB_COUNT, workers, work_iterations)
        });
        black_box(old.checksum ^ new.checksum);
        println!("song worker scheduling: {label} ({JOB_COUNT} jobs, {workers} workers)");
        print_result("old", JOB_COUNT, &old);
        print_result("new", JOB_COUNT, &new);
        print_change(&old, &new);
    }

    assert_eq!(
        benchmark_parse_options_per_song(SONG_COUNT),
        benchmark_parse_options_hoisted(SONG_COUNT),
        "parse-option reuse changed its observed configuration"
    );
    let old = measure(SONG_COUNT, || {
        benchmark_parse_options_per_song(SONG_COUNT) as u64
    });
    let new = measure(SONG_COUNT, || {
        benchmark_parse_options_hoisted(SONG_COUNT) as u64
    });
    black_box(old.checksum ^ new.checksum);
    println!("per-scan parse option setup ({SONG_COUNT} songs)");
    print_result("old", SONG_COUNT, &old);
    print_result("new", SONG_COUNT, &new);
    print_change(&old, &new);

    assert_eq!(benchmark_runtime_debug_logs(SONG_COUNT, true), SONG_COUNT);
    assert_eq!(benchmark_runtime_debug_logs(SONG_COUNT, false), 0);
    let old = measure(SONG_COUNT, || {
        benchmark_runtime_debug_logs(SONG_COUNT, true) as u64
    });
    let new = measure(SONG_COUNT, || {
        benchmark_runtime_debug_logs(SONG_COUNT, false) as u64
    });
    black_box(old.checksum ^ new.checksum);
    println!("disabled per-song debug logging ({SONG_COUNT} songs)");
    print_result("old", SONG_COUNT, &old);
    print_result("new", SONG_COUNT, &new);
    print_change(&old, &new);

    fs::remove_dir_all(discovery_root).unwrap();
    fs::remove_dir_all(nested_root).unwrap();
}

fn bench_scan_maps() {
    let fixture = benchmark_scan_map_fixture(SCAN_MAP_PACKS, SCAN_MAP_GROUPS, SCAN_MAP_SONGS);
    assert_eq!(
        benchmark_scan_maps_legacy(&fixture),
        benchmark_scan_maps_current(&fixture),
        "scan map behavior diverged"
    );
    let pack_items = SCAN_MAP_PACKS * SCAN_MAP_ROUNDS;
    assert_eq!(
        benchmark_pack_groups_legacy(&fixture, SCAN_MAP_ROUNDS),
        benchmark_pack_groups_current(&fixture, SCAN_MAP_ROUNDS),
        "pack grouping diverged"
    );
    black_box(measure(pack_items, || {
        benchmark_pack_groups_legacy(&fixture, SCAN_MAP_ROUNDS)
    }));
    black_box(measure(pack_items, || {
        benchmark_pack_groups_current(&fixture, SCAN_MAP_ROUNDS)
    }));
    let old = measure(pack_items, || {
        benchmark_pack_groups_legacy(&fixture, SCAN_MAP_ROUNDS)
    });
    let new = measure(pack_items, || {
        benchmark_pack_groups_current(&fixture, SCAN_MAP_ROUNDS)
    });
    black_box(old.checksum ^ new.checksum);
    println!("pack group indexing ({pack_items} entries including setup)");
    print_result("hash", pack_items, &old);
    print_result("compact", pack_items, &new);
    print_change(&old, &new);

    let song_items = SCAN_MAP_SONGS * SCAN_MAP_ROUNDS;
    assert_eq!(
        benchmark_song_slots_legacy(&fixture, SCAN_MAP_ROUNDS),
        benchmark_song_slots_current(&fixture, SCAN_MAP_ROUNDS),
        "song slot mapping diverged"
    );
    black_box(measure(song_items, || {
        benchmark_song_slots_legacy(&fixture, SCAN_MAP_ROUNDS)
    }));
    black_box(measure(song_items, || {
        benchmark_song_slots_current(&fixture, SCAN_MAP_ROUNDS)
    }));
    let old = measure(song_items, || {
        benchmark_song_slots_legacy(&fixture, SCAN_MAP_ROUNDS)
    });
    let new = measure(song_items, || {
        benchmark_song_slots_current(&fixture, SCAN_MAP_ROUNDS)
    });
    black_box(old.checksum ^ new.checksum);
    println!("merged-song slot indexing ({song_items} source songs including setup)");
    print_result("hash", song_items, &old);
    print_result("flat", song_items, &new);
    print_change(&old, &new);
}

fn bench_chart_requests() {
    for requests in [&[0, 1][..], &[7, 7], &[0, 1, 2, 3]] {
        let items = requests.len() * CHART_REQUEST_ROUNDS;
        assert_eq!(
            benchmark_chart_requests_legacy(requests, CHART_REQUEST_ROUNDS),
            benchmark_chart_requests_current(requests, CHART_REQUEST_ROUNDS),
            "chart request mapping diverged"
        );
        let old = measure(items, || {
            benchmark_chart_requests_legacy(requests, CHART_REQUEST_ROUNDS)
        });
        let new = measure(items, || {
            benchmark_chart_requests_current(requests, CHART_REQUEST_ROUNDS)
        });
        black_box(old.checksum ^ new.checksum);
        println!("chart request indexing ({items} entries, request {requests:?})");
        print_result("hash", items, &old);
        print_result("linear", items, &new);
        print_change(&old, &new);
    }
}

fn bench_media_map() {
    let mut text = String::new();
    for section in 0..32 {
        text.push_str(&format!("[Unused {section}]\n"));
        for entry in 0..16 {
            text.push_str(&format!("Unused-{section}-{entry}=Value-{entry}\n"));
        }
    }
    text.push_str("[GenreToSection]\nTechno=Techno Movies\nRock=Rock Movies\n");
    text.push_str("[Techno Movies]\n");
    for entry in 0..64 {
        text.push_str(&format!("Movie-{entry:03}=1\n"));
    }
    assert_eq!(
        benchmark_genre_whitelist_legacy(&text, "techno", 1),
        benchmark_genre_whitelist_current(&text, "techno", 1),
        "genre whitelist parsing diverged"
    );
    let old = measure(MEDIA_MAP_ROUNDS, || {
        benchmark_genre_whitelist_legacy(&text, "techno", MEDIA_MAP_ROUNDS)
    });
    let new = measure(MEDIA_MAP_ROUNDS, || {
        benchmark_genre_whitelist_current(&text, "techno", MEDIA_MAP_ROUNDS)
    });
    black_box(old.checksum ^ new.checksum);
    println!("genre movie INI lookup ({MEDIA_MAP_ROUNDS} parses)");
    print_result("hash", MEDIA_MAP_ROUNDS, &old);
    print_result("borrowed", MEDIA_MAP_ROUNDS, &new);
    print_change(&old, &new);
}

fn bench_rssp_boundary() {
    let root = std::env::temp_dir().join(format!(
        "deadsync-rssp-boundary-bench-{}",
        std::process::id()
    ));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    fs::create_dir_all(&root).unwrap();
    let global_timing = root.join("global-timing.ssc");
    let own_timing = root.join("own-timing.ssc");
    fs::write(&global_timing, analysis_fixture(false)).unwrap();
    fs::write(&own_timing, analysis_fixture(true)).unwrap();

    let parse_options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());
    let song_data = parse_song_file(&global_timing, &parse_options, |_| 0.0).unwrap();
    assert_eq!(
        benchmark_chart_payload_encoding_baseline(&song_data, 1),
        benchmark_chart_payload_encoding_current(&song_data, 1),
        "chart payload cache bytes diverged"
    );
    let (separate_payloads, reused_payloads) = measure_pair(
        CACHE_PAYLOAD_ROUNDS,
        || benchmark_chart_payload_encoding_baseline(&song_data, CACHE_PAYLOAD_ROUNDS),
        || benchmark_chart_payload_encoding_current(&song_data, CACHE_PAYLOAD_ROUNDS),
    );
    black_box(separate_payloads.checksum ^ reused_payloads.checksum);
    println!("cache chart-payload encoding ({CACHE_PAYLOAD_ROUNDS} songs)");
    print_result("separate", CACHE_PAYLOAD_ROUNDS, &separate_payloads);
    print_result("worker-reuse", CACHE_PAYLOAD_ROUNDS, &reused_payloads);
    print_change(&separate_payloads, &reused_payloads);

    let cache_path = root.join("song-cache.bin");
    write_song_cache_file(&cache_path, &song_data, 0.0).unwrap();
    assert_eq!(
        benchmark_cache_header_loads_baseline(&cache_path, 1),
        benchmark_cache_header_loads_current(&cache_path, 1),
        "cache header decoding diverged"
    );
    let (fresh_headers, reused_headers) = measure_pair(
        CACHE_HEADER_ROUNDS,
        || benchmark_cache_header_loads_baseline(&cache_path, CACHE_HEADER_ROUNDS),
        || benchmark_cache_header_loads_current(&cache_path, CACHE_HEADER_ROUNDS),
    );
    black_box(fresh_headers.checksum ^ reused_headers.checksum);
    println!("warm cache-header load ({CACHE_HEADER_ROUNDS} songs)");
    print_result("allocate+stat", CACHE_HEADER_ROUNDS, &fresh_headers);
    print_result("reuse", CACHE_HEADER_ROUNDS, &reused_headers);
    print_change(&fresh_headers, &reused_headers);

    let (time_signatures, tickcounts, combos) = timing_tag_fixture();
    let old_timing_checksum =
        benchmark_timing_tags_baseline(&time_signatures, &tickcounts, &combos, 1);
    let new_timing_checksum =
        benchmark_timing_tags_current(&time_signatures, &tickcounts, &combos, 1);
    assert_eq!(
        old_timing_checksum, new_timing_checksum,
        "timing-tag output diverged"
    );
    let (old_timing, new_timing) = measure_pair(
        TIMING_TAG_ROUNDS,
        || {
            benchmark_timing_tags_baseline(
                &time_signatures,
                &tickcounts,
                &combos,
                TIMING_TAG_ROUNDS,
            )
        },
        || benchmark_timing_tags_current(&time_signatures, &tickcounts, &combos, TIMING_TAG_ROUNDS),
    );
    black_box(old_timing.checksum ^ new_timing.checksum);
    println!("chart timing-tag payload ({TIMING_TAG_ROUNDS} conversions)");
    print_result("two-stage", TIMING_TAG_ROUNDS, &old_timing);
    print_result("direct", TIMING_TAG_ROUNDS, &new_timing);
    print_change(&old_timing, &new_timing);

    assert_eq!(
        benchmark_timing_handoff_baseline(1),
        benchmark_timing_handoff_current(1),
        "timing handoff output diverged"
    );
    let (borrowed_timing, owned_timing) = measure_pair(
        TIMING_HANDOFF_ROUNDS,
        || benchmark_timing_handoff_baseline(TIMING_HANDOFF_ROUNDS),
        || benchmark_timing_handoff_current(TIMING_HANDOFF_ROUNDS),
    );
    black_box(borrowed_timing.checksum ^ owned_timing.checksum);
    println!("RSSP timing ownership handoff ({TIMING_HANDOFF_ROUNDS} conversions)");
    print_result("clone", TIMING_HANDOFF_ROUNDS, &borrowed_timing);
    print_result("move", TIMING_HANDOFF_ROUNDS, &owned_timing);
    print_change(&borrowed_timing, &owned_timing);

    let note_data = note_fixture();
    let old_note_checksum = benchmark_note_parse_legacy(&note_data, 4, 1);
    let new_note_checksum = benchmark_note_parse_current(&note_data, 4, 1);
    assert_eq!(old_note_checksum, new_note_checksum, "note output diverged");
    let (old_notes, new_notes) = measure_pair(
        NOTE_PARSE_ROUNDS,
        || benchmark_note_parse_legacy(&note_data, 4, NOTE_PARSE_ROUNDS),
        || benchmark_note_parse_current(&note_data, 4, NOTE_PARSE_ROUNDS),
    );
    black_box(old_notes.checksum ^ new_notes.checksum);
    println!("compact note output ({NOTE_PARSE_ROUNDS} parses)");
    print_result("copy", NOTE_PARSE_ROUNDS, &old_notes);
    print_result("direct", NOTE_PARSE_ROUNDS, &new_notes);
    print_change(&old_notes, &new_notes);

    bench_song_boundary("global timing", &global_timing);
    bench_song_boundary("chart timing", &own_timing);

    fs::remove_dir_all(root).unwrap();
}

fn bench_song_boundary(label: &str, simfile: &std::path::Path) {
    assert_eq!(
        benchmark_song_bytes_previous(simfile),
        benchmark_song_bytes_current(simfile),
        "serialized RSSP boundary payload diverged"
    );
    let previous_checksum = benchmark_song_parse_previous(simfile, 1);
    let new_checksum = benchmark_song_parse_current(simfile, 1);
    assert_eq!(
        previous_checksum, new_checksum,
        "RSSP boundary parse diverged"
    );
    let (previous, current) = measure_pair(
        SONG_PARSE_ROUNDS,
        || benchmark_song_parse_previous(simfile, SONG_PARSE_ROUNDS),
        || benchmark_song_parse_current(simfile, SONG_PARSE_ROUNDS),
    );
    black_box(previous.checksum ^ current.checksum);
    println!("RSSP song-parse boundary: {label} ({SONG_PARSE_ROUNDS} cache misses)");
    print_result("baseline", SONG_PARSE_ROUNDS, &previous);
    print_result("direct", SONG_PARSE_ROUNDS, &current);
    print_change(&previous, &current);
}

fn analysis_fixture(own_timing: bool) -> Vec<u8> {
    let mut fixture = String::with_capacity(64 * 1024);
    fixture.push_str(
        "#VERSION:0.83;\n#TITLE:Boundary Benchmark;\n#ARTIST:DeadSync;\n\
         #BPMS:0.000=120.000,128.000=180.000;\n\
         #TIMESIGNATURES:32=3=4,0=4=4,16=7=8,16=5=8;\n\
         #TICKCOUNTS:32=8,0=4,16=6,16=12;\n\
         #COMBOS:32=3=2,0=1,16=2=4,16=5=6;\n",
    );
    for description in ["Zulu", "alpha", "İstanbul", "Beta", "gamma", "delta"] {
        fixture.push_str("#NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DESCRIPTION:");
        fixture.push_str(description);
        fixture.push_str(";\n#DIFFICULTY:Challenge;\n#METER:15;\n");
        if own_timing {
            fixture.push_str(
                "#BPMS:0=120,128=180;\n#STOPS:16=0.050,96=0.100;\n\
                 #DELAYS:32=0.025,112=0.075;\n#WARPS:48=2,120=4;\n\
                 #SPEEDS:0=1=0=0,64=2=4=0;\n#SCROLLS:0=1,80=-1;\n\
                 #FAKES:144=4,160=8;\n#TIMESIGNATURES:32=3=4,0=4=4,16=7=8,16=5=8;\n\
                 #TICKCOUNTS:32=8,0=4,16=6,16=12;\n\
                 #COMBOS:32=3=2,0=1,16=2=4,16=5=6;\n",
            );
        }
        fixture.push_str("#NOTES:\n");
        for measure in 0..64 {
            for row in 0..4 {
                fixture.push_str(match (measure + row) & 3 {
                    0 => "1000\n",
                    1 => "0100\n",
                    2 => "0010\n",
                    _ => "0001\n",
                });
            }
            if measure != 63 {
                fixture.push_str(",\n");
            }
        }
        fixture.push_str(";\n");
    }
    fixture.into_bytes()
}

fn timing_tag_fixture() -> (String, String, String) {
    let mut time_signatures = String::with_capacity(768);
    let mut tickcounts = String::with_capacity(512);
    let mut combos = String::with_capacity(768);
    for index in (0..64).rev() {
        if index != 63 {
            time_signatures.push(',');
            tickcounts.push(',');
            combos.push(',');
        }
        let beat = index * 4;
        time_signatures.push_str(&format!("{beat}={}={}", 3 + index % 5, 4 << (index % 2)));
        tickcounts.push_str(&format!("{beat}={}", index % 49));
        combos.push_str(&format!("{beat}={}={}", 1 + index % 8, 1 + index % 5));
    }
    time_signatures.push_str(",16=11=16,bad");
    tickcounts.push_str(",16=48,bad");
    combos.push_str(",16=9=7,bad");
    (time_signatures, tickcounts, combos)
}

fn note_fixture() -> Vec<u8> {
    let mut fixture = Vec::with_capacity(512 * 22);
    for measure in 0..512 {
        for row in 0..4 {
            fixture.extend_from_slice(match (measure + row) & 3 {
                0 => b"1000\n",
                1 => b"0100\n",
                2 => b"0010\n",
                _ => b"0001\n",
            });
        }
        fixture.extend_from_slice(b",\n");
    }
    fixture
}

fn discovery_fixture() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "deadsync-song-discovery-bench-{}",
        std::process::id()
    ));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    fs::create_dir_all(&root).unwrap();
    for index in 0..DISCOVERY_DIRS {
        fs::create_dir(root.join(format!("Song {index:05}"))).unwrap();
    }
    fs::create_dir(root.join("._ignored")).unwrap();
    for index in 0..DISCOVERY_FILES {
        fs::write(root.join(format!("asset-{index:03}.png")), []).unwrap();
    }
    root
}

fn nested_scan_fixture() -> std::path::PathBuf {
    let root =
        std::env::temp_dir().join(format!("deadsync-nested-scan-bench-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    fs::create_dir_all(&root).unwrap();
    for index in 0..NESTED_SCAN_SONGS {
        let song_dir = root.join(format!("Song {index:05}"));
        fs::create_dir(&song_dir).unwrap();
        fs::write(song_dir.join("chart.sm"), b"#TITLE:Benchmark;").unwrap();
    }
    let nested = root.join("Song 00000").join("Nested");
    fs::create_dir(&nested).unwrap();
    fs::write(nested.join("nested.ssc"), b"#TITLE:Nested;").unwrap();
    root
}

fn print_result(label: &str, item_count: usize, result: &BenchResult) {
    println!(
        "  {label:<3} {:>9.3} ms median  {:>9.3} ms max  {:>10.1} cycles/item  \
         {:>8.3} Mitem/s  {:>7.3} alloc/item  {:>7.3} realloc/item  \
         {:>10.1} churn B/item",
        result.median_ns / 1e6,
        result.max_ns / 1e6,
        result.cycles_per_item.unwrap_or(f64::NAN),
        item_count as f64 * 1_000.0 / result.median_ns,
        result.alloc.allocs as f64 / item_count as f64,
        result.alloc.reallocs as f64 / item_count as f64,
        result.alloc.churn_bytes() as f64 / item_count as f64,
    );
}

fn print_change(old: &BenchResult, new: &BenchResult) {
    println!(
        "  change: {:+.2}% median  {:+.2}% max  {:+.2}% cycles  {:+.2}% throughput  \
         {:+.2}% churn",
        percent_change(old.median_ns, new.median_ns),
        percent_change(old.max_ns, new.max_ns),
        percent_change(
            old.cycles_per_item.unwrap_or(f64::NAN),
            new.cycles_per_item.unwrap_or(f64::NAN),
        ),
        percent_change(1.0 / old.median_ns, 1.0 / new.median_ns),
        percent_change(
            old.alloc.churn_bytes() as f64,
            new.alloc.churn_bytes() as f64,
        ),
    );
}

fn percent_change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
    (new / old - 1.0) * 100.0
}

#[cfg(target_arch = "x86_64")]
fn cycle_counter() -> Option<u64> {
    // SAFETY: the fence and timestamp instructions require no memory operands.
    unsafe {
        std::arch::x86_64::_mm_lfence();
        Some(std::arch::x86_64::_rdtsc())
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

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn cycle_counter() -> Option<u64> {
    None
}

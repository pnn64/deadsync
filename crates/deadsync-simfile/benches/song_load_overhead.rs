use deadsync_simfile::app_runtime::{
    benchmark_parse_options_hoisted, benchmark_parse_options_per_song,
};
use deadsync_simfile::cache::{
    benchmark_cache_probes_current, benchmark_cache_probes_legacy,
    benchmark_chart_requests_current, benchmark_chart_requests_legacy,
    benchmark_runtime_debug_logs, benchmark_song_cache_paths_current,
    benchmark_song_cache_paths_legacy,
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
use deadsync_simfile::song::{benchmark_song_parse_current, benchmark_song_parse_legacy};
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
const SONG_PARSE_ROUNDS: usize = 16;
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
        let cycle_start = cycle_counter();
        let started = Instant::now();
        checksum ^= black_box(op());
        let elapsed = started.elapsed();
        let cycle_end = cycle_counter();
        samples.push(Sample {
            ns: elapsed.as_secs_f64() * 1e9,
            cycles: cycle_start
                .zip(cycle_end)
                .map(|(start, end)| end.wrapping_sub(start)),
        });
    }
    let max_ns = samples.iter().map(|sample| sample.ns).fold(0.0, f64::max);
    samples.sort_by(|left, right| left.ns.total_cmp(&right.ns));
    let median = &samples[SAMPLES / 2];

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    checksum ^= black_box(op());
    ALLOC.enabled.store(false, Ordering::Relaxed);

    BenchResult {
        median_ns: median.ns,
        max_ns,
        cycles_per_item: median
            .cycles
            .map(|cycles| cycles as f64 / item_count as f64),
        alloc: ALLOC.snapshot().delta(before),
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
    let simfile = root.join("chart.ssc");
    fs::write(&simfile, analysis_fixture()).unwrap();

    let old_checksum = benchmark_song_parse_legacy(&simfile, 1);
    let new_checksum = benchmark_song_parse_current(&simfile, 1);
    assert_eq!(old_checksum, new_checksum, "RSSP boundary parse diverged");
    let old = measure(SONG_PARSE_ROUNDS, || {
        benchmark_song_parse_legacy(&simfile, SONG_PARSE_ROUNDS)
    });
    let new = measure(SONG_PARSE_ROUNDS, || {
        benchmark_song_parse_current(&simfile, SONG_PARSE_ROUNDS)
    });
    black_box(old.checksum ^ new.checksum);
    println!("RSSP song-parse boundary ({SONG_PARSE_ROUNDS} cache misses)");
    print_result("old", SONG_PARSE_ROUNDS, &old);
    print_result("new", SONG_PARSE_ROUNDS, &new);
    print_change(&old, &new);

    fs::remove_dir_all(root).unwrap();
}

fn analysis_fixture() -> Vec<u8> {
    let mut fixture = String::with_capacity(64 * 1024);
    fixture.push_str(
        "#VERSION:0.83;\n#TITLE:Boundary Benchmark;\n#ARTIST:DeadSync;\n\
         #BPMS:0.000=120.000,128.000=180.000;\n#NOTEDATA:;\n\
         #STEPSTYPE:dance-single;\n#DIFFICULTY:Challenge;\n#METER:15;\n#NOTES:\n",
    );
    for measure in 0..512 {
        for row in 0..4 {
            fixture.push_str(match (measure + row) & 3 {
                0 => "1000\n",
                1 => "0100\n",
                2 => "0010\n",
                _ => "0001\n",
            });
        }
        if measure != 511 {
            fixture.push_str(",\n");
        }
    }
    fixture.push_str(";\n");
    fixture.into_bytes()
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

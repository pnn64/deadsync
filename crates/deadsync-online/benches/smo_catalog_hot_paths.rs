use deadsync_online::stepmaniaonline::{PackInfo, parse_catalog};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashSet;
use std::fmt::Write as _;
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 21;
const ROWS: usize = 8_192;
const OPS: usize = 8;
const HEADER: &str = "ID, Pack Name, Song Count, Size, Sync, PackType, Substyle, Min Version\n";

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

// SAFETY: allocation calls delegate unchanged to `System`; relaxed counters
// observe successful operations only while this single-threaded benchmark
// enables them.
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

#[derive(Clone, Copy, Default)]
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

    const fn churn(self) -> u64 {
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

fn measure(mut op: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..2 {
        black_box(op());
    }

    let mut times = Vec::with_capacity(SAMPLES);
    let mut cycles = Vec::with_capacity(SAMPLES);
    let mut checksum = 0u64;
    for _ in 0..SAMPLES {
        let cycle_start = cycle_counter();
        let started = Instant::now();
        let mut sample_checksum = 0u64;
        for _ in 0..OPS {
            sample_checksum = sample_checksum.wrapping_add(black_box(op()));
        }
        let elapsed = started.elapsed().as_secs_f64() * 1e9 / OPS as f64;
        let cycle_end = cycle_counter();
        times.push(elapsed);
        if let Some(sample_cycles) = cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / OPS as f64)
        {
            cycles.push(sample_cycles);
        }
        checksum ^= sample_checksum;
    }
    times.sort_by(f64::total_cmp);
    cycles.sort_by(f64::total_cmp);

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    black_box(op());
    ALLOC.enabled.store(false, Ordering::Relaxed);

    BenchResult {
        median_ns: times[SAMPLES / 2],
        p95_ns: times[SAMPLES * 95 / 100],
        median_cycles: (!cycles.is_empty()).then(|| cycles[cycles.len() / 2]),
        allocated: ALLOC.snapshot().delta(before),
        checksum,
    }
}

#[derive(Clone)]
struct LegacyPackInfo {
    id: u64,
    name: String,
    song_count: u32,
    size_bytes: u64,
    sync: Option<String>,
    pack_type: Option<String>,
    substyle: Option<String>,
    min_version: Option<String>,
    _normalized_name: String,
    _search_text: String,
}

fn legacy_pack(
    id: u64,
    name: String,
    song_count: u32,
    size_bytes: u64,
    sync: Option<String>,
    pack_type: Option<String>,
    substyle: Option<String>,
    min_version: Option<String>,
) -> LegacyPackInfo {
    let normalized_name = name.to_lowercase();
    let metadata = [
        sync.as_deref(),
        pack_type.as_deref(),
        substyle.as_deref(),
        min_version.as_deref(),
    ];
    let capacity = normalized_name.len()
        + metadata
            .iter()
            .flatten()
            .map(|value| value.len() + 1)
            .sum::<usize>()
        + 21;
    let mut search_text = String::with_capacity(capacity);
    search_text.push_str(&normalized_name);
    for value in metadata.into_iter().flatten() {
        search_text.push(' ');
        search_text.extend(value.chars().flat_map(char::to_lowercase));
    }
    search_text.push(' ');
    write!(&mut search_text, "{id}").unwrap();
    LegacyPackInfo {
        id,
        name,
        song_count,
        size_bytes,
        sync,
        pack_type,
        substyle,
        min_version,
        _normalized_name: normalized_name,
        _search_text: search_text,
    }
}

fn legacy_optional(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() || text.eq_ignore_ascii_case("none") || text.eq_ignore_ascii_case("null") {
        None
    } else {
        Some(text.to_string())
    }
}

fn legacy_parse_catalog(text: &str) -> Vec<LegacyPackInfo> {
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some(HEADER.trim_end()));
    let mut packs = Vec::with_capacity(4_096);
    let mut ids = HashSet::with_capacity(4_096);
    for (index, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let fields = line.rsplitn(7, ", ").collect::<Vec<_>>();
        assert_eq!(fields.len(), 7, "row {}", index + 2);
        let (id, quoted_name) = fields[6].split_once(", ").unwrap();
        let name = quoted_name
            .strip_prefix('"')
            .and_then(|name| name.strip_suffix('"'))
            .unwrap();
        let id = id.parse().unwrap();
        assert!(ids.insert(id));
        packs.push(legacy_pack(
            id,
            name.to_string(),
            fields[5].parse().unwrap(),
            fields[4].parse().unwrap(),
            legacy_optional(fields[3]),
            legacy_optional(fields[2]),
            legacy_optional(fields[1]),
            legacy_optional(fields[0]),
        ));
    }
    packs
}

fn string_checksum(value: &str) -> u64 {
    value.bytes().fold(value.len() as u64, |sum, byte| {
        sum.wrapping_mul(131).wrapping_add(u64::from(byte))
    })
}

fn optional_checksum(value: Option<&str>) -> u64 {
    value.map_or(0, string_checksum)
}

fn pack_checksum(
    id: u64,
    name: &str,
    song_count: u32,
    size_bytes: u64,
    metadata: [Option<&str>; 4],
) -> u64 {
    metadata.into_iter().fold(
        id ^ u64::from(song_count).rotate_left(11)
            ^ size_bytes.rotate_left(23)
            ^ string_checksum(name).rotate_left(37),
        |sum, value| sum.wrapping_mul(17).wrapping_add(optional_checksum(value)),
    )
}

fn legacy_checksum(packs: &[LegacyPackInfo]) -> u64 {
    packs.iter().fold(0u64, |sum, pack| {
        sum.wrapping_mul(257).wrapping_add(pack_checksum(
            pack.id,
            &pack.name,
            pack.song_count,
            pack.size_bytes,
            [
                pack.sync.as_deref(),
                pack.pack_type.as_deref(),
                pack.substyle.as_deref(),
                pack.min_version.as_deref(),
            ],
        ))
    })
}

fn current_checksum(packs: &[PackInfo]) -> u64 {
    packs.iter().fold(0u64, |sum, pack| {
        sum.wrapping_mul(257).wrapping_add(pack_checksum(
            pack.id,
            &pack.name,
            pack.song_count,
            pack.size_bytes,
            [
                pack.sync.as_deref(),
                pack.pack_type.as_deref(),
                pack.substyle.as_deref(),
                pack.min_version.as_deref(),
            ],
        ))
    })
}

fn fixture() -> String {
    let mut text = String::with_capacity(ROWS * 110);
    text.push_str(HEADER);
    for index in 0..ROWS {
        let pack_type = if index % 7 == 0 { "None" } else { "pad" };
        let substyle = if index % 11 == 0 { "null" } else { "technical" };
        writeln!(
            text,
            "{}, \"Tournament Pack {index:05}, Volume {}\", {}, {}, 9ms, {pack_type}, {substyle}, Stepmania 5",
            100_000 + index,
            index % 12,
            10 + index % 40,
            50_000_000 + index * 1_337,
        )
        .unwrap();
    }
    text
}

fn main() {
    let catalog = fixture();
    let expected = legacy_parse_catalog(&catalog);
    let actual = parse_catalog(&catalog).unwrap();
    assert_eq!(legacy_checksum(&expected), current_checksum(&actual));

    let old = measure(|| legacy_checksum(&legacy_parse_catalog(black_box(&catalog))));
    let new = measure(|| current_checksum(&parse_catalog(black_box(&catalog)).unwrap()));
    assert_eq!(old.checksum, new.checksum, "catalog behavior diverged");

    println!("StepManiaOnline catalog parse ({ROWS} rows)");
    print_result("old", &old);
    print_result("new", &new);
    println!(
        "  change: {:>7.2}% median  {:>7.2}% p95  {:>7.2}% cycles  {:>7.2}% throughput  \
         {:>7.2}% allocs  {:>7.2}% bytes  {:>7.2}% churn",
        improvement(old.median_ns, new.median_ns),
        improvement(old.p95_ns, new.p95_ns),
        improvement(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN),
        ),
        percent_change(throughput(&old), throughput(&new)),
        improvement(old.allocated.allocs as f64, new.allocated.allocs as f64),
        improvement(
            old.allocated.allocated_bytes as f64,
            new.allocated.allocated_bytes as f64,
        ),
        improvement(old.allocated.churn() as f64, new.allocated.churn() as f64),
    );

    assert!(new.median_ns < old.median_ns, "median regressed");
    assert!(new.p95_ns < old.p95_ns, "p95 regressed");
    if let (Some(old_cycles), Some(new_cycles)) = (old.median_cycles, new.median_cycles) {
        assert!(new_cycles < old_cycles, "cycles regressed");
    }
    assert!(new.allocated.allocs < old.allocated.allocs);
    assert!(new.allocated.reallocs < old.allocated.reallocs);
    assert!(new.allocated.frees < old.allocated.frees);
    assert!(new.allocated.allocated_bytes < old.allocated.allocated_bytes);
    assert!(new.allocated.churn() < old.allocated.churn());
}

fn print_result(label: &str, result: &BenchResult) {
    println!(
        "  {label:<3} {:>11.1} ns  p95 {:>11.1} ns  {:>11.1} cycles  {:>10.0} row/s  \
         {:>6} alloc  {:>4} realloc  {:>6} free  {:>10} B alloc  {:>10} B churn",
        result.median_ns,
        result.p95_ns,
        result.median_cycles.unwrap_or(f64::NAN),
        throughput(result),
        result.allocated.allocs,
        result.allocated.reallocs,
        result.allocated.frees,
        result.allocated.allocated_bytes,
        result.allocated.churn(),
    );
}

fn throughput(result: &BenchResult) -> f64 {
    ROWS as f64 * 1e9 / result.median_ns
}

fn improvement(old: f64, new: f64) -> f64 {
    (1.0 - new / old) * 100.0
}

fn percent_change(old: f64, new: f64) -> f64 {
    (new / old - 1.0) * 100.0
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

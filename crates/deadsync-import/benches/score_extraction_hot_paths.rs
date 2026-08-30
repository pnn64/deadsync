use deadsync_import::itg::bench_support;
use deadsync_import::xml::XmlNode;
use flate2::Compression;
use flate2::write::GzEncoder;
use std::alloc::{GlobalAlloc, Layout, System};
use std::fmt::Write as _;
use std::hint::black_box;
use std::io::Write as _;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SONGS: usize = 96;
const STEPS_PER_SONG: usize = 6;
const SCORES_PER_STEPS: usize = 8;
const SCORE_ITEMS: usize = SONGS * STEPS_PER_SONG * SCORES_PER_STEPS;
const GZIP_LINES: usize = 24_000;
const ITERATIONS: usize = 60;
const SAMPLES: usize = 20;

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

// SAFETY: all requests are delegated unchanged to `System`; relaxed counters
// only observe successful calls while this single-threaded benchmark enables them.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplied `layout`.
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

    const fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

struct BenchResult {
    ns_per_op: f64,
    p95_ns: f64,
    cycles_per_op: Option<f64>,
    items_per_second: f64,
    allocated: AllocSnapshot,
    allocation_runs: usize,
    checksum: u64,
}

fn measure(items_per_op: usize, mut operation: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..2 {
        black_box(operation());
    }

    let batch = (ITERATIONS / SAMPLES).max(1);
    let mut sample_ns = Vec::with_capacity(SAMPLES);
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..SAMPLES {
        let sample_started = Instant::now();
        for _ in 0..batch {
            checksum = checksum.wrapping_add(black_box(operation()));
        }
        sample_ns.push(sample_started.elapsed().as_secs_f64() * 1e9 / batch as f64);
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();
    sample_ns.sort_unstable_by(f64::total_cmp);

    let allocation_runs = ITERATIONS / 15;
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut allocation_checksum = 0u64;
    for _ in 0..allocation_runs {
        allocation_checksum = allocation_checksum.wrapping_add(black_box(operation()));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);
    black_box(allocation_checksum);

    let measured_runs = batch * SAMPLES;
    let seconds = elapsed.as_secs_f64();
    BenchResult {
        ns_per_op: seconds * 1e9 / measured_runs as f64,
        p95_ns: sample_ns[(sample_ns.len() * 95 / 100).min(sample_ns.len() - 1)],
        cycles_per_op: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / measured_runs as f64),
        items_per_second: measured_runs as f64 * items_per_op as f64 / seconds,
        allocated,
        allocation_runs,
        checksum,
    }
}

#[derive(Clone, Copy)]
enum AllocationGuard {
    ReallocationsDrop,
    OperationsDrop,
}

fn print_pair(title: &str, old: &BenchResult, new: &BenchResult, guard: AllocationGuard) {
    println!("\n{title}");
    print_result("old", old);
    print_result("new", new);
    println!(
        "  change: {:>7.2}% latency  {:>7.2}% cycles  {:>7.2}% throughput  \
         {:>7.2}% p95  {:>7.2}% churn",
        percent_change(old.ns_per_op, new.ns_per_op),
        percent_change(
            old.cycles_per_op.unwrap_or(f64::NAN),
            new.cycles_per_op.unwrap_or(f64::NAN),
        ),
        percent_change(old.items_per_second, new.items_per_second),
        percent_change(old.p95_ns, new.p95_ns),
        percent_change(
            old.allocated.churn_bytes() as f64,
            new.allocated.churn_bytes() as f64,
        ),
    );

    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert!(new.ns_per_op < old.ns_per_op, "{title} latency regressed");
    assert!(
        new.items_per_second > old.items_per_second,
        "{title} throughput regressed"
    );
    if let (Some(old_cycles), Some(new_cycles)) = (old.cycles_per_op, new.cycles_per_op) {
        assert!(new_cycles < old_cycles, "{title} CPU cycles regressed");
    }
    match guard {
        AllocationGuard::ReallocationsDrop => {
            assert!(
                new.allocated.reallocs < old.allocated.reallocs,
                "{title} reallocation count did not improve"
            );
            assert!(
                new.allocated.churn_bytes() < old.allocated.churn_bytes(),
                "{title} memory churn did not improve"
            );
        }
        AllocationGuard::OperationsDrop => {
            assert!(
                new.allocated.allocs < old.allocated.allocs,
                "{title} allocation count did not improve"
            );
            assert!(
                new.allocated.frees < old.allocated.frees,
                "{title} free count did not improve"
            );
            assert!(
                new.allocated.churn_bytes() < old.allocated.churn_bytes(),
                "{title} memory churn did not improve"
            );
        }
    }
}

fn print_result(label: &str, result: &BenchResult) {
    let runs = result.allocation_runs as f64;
    println!(
        "  {label:<3} {:>11.2} ns/op  {:>11.2} cycles/op  {:>11.2} p95 ns  \
         {:>7.2} Mitem/s  {:>8.1} alloc/op  {:>6.1} realloc/op  \
         {:>8.1} free/op  {:>11.1} churn B/op",
        result.ns_per_op,
        result.cycles_per_op.unwrap_or(f64::NAN),
        result.p95_ns,
        result.items_per_second / 1e6,
        result.allocated.allocs as f64 / runs,
        result.allocated.reallocs as f64 / runs,
        result.allocated.frees as f64 / runs,
        result.allocated.churn_bytes() as f64 / runs,
    );
}

fn percent_change(old: f64, new: f64) -> f64 {
    if old == 0.0 {
        return if new == 0.0 { 0.0 } else { f64::INFINITY };
    }
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

fn leaf(tag: &str, text: impl Into<String>) -> XmlNode {
    XmlNode {
        tag: tag.to_owned(),
        text: text.into(),
        ..Default::default()
    }
}

fn score_node(index: usize) -> XmlNode {
    let tap = XmlNode {
        tag: "TapNoteScores".to_owned(),
        children: vec![
            leaf("HitMine", (index % 3).to_string()),
            leaf("AvoidMine", (index % 7).to_string()),
            leaf("Miss", (index % 5).to_string()),
            leaf("W5", (index % 11).to_string()),
            leaf("W4", (index % 13).to_string()),
            leaf("W3", (index % 17).to_string()),
            leaf("W2", (index % 19).to_string()),
            leaf("W1", (index + 500).to_string()),
        ],
        ..Default::default()
    };
    let hold = XmlNode {
        tag: "HoldNoteScores".to_owned(),
        children: vec![
            leaf("LetGo", (index % 2).to_string()),
            leaf("Held", (index % 23).to_string()),
            leaf("MissedHold", (index % 3).to_string()),
        ],
        ..Default::default()
    };
    XmlNode {
        tag: "HighScore".to_owned(),
        children: vec![
            leaf("Grade", "Tier03"),
            leaf("PercentDP", "0.987654"),
            leaf("SurviveSeconds", "145.25"),
            leaf("DateTime", "2025-08-30 12:34:56"),
            tap,
            hold,
            leaf("Modifiers", "1.2xMusic, Overhead"),
        ],
        ..Default::default()
    }
}

fn fixture() -> XmlNode {
    let mut songs = Vec::with_capacity(SONGS);
    let mut score_index = 0usize;
    for song_index in 0..SONGS {
        let mut steps = Vec::with_capacity(STEPS_PER_SONG);
        for steps_index in 0..STEPS_PER_SONG {
            let mut high_scores = Vec::with_capacity(SCORES_PER_STEPS + 1);
            high_scores.push(leaf("NumTimesPlayed", SCORES_PER_STEPS.to_string()));
            for _ in 0..SCORES_PER_STEPS {
                high_scores.push(score_node(score_index));
                score_index += 1;
            }
            steps.push(XmlNode {
                tag: "Steps".to_owned(),
                attrs: vec![
                    ("StepsType".to_owned(), "dance-single".to_owned()),
                    ("Difficulty".to_owned(), format!("Benchmark-{steps_index}")),
                    ("Description".to_owned(), String::new()),
                ],
                children: vec![XmlNode {
                    tag: "HighScoreList".to_owned(),
                    children: high_scores,
                    ..Default::default()
                }],
                ..Default::default()
            });
        }
        songs.push(XmlNode {
            tag: "Song".to_owned(),
            attrs: vec![(
                "Dir".to_owned(),
                format!("Songs/Benchmark Pack/Song {song_index:04}/"),
            )],
            children: steps,
            ..Default::default()
        });
    }
    XmlNode {
        tag: "SongScores".to_owned(),
        children: songs,
        ..Default::default()
    }
}

fn gzip_fixture() -> (Vec<u8>, usize) {
    let mut text = String::with_capacity(GZIP_LINES * 128);
    let mut state = 0x9e37_79b9_7f4a_7c15u64;
    for index in 0..GZIP_LINES {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        writeln!(
            text,
            "<HighScore><Grade>Tier03</Grade><PercentDP>0.{:06}</PercentDP>\
             <DateTime>2025-08-30 12:{:02}:{:02}</DateTime>\
             <Name>{state:016x}-{index:08x}</Name></HighScore>",
            state % 1_000_000,
            index % 60,
            state % 60,
        )
        .expect("write gzip fixture");
    }
    let decoded_len = text.len();
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder
        .write_all(text.as_bytes())
        .expect("compress gzip fixture");
    (encoder.finish().expect("finish gzip fixture"), decoded_len)
}

fn main() {
    let root = fixture();
    let (compressed, decoded_bytes) = gzip_fixture();

    print_pair(
        "gzip output capacity from footer",
        &measure(decoded_bytes, || {
            bench_support::gzip_unreserved_copy(black_box(&compressed))
        }),
        &measure(decoded_bytes, || {
            bench_support::gzip_reserved_copy(black_box(&compressed))
        }),
        AllocationGuard::ReallocationsDrop,
    );
    print_pair(
        "reuse valid UTF-8 decode buffer",
        &measure(decoded_bytes, || {
            bench_support::gzip_reserved_copy(black_box(&compressed))
        }),
        &measure(decoded_bytes, || {
            bench_support::gzip_reserved_reuse(black_box(&compressed))
        }),
        AllocationGuard::OperationsDrop,
    );
    print_pair(
        "owned XML text transfer",
        &measure(SCORE_ITEMS, || {
            bench_support::borrowed_from_owned(black_box(root.clone()))
        }),
        &measure(SCORE_ITEMS, || {
            bench_support::consumed(black_box(root.clone()))
        }),
        AllocationGuard::OperationsDrop,
    );
}

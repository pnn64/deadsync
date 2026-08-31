use deadlib_present::actors::TextAlign;
use deadlib_present::anim::EffectMode;
use deadsync_song_lua::{
    SongLuaTextGlowMode, parse_overlay_effect_mode, parse_overlay_effect_mode_reference_for_bench,
    parse_overlay_text_align, parse_overlay_text_align_reference_for_bench,
    parse_overlay_text_glow_mode, parse_overlay_text_glow_mode_reference_for_bench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 31;
const OPS: usize = 512;
const REPEATS: usize = 128;

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

// SAFETY: all requests are delegated unchanged to `System`; counters are
// observed only by this single-threaded benchmark while measurement is gated.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes from the allocator caller.
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
        // SAFETY: the pointer-layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the request is forwarded unchanged to `System`.
        let new_ptr = unsafe { System.realloc(ptr, old, new_size) };
        if !new_ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.allocated_bytes
                .fetch_add(new_size as u64, Ordering::Relaxed);
            self.freed_bytes
                .fetch_add(old.size() as u64, Ordering::Relaxed);
        }
        new_ptr
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

struct Row {
    median_ns: f64,
    p95_ns: f64,
    median_cycles: Option<f64>,
    alloc: AllocSnapshot,
    checksum: u64,
    ops: usize,
    items: usize,
}

fn timed_sample(ops: usize, op: &mut impl FnMut() -> u64) -> (f64, Option<f64>, u64) {
    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..ops {
        checksum = checksum.wrapping_add(black_box(op()));
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();
    (
        elapsed.as_secs_f64() * 1e9 / ops as f64,
        cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / ops as f64),
        checksum,
    )
}

fn allocation_sample(ops: usize, op: &mut impl FnMut() -> u64) -> (AllocSnapshot, u64) {
    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut checksum = 0u64;
    for _ in 0..ops {
        checksum = checksum.wrapping_add(black_box(op()));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    (ALLOC.snapshot().delta(before), checksum)
}

fn measure_pair(
    items: usize,
    mut old_op: impl FnMut() -> u64,
    mut new_op: impl FnMut() -> u64,
) -> (Row, Row) {
    for _ in 0..8 {
        black_box(old_op());
        black_box(new_op());
    }

    let mut old_times = Vec::with_capacity(SAMPLES);
    let mut new_times = Vec::with_capacity(SAMPLES);
    let mut old_cycles = Vec::with_capacity(SAMPLES);
    let mut new_cycles = Vec::with_capacity(SAMPLES);
    let mut old_checksum = 0u64;
    let mut new_checksum = 0u64;
    for sample in 0..SAMPLES {
        let ((old_ns, old_cycle, old_sum), (new_ns, new_cycle, new_sum)) = if sample % 2 == 0 {
            let old = timed_sample(OPS, &mut old_op);
            let new = timed_sample(OPS, &mut new_op);
            (old, new)
        } else {
            let new = timed_sample(OPS, &mut new_op);
            let old = timed_sample(OPS, &mut old_op);
            (old, new)
        };
        old_times.push(old_ns);
        new_times.push(new_ns);
        if let Some(cycles) = old_cycle {
            old_cycles.push(cycles);
        }
        if let Some(cycles) = new_cycle {
            new_cycles.push(cycles);
        }
        old_checksum ^= old_sum;
        new_checksum ^= new_sum;
    }
    old_times.sort_by(f64::total_cmp);
    new_times.sort_by(f64::total_cmp);
    old_cycles.sort_by(f64::total_cmp);
    new_cycles.sort_by(f64::total_cmp);

    let (old_alloc, old_alloc_sum) = allocation_sample(OPS, &mut old_op);
    let (new_alloc, new_alloc_sum) = allocation_sample(OPS, &mut new_op);
    old_checksum ^= old_alloc_sum;
    new_checksum ^= new_alloc_sum;

    let row = |times: Vec<f64>, cycles: Vec<f64>, alloc, checksum| Row {
        median_ns: times[SAMPLES / 2],
        p95_ns: times[SAMPLES * 95 / 100],
        median_cycles: (!cycles.is_empty()).then(|| cycles[cycles.len() / 2]),
        alloc,
        checksum,
        ops: OPS,
        items,
    };
    (
        row(old_times, old_cycles, old_alloc, old_checksum),
        row(new_times, new_cycles, new_alloc, new_checksum),
    )
}

fn mix(checksum: u64, value: u64) -> u64 {
    checksum.wrapping_mul(1_099_511_628_211).wrapping_add(value)
}

fn effect_mode_value(value: Option<EffectMode>) -> u64 {
    match value {
        None => 0,
        Some(EffectMode::None) => 1,
        Some(EffectMode::DiffuseRamp) => 2,
        Some(EffectMode::DiffuseShift) => 3,
        Some(EffectMode::GlowShift) => 4,
        Some(EffectMode::Pulse) => 5,
        Some(EffectMode::Bob) => 6,
        Some(EffectMode::Bounce) => 7,
        Some(EffectMode::Wag) => 8,
        Some(EffectMode::Spin) => 9,
    }
}

fn text_align_value(value: Option<TextAlign>) -> u64 {
    match value {
        None => 0,
        Some(TextAlign::Left) => 1,
        Some(TextAlign::Center) => 2,
        Some(TextAlign::Right) => 3,
    }
}

fn text_glow_value(value: Option<SongLuaTextGlowMode>) -> u64 {
    match value {
        None => 0,
        Some(SongLuaTextGlowMode::Inner) => 1,
        Some(SongLuaTextGlowMode::Stroke) => 2,
        Some(SongLuaTextGlowMode::Both) => 3,
    }
}

fn assert_improved(name: &str, old: &Row, new: &Row) {
    assert_eq!(old.checksum, new.checksum, "{name} behavior diverged");
    assert!(
        new.median_ns < old.median_ns,
        "{name} median regressed: old={} ns new={} ns",
        old.median_ns,
        new.median_ns
    );
    assert!(
        new.p95_ns < old.p95_ns,
        "{name} p95 regressed: old={} ns new={} ns",
        old.p95_ns,
        new.p95_ns
    );
    if let (Some(old_cycles), Some(new_cycles)) = (old.median_cycles, new.median_cycles) {
        assert!(
            new_cycles < old_cycles,
            "{name} cycles regressed: old={old_cycles} new={new_cycles}"
        );
    }
    assert!(new.alloc.allocs < old.alloc.allocs);
    assert!(new.alloc.reallocs <= old.alloc.reallocs);
    assert!(new.alloc.frees < old.alloc.frees);
    assert!(new.alloc.allocated_bytes < old.alloc.allocated_bytes);
    assert!(new.alloc.churn() < old.alloc.churn());
    assert_eq!(new.alloc.allocs, 0, "{name} still allocates");
    assert_eq!(new.alloc.reallocs, 0, "{name} still reallocates");
    assert_eq!(new.alloc.frees, 0, "{name} still frees");
    assert_eq!(new.alloc.churn(), 0, "{name} still produces churn");
}

fn print_pair(name: &str, old: &Row, new: &Row) {
    println!("{name}");
    print_row("old", old);
    print_row("new", new);
    println!(
        "  change: {:+.2}% median  {:+.2}% p95  {:+.2}% cycles  {:+.2}% throughput  {:+.2}% allocs  {:+.2}% churn",
        change(old.median_ns, new.median_ns),
        change(old.p95_ns, new.p95_ns),
        change(
            old.median_cycles.unwrap_or(f64::NAN),
            new.median_cycles.unwrap_or(f64::NAN)
        ),
        change(throughput(old), throughput(new)),
        change(old.alloc.allocs as f64, new.alloc.allocs as f64),
        change(old.alloc.churn() as f64, new.alloc.churn() as f64),
    );
}

fn print_row(label: &str, row: &Row) {
    let divisor = row.ops as f64;
    println!(
        "  {label:<3} {:>10.1} ns  p95 {:>10.1} ns  {:>10.1} cycles  {:>12.0} item/s  \
         {:>7.1} alloc  {:>6.1} realloc  {:>7.1} free  {:>10.1} allocated B  {:>10.1} churn B/op",
        row.median_ns,
        row.p95_ns,
        row.median_cycles.unwrap_or(f64::NAN),
        throughput(row),
        row.alloc.allocs as f64 / divisor,
        row.alloc.reallocs as f64 / divisor,
        row.alloc.frees as f64 / divisor,
        row.alloc.allocated_bytes as f64 / divisor,
        row.alloc.churn() as f64 / divisor,
    );
}

fn throughput(row: &Row) -> f64 {
    row.items as f64 * 1e9 / row.median_ns
}

fn change(old: f64, new: f64) -> f64 {
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

fn main() {
    let effect_names = [
        " none ",
        "DIFFUSERAMP",
        "DiffuseShift",
        "glowSHIFT",
        "Pulse",
        "BOB",
        "Bounce",
        "Wag",
        "SPIN",
        "\"none\"",
        "unknown",
    ];
    let effect_suite = |lookup: fn(&str) -> Option<EffectMode>| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for name in effect_names {
                checksum = mix(
                    checksum,
                    effect_mode_value(black_box(lookup(black_box(name)))),
                );
            }
        }
        checksum
    };
    let (old_effect, new_effect) = measure_pair(
        effect_names.len() * REPEATS,
        || effect_suite(parse_overlay_effect_mode_reference_for_bench),
        || effect_suite(parse_overlay_effect_mode),
    );
    assert_improved("overlay effect-mode parsing", &old_effect, &new_effect);
    print_pair(
        "overlay effect-mode parsing (11 representative tokens)",
        &old_effect,
        &new_effect,
    );

    let align_names = [
        " LEFT ",
        "'HorizAlign_Left'",
        "\"center\"",
        "Middle",
        "HORIZALIGN_CENTER",
        "horizalign_middle",
        "right",
        "'HORIZALIGN_RIGHT'",
        "unknown",
    ];
    let align_suite = |lookup: fn(&str) -> Option<TextAlign>| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for name in align_names {
                checksum = mix(
                    checksum,
                    text_align_value(black_box(lookup(black_box(name)))),
                );
            }
        }
        checksum
    };
    let (old_align, new_align) = measure_pair(
        align_names.len() * REPEATS,
        || align_suite(parse_overlay_text_align_reference_for_bench),
        || align_suite(parse_overlay_text_align),
    );
    assert_improved("overlay text-alignment parsing", &old_align, &new_align);
    print_pair(
        "overlay text-alignment parsing (9 representative tokens)",
        &old_align,
        &new_align,
    );

    let glow_names = [
        " INNER ",
        "'TextGlowMode_Inner'",
        "Stroke",
        "\"TEXTGLOWMODE_STROKE\"",
        "both",
        "TextGlowMode_Both",
        "unknown",
    ];
    let glow_suite = |lookup: fn(&str) -> Option<SongLuaTextGlowMode>| {
        let mut checksum = 0u64;
        for _ in 0..REPEATS {
            for name in glow_names {
                checksum = mix(
                    checksum,
                    text_glow_value(black_box(lookup(black_box(name)))),
                );
            }
        }
        checksum
    };
    let (old_glow, new_glow) = measure_pair(
        glow_names.len() * REPEATS,
        || glow_suite(parse_overlay_text_glow_mode_reference_for_bench),
        || glow_suite(parse_overlay_text_glow_mode),
    );
    assert_improved("overlay text-glow parsing", &old_glow, &new_glow);
    print_pair(
        "overlay text-glow parsing (7 representative tokens)",
        &old_glow,
        &new_glow,
    );
}

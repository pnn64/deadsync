use deadlib_audio_core::{mixer::bench_support as mixer, render::bench_support as render};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 4_096;
const CALLBACKS: usize = 50_000;
const SAMPLE_OPS: usize = 100;

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

// SAFETY: each operation delegates unchanged to `System`; relaxed counters
// only observe successful allocator calls while the benchmark gate is enabled.
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

    const fn operations(self) -> u64 {
        self.allocs + self.reallocs + self.frees
    }
}

struct BenchResult {
    ns_per_callback: f64,
    median_sample_ns: f64,
    p95_sample_ns: f64,
    cycles_per_callback: Option<f64>,
    samples_per_second: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure(mut callback: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..(CALLBACKS / 20) {
        black_box(callback());
    }

    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    let mut sample_ns = Vec::with_capacity(CALLBACKS / SAMPLE_OPS);
    for _ in 0..CALLBACKS / SAMPLE_OPS {
        let sample_started = Instant::now();
        for _ in 0..SAMPLE_OPS {
            checksum = checksum.wrapping_add(black_box(callback()));
        }
        sample_ns
            .push(sample_started.elapsed().as_secs_f64() * 1_000_000_000.0 / SAMPLE_OPS as f64);
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();
    sample_ns.sort_unstable_by(f64::total_cmp);
    let median_sample_ns = sample_ns[sample_ns.len() / 2];
    let p95_index = (sample_ns.len() * 95).div_ceil(100).saturating_sub(1);
    let p95_sample_ns = sample_ns[p95_index];

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut allocation_checksum = 0u64;
    for _ in 0..CALLBACKS {
        allocation_checksum = allocation_checksum.wrapping_add(black_box(callback()));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);
    black_box(allocation_checksum);

    let seconds = elapsed.as_secs_f64();
    BenchResult {
        ns_per_callback: seconds * 1_000_000_000.0 / CALLBACKS as f64,
        median_sample_ns,
        p95_sample_ns,
        cycles_per_callback: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / CALLBACKS as f64),
        samples_per_second: CALLBACKS as f64 * SAMPLES as f64 / seconds,
        allocated,
        checksum,
    }
}

fn print_pair(title: &str, old: &BenchResult, new: &BenchResult) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert_eq!(old.allocated.operations(), 0, "{title} old path allocated");
    assert_eq!(new.allocated.operations(), 0, "{title} new path allocated");
    assert_eq!(new.allocated.churn_bytes(), 0, "{title} new path churned");
    println!("\n{title}");
    print_result("old", old);
    print_result("new", new);
    println!(
        "  change: {:>7.2}% latency  {:>7.2}% cycles  {:>7.2}% throughput  \
         {:>7.2}% median  {:>7.2}% p95",
        percent_change(old.ns_per_callback, new.ns_per_callback),
        percent_change(
            old.cycles_per_callback.unwrap_or(f64::NAN),
            new.cycles_per_callback.unwrap_or(f64::NAN),
        ),
        percent_change(old.samples_per_second, new.samples_per_second),
        percent_change(old.median_sample_ns, new.median_sample_ns),
        percent_change(old.p95_sample_ns, new.p95_sample_ns),
    );
}

fn print_result(label: &str, result: &BenchResult) {
    let count = CALLBACKS as f64;
    println!(
        "  {label:<3} {:>9.2} ns/cb  {:>9.2} cycles/cb  {:>9.2} median ns  \
         {:>9.2} p95 ns  \
         {:>8.1} Msamp/s  {:>5.2} alloc/cb  {:>5.2} realloc/cb  {:>5.2} free/cb  {:>8.1} churn B/cb",
        result.ns_per_callback,
        result.cycles_per_callback.unwrap_or(f64::NAN),
        result.median_sample_ns,
        result.p95_sample_ns,
        result.samples_per_second / 1_000_000.0,
        result.allocated.allocs as f64 / count,
        result.allocated.reallocs as f64 / count,
        result.allocated.frees as f64 / count,
        result.allocated.churn_bytes() as f64 / count,
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

const fn f32_checksum(samples: &[f32]) -> u64 {
    samples[0].to_bits() as u64
        ^ (samples[SAMPLES / 2].to_bits() as u64).rotate_left(17)
        ^ (samples[SAMPLES - 1].to_bits() as u64).rotate_left(33)
}

const fn i16_checksum(samples: &[i16]) -> u64 {
    samples[0] as u16 as u64
        ^ (samples[SAMPLES / 2] as u16 as u64).rotate_left(17)
        ^ (samples[SAMPLES - 1] as u16 as u64).rotate_left(33)
}

fn main() {
    let source: Vec<i16> = (0..SAMPLES)
        .map(|index| ((index as i32 * 7_919 + 13_337) as i16).wrapping_sub(8_000))
        .collect();

    let mut old_muted = vec![f32::NAN; SAMPLES];
    let mut old_muted_gain = 1.0f32;
    let muted_old = measure(|| {
        old_muted_gain = 1.0;
        render::music_convert_old(
            black_box(&source),
            &mut old_muted,
            black_box(2),
            black_box(0.0),
            black_box(1.0),
            &mut old_muted_gain,
        );
        f32_checksum(&old_muted)
    });
    let mut new_muted = vec![f32::NAN; SAMPLES];
    let mut new_muted_gain = 1.0f32;
    let muted_new = measure(|| {
        new_muted_gain = 1.0;
        render::music_convert_new(
            black_box(&source),
            &mut new_muted,
            black_box(2),
            black_box(0.0),
            black_box(1.0),
            &mut new_muted_gain,
        );
        f32_checksum(&new_muted)
    });
    assert!(
        old_muted
            .iter()
            .zip(&new_muted)
            .all(|(&old, &new)| old.to_bits() == new.to_bits()),
        "muted music conversion changed samples"
    );
    print_pair("muted stable-gain music conversion", &muted_old, &muted_new);

    let mut old_unity = vec![f32::NAN; SAMPLES];
    let mut old_unity_gain = 1.0f32;
    let unity_old = measure(|| {
        old_unity_gain = 1.0;
        render::music_convert_old(
            black_box(&source),
            &mut old_unity,
            black_box(2),
            black_box(1.0),
            black_box(1.0),
            &mut old_unity_gain,
        );
        f32_checksum(&old_unity)
    });
    let mut new_unity = vec![f32::NAN; SAMPLES];
    let mut new_unity_gain = 1.0f32;
    let unity_new = measure(|| {
        new_unity_gain = 1.0;
        render::music_convert_new(
            black_box(&source),
            &mut new_unity,
            black_box(2),
            black_box(1.0),
            black_box(1.0),
            &mut new_unity_gain,
        );
        f32_checksum(&new_unity)
    });
    assert!(
        old_unity
            .iter()
            .zip(&new_unity)
            .all(|(&old, &new)| old.to_bits() == new.to_bits()),
        "unity music conversion changed samples"
    );
    print_pair("unity stable-gain music conversion", &unity_old, &unity_new);

    let mut old_ramp = vec![f32::NAN; SAMPLES];
    let mut old_ramp_gain = 1.0f32;
    let ramp_old = measure(|| {
        old_ramp_gain = 1.0;
        render::music_convert_old(
            black_box(&source),
            &mut old_ramp,
            black_box(2),
            black_box(0.9),
            black_box(0.5),
            &mut old_ramp_gain,
        );
        f32_checksum(&old_ramp) ^ u64::from(old_ramp_gain.to_bits())
    });
    let mut new_ramp = vec![f32::NAN; SAMPLES];
    let mut new_ramp_gain = 1.0f32;
    let ramp_new = measure(|| {
        new_ramp_gain = 1.0;
        render::music_convert_new(
            black_box(&source),
            &mut new_ramp,
            black_box(2),
            black_box(0.9),
            black_box(0.5),
            &mut new_ramp_gain,
        );
        f32_checksum(&new_ramp) ^ u64::from(new_ramp_gain.to_bits())
    });
    assert!(
        old_ramp
            .iter()
            .zip(&new_ramp)
            .all(|(&old, &new)| old.to_bits() == new.to_bits()),
        "stereo gain-ramp conversion changed samples"
    );
    assert_eq!(old_ramp_gain.to_bits(), new_ramp_gain.to_bits());
    print_pair("stereo music gain-ramp conversion", &ramp_old, &ramp_new);

    let mut old_scratch = vec![0.0f32; SAMPLES];
    let mut old_f32 = vec![0.0f32; SAMPLES];
    let old_direct = measure(|| {
        render::music_copy_old(
            black_box(&source),
            &mut old_scratch,
            &mut old_f32,
            black_box(0.9),
        );
        black_box(&old_f32);
        f32_checksum(&old_f32)
    });
    let mut new_f32 = vec![0.0f32; SAMPLES];
    let new_direct = measure(|| {
        render::music_direct_new(black_box(&source), &mut new_f32, black_box(0.9));
        black_box(&new_f32);
        f32_checksum(&new_f32)
    });
    assert!(
        old_f32
            .iter()
            .zip(&new_f32)
            .all(|(&old, &new)| old.to_bits() == new.to_bits()),
        "native f32 render changed samples"
    );
    print_pair("native f32 music render", &old_direct, &new_direct);

    let mut old_mix = vec![0.0f32; SAMPLES];
    let old_sfx = measure(|| {
        old_mix.fill(0.125);
        mixer::mix_sfx_old(black_box(&source), &mut old_mix, black_box(1.0));
        black_box(&old_mix);
        f32_checksum(&old_mix)
    });
    let mut new_mix = vec![0.0f32; SAMPLES];
    let new_sfx = measure(|| {
        new_mix.fill(0.125);
        mixer::mix_sfx_new(black_box(&source), &mut new_mix, black_box(1.0));
        black_box(&new_mix);
        f32_checksum(&new_mix)
    });
    assert!(
        old_mix
            .iter()
            .zip(&new_mix)
            .all(|(&old, &new)| old.to_bits() == new.to_bits()),
        "unity-gain SFX mix changed samples"
    );
    print_pair("unity-gain SFX sample mix", &old_sfx, &new_sfx);

    let silence = vec![0.0f32; SAMPLES];
    let mut old_i16 = vec![i16::MAX; SAMPLES];
    let old_silence = measure(|| {
        render::silent_i16_old(black_box(&silence), &mut old_i16);
        black_box(&old_i16);
        i16_checksum(&old_i16)
    });
    let mut new_i16 = vec![i16::MAX; SAMPLES];
    let new_silence = measure(|| {
        render::silent_i16_new(black_box(&silence), &mut new_i16);
        black_box(&new_i16);
        i16_checksum(&new_i16)
    });
    assert_eq!(old_i16, new_i16, "silent i16 render changed samples");
    print_pair("silent i16 output", &old_silence, &new_silence);
}

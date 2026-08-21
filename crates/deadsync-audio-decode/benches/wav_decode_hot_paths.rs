use deadsync_audio_decode::wav::bench_support::{SampleFormat, decode_new};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLES: usize = 8_192;
const RUNS: usize = 10_000;
const WORST_SAMPLES: usize = 32;

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

// SAFETY: every allocator operation delegates unchanged to `System`; relaxed
// counters only observe successful calls while the benchmark gate is enabled.
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

    const fn operations(self) -> u64 {
        self.allocs + self.reallocs + self.frees
    }

    const fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

struct BenchResult {
    ns_per_sample: f64,
    cycles_per_sample: Option<f64>,
    samples_per_second: f64,
    worst_ns_per_sample: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure(mut operation: impl FnMut() -> u64) -> BenchResult {
    let sample_runs = (RUNS / 20).max(1);
    for _ in 0..sample_runs {
        black_box(operation());
    }

    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    for _ in 0..RUNS {
        checksum = checksum.rotate_left(5) ^ black_box(operation());
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    for _ in 0..RUNS {
        black_box(operation());
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);

    let mut worst_ns_per_sample = 0.0f64;
    for _ in 0..WORST_SAMPLES {
        let started = Instant::now();
        for _ in 0..sample_runs {
            black_box(operation());
        }
        worst_ns_per_sample = worst_ns_per_sample.max(
            started.elapsed().as_secs_f64() * 1_000_000_000.0 / (sample_runs * SAMPLES) as f64,
        );
    }

    let samples = (RUNS * SAMPLES) as f64;
    let seconds = elapsed.as_secs_f64();
    BenchResult {
        ns_per_sample: seconds * 1_000_000_000.0 / samples,
        cycles_per_sample: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / samples),
        samples_per_second: samples / seconds,
        worst_ns_per_sample,
        allocated,
        checksum,
    }
}

fn print_pair(name: &str, old: &BenchResult, new: &BenchResult) {
    println!("\n{name}");
    print_result("old", old);
    print_result("new", new);
    assert_eq!(new.checksum, old.checksum, "{name} output diverged");
    assert_eq!(old.allocated.operations(), 0, "{name} old path allocated");
    assert_eq!(new.allocated.operations(), 0, "{name} new path allocated");
    assert_eq!(new.allocated.churn_bytes(), 0, "{name} new path churned");
}

fn print_result(label: &str, result: &BenchResult) {
    println!(
        "{label:<4} {:>8.3} ns/sample  {:>8.3} cycles/sample  {:>8.3} Msample/s  \
         worst {:>8.3} ns  {:>5.2} alloc/run  {:>5.2} realloc/run  \
         {:>5.2} free/run  {:>8.1} churn B/run  {:016x}",
        result.ns_per_sample,
        result.cycles_per_sample.unwrap_or(f64::NAN),
        result.samples_per_second / 1_000_000.0,
        result.worst_ns_per_sample,
        result.allocated.allocs as f64 / RUNS as f64,
        result.allocated.reallocs as f64 / RUNS as f64,
        result.allocated.frees as f64 / RUNS as f64,
        result.allocated.churn_bytes() as f64 / RUNS as f64,
        result.checksum,
    );
}

fn output_checksum(output: &[i16]) -> u64 {
    let output = black_box(output);
    output.len() as u64
        ^ output.first().copied().unwrap_or_default() as u16 as u64
        ^ ((output.get(output.len() / 2).copied().unwrap_or_default() as u16 as u64) << 16)
        ^ ((output.last().copied().unwrap_or_default() as u16 as u64) << 32)
}

fn sample_bytes(format: SampleFormat) -> usize {
    match format {
        SampleFormat::Pcm16 => 2,
        SampleFormat::Pcm24 => 3,
        SampleFormat::Float32 => 4,
    }
}

fn decode_old(bytes: &[u8], format: SampleFormat, out: &mut Vec<i16>) -> Result<(), &'static str> {
    let width = sample_bytes(format);
    if !bytes.len().is_multiple_of(width) {
        return Err("WAV packet ended mid-sample");
    }
    out.clear();
    out.reserve(bytes.len() / width);
    for sample in bytes.chunks_exact(width) {
        out.push(match format {
            SampleFormat::Pcm16 => i16::from_le_bytes([sample[0], sample[1]]),
            SampleFormat::Pcm24 => {
                let signed = i32::from_le_bytes([
                    sample[0],
                    sample[1],
                    sample[2],
                    if sample[2] & 0x80 != 0 { 0xff } else { 0x00 },
                ]);
                (signed >> 8).clamp(i16::MIN as i32, i16::MAX as i32) as i16
            }
            SampleFormat::Float32 => {
                let value = f32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]]) as f64;
                if value.is_finite() {
                    (value * 32767.0).round().clamp(-32768.0, 32767.0) as i16
                } else {
                    0
                }
            }
        });
    }
    Ok(())
}

fn pcm16_input() -> Vec<u8> {
    (0..SAMPLES)
        .flat_map(|index| (index.wrapping_mul(25_173) as i16).to_le_bytes())
        .collect()
}

fn pcm24_input() -> Vec<u8> {
    (0..SAMPLES)
        .flat_map(|index| {
            let bytes = (index as i32).wrapping_mul(1_103_515_245).to_le_bytes();
            [bytes[0], bytes[1], bytes[2]]
        })
        .collect()
}

fn float32_input() -> Vec<u8> {
    (0..SAMPLES)
        .flat_map(|index| ((index as f32 * 0.017).sin() * 1.1).to_le_bytes())
        .collect()
}

fn run_case(name: &str, bytes: &[u8], format: SampleFormat) {
    let mut expected = Vec::with_capacity(SAMPLES);
    let mut actual = Vec::with_capacity(SAMPLES);
    decode_old(bytes, format, &mut expected).expect("benchmark packet is aligned");
    decode_new(bytes, format, &mut actual).expect("benchmark packet is aligned");
    assert_eq!(actual, expected, "{name} output diverged before timing");

    let old = measure(|| {
        decode_old(black_box(bytes), black_box(format), &mut expected)
            .expect("benchmark packet is aligned");
        output_checksum(&expected)
    });
    let new = measure(|| {
        decode_new(black_box(bytes), black_box(format), &mut actual)
            .expect("benchmark packet is aligned");
        output_checksum(&actual)
    });
    print_pair(name, &old, &new);
}

fn main() {
    run_case("16-bit PCM decode", &pcm16_input(), SampleFormat::Pcm16);
    run_case("24-bit PCM decode", &pcm24_input(), SampleFormat::Pcm24);
    run_case(
        "32-bit float decode",
        &float32_input(),
        SampleFormat::Float32,
    );
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

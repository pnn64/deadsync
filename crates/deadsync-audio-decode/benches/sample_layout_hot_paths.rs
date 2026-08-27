use deadsync_audio_decode::resample::{
    PlanarAccum, write_channel_mapped_i16, write_resampler_output,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const PACKET_FRAMES: usize = 4_096;
const RESAMPLE_FRAMES: usize = 256;
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

    const fn operations(self) -> u64 {
        self.allocs + self.reallocs + self.frees
    }

    const fn churn_bytes(self) -> u64 {
        self.alloc_bytes + self.realloc_bytes + self.free_bytes
    }
}

struct BenchResult {
    ns_per_frame: f64,
    cycles_per_frame: Option<f64>,
    frames_per_second: f64,
    worst_ns_per_frame: f64,
    allocated: AllocSnapshot,
    checksum: u64,
}

fn measure(frames_per_run: usize, mut operation: impl FnMut() -> u64) -> BenchResult {
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

    let mut worst_ns_per_frame = 0.0f64;
    for _ in 0..WORST_SAMPLES {
        let started = Instant::now();
        for _ in 0..sample_runs {
            black_box(operation());
        }
        worst_ns_per_frame = worst_ns_per_frame.max(
            started.elapsed().as_secs_f64() * 1_000_000_000.0
                / (sample_runs * frames_per_run) as f64,
        );
    }

    let frames = (RUNS * frames_per_run) as f64;
    let seconds = elapsed.as_secs_f64();
    BenchResult {
        ns_per_frame: seconds * 1_000_000_000.0 / frames,
        cycles_per_frame: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / frames),
        frames_per_second: frames / seconds,
        worst_ns_per_frame,
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
        "{label:<4} {:>8.3} ns/frame  {:>8.3} cycles/frame  {:>8.3} Mframe/s  \
         worst {:>8.3} ns  {:>5.2} alloc/run  {:>5.2} realloc/run  \
         {:>5.2} free/run  {:>8.1} churn B/run  {:016x}",
        result.ns_per_frame,
        result.cycles_per_frame.unwrap_or(f64::NAN),
        result.frames_per_second / 1_000_000.0,
        result.worst_ns_per_frame,
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
        ^ u64::from(output.first().copied().unwrap_or_default() as u16)
        ^ (u64::from(output.get(output.len() / 2).copied().unwrap_or_default() as u16) << 16)
        ^ (u64::from(output.last().copied().unwrap_or_default() as u16) << 32)
}

fn planar_checksum(output: &PlanarAccum) -> u64 {
    black_box(&output.channels)
        .iter()
        .fold(0u64, |checksum, channel| {
            checksum.rotate_left(7)
                ^ u64::from(channel.first().copied().unwrap_or_default().to_bits())
                ^ (u64::from(channel.last().copied().unwrap_or_default().to_bits()) << 32)
                ^ channel.len() as u64
        })
}

fn deinterleave_old(input: &[i16], output: &mut PlanarAccum) {
    output.clear();
    let channels = output.channels.len();
    let frames = input.len() / channels;
    for channel in &mut output.channels {
        channel.reserve(frames);
    }
    for frame in input.chunks_exact(channels) {
        for (channel, sample) in output.channels.iter_mut().zip(frame) {
            channel.push(f32::from(*sample) / 32768.0);
        }
    }
}

fn sample_to_i16_old(sample: f32) -> i16 {
    (sample * 32767.0).round() as i16
}

fn resampler_output_old(input: &[Vec<f32>], output: &mut Vec<i16>) {
    output.resize(RESAMPLE_FRAMES * 2, 0);
    let mut frame = 0;
    while frame < RESAMPLE_FRAMES {
        let base = frame * 2;
        output[base] = sample_to_i16_old(input[0][frame]);
        output[base + 1] = sample_to_i16_old(input[1][frame]);
        frame += 1;
    }
}

fn channel_map_old(input: &[i16], in_ch: usize, out_ch: usize, output: &mut Vec<i16>) {
    output.resize(PACKET_FRAMES * out_ch, 0);
    let mut frame = 0;
    while frame < PACKET_FRAMES {
        let input_base = frame * in_ch;
        let output_base = frame * out_ch;
        for channel in 0..out_ch {
            output[output_base + channel] = input[input_base + channel % in_ch];
        }
        frame += 1;
    }
}

fn main() {
    let stereo = (0..PACKET_FRAMES * 2)
        .map(|index| index.wrapping_mul(25_173) as i16)
        .collect::<Vec<_>>();
    let mut old_planar = PlanarAccum::new(2, PACKET_FRAMES);
    let mut new_planar = PlanarAccum::new(2, PACKET_FRAMES);
    let old = measure(PACKET_FRAMES, || {
        deinterleave_old(black_box(&stereo), &mut old_planar);
        planar_checksum(&old_planar)
    });
    let new = measure(PACKET_FRAMES, || {
        new_planar.clear();
        new_planar.push_i16_interleaved(black_box(&stereo), black_box(2));
        planar_checksum(&new_planar)
    });
    print_pair("stereo i16 to planar f32", &old, &new);

    let planar = vec![
        (0..RESAMPLE_FRAMES)
            .map(|frame| (frame as f32 * 0.017).sin() * 1.25)
            .collect::<Vec<_>>(),
        (0..RESAMPLE_FRAMES)
            .map(|frame| (frame as f32 * 0.029).cos() * 1.25)
            .collect::<Vec<_>>(),
    ];
    let mut old_interleaved = Vec::with_capacity(RESAMPLE_FRAMES * 2);
    let mut new_interleaved = Vec::with_capacity(RESAMPLE_FRAMES * 2);
    let old = measure(RESAMPLE_FRAMES, || {
        resampler_output_old(black_box(&planar), &mut old_interleaved);
        output_checksum(&old_interleaved)
    });
    let new = measure(RESAMPLE_FRAMES, || {
        write_resampler_output(
            black_box(&planar),
            black_box(RESAMPLE_FRAMES),
            black_box(2),
            &mut new_interleaved,
        );
        output_checksum(&new_interleaved)
    });
    print_pair("planar f32 to stereo i16", &old, &new);

    let mono = stereo.iter().copied().step_by(2).collect::<Vec<_>>();
    let mut old_stereo = Vec::with_capacity(PACKET_FRAMES * 2);
    let mut new_stereo = Vec::with_capacity(PACKET_FRAMES * 2);
    let old = measure(PACKET_FRAMES, || {
        channel_map_old(black_box(&mono), 1, 2, &mut old_stereo);
        output_checksum(&old_stereo)
    });
    let new = measure(PACKET_FRAMES, || {
        write_channel_mapped_i16(
            black_box(&mono),
            black_box(1),
            black_box(2),
            &mut new_stereo,
        );
        output_checksum(&new_stereo)
    });
    print_pair("mono i16 to stereo i16", &old, &new);

    let surround = (0..PACKET_FRAMES * 6)
        .map(|index| index.wrapping_mul(25_173) as i16)
        .collect::<Vec<_>>();
    let old = measure(PACKET_FRAMES, || {
        channel_map_old(black_box(&surround), 6, 2, &mut old_stereo);
        output_checksum(&old_stereo)
    });
    let new = measure(PACKET_FRAMES, || {
        write_channel_mapped_i16(
            black_box(&surround),
            black_box(6),
            black_box(2),
            &mut new_stereo,
        );
        output_checksum(&new_stereo)
    });
    print_pair("six-channel i16 to stereo i16", &old, &new);
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

use deadsync_audio::{
    AudioMixLevels, AudioRenderCallbackResult, MusicBlockTiming, MusicBlockWriter, PlayedMapReader,
    RenderState, bump_music_map_generation, i16_to_f32, music_map_generation, music_transport,
    reset_music_stream_clock_state, reset_music_target_gain, set_audio_mix_levels,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

const SAMPLE_RATE: usize = 48_000;
const CHANNELS: usize = 2;
const BLOCK_FRAMES: usize = 256;
const WARMUP_CALLBACKS: usize = 5_000;
const TAIL_CALLBACKS: usize = 20_000;
const PIPE_CALLBACKS: usize = 100_000;
const SCALE_CALLBACKS: usize = 20_000;
const RESET_CALLBACKS: usize = 200;
const RUNS: usize = 7;

struct CountAlloc;

static ALLOCS: AtomicU64 = AtomicU64::new(0);
static DEALLOCS: AtomicU64 = AtomicU64::new(0);
static REALLOCS: AtomicU64 = AtomicU64::new(0);
static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);
static FREED_BYTES: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for CountAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        // SAFETY: this forwards the allocation contract unchanged to `System`.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        DEALLOCS.fetch_add(1, Ordering::Relaxed);
        FREED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        // SAFETY: this forwards the allocation contract unchanged to `System`.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        REALLOCS.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(size as u64, Ordering::Relaxed);
        FREED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        // SAFETY: this forwards the allocation contract unchanged to `System`.
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static GLOBAL: CountAlloc = CountAlloc;

#[derive(Clone, Copy)]
struct AllocSnap {
    allocs: u64,
    deallocs: u64,
    reallocs: u64,
    allocated_bytes: u64,
    freed_bytes: u64,
}

impl AllocSnap {
    fn now() -> Self {
        Self {
            allocs: ALLOCS.load(Ordering::Relaxed),
            deallocs: DEALLOCS.load(Ordering::Relaxed),
            reallocs: REALLOCS.load(Ordering::Relaxed),
            allocated_bytes: ALLOC_BYTES.load(Ordering::Relaxed),
            freed_bytes: FREED_BYTES.load(Ordering::Relaxed),
        }
    }

    fn since(self, earlier: Self) -> Self {
        Self {
            allocs: self.allocs - earlier.allocs,
            deallocs: self.deallocs - earlier.deallocs,
            reallocs: self.reallocs - earlier.reallocs,
            allocated_bytes: self.allocated_bytes - earlier.allocated_bytes,
            freed_bytes: self.freed_bytes - earlier.freed_bytes,
        }
    }

    fn assert_idle(self) {
        assert_eq!(self.allocs, 0, "steady-state allocation");
        assert_eq!(self.deallocs, 0, "steady-state deallocation");
        assert_eq!(self.reallocs, 0, "steady-state reallocation");
        assert_eq!(self.allocated_bytes, 0, "steady-state allocated bytes");
        assert_eq!(self.freed_bytes, 0, "steady-state freed bytes");
    }
}

#[cfg(windows)]
fn thread_cycles() -> u64 {
    unsafe extern "system" {
        fn GetCurrentThread() -> isize;
        fn QueryThreadCycleTime(thread: isize, cycles: *mut u64) -> i32;
    }

    let mut cycles = 0;
    // SAFETY: the pseudo-handle denotes this thread and `cycles` is a valid
    // writable `u64` for the duration of the call.
    let ok = unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut cycles) };
    assert_ne!(ok, 0, "QueryThreadCycleTime failed");
    cycles
}

#[cfg(not(windows))]
fn thread_cycles() -> u64 {
    0
}

struct AudioHarness {
    writer: MusicBlockWriter,
    played_map: PlayedMapReader,
    render: RenderState,
    input: Vec<i16>,
    output: Vec<f32>,
    callback_frames: usize,
    next_music_sec: f64,
    played_music_sec: f64,
    next_stream_frame: i64,
}

impl AudioHarness {
    fn new(callback_frames: usize) -> Self {
        reset_music_stream_clock_state();
        reset_music_target_gain();
        set_audio_mix_levels(AudioMixLevels {
            master_volume: 100,
            music_volume: 100,
            sfx_volume: 100,
            assist_tick_volume: 100,
        });

        let (stream, render_handle) = music_transport(CHANNELS);
        let render = RenderState::new(render_handle, CHANNELS);
        let sample_count = callback_frames * CHANNELS;
        let input = (0..sample_count)
            .map(|i| ((i as i32 * 251 + 17_123) % 65_535 - 32_767) as i16)
            .collect();

        Self {
            writer: stream.writer,
            played_map: stream.played_map,
            render,
            input,
            output: vec![0.0; sample_count],
            callback_frames,
            next_music_sec: 0.0,
            played_music_sec: 0.0,
            next_stream_frame: 0,
        }
    }

    fn submit_blocks(&mut self) {
        let generation = music_map_generation();
        for samples in self.input.chunks(BLOCK_FRAMES * CHANNELS) {
            let frames = samples.len() / CHANNELS;
            assert_eq!(
                self.writer.try_push(
                    samples,
                    MusicBlockTiming {
                        generation,
                        music_start_sec: self.next_music_sec,
                        music_sec_per_frame: 1.0 / SAMPLE_RATE as f64,
                    },
                ),
                samples.len(),
            );
            self.next_music_sec += frames as f64 / SAMPLE_RATE as f64;
        }
    }

    fn render_callback(&mut self) -> AudioRenderCallbackResult {
        #[cfg(windows)]
        let result = self
            .render
            .render_f32_qpc(&mut self.output, 0, std::iter::empty());
        #[cfg(not(windows))]
        let result = self
            .render
            .render_f32_host_nanos(&mut self.output, 0, std::iter::empty());
        black_box(&self.output);
        black_box(result)
    }

    fn drain_maps(&mut self, check: bool) {
        let current_generation = music_map_generation();
        let mut remaining = self.callback_frames;
        let mut expected_music_sec = self.played_music_sec;
        while remaining > 0 {
            let expected_frames = remaining.min(BLOCK_FRAMES);
            let seg = loop {
                let (generation, seg) = self.played_map.pop().expect("played timing segment");
                if generation == current_generation {
                    break seg;
                }
                black_box(seg);
            };
            if check {
                assert_eq!(seg.stream_frame_start, self.next_stream_frame);
                assert_eq!(seg.frames, expected_frames as i64);
                assert_eq!(seg.music_start_sec.to_bits(), expected_music_sec.to_bits());
                assert_eq!(
                    seg.music_sec_per_frame.to_bits(),
                    (1.0 / SAMPLE_RATE as f64).to_bits(),
                );
            }
            expected_music_sec += expected_frames as f64 / SAMPLE_RATE as f64;
            self.next_stream_frame += expected_frames as i64;
            remaining -= expected_frames;
            black_box(seg);
        }
        self.played_music_sec = expected_music_sec;
        while let Some((generation, seg)) = self.played_map.pop() {
            assert_ne!(
                generation, current_generation,
                "unexpected current map segment"
            );
            black_box(seg);
        }
    }

    fn step(&mut self) {
        self.submit_blocks();
        let result = self.render_callback();
        black_box(result);
        self.drain_maps(false);
    }

    fn prepare_reset(&mut self, stale_blocks: usize) {
        let old_generation = music_map_generation();
        let stale = &self.input[..BLOCK_FRAMES * CHANNELS];
        for _ in 0..stale_blocks {
            assert_eq!(
                self.writer.try_push(
                    stale,
                    MusicBlockTiming {
                        generation: old_generation,
                        music_start_sec: self.next_music_sec,
                        music_sec_per_frame: 1.0 / SAMPLE_RATE as f64,
                    },
                ),
                stale.len(),
            );
        }
        let _ = bump_music_map_generation();
        self.submit_blocks();
    }

    fn finish_reset(&mut self, verify: bool) {
        let result = self.render_callback();
        assert!(!result.output_underrun);
        if verify {
            for (&actual, &input) in self.output.iter().zip(&self.input) {
                assert_eq!(actual.to_bits(), i16_to_f32(input).to_bits());
            }
        }
        self.drain_maps(false);
    }

    fn verify(&mut self) {
        self.submit_blocks();
        let result = self.render_callback();
        assert_eq!(
            result,
            AudioRenderCallbackResult {
                output_underrun: false,
                callback_gap_ns: 0,
            },
        );
        for (&actual, &input) in self.output.iter().zip(&self.input) {
            assert_eq!(actual.to_bits(), i16_to_f32(input).to_bits());
        }
        self.drain_maps(true);
    }

    fn warm(&mut self) {
        self.verify();
        for _ in 0..WARMUP_CALLBACKS {
            self.step();
        }
    }
}

#[derive(Clone, Copy)]
struct TailStats {
    ns_p50: u64,
    ns_p95: u64,
    ns_p99: u64,
    cycles_p50: u64,
    cycles_p95: u64,
    cycles_p99: u64,
}

#[derive(Clone, Copy)]
struct RateStats {
    ns_per_callback: f64,
    cycles_per_callback: f64,
}

fn percentile(sorted: &[u64], percentile: f64) -> u64 {
    sorted[((sorted.len() - 1) as f64 * percentile).round() as usize]
}

fn median_u64(mut values: Vec<u64>) -> u64 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn median_f64(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

fn cold_memory() {
    let before = AllocSnap::now();
    let mut harness = AudioHarness::new(480);
    harness.verify();
    let warm = AllocSnap::now().since(before);
    drop(harness);
    let dropped = AllocSnap::now().since(before);
    assert_eq!(dropped.allocated_bytes, dropped.freed_bytes);
    println!(
        "cold memory: alloc={} realloc={} retained={} B",
        warm.allocs,
        warm.reallocs,
        warm.allocated_bytes - warm.freed_bytes,
    );
}

fn tail_run() -> TailStats {
    let mut harness = AudioHarness::new(480);
    harness.warm();

    let mut cycles = Vec::with_capacity(TAIL_CALLBACKS);
    let before = AllocSnap::now();
    for _ in 0..TAIL_CALLBACKS {
        harness.submit_blocks();
        let start = thread_cycles();
        harness.render_callback();
        cycles.push(thread_cycles() - start);
        harness.drain_maps(false);
    }
    AllocSnap::now().since(before).assert_idle();

    let mut nanos = Vec::with_capacity(TAIL_CALLBACKS);
    let before = AllocSnap::now();
    for _ in 0..TAIL_CALLBACKS {
        harness.submit_blocks();
        let start = Instant::now();
        harness.render_callback();
        nanos.push(start.elapsed().as_nanos() as u64);
        harness.drain_maps(false);
    }
    AllocSnap::now().since(before).assert_idle();

    cycles.sort_unstable();
    nanos.sort_unstable();
    TailStats {
        ns_p50: percentile(&nanos, 0.50),
        ns_p95: percentile(&nanos, 0.95),
        ns_p99: percentile(&nanos, 0.99),
        cycles_p50: percentile(&cycles, 0.50),
        cycles_p95: percentile(&cycles, 0.95),
        cycles_p99: percentile(&cycles, 0.99),
    }
}

fn callback_tails() {
    let stats: Vec<_> = (0..RUNS).map(|_| tail_run()).collect();
    println!(
        "callback 480f: ns p50={} p95={} p99={}; cycles p50={} p95={} p99={}",
        median_u64(stats.iter().map(|s| s.ns_p50).collect()),
        median_u64(stats.iter().map(|s| s.ns_p95).collect()),
        median_u64(stats.iter().map(|s| s.ns_p99).collect()),
        median_u64(stats.iter().map(|s| s.cycles_p50).collect()),
        median_u64(stats.iter().map(|s| s.cycles_p95).collect()),
        median_u64(stats.iter().map(|s| s.cycles_p99).collect()),
    );
}

fn reset_tail_run() -> TailStats {
    let mut harness = AudioHarness::new(480);
    harness.warm();
    let pool_blocks = deadsync_audio::ring::RING_CAP_SAMPLES.div_ceil(BLOCK_FRAMES * CHANNELS);
    let stale_blocks = pool_blocks - harness.callback_frames.div_ceil(BLOCK_FRAMES);
    harness.prepare_reset(stale_blocks);
    harness.finish_reset(true);

    let mut cycles = Vec::with_capacity(RESET_CALLBACKS);
    let before = AllocSnap::now();
    for _ in 0..RESET_CALLBACKS {
        harness.prepare_reset(stale_blocks);
        let start = thread_cycles();
        harness.finish_reset(false);
        cycles.push(thread_cycles() - start);
    }
    AllocSnap::now().since(before).assert_idle();

    let mut nanos = Vec::with_capacity(RESET_CALLBACKS);
    let before = AllocSnap::now();
    for _ in 0..RESET_CALLBACKS {
        harness.prepare_reset(stale_blocks);
        let start = Instant::now();
        harness.finish_reset(false);
        nanos.push(start.elapsed().as_nanos() as u64);
    }
    AllocSnap::now().since(before).assert_idle();

    cycles.sort_unstable();
    nanos.sort_unstable();
    TailStats {
        ns_p50: percentile(&nanos, 0.50),
        ns_p95: percentile(&nanos, 0.95),
        ns_p99: percentile(&nanos, 0.99),
        cycles_p50: percentile(&cycles, 0.50),
        cycles_p95: percentile(&cycles, 0.95),
        cycles_p99: percentile(&cycles, 0.99),
    }
}

fn reset_tails() {
    let stats: Vec<_> = (0..RUNS).map(|_| reset_tail_run()).collect();
    println!(
        "reset 126 stale blocks + 480f: ns p50={} p95={} p99={}; cycles p50={} p95={} p99={}; steady alloc/realloc/dealloc=0/0/0",
        median_u64(stats.iter().map(|s| s.ns_p50).collect()),
        median_u64(stats.iter().map(|s| s.ns_p95).collect()),
        median_u64(stats.iter().map(|s| s.ns_p99).collect()),
        median_u64(stats.iter().map(|s| s.cycles_p50).collect()),
        median_u64(stats.iter().map(|s| s.cycles_p95).collect()),
        median_u64(stats.iter().map(|s| s.cycles_p99).collect()),
    );
}

fn pipeline_run(callback_frames: usize) -> RateStats {
    let mut harness = AudioHarness::new(callback_frames);
    harness.warm();
    let before = AllocSnap::now();
    let cycle_start = thread_cycles();
    let start = Instant::now();
    for _ in 0..PIPE_CALLBACKS {
        harness.step();
    }
    let elapsed = start.elapsed();
    let cycles = thread_cycles() - cycle_start;
    AllocSnap::now().since(before).assert_idle();
    RateStats {
        ns_per_callback: elapsed.as_nanos() as f64 / PIPE_CALLBACKS as f64,
        cycles_per_callback: cycles as f64 / PIPE_CALLBACKS as f64,
    }
}

fn pipeline_throughput() {
    let stats: Vec<_> = (0..RUNS).map(|_| pipeline_run(480)).collect();
    let ns = median_f64(stats.iter().map(|s| s.ns_per_callback).collect());
    let cycles = median_f64(stats.iter().map(|s| s.cycles_per_callback).collect());
    let callbacks_per_sec = 1e9 / ns;
    let samples_per_sec = callbacks_per_sec * (480 * CHANNELS) as f64;
    println!(
        "pipeline 480f: {callbacks_per_sec:.1} callbacks/s, {:.1} Msamples/s, {ns:.1} ns/callback, {cycles:.1} cycles/callback; steady alloc/realloc/dealloc=0/0/0",
        samples_per_sec / 1e6,
    );
}

fn callback_rate_run(callback_frames: usize) -> RateStats {
    let mut harness = AudioHarness::new(callback_frames);
    harness.warm();

    let before = AllocSnap::now();
    let mut cycles = 0;
    for _ in 0..SCALE_CALLBACKS {
        harness.submit_blocks();
        let start = thread_cycles();
        harness.render_callback();
        cycles += thread_cycles() - start;
        harness.drain_maps(false);
    }
    AllocSnap::now().since(before).assert_idle();

    let before = AllocSnap::now();
    let mut nanos = 0;
    for _ in 0..SCALE_CALLBACKS {
        harness.submit_blocks();
        let start = Instant::now();
        harness.render_callback();
        nanos += start.elapsed().as_nanos();
        harness.drain_maps(false);
    }
    AllocSnap::now().since(before).assert_idle();
    RateStats {
        ns_per_callback: nanos as f64 / SCALE_CALLBACKS as f64,
        cycles_per_callback: cycles as f64 / SCALE_CALLBACKS as f64,
    }
}

fn callback_scaling() {
    for callback_frames in [64, 128, 256, 480, 512, 1024] {
        let stats: Vec<_> = (0..RUNS)
            .map(|_| callback_rate_run(callback_frames))
            .collect();
        println!(
            "callback {callback_frames:4}f: {:7.1} ns, {:8.1} cycles",
            median_f64(stats.iter().map(|s| s.ns_per_callback).collect()),
            median_f64(stats.iter().map(|s| s.cycles_per_callback).collect()),
        );
    }
}

fn main() {
    println!("audio blocks: 48 kHz stereo, 256-frame producer blocks, f32 output, no SFX");
    cold_memory();
    callback_tails();
    reset_tails();
    pipeline_throughput();
    callback_scaling();
}

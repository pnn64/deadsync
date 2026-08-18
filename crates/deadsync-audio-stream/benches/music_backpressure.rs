use deadlib_audio_core::{
    CallbackClockSource, CallbackInfo, MixControls, OutputBufferMut, QueuedSfx, RenderState,
    activate_music_track, bump_music_map_generation, music_transport,
    reset_music_stream_clock_state, stop_music_track,
};
use deadsync_audio_stream::{
    Cut, MusicBackpressureBenchmarkMode, MusicDecodeContext, OutputFormat,
    spawn_music_decoder_thread_for_benchmark,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: usize = 2;
const CALLBACK_FRAMES: usize = 512;
const WARMUP_FRAMES: usize = 94 * CALLBACK_FRAMES;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn begin(&self) {
        self.allocs.store(0, Ordering::Relaxed);
        self.reallocs.store(0, Ordering::Relaxed);
        self.bytes.store(0, Ordering::Relaxed);
        self.enabled.store(true, Ordering::Release);
    }

    fn end(&self) -> AllocStats {
        self.enabled.store(false, Ordering::Release);
        AllocStats {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every operation delegates to `System` with the caller's unchanged
// pointer and layout. Relaxed atomics only observe successful allocations.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplied `layout`.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the pointer/layout pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: all arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            self.bytes.fetch_add(new_size as u64, Ordering::Relaxed);
        }
        out
    }
}

#[derive(Clone, Copy)]
struct AllocStats {
    allocs: u64,
    reallocs: u64,
    bytes: u64,
}

struct RunResult {
    label: &'static str,
    elapsed: Duration,
    worker_cycles: Option<u64>,
    waits: u64,
    transition_pushes: u64,
    max_outstanding: u64,
    underrun_callbacks: usize,
    first_underrun_sec: Option<f64>,
    last_underrun_sec: Option<f64>,
    render_p99: Duration,
    render_max: Duration,
    render_max_at_sec: f64,
    allocations: AllocStats,
}

fn main() {
    let path = std::env::var_os("DEADSYNC_BENCH_SONG")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            panic!("set DEADSYNC_BENCH_SONG to a long audio file before running this benchmark")
        });
    let seconds = std::env::var("DEADSYNC_BENCH_SECONDS")
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 2.0)
        .unwrap_or(8.0);
    warm_file_cache(&path);
    let parker_allocations = probe_parker_allocations();
    println!(
        "park_timeout probe (100 waits after warmup): alloc={}/{}/{}B",
        parker_allocations.allocs, parker_allocations.reallocs, parker_allocations.bytes
    );

    let variants = [
        (
            "fixed 300 us",
            MusicBackpressureBenchmarkMode::Fixed300Micros,
        ),
        ("fixed 2 ms", MusicBackpressureBenchmarkMode::Fixed2Millis),
        (
            "occupancy deadline",
            MusicBackpressureBenchmarkMode::OccupancyDeadline,
        ),
        (
            "occupancy sleep",
            MusicBackpressureBenchmarkMode::OccupancySleep,
        ),
    ];
    let results: Vec<_> = variants
        .into_iter()
        .map(|(label, mode)| run_variant(label, mode, &path, seconds))
        .collect();

    println!(
        "music backpressure: {} ({seconds:.1}s rendered, {SAMPLE_RATE} Hz, {CALLBACK_FRAMES}-frame callbacks)",
        path.display()
    );
    for result in &results {
        println!(
            "{:<20} wall={:>6.3}s cycles={:>12} waits={:>7} transition={:>3} max_q={:>3} underrun_cb={:>3} underrun_at={:>9} render_p99={:>7.2}us render_max={:>7.2}us@{:>4.2}s alloc={}/{}/{}B",
            result.label,
            result.elapsed.as_secs_f64(),
            result
                .worker_cycles
                .map_or_else(|| "n/a".to_owned(), |cycles| cycles.to_string()),
            result.waits,
            result.transition_pushes,
            result.max_outstanding,
            result.underrun_callbacks,
            match (result.first_underrun_sec, result.last_underrun_sec) {
                (Some(first), Some(last)) => format!("{first:.2}-{last:.2}s"),
                _ => "-".to_owned(),
            },
            result.render_p99.as_secs_f64() * 1e6,
            result.render_max.as_secs_f64() * 1e6,
            result.render_max_at_sec,
            result.allocations.allocs,
            result.allocations.reallocs,
            result.allocations.bytes,
        );
    }
}

fn probe_parker_allocations() -> AllocStats {
    thread::spawn(|| {
        thread::park_timeout(Duration::from_millis(1));
        ALLOC.begin();
        for _ in 0..100 {
            thread::park_timeout(Duration::from_millis(1));
        }
        ALLOC.end()
    })
    .join()
    .expect("parker allocation probe did not panic")
}

fn run_variant(
    label: &'static str,
    mode: MusicBackpressureBenchmarkMode,
    path: &Path,
    seconds: f64,
) -> RunResult {
    stop_music_track();
    reset_music_stream_clock_state();
    let generation = bump_music_map_generation();
    let (stream, render_handle) = music_transport(SAMPLE_RATE, CHANNELS);
    let mut render = RenderState::new(render_handle, Arc::new(MixControls::new()), CHANNELS);
    let music = spawn_music_decoder_thread_for_benchmark(
        path.to_owned(),
        Cut {
            start_sec: 0.0,
            length_sec: f64::INFINITY,
            fade_in_sec: 0.0,
            fade_out_sec: 0.0,
        },
        false,
        1.0,
        false,
        stream.writer,
        MusicDecodeContext {
            output: OutputFormat {
                sample_rate_hz: SAMPLE_RATE,
                channels: CHANNELS,
            },
            generation,
        },
        mode,
    );

    thread::sleep(Duration::from_millis(50));
    activate_music_track();
    let callbacks = ((seconds * f64::from(SAMPLE_RATE)).round() as usize).div_ceil(CALLBACK_FRAMES);
    let mut render_times = Vec::with_capacity(callbacks);
    let mut output = vec![0i16; CALLBACK_FRAMES * CHANNELS];
    let mut rendered_frames = 0usize;
    let warmup_frames = WARMUP_FRAMES;
    let measured_frames = (seconds * f64::from(SAMPLE_RATE)).round() as usize;
    let target_frames = warmup_frames + measured_frames;
    let playback_started = Instant::now();
    let mut measured_started = None;
    let mut cycle_start = None;
    let mut underrun_callbacks = 0;
    let mut first_underrun_sec = None;
    let mut last_underrun_sec = None;
    let mut render_max = Duration::ZERO;
    let mut render_max_at_sec = 0.0;
    let transition_frame = warmup_frames + measured_frames / 2;
    let mut transitioned = false;
    while rendered_frames < target_frames {
        if rendered_frames >= warmup_frames && measured_started.is_none() {
            music.reset_backpressure_stats_for_benchmark();
            cycle_start = query_thread_cycles(&music.thread);
            ALLOC.begin();
            measured_started = Some(Instant::now());
        }
        let frames = CALLBACK_FRAMES.min(target_frames - rendered_frames);
        let callback_at = playback_started + frames_duration(rendered_frames);
        let now = Instant::now();
        if callback_at > now {
            thread::sleep(callback_at - now);
        }
        let render_started = Instant::now();
        let report = render.render(
            OutputBufferMut::I16(&mut output[..frames * CHANNELS]),
            CallbackInfo {
                anchor_nanos: playback_started
                    .elapsed()
                    .as_nanos()
                    .min(u128::from(u64::MAX)) as u64,
                clock: CallbackClockSource::Instant,
            },
            std::iter::empty::<QueuedSfx>(),
        );
        if rendered_frames >= warmup_frames {
            let render_elapsed = render_started.elapsed();
            let offset_sec = (rendered_frames - warmup_frames) as f64 / f64::from(SAMPLE_RATE);
            render_times.push(render_elapsed);
            if render_elapsed > render_max {
                render_max = render_elapsed;
                render_max_at_sec = offset_sec;
            }
            underrun_callbacks += usize::from(report.output_underrun);
            if report.output_underrun {
                first_underrun_sec.get_or_insert(offset_sec);
                last_underrun_sec = Some(offset_sec);
            }
        }
        rendered_frames += frames;
        if !transitioned && rendered_frames >= transition_frame {
            let generation = bump_music_map_generation();
            music.set_rate_for_benchmark(1.25, generation);
            transitioned = true;
        }
    }
    stop_music_track();
    music.stop_for_benchmark();
    let finish_deadline = Instant::now() + Duration::from_secs(5);
    while !music.thread.is_finished() && Instant::now() < finish_deadline {
        thread::sleep(Duration::from_millis(1));
    }
    assert!(music.thread.is_finished(), "{label} decoder did not stop");
    let cycle_end = query_thread_cycles(&music.thread);
    let stats = music.backpressure_stats();
    let writer = music
        .thread
        .join()
        .expect("benchmark decoder did not panic");
    let allocations = ALLOC.end();
    drop(writer);
    drop(render);
    reset_music_stream_clock_state();

    render_times.sort_unstable();
    RunResult {
        label,
        elapsed: measured_started
            .expect("benchmark entered measured phase")
            .elapsed(),
        worker_cycles: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.saturating_sub(start)),
        waits: stats.waits,
        transition_pushes: stats.transition_pushes,
        max_outstanding: stats.max_outstanding,
        underrun_callbacks,
        first_underrun_sec,
        last_underrun_sec,
        render_p99: render_times[(render_times.len() * 99 / 100).min(render_times.len() - 1)],
        render_max,
        render_max_at_sec,
        allocations,
    }
}

fn frames_duration(frames: usize) -> Duration {
    Duration::from_secs_f64(frames as f64 / f64::from(SAMPLE_RATE))
}

fn warm_file_cache(path: &Path) {
    let mut file = File::open(path).expect("benchmark song opens");
    let mut buffer = vec![0; 1 << 20];
    while file.read(&mut buffer).expect("benchmark song reads") != 0 {}
}

#[cfg(windows)]
fn query_thread_cycles(
    thread: &thread::JoinHandle<deadlib_audio_core::MusicBlockWriter>,
) -> Option<u64> {
    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn QueryThreadCycleTime(thread: *mut c_void, cycles: *mut u64) -> i32;
    }

    let mut cycles = 0;
    // SAFETY: the join handle remains alive for the call and `cycles` is a
    // writable output pointer of the required type.
    let ok = unsafe { QueryThreadCycleTime(thread.as_raw_handle().cast::<c_void>(), &mut cycles) };
    (ok != 0).then_some(cycles)
}

#[cfg(not(windows))]
fn query_thread_cycles(
    _thread: &thread::JoinHandle<deadlib_audio_core::MusicBlockWriter>,
) -> Option<u64> {
    None
}

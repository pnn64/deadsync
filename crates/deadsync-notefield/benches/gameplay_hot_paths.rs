use deadsync_notefield::{
    CameraWrapBench, CommonNoteTransformBench, CueScanBench, FeedbackLaneCacheBench,
    HoldTravelReuseBench, IdentityAccelBench, LaneVisualCacheBench, MeasureLineMode,
    MeasureLinePlanBench, MeasureLineTraversalBench, NotefieldPrepBench, TapExplosionCullBench,
    VisibleLaneCursorBench, VisibleRangeBench, XmodTimingBench,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const WARMUP_FRAMES: usize = 128;
const MEASURE_FRAMES: usize = 2_000;

struct CountingAlloc {
    allocs: AtomicU64,
    reallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: allocation requests are forwarded to `System` unchanged. The
// independent atomics only observe successful allocation and growth calls.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the allocator caller supplied this exact layout.
        let out = unsafe { System.alloc(layout) };
        if !out.is_null() {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        out
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the allocator caller guarantees this pointer/layout is live.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the allocator caller supplied the live pointer and old layout.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            if new_size > old.size() {
                self.bytes
                    .fetch_add((new_size - old.size()) as u64, Ordering::Relaxed);
            }
        }
        out
    }
}

#[derive(Clone, Copy, Default)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    bytes: u64,
}

impl AllocSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            bytes: self.bytes - before.bytes,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Output {
    checksum: u64,
    samples: usize,
}

struct BenchResult {
    elapsed: Duration,
    cycles: u64,
    alloc: AllocSnapshot,
    frame_ns: Vec<u64>,
    output: Output,
}

fn main() {
    println!("gameplay notefield hot-path microbenchmarks");

    let preparation = NotefieldPrepBench::default();
    run_pair(
        "uniform reverse preparation",
        "256 x 8-lane preparations with Reverse only and no lane-varying modifiers",
        |frame| {
            let output = preparation.old_reverse_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
        |frame| {
            let output = preparation.new_reverse_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
    );
    run_pair(
        "empty song-Lua column offsets",
        "256 x 8-lane preparations without song-defined column offset windows",
        |frame| {
            let output = preparation.old_song_lua_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
        |frame| {
            let output = preparation.new_song_lua_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
    );
    run_pair(
        "inactive invert/tornado preparation",
        "256 x 8-lane preparations with ordinary horizontal effects",
        |frame| {
            let output = preparation.old_geometry_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
        |frame| {
            let output = preparation.new_geometry_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
    );

    let tap_explosions = TapExplosionCullBench::default();
    run_pair(
        "idle tap-explosion Lua culling",
        "8 lanes, 256 note-hide windows, one explosion every 257 frames",
        |frame| {
            let output = tap_explosions.old_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
        |frame| {
            let output = tap_explosions.new_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
    );

    let xmod = XmodTimingBench::default();
    run_pair(
        "X/M displayed-beat cache",
        "96 visible notes, 1024 scroll segments",
        |frame| {
            let output = xmod.old_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
        |frame| {
            let output = xmod.new_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
    );

    let cues = CueScanBench::default();
    run_pair(
        "visible timing-cue index",
        "8192 BPM + 8192 scroll segments, stops and delays",
        |frame| {
            let output = cues.old_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
        |frame| {
            let output = cues.new_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
    );

    for (segments, fixture) in [
        (1, "one time-signature segment"),
        (256, "256 time-signature segments"),
        (8_192, "8192 time-signature segments"),
    ] {
        let measure_plan = MeasureLinePlanBench::with_segment_count(segments);
        run_pair(
            "normal-play measure-line planning",
            fixture,
            |frame| {
                let output = measure_plan.old_frame(frame);
                Output {
                    checksum: output.checksum,
                    samples: output.samples,
                }
            },
            |frame| {
                let output = measure_plan.new_frame(frame);
                Output {
                    checksum: output.checksum,
                    samples: output.samples,
                }
            },
        );
    }

    for (mode, fixture) in [
        (
            MeasureLineMode::Measure,
            "measure-only bars over a normal XMod visibility window",
        ),
        (
            MeasureLineMode::Quarter,
            "quarter-note bars over a normal XMod visibility window",
        ),
    ] {
        let traversal = MeasureLineTraversalBench::new(mode);
        run_pair(
            "visible measure-line subdivision",
            fixture,
            |frame| {
                let output = traversal.old_frame(frame);
                Output {
                    checksum: output.checksum,
                    samples: output.samples,
                }
            },
            |frame| {
                let output = traversal.new_frame(frame);
                Output {
                    checksum: output.checksum,
                    samples: output.samples,
                }
            },
        );
    }

    let camera_wrap = CameraWrapBench::default();
    run_pair(
        "notefield camera wrapping",
        "384 field actors in a reusable allocation-free buffer",
        |frame| {
            let output = camera_wrap.old_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
        |frame| {
            let output = camera_wrap.new_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
    );

    let visible_range = VisibleRangeBench::default();
    run_pair(
        "visible-range note-count bound",
        "8192 chart-density entries, regular and cue ranges",
        |frame| {
            let output = visible_range.old_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
        |frame| {
            let output = visible_range.new_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
    );

    let old_lane_windows = VisibleLaneCursorBench::default();
    let mut new_lane_windows = old_lane_windows.clone();
    run_pair(
        "visible lane-window cursors",
        "8192 notes per lane, 4 tap and hold lanes during 120 Hz playback with seeks",
        |frame| {
            let output = old_lane_windows.old_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
        move |frame| {
            let output = new_lane_windows.new_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
    );

    let lane_visuals = LaneVisualCacheBench::default();
    run_pair(
        "per-lane visual preparation",
        "96 visible note/hold entries across 4 lanes with column effects",
        |frame| {
            let output = lane_visuals.old_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
        |frame| {
            let output = lane_visuals.new_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
    );

    let feedback_lanes = FeedbackLaneCacheBench::default();
    run_pair(
        "notefield feedback lane reuse",
        "4 receptors with active tap and mine feedback passes",
        |frame| {
            let output = feedback_lanes.old_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
        |frame| {
            let output = feedback_lanes.new_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
    );

    let common_transforms = CommonNoteTransformBench::default();
    run_pair(
        "identity appearance fast path",
        "96 visible notes with ordinary appearance settings",
        |frame| {
            let output = common_transforms.old_appearance_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
        |frame| {
            let output = common_transforms.new_appearance_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
    );
    run_pair(
        "frame-cached identity appearance",
        "96 visible notes with ordinary actor-alpha and glow output",
        |frame| {
            let output = common_transforms.old_frame_identity_appearance(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
        |frame| {
            let output = common_transforms.new_frame_identity_appearance(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
    );
    run_pair(
        "cached ordinary hold visibility",
        "96 hold body/head samples without appearance modifiers",
        |frame| {
            let output = common_transforms.old_identity_hold_appearance_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
        |frame| {
            let output = common_transforms.new_identity_hold_appearance_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
    );
    run_pair(
        "shared hold visibility evaluation",
        "96 hold body/head samples with active appearance modifiers",
        |frame| {
            let output = common_transforms.old_hold_appearance_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
        |frame| {
            let output = common_transforms.new_hold_appearance_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
    );
    run_pair(
        "identity note-rotation fast path",
        "96 visible notes without confusion or dizzy effects",
        |frame| {
            let output = common_transforms.old_rotation_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
        |frame| {
            let output = common_transforms.new_rotation_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
    );
    run_pair(
        "static horizontal lane placement",
        "96 visible notes across 4 lanes with static flip/invert/move",
        |frame| {
            let output = common_transforms.old_static_x_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
        |frame| {
            let output = common_transforms.new_static_x_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
    );

    let identity_accel = IdentityAccelBench::default();
    run_pair(
        "identity scroll acceleration",
        "96 visible note and hold travel samples without acceleration modifiers",
        |frame| {
            let output = identity_accel.old_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
        |frame| {
            let output = identity_accel.new_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
    );
    let hold_travel = HoldTravelReuseBench::default();
    run_pair(
        "hold-head travel reuse",
        "24 visible holds with active acceleration effects",
        |frame| {
            let output = hold_travel.old_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
        |frame| {
            let output = hold_travel.new_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
    );
    run_pair(
        "hold-head travel reuse without acceleration",
        "24 visible holds with ordinary scroll travel",
        |frame| {
            let output = hold_travel.old_identity_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
        |frame| {
            let output = hold_travel.new_identity_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
    );
    run_pair(
        "cached lane zoom",
        "96 visible notes across 4 lanes with Tiny and no Pulse",
        |frame| {
            let output = common_transforms.old_lane_zoom_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
        |frame| {
            let output = common_transforms.new_lane_zoom_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
    );
    run_pair(
        "cached static lane rotation",
        "96 visible notes across 4 lanes with static Confusion offsets",
        |frame| {
            let output = common_transforms.old_lane_rotation_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
        |frame| {
            let output = common_transforms.new_lane_rotation_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
    );
    run_pair(
        "common lane transform cache",
        "96 visible notes across 4 lanes without visual transform modifiers",
        |frame| {
            let output = common_transforms.old_identity_lane_cache_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
        |frame| {
            let output = common_transforms.new_identity_lane_cache_frame(frame);
            Output {
                checksum: output.checksum,
                samples: output.samples,
            }
        },
    );
}

fn run_pair(
    name: &str,
    fixture: &str,
    mut old_frame: impl FnMut(usize) -> Output,
    mut new_frame: impl FnMut(usize) -> Output,
) {
    let old = run(&mut old_frame);
    let new = run(&mut new_frame);
    assert_eq!(old.output, new.output, "{name} output checksum mismatch");
    println!("\n{name}\n  {fixture}, {MEASURE_FRAMES} frames");
    print_result("old", &old);
    print_result("new", &new);
    println!(
        "  speedup {:.2}x | cycles reduction {:.1}%",
        old.elapsed.as_secs_f64() / new.elapsed.as_secs_f64(),
        100.0 * (1.0 - new.cycles as f64 / old.cycles as f64),
    );
}

fn run(frame: &mut impl FnMut(usize) -> Output) -> BenchResult {
    for index in 0..WARMUP_FRAMES {
        black_box(frame(index));
    }
    let mut frame_ns = Vec::with_capacity(MEASURE_FRAMES);
    let before_alloc = ALLOC.snapshot();
    let before_cycles = read_cycles();
    let started = Instant::now();
    let mut output = Output::default();
    for index in WARMUP_FRAMES..WARMUP_FRAMES + MEASURE_FRAMES {
        let frame_started = Instant::now();
        let current = black_box(frame(index));
        frame_ns.push(frame_started.elapsed().as_nanos() as u64);
        output.checksum = output.checksum.rotate_left(11) ^ current.checksum;
        output.samples = output.samples.wrapping_add(current.samples);
    }
    BenchResult {
        elapsed: started.elapsed(),
        cycles: read_cycles().saturating_sub(before_cycles),
        alloc: ALLOC.snapshot().delta(before_alloc),
        frame_ns,
        output,
    }
}

fn print_result(name: &str, result: &BenchResult) {
    let frames = MEASURE_FRAMES as f64;
    let mut samples = result.frame_ns.clone();
    samples.sort_unstable();
    println!(
        "  {name:<4} {:>10.1} ns/frame {:>10.0} cycles/frame {:>10.0} frames/s",
        result.elapsed.as_secs_f64() * 1.0e9 / frames,
        result.cycles as f64 / frames,
        frames / result.elapsed.as_secs_f64(),
    );
    println!(
        "       p50 {:>8} ns p95 {:>8} ns p99 {:>8} ns worst {:>8} ns",
        percentile(&samples, 50),
        percentile(&samples, 95),
        percentile(&samples, 99),
        samples.last().copied().unwrap_or_default(),
    );
    println!(
        "       allocs={} reallocs={} bytes={}",
        result.alloc.allocs, result.alloc.reallocs, result.alloc.bytes,
    );
}

fn percentile(samples: &[u64], percentile: usize) -> u64 {
    let index = samples.len().saturating_mul(percentile).saturating_sub(1) / 100;
    samples.get(index).copied().unwrap_or_default()
}

#[cfg(target_arch = "x86_64")]
fn read_cycles() -> u64 {
    // SAFETY: LFENCE/RDTSC only serialize and read this thread's timestamp
    // counter; they do not dereference memory.
    unsafe {
        core::arch::x86_64::_mm_lfence();
        let cycles = core::arch::x86_64::_rdtsc();
        core::arch::x86_64::_mm_lfence();
        cycles
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn read_cycles() -> u64 {
    0
}

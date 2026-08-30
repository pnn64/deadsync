use deadlib_render_core::{BackendType, ClockDomainTrace, PresentModeTrace};
use deadsync_config::frame_pacing::StutterSampleRing;
use deadsync_config::prelude::{LogLevel, VersionOverlaySide};
use deadsync_theme_simply_love::i18n;
use deadsync_theme_simply_love::screens::components::shared::frame_stats_overlay::{
    benchmark_build_legacy as benchmark_frame_stats_build_legacy, push as push_frame_stats,
};
use deadsync_theme_simply_love::screens::components::shared::stats_overlay::{
    benchmark_build_legacy as benchmark_stats_build_legacy, benchmark_build_stutter_legacy,
    benchmark_timing_text_current, benchmark_timing_text_legacy, push as push_stats, push_stutter,
};
use deadsync_theme_simply_love::screens::components::shared::timers::TimerText;
use deadsync_theme_simply_love::screens::components::shared::{gamepad_overlay, version_overlay};
use deadsync_theme_simply_love::screens::evaluation_summary::{
    benchmark_eval_numeric_text, benchmark_profile_name_changed,
};
use deadsync_theme_simply_love::screens::mappings::MappingTextBenchmark;
use deadsync_theme_simply_love::screens::options::QrOverlayBenchmark;
use deadsync_theme_simply_love::screens::options::ScoreImportPickerBenchmark;
use deadsync_theme_simply_love::screens::pad_config::{
    benchmark_pad_text_current, benchmark_pad_text_legacy,
};
use deadsync_theme_simply_love::screens::player_options::PlayerOptionsSearchBenchmark;
use deadsync_theme_simply_love::screens::practice::benchmark_edit_info_text_into;
use deadsync_theme_simply_love::screens::select_color::{
    benchmark_wheel_current, benchmark_wheel_legacy,
};
use deadsync_theme_simply_love::screens::select_mode::SelectModeTextBenchmark;
use deadsync_theme_simply_love::screens::select_music::{
    benchmark_info_text_front_cached, benchmark_info_text_hashed,
};
use deadsync_theme_simply_love::screens::test_lights::LightsTextBenchmark;
use deadsync_theme_simply_love::views::{
    AudioTimingView, FrameStatsSample, FrameStatsSummary, OverlayAnchor, OverlayStyle,
    TimingHealth, VisibleStutterSample,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashSet;
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc::new();

const PROFILE_OPS: usize = 500_000;
const NUMERIC_OPS: usize = 500_000;
const TIMER_OPS: usize = 500_000;
const TRANSLATION_OPS: usize = 500_000;
const PRACTICE_OPS: usize = 300_000;
const INFO_TEXT_OPS: usize = 1_000_000;
const MAPPING_TEXT_OPS: usize = 100_000;
const TIMING_TEXT_OPS: usize = 200_000;
const OVERLAY_ACTOR_OPS: usize = 200_000;
const SELECT_COLOR_WHEEL_OPS: usize = 200_000;
const STUTTER_FILTER_OPS: usize = 500_000;
const FRAME_STATS_OVERLAY_OPS: usize = 5_000;
const PAD_TEXT_OPS: usize = 100_000;
const SELECT_MODE_TEXT_OPS: usize = 500_000;
const SCORE_PICKER_OPS: usize = 50_000;
const LIGHTS_TEXT_OPS: usize = 300_000;
const OPTIONS_SEARCH_OPS: usize = 200_000;
const QR_OVERLAY_OPS: usize = 25_000;
const SMALL_OVERLAY_OPS: usize = 300_000;

struct CountingAlloc {
    enabled: AtomicBool,
    allocs: AtomicU64,
    reallocs: AtomicU64,
    deallocs: AtomicU64,
    bytes: AtomicU64,
}

impl CountingAlloc {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            allocs: AtomicU64::new(0),
            reallocs: AtomicU64::new(0),
            deallocs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> AllocSnapshot {
        AllocSnapshot {
            allocs: self.allocs.load(Ordering::Relaxed),
            reallocs: self.reallocs.load(Ordering::Relaxed),
            deallocs: self.deallocs.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every operation delegates unchanged to `System`; relaxed counters
// only observe successful calls while measurement is enabled.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `layout` comes from the allocator caller.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.allocs.fetch_add(1, Ordering::Relaxed);
            self.bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.enabled.load(Ordering::Relaxed) {
            self.deallocs.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: this pair came from the delegated allocator.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: arguments are forwarded unchanged to `System`.
        let out = unsafe { System.realloc(ptr, old, new_size) };
        if !out.is_null() && self.enabled.load(Ordering::Relaxed) {
            self.reallocs.fetch_add(1, Ordering::Relaxed);
            if new_size > old.size() {
                self.bytes
                    .fetch_add((new_size - old.size()) as u64, Ordering::Relaxed);
            }
        }
        out
    }
}

#[derive(Clone, Copy)]
struct AllocSnapshot {
    allocs: u64,
    reallocs: u64,
    deallocs: u64,
    bytes: u64,
}

impl AllocSnapshot {
    const fn delta(self, before: Self) -> Self {
        Self {
            allocs: self.allocs - before.allocs,
            reallocs: self.reallocs - before.reallocs,
            deallocs: self.deallocs - before.deallocs,
            bytes: self.bytes - before.bytes,
        }
    }
}

struct BenchResult {
    ns_per_op: f64,
    worst_sample_ns: f64,
    cycles_per_op: Option<f64>,
    allocated: AllocSnapshot,
    checksum: u64,
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

fn measure(iterations: usize, sample_ops: usize, mut op: impl FnMut() -> u64) -> BenchResult {
    for _ in 0..iterations.min(2_000) {
        black_box(op());
    }

    let cycle_start = cycle_counter();
    let started = Instant::now();
    let mut checksum = 0u64;
    let mut worst_sample_ns = 0.0f64;
    for _ in 0..(iterations / sample_ops) {
        let sample_started = Instant::now();
        for _ in 0..sample_ops {
            checksum = checksum.wrapping_add(black_box(op()));
        }
        worst_sample_ns = worst_sample_ns
            .max(sample_started.elapsed().as_secs_f64() * 1_000_000_000.0 / sample_ops as f64);
    }
    let elapsed = started.elapsed();
    let cycle_end = cycle_counter();

    let before = ALLOC.snapshot();
    ALLOC.enabled.store(true, Ordering::Relaxed);
    let mut allocation_checksum = 0u64;
    for _ in 0..iterations {
        allocation_checksum = allocation_checksum.wrapping_add(black_box(op()));
    }
    ALLOC.enabled.store(false, Ordering::Relaxed);
    let allocated = ALLOC.snapshot().delta(before);
    black_box(allocation_checksum);

    BenchResult {
        ns_per_op: elapsed.as_secs_f64() * 1_000_000_000.0 / iterations as f64,
        worst_sample_ns,
        cycles_per_op: cycle_start
            .zip(cycle_end)
            .map(|(start, end)| end.wrapping_sub(start) as f64 / iterations as f64),
        allocated,
        checksum,
    }
}

fn print_result(label: &str, iterations: usize, result: &BenchResult) {
    let ops = iterations as f64;
    let churn = result.allocated.allocs + result.allocated.reallocs + result.allocated.deallocs;
    println!(
        "{label:<12} {:>10.2} ns/op  {:>10.2} cycles/op  {:>10.2} worst ns  \
         {:>8.3} Mop/s  {:>7.2} alloc  {:>7.2} realloc  {:>7.2} free  \
         {:>7.2} churn  {:>10.1} B/op",
        result.ns_per_op,
        result.cycles_per_op.unwrap_or(f64::NAN),
        result.worst_sample_ns,
        1_000.0 / result.ns_per_op,
        result.allocated.allocs as f64 / ops,
        result.allocated.reallocs as f64 / ops,
        result.allocated.deallocs as f64 / ops,
        churn as f64 / ops,
        result.allocated.bytes as f64 / ops,
    );
}

fn print_pair(title: &str, iterations: usize, old: &BenchResult, new: &BenchResult) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    assert_eq!(new.allocated.allocs, 0, "{title} still allocates");
    assert_eq!(new.allocated.reallocs, 0, "{title} still reallocates");
    assert_eq!(new.allocated.deallocs, 0, "{title} still frees");
    assert_eq!(new.allocated.bytes, 0, "{title} still allocates bytes");
    println!("\n{title}");
    print_result("old", iterations, old);
    print_result("new", iterations, new);
    println!(
        "improvement  {:>8.2}x throughput  {:>8.2}% fewer allocated bytes",
        old.ns_per_op / new.ns_per_op,
        100.0 * (1.0 - new.allocated.bytes as f64 / old.allocated.bytes as f64),
    );
}

fn print_reduced_pair(title: &str, iterations: usize, old: &BenchResult, new: &BenchResult) {
    assert_eq!(old.checksum, new.checksum, "{title} behavior diverged");
    println!("\n{title}");
    print_result("old", iterations, old);
    print_result("new", iterations, new);
    println!(
        "improvement  {:>8.2}x throughput  {:>8.2}% fewer allocated bytes",
        old.ns_per_op / new.ns_per_op,
        100.0 * (1.0 - new.allocated.bytes as f64 / old.allocated.bytes as f64),
    );
    assert!(
        new.allocated.allocs < old.allocated.allocs,
        "{title} did not reduce allocations"
    );
    assert!(
        new.allocated.bytes < old.allocated.bytes,
        "{title} did not reduce allocated bytes"
    );
}

const fn timing_fixture() -> TimingHealth {
    TimingHealth {
        interval_ns: 16_666_667,
        display_error_ms: -0.42,
        display_catching_up: true,
        present_mode: PresentModeTrace::Fifo,
        display_clock: ClockDomainTrace::Device,
        host_clock: ClockDomainTrace::Monotonic,
        in_flight_images: 2,
        waited_for_image: true,
        applied_back_pressure: false,
        queue_idle_waited: false,
        suboptimal: false,
        submitted_present_id: 12_345,
        completed_present_id: 12_344,
        calibration_error_ns: 83_000,
        host_mapped: true,
        audio: Some(AudioTimingView {
            backend: "WASAPI",
            requested_output_mode: "exclusive",
            fallback_from_native: false,
            timing_clock: "device",
            timing_quality: "precise",
            sample_rate_hz: 48_000,
            device_period_ns: 2_666_667,
            stream_latency_ns: 5_333_334,
            buffer_frames: 256,
            padding_frames: 128,
            queued_frames: 384,
            estimated_output_delay_ns: 8_000_000,
            clock_fallback_count: 1,
            timing_sanity_failure_count: 2,
            underrun_count: 3,
        }),
    }
}

fn stutter_fixture() -> [VisibleStutterSample; 5] {
    std::array::from_fn(|index| VisibleStutterSample {
        timestamp_seconds: 61.25 + index as f32,
        frame_ms: 33.3 + index as f32,
        frame_multiple: (index as f32).mul_add(0.25, 2.0),
        severity: 1 + (index % 3) as u8,
        age_seconds: index as f32 * 0.4,
    })
}

fn stutter_ring_fixture() -> StutterSampleRing {
    let mut ring = StutterSampleRing::new();
    for index in 0..5 {
        ring.push(
            (index as f32).mul_add(0.25, 10.0),
            (index as f32).mul_add(0.003, 0.025),
            1.0 / 120.0,
            1 + (index % 3) as u8,
        );
    }
    ring
}

fn visible_stutter_checksum(samples: &[VisibleStutterSample]) -> u64 {
    samples
        .iter()
        .fold(samples.len() as u64, |checksum, sample| {
            checksum.rotate_left(7)
                ^ u64::from(sample.timestamp_seconds.to_bits())
                ^ u64::from(sample.frame_ms.to_bits()).rotate_left(13)
                ^ u64::from(sample.frame_multiple.to_bits()).rotate_left(23)
                ^ u64::from(sample.severity)
                ^ u64::from(sample.age_seconds.to_bits()).rotate_left(31)
        })
}

fn frame_stats_fixture() -> (Vec<FrameStatsSample>, FrameStatsSummary) {
    let samples = (0..128)
        .map(|index| FrameStatsSample {
            host_nanos: index + 1,
            frame_us: 8_000 + (index as u32 % 9) * 700,
            maintenance_us: 250,
            input_us: 200,
            update_us: 1_200,
            compose_us: 1_600,
            upload_us: 300,
            draw_us: 1_800,
            gpu_wait_us: 800,
            display_error_us: index as i32 * 10 - 600,
            catching_up: index % 31 == 0,
        })
        .collect();
    let summary = FrameStatsSummary {
        avg_frame_us: 8_333,
        p99_frame_us: 13_600,
        max_frame_us: 13_600,
        fps: 120.0,
        display_error_ms: -0.42,
        display_error_p99_ms: 1.2,
        display_catching_up: false,
        in_gameplay: true,
        audio_callback_gap_ms: 2.1,
        audio_underruns: 1,
        audio_output_delay_ms: 8.0,
        audio_queued_frames: 384,
        frame_jitter_us: 350,
        display_error_jitter_us: 220,
        spike_hold_us: 13_600,
        target_frame_us: 8_333,
        cpu_work_us: 5_350,
        gpu_wait_us: 800,
        over_budget_count: 0,
        catch_up_count: 4,
    };
    (samples, summary)
}

fn legacy_profile_name_changed(sides: [&[&str]; 2]) -> bool {
    sides
        .into_iter()
        .any(|names| names.iter().copied().collect::<HashSet<_>>().len() > 1)
}

fn legacy_eval_numeric_text(percent: f64, ex: f64, counts: &[u32; 8]) -> usize {
    let percent = format!("{percent:.2}");
    let ex = format!("{ex:.2}");
    let mut bytes = percent.len() + ex.len();
    for count in counts {
        bytes += count.to_string().len();
    }
    bytes
}

struct LegacyTimerText {
    second: u64,
    text: Arc<str>,
}

impl LegacyTimerText {
    fn new(second: u64) -> Self {
        Self {
            second,
            text: legacy_elapsed_text(second),
        }
    }

    fn sync(&mut self, second: u64) {
        if second == self.second {
            return;
        }
        self.second = second;
        self.text = legacy_elapsed_text(second);
    }
}

fn legacy_elapsed_text(second: u64) -> Arc<str> {
    let hours = second / 3600;
    let minutes = (second % 3600) / 60;
    let seconds = second % 60;
    if second < 3600 {
        format!("{minutes:02}:{seconds:02}").into()
    } else if second < 36000 {
        format!("{hours}:{minutes:02}:{seconds:02}").into()
    } else {
        format!("{hours:02}:{minutes:02}:{seconds:02}").into()
    }
}

fn text_checksum(text: &str) -> u64 {
    text.bytes().fold(text.len() as u64, |checksum, byte| {
        checksum.rotate_left(5) ^ u64::from(byte)
    })
}

fn overlay_actor_checksum(actors: &[deadlib_present::actors::Actor]) -> u64 {
    use deadlib_present::actors::Actor;

    actors.iter().fold(actors.len() as u64, |checksum, actor| {
        let value = match actor {
            Actor::Text {
                content, offset, z, ..
            } => {
                text_checksum(content)
                    ^ u64::from(offset[0].to_bits())
                    ^ u64::from(offset[1].to_bits()).rotate_left(13)
                    ^ u64::from(*z as u16).rotate_left(29)
            }
            Actor::Frame { children, .. } => overlay_actor_checksum(children),
            Actor::SharedFrame { children, .. } => overlay_actor_checksum(children),
            _ => 1,
        };
        checksum.rotate_left(11) ^ value
    })
}

fn texts_checksum(texts: [Arc<str>; 3]) -> u64 {
    texts.into_iter().fold(0u64, |checksum, text| {
        checksum.rotate_left(11) ^ text_checksum(&text)
    })
}

fn legacy_edit_info_text_into(
    out: &mut String,
    cursor_beat: f32,
    current_second: f32,
    selection_anchor: Option<f32>,
    selection_end: Option<f32>,
    suffix: &str,
) {
    let mut status = String::new();
    i18n::tr_fmt_into(
        &mut status,
        "Practice",
        "InfoCurrentBeat",
        &[("beat", &format!("{cursor_beat:.3}"))],
    );
    status.push('\n');
    i18n::tr_fmt_into(
        &mut status,
        "Practice",
        "InfoCurrentSecond",
        &[("sec", &format!("{current_second:.6}"))],
    );
    status.push('\n');
    i18n::tr_fmt_into(&mut status, "Practice", "InfoSnapTo", &[("snap", "16th")]);
    status.push('\n');
    let selection = match (selection_anchor, selection_end) {
        (Some(start), Some(stop)) if stop > start => {
            let mut text = String::new();
            i18n::tr_fmt_into(
                &mut text,
                "Practice",
                "InfoSelectionBeatRange",
                &[
                    ("start", &format!("{start:.3}")),
                    ("stop", &format!("{stop:.3}")),
                ],
            );
            Some(text)
        }
        _ => None,
    };
    if let Some(selection) = selection {
        status.push_str(&selection);
        status.push('\n');
    }
    status.push_str(suffix);
    out.clear();
    out.push_str(&status);
}

fn main() {
    i18n::init_for_tests();
    let p1 = ["Player One"; 12];
    let p2 = ["Player Two"; 12];
    let sides = [&p1[..], &p2[..]];
    let old_profile = measure(PROFILE_OPS, 500, || {
        u64::from(legacy_profile_name_changed(black_box(sides)))
    });
    let new_profile = measure(PROFILE_OPS, 500, || {
        u64::from(benchmark_profile_name_changed(black_box(sides)))
    });
    print_pair(
        "1. evaluation profile-name scan",
        PROFILE_OPS,
        &old_profile,
        &new_profile,
    );

    let counts = [20, 1_024, 3_456, 789, 12, 3, 0, 19];
    let old_numeric = measure(NUMERIC_OPS, 500, || {
        legacy_eval_numeric_text(black_box(98.76), black_box(97.53), black_box(&counts)) as u64
    });
    let new_numeric = measure(NUMERIC_OPS, 500, || {
        benchmark_eval_numeric_text(black_box(98.76), black_box(97.53), black_box(&counts)) as u64
    });
    print_pair(
        "2. evaluation numeric text",
        NUMERIC_OPS,
        &old_numeric,
        &new_numeric,
    );

    let mut old_second = 0_u64;
    let mut old_timer = LegacyTimerText::new(old_second);
    let old_timer_result = measure(TIMER_OPS, 500, || {
        old_second = (old_second + 1) % 36_000;
        old_timer.sync(old_second);
        text_checksum(&old_timer.text)
    });
    let mut new_second = 0_u64;
    let mut new_timer = TimerText::default();
    let new_timer_result = measure(TIMER_OPS, 500, || {
        new_second = (new_second + 1) % 36_000;
        new_timer.sync(new_second as f32);
        text_checksum(new_timer.text())
    });
    print_pair(
        "3. retained elapsed timer text",
        TIMER_OPS,
        &old_timer_result,
        &new_timer_result,
    );

    let args = [("remaining", "12"), ("s", "s")];
    let mut old_translation = String::with_capacity(96);
    let old_translation_result = measure(TRANSLATION_OPS, 500, || {
        old_translation.clear();
        old_translation.push_str(&i18n::tr_fmt("Lobby", "DisconnectHoldingFormat", &args));
        text_checksum(&old_translation)
    });
    let mut new_translation = String::with_capacity(96);
    let new_translation_result = measure(TRANSLATION_OPS, 500, || {
        new_translation.clear();
        i18n::tr_fmt_into(
            &mut new_translation,
            "Lobby",
            "DisconnectHoldingFormat",
            &args,
        );
        text_checksum(&new_translation)
    });
    print_pair(
        "4. translation into retained buffer",
        TRANSLATION_OPS,
        &old_translation_result,
        &new_translation_result,
    );

    let suffix = "Difficulty: Hard 12\n\nSteps: 1,024\nHolds: 128";
    let mut old_edit = String::new();
    let mut new_edit = String::new();
    legacy_edit_info_text_into(
        &mut old_edit,
        128.375,
        63.125,
        Some(96.0),
        Some(144.5),
        suffix,
    );
    benchmark_edit_info_text_into(
        &mut new_edit,
        128.375,
        63.125,
        Some(96.0),
        Some(144.5),
        3,
        suffix,
    );
    assert_eq!(old_edit, new_edit);
    let old_edit_result = measure(PRACTICE_OPS, 300, || {
        legacy_edit_info_text_into(
            &mut old_edit,
            128.375,
            63.125,
            Some(96.0),
            Some(144.5),
            suffix,
        );
        text_checksum(&old_edit)
    });
    let new_edit_result = measure(PRACTICE_OPS, 300, || {
        benchmark_edit_info_text_into(
            &mut new_edit,
            128.375,
            63.125,
            Some(96.0),
            Some(144.5),
            3,
            suffix,
        );
        text_checksum(&new_edit)
    });
    print_pair(
        "5. Practice edit-info rebuild",
        PRACTICE_OPS,
        &old_edit_result,
        &new_edit_result,
    );

    assert_eq!(
        benchmark_info_text_hashed(),
        benchmark_info_text_front_cached()
    );
    let old_info = measure(INFO_TEXT_OPS, 1_000, || {
        texts_checksum(benchmark_info_text_hashed())
    });
    let new_info = measure(INFO_TEXT_OPS, 1_000, || {
        texts_checksum(benchmark_info_text_front_cached())
    });
    print_pair(
        "6. Select Music stable info text",
        INFO_TEXT_OPS,
        &old_info,
        &new_info,
    );

    let mappings = MappingTextBenchmark::new();
    assert_eq!(mappings.legacy_checksum(), mappings.retained_checksum());
    let old_mappings = measure(MAPPING_TEXT_OPS, 100, || mappings.legacy_checksum());
    let new_mappings = measure(MAPPING_TEXT_OPS, 100, || mappings.retained_checksum());
    print_pair(
        "7. retained mappings labels",
        MAPPING_TEXT_OPS,
        &old_mappings,
        &new_mappings,
    );

    let timing = timing_fixture();
    assert_eq!(
        benchmark_timing_text_legacy(timing),
        benchmark_timing_text_current(timing)
    );
    let old_timing = measure(TIMING_TEXT_OPS, 200, || {
        text_checksum(&benchmark_timing_text_legacy(black_box(timing)))
    });
    let new_timing = measure(TIMING_TEXT_OPS, 200, || {
        text_checksum(&benchmark_timing_text_current(black_box(timing)))
    });
    print_reduced_pair(
        "8. one-pass timing telemetry",
        TIMING_TEXT_OPS,
        &old_timing,
        &new_timing,
    );

    let stutters = stutter_fixture();
    let mut old_actors = Vec::with_capacity(8);
    let old_overlay = measure(OVERLAY_ACTOR_OPS, 200, || {
        old_actors.clear();
        old_actors.extend(benchmark_stats_build_legacy(
            BackendType::OpenGL,
            120.0,
            42,
            None,
        ));
        old_actors.extend(benchmark_build_stutter_legacy(black_box(&stutters)));
        black_box(&old_actors);
        old_actors.len() as u64
    });
    let mut new_actors = Vec::with_capacity(8);
    let new_overlay = measure(OVERLAY_ACTOR_OPS, 200, || {
        new_actors.clear();
        push_stats(&mut new_actors, BackendType::OpenGL, 120.0, 42, None);
        push_stutter(&mut new_actors, black_box(&stutters));
        black_box(&new_actors);
        new_actors.len() as u64
    });
    print_pair(
        "9. direct overlay actor append",
        OVERLAY_ACTOR_OPS,
        &old_overlay,
        &new_overlay,
    );

    let mut old_wheel_actors = Vec::with_capacity(16);
    let old_wheel = measure(SELECT_COLOR_WHEEL_OPS, 200, || {
        let checksum = benchmark_wheel_legacy(&mut old_wheel_actors, true, 3.25);
        black_box(&old_wheel_actors);
        checksum
    });
    let mut new_wheel_actors = Vec::with_capacity(16);
    let new_wheel = measure(SELECT_COLOR_WHEEL_OPS, 200, || {
        let checksum = benchmark_wheel_current(&mut new_wheel_actors, true, 3.25);
        black_box(&new_wheel_actors);
        checksum
    });
    print_pair(
        "10. stack/direct Select Color wheel",
        SELECT_COLOR_WHEEL_OPS,
        &old_wheel,
        &new_wheel,
    );

    let stutter_ring = stutter_ring_fixture();
    assert_eq!(
        stutter_ring.visible_legacy(11.25).as_slice(),
        &*stutter_ring.visible(11.25)
    );
    let old_stutter_filter = measure(STUTTER_FILTER_OPS, 500, || {
        visible_stutter_checksum(&stutter_ring.visible_legacy(black_box(11.25)))
    });
    let new_stutter_filter = measure(STUTTER_FILTER_OPS, 500, || {
        visible_stutter_checksum(&stutter_ring.visible(black_box(11.25)))
    });
    print_pair(
        "11. fixed stutter filtering",
        STUTTER_FILTER_OPS,
        &old_stutter_filter,
        &new_stutter_filter,
    );

    let (frame_samples, frame_summary) = frame_stats_fixture();
    let frame_capacity = frame_samples.len() * 7 + 48;
    let mut old_frame_actors = Vec::with_capacity(frame_capacity);
    let old_frame_overlay = measure(FRAME_STATS_OVERLAY_OPS, 10, || {
        old_frame_actors.clear();
        old_frame_actors.extend(benchmark_frame_stats_build_legacy(
            black_box(&frame_samples),
            black_box(frame_summary),
            OverlayAnchor::TopLeft,
            false,
            OverlayStyle::Detailed,
            [1280.0, 720.0],
        ));
        black_box(&old_frame_actors);
        old_frame_actors.len() as u64
    });
    let mut new_frame_actors = Vec::with_capacity(frame_capacity);
    let new_frame_overlay = measure(FRAME_STATS_OVERLAY_OPS, 10, || {
        new_frame_actors.clear();
        push_frame_stats(
            &mut new_frame_actors,
            black_box(&frame_samples),
            black_box(frame_summary),
            OverlayAnchor::TopLeft,
            false,
            OverlayStyle::Detailed,
            [1280.0, 720.0],
        );
        black_box(&new_frame_actors);
        new_frame_actors.len() as u64
    });
    print_reduced_pair(
        "12. direct frame-stats actor append",
        FRAME_STATS_OVERLAY_OPS,
        &old_frame_overlay,
        &new_frame_overlay,
    );

    let mut old_pad_text = Vec::with_capacity(40);
    let old_pad = measure(PAD_TEXT_OPS, 100, || {
        benchmark_pad_text_legacy(&mut old_pad_text)
    });
    let mut new_pad_text = Vec::with_capacity(40);
    let new_pad = measure(PAD_TEXT_OPS, 100, || {
        benchmark_pad_text_current(&mut new_pad_text)
    });
    print_pair(
        "13. stack/inline pad frame data",
        PAD_TEXT_OPS,
        &old_pad,
        &new_pad,
    );

    let old_select_mode_text = SelectModeTextBenchmark::new();
    let old_select_mode = measure(SELECT_MODE_TEXT_OPS, 500, || {
        old_select_mode_text.legacy_frame(1)
    });
    let mut new_select_mode_text = SelectModeTextBenchmark::new();
    let new_select_mode = measure(SELECT_MODE_TEXT_OPS, 500, || {
        new_select_mode_text.current_frame(1)
    });
    print_pair(
        "14. retained Select Mode text",
        SELECT_MODE_TEXT_OPS,
        &old_select_mode,
        &new_select_mode,
    );

    let picker = ScoreImportPickerBenchmark::new();
    let mut old_picker_actors = Vec::with_capacity(20);
    let old_picker = measure(SCORE_PICKER_OPS, 50, || {
        picker.legacy_frame(&mut old_picker_actors, 7)
    });
    let mut new_picker_actors = Vec::with_capacity(20);
    let new_picker = measure(SCORE_PICKER_OPS, 50, || {
        picker.current_frame(&mut new_picker_actors, 7)
    });
    print_pair(
        "15. retained/direct score-pack rows",
        SCORE_PICKER_OPS,
        &old_picker,
        &new_picker,
    );

    let old_lights_text = LightsTextBenchmark::new();
    let old_lights = measure(LIGHTS_TEXT_OPS, 300, || old_lights_text.legacy_frame());
    let mut new_lights_text = LightsTextBenchmark::new();
    let new_lights = measure(LIGHTS_TEXT_OPS, 300, || new_lights_text.current_frame());
    print_pair(
        "16. retained Test Lights text",
        LIGHTS_TEXT_OPS,
        &old_lights,
        &new_lights,
    );

    let search = PlayerOptionsSearchBenchmark::new();
    let old_options_search = measure(OPTIONS_SEARCH_OPS, 200, || search.legacy_frame());
    let new_options_search = measure(OPTIONS_SEARCH_OPS, 200, || search.current_frame());
    print_pair(
        "17. prepared options-search rows",
        OPTIONS_SEARCH_OPS,
        &old_options_search,
        &new_options_search,
    );

    let qr = QrOverlayBenchmark::new();
    let mut old_qr_actors = Vec::with_capacity(24);
    let old_qr = measure(QR_OVERLAY_OPS, 25, || qr.legacy_frame(&mut old_qr_actors));
    let mut new_qr_actors = Vec::with_capacity(24);
    let new_qr = measure(QR_OVERLAY_OPS, 25, || qr.current_frame(&mut new_qr_actors));
    print_pair(
        "18. shared/direct QR-login overlay",
        QR_OVERLAY_OPS,
        &old_qr,
        &new_qr,
    );

    let mut old_version_actors = Vec::with_capacity(2);
    let old_version = measure(SMALL_OVERLAY_OPS, 300, || {
        old_version_actors.clear();
        old_version_actors.extend(version_overlay::build(
            VersionOverlaySide::Right,
            LogLevel::Debug,
            Some("123456789abcdef"),
        ));
        overlay_actor_checksum(&old_version_actors)
    });
    let mut new_version_actors = Vec::with_capacity(2);
    let new_version = measure(SMALL_OVERLAY_OPS, 300, || {
        new_version_actors.clear();
        version_overlay::push(
            &mut new_version_actors,
            VersionOverlaySide::Right,
            LogLevel::Debug,
            Some("123456789abcdef"),
        );
        overlay_actor_checksum(&new_version_actors)
    });
    print_pair(
        "19. direct version-watermark append",
        SMALL_OVERLAY_OPS,
        &old_version,
        &new_version,
    );

    let message: Arc<str> = Arc::from("Benchmark gamepad connected");
    let mut old_message_actors = Vec::with_capacity(2);
    let old_message = measure(SMALL_OVERLAY_OPS, 300, || {
        old_message_actors.clear();
        old_message_actors.extend(gamepad_overlay::benchmark_build_owned(&message));
        overlay_actor_checksum(&old_message_actors)
    });
    let mut new_message_actors = Vec::with_capacity(2);
    let new_message = measure(SMALL_OVERLAY_OPS, 300, || {
        new_message_actors.clear();
        gamepad_overlay::push(
            &mut new_message_actors,
            gamepad_overlay::Params {
                message: Arc::clone(&message),
            },
        );
        overlay_actor_checksum(&new_message_actors)
    });
    print_pair(
        "20. retained/direct system-message overlay",
        SMALL_OVERLAY_OPS,
        &old_message,
        &new_message,
    );
}

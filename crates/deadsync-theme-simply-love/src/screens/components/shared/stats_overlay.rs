use crate::act;
use crate::views::{TimingHealth, VisibleStutterSample};
use deadlib_present::actors::Actor;
use deadlib_present::cache::{TextCache, cached_text, text_cache_with_capacity};
use deadlib_present::space::{screen_height, screen_width};
use deadlib_render_core::BackendType;
use std::cell::RefCell;
use std::fmt::{self, Write};
use std::sync::Arc;

const TEXT_CACHE_LIMIT: usize = 4096;
const DEBUG_OVERLAY_Z: i16 = 32020;

thread_local! {
    static STATS_TEXT_CACHE: RefCell<TextCache<(u32, u32, u8)>> = RefCell::new(text_cache_with_capacity(256));
    static STUTTER_TIME_CACHE: RefCell<TextCache<u32>> = RefCell::new(text_cache_with_capacity(1024));
    static STUTTER_LINE_CACHE: RefCell<TextCache<(u32, u32, u32)>> = RefCell::new(text_cache_with_capacity(2048));
}

#[inline(always)]
const fn backend_key(backend: BackendType) -> u8 {
    match backend {
        #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
        BackendType::Vulkan => 0,
        #[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
        BackendType::VulkanWgpu => 1,
        BackendType::OpenGL => 2,
        BackendType::OpenGLWgpu => 3,
        #[cfg(target_os = "macos")]
        BackendType::Metal => 4,
        BackendType::Software => 5,
        #[cfg(target_os = "windows")]
        BackendType::DirectX => 6,
        #[cfg(target_os = "macos")]
        BackendType::MetalWgpu => 7,
    }
}

#[inline(always)]
fn cached_stats_text(backend: BackendType, fps: f32, vpf: u32) -> Arc<str> {
    let key = (fps.max(0.0).to_bits(), vpf, backend_key(backend));
    cached_text(&STATS_TEXT_CACHE, key, TEXT_CACHE_LIMIT, || {
        format!("{:.0} FPS\n{} VPF\n{}", fps.max(0.0), vpf, backend)
    })
}

#[inline(always)]
const fn flag(value: bool) -> u8 {
    if value { 1 } else { 0 }
}

struct Milliseconds(u64);

impl fmt::Display for Milliseconds {
    #[inline]
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 == 0 {
            formatter.write_str("n/a")
        } else {
            write!(formatter, "{:.2}ms", self.0 as f64 / 1_000_000.0)
        }
    }
}

fn timing_text(timing: TimingHealth) -> String {
    let mut text = String::with_capacity(if timing.audio.is_some() { 288 } else { 160 });
    let _ = write!(
        text,
        "Disp err {:+.2}ms catch:{}\nPresent int {}\nMode {} {}->{} map:{}\nQueue {} iw:{} bp:{} qi:{} sub:{}\nIDs {}/{} cal {}",
        timing.display_error_ms,
        flag(timing.display_catching_up),
        Milliseconds(timing.interval_ns),
        timing.present_mode,
        timing.display_clock,
        timing.host_clock,
        flag(timing.host_mapped),
        timing.in_flight_images,
        flag(timing.waited_for_image),
        flag(timing.applied_back_pressure),
        flag(timing.queue_idle_waited),
        flag(timing.suboptimal),
        timing.submitted_present_id,
        timing.completed_present_id,
        Milliseconds(timing.calibration_error_ns),
    );
    if let Some(audio) = timing.audio {
        let _ = write!(
            text,
            "\nAudio {} {}Hz req {} fb:{}\nClk {} {} sf:{} cf:{} out {} xr {}\nBuf {} pad {} q {} tick {} span {}",
            audio.backend,
            audio.sample_rate_hz,
            audio.requested_output_mode,
            flag(audio.fallback_from_native),
            audio.timing_clock,
            audio.timing_quality,
            audio.timing_sanity_failure_count,
            audio.clock_fallback_count,
            Milliseconds(audio.estimated_output_delay_ns),
            audio.underrun_count,
            audio.buffer_frames,
            audio.padding_frames,
            audio.queued_frames,
            Milliseconds(audio.device_period_ns),
            Milliseconds(audio.stream_latency_ns),
        );
    }
    text
}

/// Stats overlay: base FPS block plus optional timing-health block, top-right, miso, white.
pub fn push(
    actors: &mut Vec<Actor>,
    backend: BackendType,
    fps: f32,
    vpf: u32,
    timing: Option<TimingHealth>,
) {
    const MARGIN_X: f32 = -16.0;
    const MARGIN_Y: f32 = 16.0;
    const TIMING_OFFSET_Y: f32 = 48.0;

    let w = screen_width();

    let stats_text = cached_stats_text(backend, fps, vpf);
    actors.reserve(1 + usize::from(timing.is_some()));
    actors.push(act!(text:
        align(1.0, 0.0): // Align the whole text block to its top-right corner
        xy(w + MARGIN_X, MARGIN_Y): // Position the block's top-right corner
        zoom(0.65):
        diffuse(1.0, 1.0, 1.0, 1.0):
        font("miso"):
        settext(stats_text): // Use the new multi-line string
        horizalign(right):   // Align each line of text to the right within the block
        z(DEBUG_OVERLAY_Z)
    ));
    if let Some(timing) = timing {
        let timing_text = timing_text(timing);
        actors.push(act!(text:
            align(1.0, 0.0):
            xy(w + MARGIN_X, MARGIN_Y + TIMING_OFFSET_Y):
            zoom(0.5):
            diffuse(1.0, 1.0, 1.0, 1.0):
            font("miso"):
            settext(timing_text):
            horizalign(right):
            z(DEBUG_OVERLAY_Z)
        ));
    }
}

fn format_stutter_time(seconds: f32) -> Arc<str> {
    let centi_total = (seconds.max(0.0) * 100.0).round() as u64;
    let key = (centi_total.min(u64::from(u32::MAX))) as u32;
    cached_text(&STUTTER_TIME_CACHE, key, TEXT_CACHE_LIMIT, || {
        let minutes = centi_total / 6_000;
        let rem = centi_total % 6_000;
        let secs = rem / 100;
        let centis = rem % 100;
        format!("{minutes:02}:{secs:02}.{centis:02}")
    })
}

fn stutter_color(severity: u8, age_seconds: f32) -> [f32; 4] {
    const STUTTER_FADE_SECONDS: f32 = 3.4;
    let alpha = (1.0 - age_seconds / STUTTER_FADE_SECONDS).clamp(0.0, 1.0);
    let rgb = match severity {
        1 => [1.0, 1.0, 1.0],
        2 => [1.0, 1.0, 0.0],
        _ => [1.0, 0.4, 0.4],
    };
    [rgb[0], rgb[1], rgb[2], alpha]
}

pub fn push_stutter(actors: &mut Vec<Actor>, events: &[VisibleStutterSample]) {
    if events.is_empty() {
        return;
    }
    // Match ITG/Simply Love ScreenStatsOverlay skip box metrics:
    // SkipX=SCREEN_RIGHT-100, SkipY=SCREEN_BOTTOM-85, SkipWidth=190, SkipSpacingY=14.
    const SKIP_X_FROM_RIGHT: f32 = 100.0;
    const SKIP_Y_FROM_BOTTOM: f32 = 85.0;
    const SKIP_WIDTH: f32 = 190.0;
    const SKIP_SPACING_Y: f32 = 14.0;
    const SKIP_SLOTS: usize = 5;
    const EDGE_PAD_Y: f32 = 10.0;
    const TEXT_ZOOM: f32 = 1.0;
    let w = screen_width();
    let h = screen_height();
    let skip_x = w - SKIP_X_FROM_RIGHT;
    let skip_y = h - SKIP_Y_FROM_BOTTOM;
    let half_h = (SKIP_SPACING_Y * SKIP_SLOTS as f32).mul_add(0.5, EDGE_PAD_Y);
    let top = skip_y - half_h;
    let bottom = skip_y + half_h;
    actors.reserve(events.len().min(SKIP_SLOTS) + 1);
    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(SKIP_WIDTH.mul_add(-0.5, skip_x), top):
        zoomto(SKIP_WIDTH, bottom - top):
        diffuse(0.0, 0.0, 0.0, 0.4):
        z(DEBUG_OVERLAY_Z)
    ));
    let visible = events.len().min(SKIP_SLOTS);
    let line_top = top + EDGE_PAD_Y;
    let line_bottom = bottom - EDGE_PAD_Y;
    for (i, event) in events.iter().take(visible).enumerate() {
        // Match ScreenStatsOverlay's fixed 5-row lane geometry.
        let y = if SKIP_SLOTS == 1 {
            line_top
        } else {
            (line_bottom - line_top).mul_add(i as f32 / (SKIP_SLOTS - 1) as f32, line_top)
        };
        let c = stutter_color(event.severity, event.age_seconds);
        let t = format_stutter_time(event.timestamp_seconds);
        let line = cached_text(
            &STUTTER_LINE_CACHE,
            (
                (event.timestamp_seconds.max(0.0) * 100.0).round() as u32,
                event.frame_ms.max(0.0).to_bits(),
                event.frame_multiple.max(0.0).to_bits(),
            ),
            TEXT_CACHE_LIMIT,
            || {
                format!(
                    "{t}: {:.0}ms ({:.0})",
                    event.frame_ms.max(0.0),
                    event.frame_multiple.max(0.0)
                )
            },
        );
        actors.push(act!(text:
            align(0.5, 0.0):
            xy(skip_x, y - 7.0):
            zoom(TEXT_ZOOM):
            shadowlength(0.0):
            diffuse(c[0], c[1], c[2], c[3]):
            font("miso"):
            settext(line):
            horizalign(center):
            z(DEBUG_OVERLAY_Z + 1)
        ));
    }
}

#[cfg(any(test, feature = "bench-support"))]
#[inline(always)]
fn legacy_ms_text(ns: u64) -> String {
    if ns == 0 {
        "n/a".to_string()
    } else {
        format!("{:.2}ms", ns as f64 / 1_000_000.0)
    }
}

#[cfg(any(test, feature = "bench-support"))]
fn legacy_timing_text(timing: TimingHealth) -> String {
    let mut text = format!(
        "Disp err {:+.2}ms catch:{}\nPresent int {}\nMode {} {}->{} map:{}\nQueue {} iw:{} bp:{} qi:{} sub:{}\nIDs {}/{} cal {}",
        timing.display_error_ms,
        flag(timing.display_catching_up),
        legacy_ms_text(timing.interval_ns),
        timing.present_mode,
        timing.display_clock,
        timing.host_clock,
        flag(timing.host_mapped),
        timing.in_flight_images,
        flag(timing.waited_for_image),
        flag(timing.applied_back_pressure),
        flag(timing.queue_idle_waited),
        flag(timing.suboptimal),
        timing.submitted_present_id,
        timing.completed_present_id,
        legacy_ms_text(timing.calibration_error_ns),
    );
    if let Some(audio) = timing.audio {
        let _ = write!(
            text,
            "\nAudio {} {}Hz req {} fb:{}\nClk {} {} sf:{} cf:{} out {} xr {}\nBuf {} pad {} q {} tick {} span {}",
            audio.backend,
            audio.sample_rate_hz,
            audio.requested_output_mode,
            flag(audio.fallback_from_native),
            audio.timing_clock,
            audio.timing_quality,
            audio.timing_sanity_failure_count,
            audio.clock_fallback_count,
            legacy_ms_text(audio.estimated_output_delay_ns),
            audio.underrun_count,
            audio.buffer_frames,
            audio.padding_frames,
            audio.queued_frames,
            legacy_ms_text(audio.device_period_ns),
            legacy_ms_text(audio.stream_latency_ns),
        );
    }
    text
}

#[cfg(any(test, feature = "bench-support"))]
#[must_use]
pub fn benchmark_timing_text_legacy(timing: TimingHealth) -> String {
    legacy_timing_text(timing)
}

#[cfg(any(test, feature = "bench-support"))]
#[must_use]
pub fn benchmark_timing_text_current(timing: TimingHealth) -> String {
    timing_text(timing)
}

#[cfg(any(test, feature = "bench-support"))]
#[must_use]
pub fn benchmark_build_legacy(
    backend: BackendType,
    fps: f32,
    vpf: u32,
    timing: Option<TimingHealth>,
) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(2);
    push(&mut actors, backend, fps, vpf, timing);
    actors
}

#[cfg(any(test, feature = "bench-support"))]
#[must_use]
pub fn benchmark_build_stutter_legacy(events: &[VisibleStutterSample]) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(events.len() + 1);
    push_stutter(&mut actors, events);
    actors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::AudioTimingView;
    use deadlib_render_core::{ClockDomainTrace, PresentModeTrace};

    fn timing(audio: bool) -> TimingHealth {
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
            audio: audio.then_some(AudioTimingView {
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

    #[test]
    fn one_pass_timing_text_matches_legacy_output() {
        for timing in [timing(false), timing(true)] {
            assert_eq!(timing_text(timing), legacy_timing_text(timing));
        }
        let mut zero = timing(false);
        zero.interval_ns = 0;
        zero.calibration_error_ns = 0;
        assert_eq!(timing_text(zero), legacy_timing_text(zero));
    }

    #[test]
    fn direct_overlay_append_matches_owned_builders() {
        let stutters = [VisibleStutterSample {
            timestamp_seconds: 61.25,
            frame_ms: 33.3,
            frame_multiple: 2.0,
            severity: 2,
            age_seconds: 0.4,
        }];
        let mut expected =
            benchmark_build_legacy(BackendType::OpenGL, 120.0, 42, Some(timing(true)));
        expected.extend(benchmark_build_stutter_legacy(&stutters));

        let mut actual = Vec::new();
        push(
            &mut actual,
            BackendType::OpenGL,
            120.0,
            42,
            Some(timing(true)),
        );
        push_stutter(&mut actual, &stutters);

        assert_eq!(format!("{actual:#?}"), format!("{expected:#?}"));
    }
}

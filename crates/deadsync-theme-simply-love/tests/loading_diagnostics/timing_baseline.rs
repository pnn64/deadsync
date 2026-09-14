// Frozen from 0.5.1214 (012adc4ce) for behavior and performance comparisons.
use crate::act;
use crate::views::TimingHealth;
use deadlib_present::actors::Actor;
use deadlib_present::cache::{TextCache, cached_text, text_cache_with_capacity};
use deadlib_present::space::{screen_height, screen_width};
use deadlib_render_core::{BackendType, ClockDomainTrace, PresentModeTrace};
use deadsync_config::frame_pacing::VisibleStutterSample;
use std::cell::RefCell;
use std::fmt::{self, Write};
use std::sync::Arc;

const TEXT_CACHE_LIMIT: usize = 4096;
const DEBUG_OVERLAY_Z: i16 = 32020;

thread_local! {
    static STATS_TEXT_CACHE: RefCell<TextCache<(u32, u32, u8)>> = RefCell::new(text_cache_with_capacity(256));
    static TIMING_TEXT_CACHE: RefCell<TextCache<TimingTextKey>> = RefCell::new(text_cache_with_capacity(256));
    static STUTTER_TIME_CACHE: RefCell<TextCache<u32>> = RefCell::new(text_cache_with_capacity(1024));
    static STUTTER_LINE_CACHE: RefCell<TextCache<(u32, u32, u32)>> = RefCell::new(text_cache_with_capacity(2048));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct AudioTimingTextKey {
    backend: &'static str,
    requested_output_mode: &'static str,
    fallback_from_native: bool,
    timing_clock: &'static str,
    timing_quality: &'static str,
    sample_rate_hz: u32,
    device_period_ns: u64,
    stream_latency_ns: u64,
    buffer_frames: u32,
    padding_frames: u32,
    queued_frames: u32,
    estimated_output_delay_ns: u64,
    clock_fallback_count: u64,
    timing_sanity_failure_count: u64,
    underrun_count: u64,
}

impl From<deadsync_theme::views::AudioTimingView> for AudioTimingTextKey {
    fn from(audio: deadsync_theme::views::AudioTimingView) -> Self {
        Self {
            backend: audio.backend,
            requested_output_mode: audio.requested_output_mode,
            fallback_from_native: audio.fallback_from_native,
            timing_clock: audio.timing_clock,
            timing_quality: audio.timing_quality,
            sample_rate_hz: audio.sample_rate_hz,
            device_period_ns: audio.device_period_ns,
            stream_latency_ns: audio.stream_latency_ns,
            buffer_frames: audio.buffer_frames,
            padding_frames: audio.padding_frames,
            queued_frames: audio.queued_frames,
            estimated_output_delay_ns: audio.estimated_output_delay_ns,
            clock_fallback_count: audio.clock_fallback_count,
            timing_sanity_failure_count: audio.timing_sanity_failure_count,
            underrun_count: audio.underrun_count,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TimingTextKey {
    interval_ns: u64,
    display_error_bits: u32,
    display_catching_up: bool,
    present_mode: u8,
    display_clock: u8,
    host_clock: u8,
    in_flight_images: u8,
    waited_for_image: bool,
    applied_back_pressure: bool,
    queue_idle_waited: bool,
    suboptimal: bool,
    submitted_present_id: u32,
    completed_present_id: u32,
    calibration_error_ns: u64,
    host_mapped: bool,
    audio: Option<AudioTimingTextKey>,
}

#[inline(always)]
const fn present_mode_key(mode: PresentModeTrace) -> u8 {
    match mode {
        PresentModeTrace::Unknown => 0,
        PresentModeTrace::Fifo => 1,
        PresentModeTrace::FifoRelaxed => 2,
        PresentModeTrace::Mailbox => 3,
        PresentModeTrace::Immediate => 4,
    }
}

#[inline(always)]
const fn clock_domain_key(clock: ClockDomainTrace) -> u8 {
    match clock {
        ClockDomainTrace::Unknown => 0,
        ClockDomainTrace::Device => 1,
        ClockDomainTrace::Monotonic => 2,
        ClockDomainTrace::MonotonicRaw => 3,
        ClockDomainTrace::Qpc => 4,
    }
}

#[inline(always)]
fn timing_text_key(timing: TimingHealth) -> TimingTextKey {
    TimingTextKey {
        interval_ns: timing.interval_ns,
        display_error_bits: timing.display_error_ms.to_bits(),
        display_catching_up: timing.display_catching_up,
        present_mode: present_mode_key(timing.present_mode),
        display_clock: clock_domain_key(timing.display_clock),
        host_clock: clock_domain_key(timing.host_clock),
        in_flight_images: timing.in_flight_images,
        waited_for_image: timing.waited_for_image,
        applied_back_pressure: timing.applied_back_pressure,
        queue_idle_waited: timing.queue_idle_waited,
        suboptimal: timing.suboptimal,
        submitted_present_id: timing.submitted_present_id,
        completed_present_id: timing.completed_present_id,
        calibration_error_ns: timing.calibration_error_ns,
        host_mapped: timing.host_mapped,
        audio: timing.audio.map(Into::into),
    }
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

/// Thread-lifetime, bounded telemetry text interning. Exact source bits form
/// the key, so cache hits preserve formatting byte-for-byte. The cache never
/// evicts, stops admitting entries at `TEXT_CACHE_LIMIT`, and drops with the
/// presentation thread; stable telemetry frames only clone an `Arc` handle.
#[inline(always)]
fn retained_timing_text(timing: TimingHealth) -> Arc<str> {
    cached_text(
        &TIMING_TEXT_CACHE,
        timing_text_key(timing),
        TEXT_CACHE_LIMIT,
        || timing_text(timing),
    )
}

pub(super) fn readout(timing: TimingHealth) -> Arc<str> {
    retained_timing_text(timing)
}
pub(super) fn owned(timing: TimingHealth) -> String {
    timing_text(timing)
}
pub(super) fn reset() {
    TIMING_TEXT_CACHE.with(|c| *c.borrow_mut() = text_cache_with_capacity(256));
}

pub(super) fn storage() -> (usize, usize) {
    TIMING_TEXT_CACHE.with(|c| {
        let c = c.borrow();
        (c.len(), c.values().map(|s| s.len()).sum())
    })
}
pub(super) fn metadata_bytes() -> usize {
    std::mem::size_of::<TextCache<TimingTextKey>>()
}

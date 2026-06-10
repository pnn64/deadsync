#[cfg(unix)]
use deadsync_audio::OutputTimingQuality;
use deadsync_audio::{AudioRenderCallbackResult, StutterDiagAudioEventKind};
use deadsync_platform::host_time::now_nanos;

#[inline(always)]
fn stutter_diag_enabled() -> bool {
    log::log_enabled!(log::Level::Trace)
}

#[inline(always)]
pub fn publish_output_timing(
    sample_rate_hz: u32,
    device_period_ns: u64,
    stream_latency_ns: u64,
    buffer_frames: u32,
    padding_frames: u32,
    queued_frames: u32,
    estimated_output_delay_ns: u64,
) {
    deadsync_audio::publish_output_timing(
        sample_rate_hz,
        device_period_ns,
        stream_latency_ns,
        buffer_frames,
        padding_frames,
        queued_frames,
        estimated_output_delay_ns,
    );
}

#[inline(always)]
#[cfg(unix)]
pub fn publish_output_timing_quality(quality: OutputTimingQuality) {
    deadsync_audio::publish_output_timing_quality(quality);
}

#[inline(always)]
pub fn note_output_underrun() {
    deadsync_audio::note_output_underrun(now_nanos(), stutter_diag_enabled());
}

#[inline(always)]
#[cfg(unix)]
pub fn note_output_clock_fallback() {
    deadsync_audio::note_output_clock_fallback(now_nanos(), stutter_diag_enabled());
}

#[inline(always)]
pub fn report_audio_render_callback(result: AudioRenderCallbackResult) {
    if result.callback_gap_ns != 0
        && stutter_diag_enabled()
        && result.callback_gap_ns >= deadsync_audio::stutter_diag_callback_gap_threshold_ns()
    {
        deadsync_audio::record_stutter_diag_event(
            StutterDiagAudioEventKind::CallbackGap,
            now_nanos(),
            result.callback_gap_ns,
            deadsync_audio::current_output_timing_quality(),
        );
    }
    if result.output_underrun {
        note_output_underrun();
    }
}

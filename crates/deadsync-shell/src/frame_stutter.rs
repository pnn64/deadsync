use deadlib_present::actors::{Actor, actor_tree_stats};
use deadlib_present::compose::TextLayoutFrameStats;
use deadlib_render::DrawStats;
use deadsync_config::frame_pacing::stutter_severity;
use deadsync_screens::Screen;

#[derive(Clone, Copy, Debug, Default)]
pub struct ComposeBreakdown {
    pub actor_build_us: u32,
    pub build_screen_us: u32,
    pub resolve_textures_us: u32,
    pub render_objects: u32,
    pub render_cameras: u32,
    pub text_layout: TextLayoutFrameStats,
}

#[allow(clippy::too_many_arguments)]
pub fn trace_frame_stutter(
    frame_seconds: f32,
    expected_seconds: f32,
    total_elapsed: f32,
    screen: Screen,
    pre_redraw_gap_us: u32,
    request_to_redraw_us: u32,
    redraw_request_reason: &'static str,
    input_us: u32,
    update_us: u32,
    compose_us: u32,
    upload_us: u32,
    draw_us: u32,
    actors: &[Actor],
    draw_stats: DrawStats,
    compose_breakdown: ComposeBreakdown,
    display_error_seconds: f32,
    display_catching_up: bool,
) {
    if !log::log_enabled!(log::Level::Trace) {
        return;
    }
    let severity = stutter_severity(frame_seconds, expected_seconds);
    if severity == 0 {
        return;
    }
    let frame_us_f = (frame_seconds * 1_000_000.0).max(0.0);
    let frame_us = if frame_us_f > u32::MAX as f32 {
        u32::MAX
    } else {
        frame_us_f as u32
    };
    let frame_work_us = input_us
        .saturating_add(update_us)
        .saturating_add(compose_us)
        .saturating_add(upload_us)
        .saturating_add(draw_us);
    let unaccounted_gap_us =
        frame_us.saturating_sub(pre_redraw_gap_us.saturating_add(frame_work_us));
    let draw_split_us = draw_stats
        .acquire_us
        .saturating_add(draw_stats.submit_us)
        .saturating_add(draw_stats.present_us)
        .saturating_add(draw_stats.gpu_wait_us)
        .saturating_add(draw_stats.backend_setup_us)
        .saturating_add(draw_stats.backend_prepare_us)
        .saturating_add(draw_stats.backend_record_us);
    let draw_other_us = draw_us.saturating_sub(draw_split_us);
    let redraw_late_us = pre_redraw_gap_us.saturating_sub(request_to_redraw_us);
    let dominant = dominant_phase(
        request_to_redraw_us,
        input_us,
        update_us,
        compose_us,
        upload_us,
        draw_stats,
        draw_other_us,
        unaccounted_gap_us,
        redraw_late_us,
    );
    let multiple = if expected_seconds > 0.0 {
        frame_seconds / expected_seconds
    } else {
        0.0
    };
    let actor_stats = actor_tree_stats(actors);
    let present = draw_stats.present_stats;
    let audio = deadsync_audio_stream::get_output_timing_snapshot();
    log::trace!(
        "Frame stutter t={:.3}s sev={} screen={:?} dt={:.3}ms expected={:.3}ms x{:.2} req={} dom={} dom_ms={:.3} phases_ms=[pre_redraw:{:.3} input:{:.3} update:{:.3} compose:{:.3} upload:{:.3} draw:{:.3} unaccounted:{:.3}] compose_dbg=[actors:{:.3} build:{:.3} resolve:{:.3} nodes:{} sprites:{} text:{} chars:{} frames:{} mesh:{} tmesh:{} cameras:{} shadows:{} objects:{} render_cameras:{} txt_hits:{} txt_shared:{} txt_miss:{} txt_lines:{} txt_glyphs:{} txt_entries:{} txt_aliases:{}] redraw_ms=[redrive_late:{:.3} request_to_redraw:{:.3}] draw_sub_ms=[acquire:{:.3} submit:{:.3} present:{:.3} gpu_wait:{:.3} other:{:.3}] draw_cpu_ms=[setup:{:.3} prep:{:.3} record:{:.3}] display_dbg=[active:{} err_ms:{:+.3} catch:{}] present_dbg=[mode:{} display:{} host:{} mapped:{} inflight:{} image_wait:{} back_pressure:{} queue_idle:{} subopt:{} submit_id:{} done_id:{} refresh_ms:{:.3} interval_ms:{:.3} margin_ms:{:.3} cal_ms:{:.3}] audio_dbg=[path:{} req:{} fallback:{} clock:{} qual:{} sf:{} cf:{} rate:{} buf:{} pad:{} q:{} tick_ms:{:.3} span_ms:{:.3} out_ms:{:.3} underruns:{}]",
        total_elapsed,
        severity,
        screen,
        frame_seconds * 1000.0,
        expected_seconds * 1000.0,
        multiple,
        redraw_request_reason,
        dominant.0,
        dominant.1 as f32 / 1000.0,
        pre_redraw_gap_us as f32 / 1000.0,
        input_us as f32 / 1000.0,
        update_us as f32 / 1000.0,
        compose_us as f32 / 1000.0,
        upload_us as f32 / 1000.0,
        draw_us as f32 / 1000.0,
        unaccounted_gap_us as f32 / 1000.0,
        compose_breakdown.actor_build_us as f32 / 1000.0,
        compose_breakdown.build_screen_us as f32 / 1000.0,
        compose_breakdown.resolve_textures_us as f32 / 1000.0,
        actor_stats.total,
        actor_stats.sprites,
        actor_stats.texts,
        actor_stats.text_chars,
        actor_stats.frames,
        actor_stats.meshes,
        actor_stats.textured_meshes,
        actor_stats.cameras,
        actor_stats.shadows,
        compose_breakdown.render_objects,
        compose_breakdown.render_cameras,
        compose_breakdown.text_layout.owned_hits,
        compose_breakdown.text_layout.shared_hits,
        compose_breakdown.text_layout.misses,
        compose_breakdown.text_layout.built_lines,
        compose_breakdown.text_layout.built_glyphs,
        compose_breakdown.text_layout.owned_entries,
        compose_breakdown.text_layout.shared_aliases,
        redraw_late_us as f32 / 1000.0,
        request_to_redraw_us as f32 / 1000.0,
        draw_stats.acquire_us as f32 / 1000.0,
        draw_stats.submit_us as f32 / 1000.0,
        draw_stats.present_us as f32 / 1000.0,
        draw_stats.gpu_wait_us as f32 / 1000.0,
        draw_other_us as f32 / 1000.0,
        draw_stats.backend_setup_us as f32 / 1000.0,
        draw_stats.backend_prepare_us as f32 / 1000.0,
        draw_stats.backend_record_us as f32 / 1000.0,
        u8::from(screen == Screen::Gameplay),
        display_error_seconds * 1000.0,
        u8::from(display_catching_up),
        present.mode,
        present.display_clock,
        present.host_clock,
        present.host_present_ns != 0,
        present.in_flight_images,
        present.waited_for_image,
        present.applied_back_pressure,
        present.queue_idle_waited,
        present.suboptimal,
        present.submitted_present_id,
        present.completed_present_id,
        present.refresh_ns as f32 / 1_000_000.0,
        present.actual_interval_ns as f32 / 1_000_000.0,
        present.present_margin_ns as f32 / 1_000_000.0,
        present.calibration_error_ns as f32 / 1_000_000.0,
        audio.backend,
        audio.requested_output_mode.as_str(),
        audio.fallback_from_native,
        audio.timing_clock,
        audio.timing_quality,
        audio.timing_sanity_failure_count,
        audio.clock_fallback_count,
        audio.sample_rate_hz,
        audio.buffer_frames,
        audio.padding_frames,
        audio.queued_frames,
        audio.device_period_ns as f32 / 1_000_000.0,
        audio.stream_latency_ns as f32 / 1_000_000.0,
        audio.estimated_output_delay_ns as f32 / 1_000_000.0,
        audio.underrun_count
    );
}

#[allow(clippy::too_many_arguments)]
fn dominant_phase(
    redraw_delivery_us: u32,
    input_us: u32,
    update_us: u32,
    compose_us: u32,
    upload_us: u32,
    draw_stats: DrawStats,
    draw_other_us: u32,
    unaccounted_us: u32,
    redraw_late_us: u32,
) -> (&'static str, u32) {
    let mut dominant = ("redraw_delivery", redraw_delivery_us);
    for candidate in [
        ("input", input_us),
        ("update", update_us),
        ("compose", compose_us),
        ("upload", upload_us),
        ("present", draw_stats.present_us),
        ("gpu_wait", draw_stats.gpu_wait_us),
        ("draw_setup", draw_stats.backend_setup_us),
        ("draw_prepare", draw_stats.backend_prepare_us),
        ("draw_record", draw_stats.backend_record_us),
        ("draw_other", draw_other_us),
        ("unaccounted", unaccounted_us),
        ("redrive_late", redraw_late_us),
    ] {
        if candidate.1 > dominant.1 {
            dominant = candidate;
        }
    }
    dominant
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dominant_phase_selects_largest_measurement() {
        let draw = DrawStats {
            present_us: 900,
            ..DrawStats::default()
        };
        assert_eq!(
            dominant_phase(100, 200, 300, 400, 500, draw, 600, 700, 800),
            ("present", 900)
        );
    }
}

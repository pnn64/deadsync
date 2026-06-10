pub mod mixer;
pub mod output;
pub mod position;
pub mod ring;
pub mod telemetry;

pub use mixer::{
    ActiveSfx, MAX_ACTIVE_SFX, MAX_SCHEDULE_AHEAD_FRAMES, QueuedSfx, ScheduledOnset, SfxLane,
    assist_sfx_generation, bump_assist_sfx_generation, bump_screen_sfx_generation, f32_to_i16,
    i16_to_f32, mix_active_sfx, push_queued_sfx, scheduled_onset_decision, sfx_is_stale,
    sfx_stop_generation,
};
pub use output::{
    AudioMixLevels, AudioOutputMode, Cut, InitConfig, LinuxAudioBackend, OutputBackendReady,
    OutputDeviceInfo, OutputTimingSnapshot, mix_level_gains, pack_audio_mix_levels,
    unpack_audio_mix_levels,
};
pub use position::{
    CallbackClockSource, CallbackClockWindow, MUSIC_POS_MAP_BACKLOG_FRAMES,
    MusicStreamClockSnapshot, PlaybackPosMap, activate_music_track, active_music_track_id,
    clear_music_stream_clock_seed, fallback_music_position, fallback_stream_position_frames,
    load_callback_clock_snapshot_now, mark_music_track_started, music_clock_seed_enabled,
    music_gain_snap_generation, music_nanos_from_seconds, music_target_gain, music_total_frames,
    music_track_active, music_track_active_relaxed, music_track_has_started,
    music_track_start_frame, next_music_track_id, normalized_music_rate,
    note_timing_diag_callback_gap, publish_callback_window_end,
    publish_callback_window_start_nanos, reset_music_stream_clock_state, reset_music_target_gain,
    seed_music_stream_clock, seeded_music_position, set_music_clock_rate, set_music_target_gain,
    snap_music_gain_generation, stop_music_track, stream_position_frames_from_window,
    timing_diag_last_callback_gap_ns,
};
pub use telemetry::{
    AUDIO_STUTTER_DIAG_EVENT_COUNT, OutputTelemetryBackend, OutputTelemetryClock,
    OutputTimingQuality, StutterDiagAudioEvent, StutterDiagAudioEventKind,
    collect_stutter_diag_events, current_output_timing_quality, get_output_timing_snapshot,
    note_output_clock_fallback, note_output_timing_sanity_failure, note_output_underrun,
    publish_output_backend_ready, publish_output_timing, publish_output_timing_quality,
    record_stutter_diag_event, stutter_diag_callback_gap_threshold_ns, stutter_diag_trigger_seq,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct MusicMapSeg {
    pub stream_frame_start: i64,
    pub frames: i64,
    pub music_start_sec: f64,
    pub music_sec_per_frame: f64,
}

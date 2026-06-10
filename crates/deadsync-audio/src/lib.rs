pub mod mixer;
pub mod output;
pub mod position;
pub mod ring;
pub mod telemetry;

pub use mixer::{
    ActiveSfx, MAX_ACTIVE_SFX, MAX_SCHEDULE_AHEAD_FRAMES, QueuedSfx, ScheduledOnset, SfxLane,
    f32_to_i16, i16_to_f32, mix_active_sfx, push_queued_sfx, scheduled_onset_decision,
};
pub use output::{
    AudioMixLevels, AudioOutputMode, Cut, InitConfig, LinuxAudioBackend, OutputBackendReady,
    OutputDeviceInfo, OutputTimingSnapshot, mix_level_gains, pack_audio_mix_levels,
    unpack_audio_mix_levels,
};
pub use position::{
    MUSIC_POS_MAP_BACKLOG_FRAMES, MusicStreamClockSnapshot, PlaybackPosMap,
    fallback_music_position, music_clock_seed_enabled, music_nanos_from_seconds,
    normalized_music_rate,
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

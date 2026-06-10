pub mod mixer;
pub mod output;
pub mod position;
pub mod ring;
pub mod telemetry;

pub use mixer::{
    MAX_SCHEDULE_AHEAD_FRAMES, ScheduledOnset, f32_to_i16, i16_to_f32, scheduled_onset_decision,
};
pub use output::{
    AudioOutputMode, Cut, LinuxAudioBackend, OutputBackendReady, OutputDeviceInfo,
    OutputTimingSnapshot,
};
pub use position::{
    MUSIC_POS_MAP_BACKLOG_FRAMES, PlaybackPosMap, fallback_music_position,
    music_clock_seed_enabled, music_nanos_from_seconds, normalized_music_rate,
};
pub use telemetry::{
    OutputTelemetryBackend, OutputTelemetryClock, OutputTimingQuality, StutterDiagAudioEvent,
    StutterDiagAudioEventKind,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct MusicMapSeg {
    pub stream_frame_start: i64,
    pub frames: i64,
    pub music_start_sec: f64,
    pub music_sec_per_frame: f64,
}

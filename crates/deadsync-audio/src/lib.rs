pub mod ring;
pub mod telemetry;

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

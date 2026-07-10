use deadlib_render::{ClockDomainTrace, PresentModeTrace, PresentStats};
use deadsync_audio::OutputTimingSnapshot;

#[derive(Clone, Copy, Debug)]
pub struct TimingHealth {
    pub interval_ns: u64,
    pub display_error_ms: f32,
    pub display_catching_up: bool,
    pub present_mode: PresentModeTrace,
    pub display_clock: ClockDomainTrace,
    pub host_clock: ClockDomainTrace,
    pub in_flight_images: u8,
    pub waited_for_image: bool,
    pub applied_back_pressure: bool,
    pub queue_idle_waited: bool,
    pub suboptimal: bool,
    pub submitted_present_id: u32,
    pub completed_present_id: u32,
    pub calibration_error_ns: u64,
    pub host_mapped: bool,
    pub audio: Option<OutputTimingSnapshot>,
}

pub fn timing_health(
    present: PresentStats,
    display_error_seconds: f32,
    display_catching_up: bool,
    audio: OutputTimingSnapshot,
) -> TimingHealth {
    let interval_ns = if present.actual_interval_ns != 0 {
        present.actual_interval_ns
    } else {
        present.refresh_ns
    };
    TimingHealth {
        interval_ns,
        display_error_ms: display_error_seconds * 1000.0,
        display_catching_up,
        present_mode: present.mode,
        display_clock: present.display_clock,
        host_clock: present.host_clock,
        in_flight_images: present.in_flight_images,
        waited_for_image: present.waited_for_image,
        applied_back_pressure: present.applied_back_pressure,
        queue_idle_waited: present.queue_idle_waited,
        suboptimal: present.suboptimal,
        submitted_present_id: present.submitted_present_id,
        completed_present_id: present.completed_present_id,
        calibration_error_ns: present.calibration_error_ns,
        host_mapped: present.host_present_ns != 0
            && present.display_clock != ClockDomainTrace::Unknown
            && present.host_clock != ClockDomainTrace::Unknown,
        audio: audio.has_measurement().then_some(audio),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_audio::{
        AudioOutputMode, OutputTelemetryBackend, OutputTelemetryClock, OutputTimingQuality,
    };

    fn audio_snapshot() -> OutputTimingSnapshot {
        OutputTimingSnapshot {
            backend: OutputTelemetryBackend::Unknown,
            requested_output_mode: AudioOutputMode::Auto,
            fallback_from_native: false,
            timing_clock: OutputTelemetryClock::Unknown,
            timing_quality: OutputTimingQuality::Unknown,
            sample_rate_hz: 0,
            device_period_ns: 0,
            stream_latency_ns: 0,
            buffer_frames: 0,
            padding_frames: 0,
            queued_frames: 0,
            estimated_output_delay_ns: 0,
            clock_fallback_count: 0,
            timing_sanity_failure_count: 0,
            underrun_count: 0,
        }
    }

    #[test]
    fn actual_present_interval_overrides_refresh_period() {
        let health = timing_health(
            PresentStats {
                refresh_ns: 16_666_667,
                actual_interval_ns: 8_333_333,
                ..PresentStats::default()
            },
            -0.0015,
            true,
            audio_snapshot(),
        );
        assert_eq!(health.interval_ns, 8_333_333);
        assert_eq!(health.display_error_ms, -1.5);
        assert!(health.display_catching_up);
    }

    #[test]
    fn refresh_period_is_used_before_first_actual_present() {
        let health = timing_health(
            PresentStats {
                refresh_ns: 16_666_667,
                ..PresentStats::default()
            },
            0.0,
            false,
            audio_snapshot(),
        );
        assert_eq!(health.interval_ns, 16_666_667);
    }

    #[test]
    fn host_mapping_requires_timestamp_and_known_clock_domains() {
        let mapped = timing_health(
            PresentStats {
                host_present_ns: 10,
                display_clock: ClockDomainTrace::Device,
                host_clock: ClockDomainTrace::Monotonic,
                ..PresentStats::default()
            },
            0.0,
            false,
            audio_snapshot(),
        );
        assert!(mapped.host_mapped);

        for present in [
            PresentStats {
                host_present_ns: 0,
                display_clock: ClockDomainTrace::Device,
                host_clock: ClockDomainTrace::Monotonic,
                ..PresentStats::default()
            },
            PresentStats {
                host_present_ns: 10,
                display_clock: ClockDomainTrace::Unknown,
                host_clock: ClockDomainTrace::Monotonic,
                ..PresentStats::default()
            },
            PresentStats {
                host_present_ns: 10,
                display_clock: ClockDomainTrace::Device,
                host_clock: ClockDomainTrace::Unknown,
                ..PresentStats::default()
            },
        ] {
            assert!(!timing_health(present, 0.0, false, audio_snapshot()).host_mapped);
        }
    }

    #[test]
    fn audio_snapshot_is_hidden_until_it_has_measurements() {
        let empty = timing_health(PresentStats::default(), 0.0, false, audio_snapshot());
        assert!(empty.audio.is_none());

        let measured = timing_health(
            PresentStats::default(),
            0.0,
            false,
            OutputTimingSnapshot {
                buffer_frames: 512,
                ..audio_snapshot()
            },
        );
        assert_eq!(measured.audio.map(|audio| audio.buffer_frames), Some(512));
    }

    #[test]
    fn renderer_queue_telemetry_is_copied_without_translation() {
        let health = timing_health(
            PresentStats {
                in_flight_images: 3,
                waited_for_image: true,
                applied_back_pressure: true,
                queue_idle_waited: true,
                suboptimal: true,
                submitted_present_id: 41,
                completed_present_id: 39,
                calibration_error_ns: 700,
                ..PresentStats::default()
            },
            0.0,
            false,
            audio_snapshot(),
        );
        assert_eq!(health.in_flight_images, 3);
        assert!(health.waited_for_image);
        assert!(health.applied_back_pressure);
        assert!(health.queue_idle_waited);
        assert!(health.suboptimal);
        assert_eq!(health.submitted_present_id, 41);
        assert_eq!(health.completed_present_id, 39);
        assert_eq!(health.calibration_error_ns, 700);
    }
}

// Frozen from 8da49e3d5, before diagnostic filtering changes.
impl AudioDiagEventSlot {
    pub(super) fn load_reference(&self) -> Option<(u64, StutterDiagAudioEvent)> {
        let version_start = self.version.load(Ordering::Acquire);
        if version_start == 0 || version_start & 1 != 0 {
            return None;
        }
        let event = StutterDiagAudioEvent {
            at_host_nanos: self.at_host_nanos.load(Ordering::Relaxed),
            kind: StutterDiagAudioEventKind::from_bits(self.kind.load(Ordering::Relaxed))?,
            value_ns: self.value_ns.load(Ordering::Relaxed),
            sample_rate_hz: self.sample_rate_hz.load(Ordering::Relaxed),
            buffer_frames: self.buffer_frames.load(Ordering::Relaxed),
            padding_frames: self.padding_frames.load(Ordering::Relaxed),
            queued_frames: self.queued_frames.load(Ordering::Relaxed),
            device_period_ns: self.device_period_ns.load(Ordering::Relaxed),
            estimated_output_delay_ns: self.estimated_output_delay_ns.load(Ordering::Relaxed),
            timing_quality: OutputTimingQuality::from_bits(
                self.timing_quality.load(Ordering::Relaxed),
            ),
        };
        let version_end = self.version.load(Ordering::Acquire);
        (version_start == version_end).then_some((version_end >> 1, event))
    }
}
impl AudioTelemetryState {
    pub(super) fn collect_stutter_diag_events_reference(
        &self,
        now_host_nanos: u64,
        window_ns: u64,
        out: &mut Vec<StutterDiagAudioEvent>,
    ) {
        let head = self.stutter_diag_event_head.load(Ordering::Acquire);
        let start = head.saturating_sub(AUDIO_STUTTER_DIAG_EVENT_COUNT as u64);
        for seq in (start + 1)..=head {
            let slot =
                &self.stutter_diag_events[(seq as usize - 1) % AUDIO_STUTTER_DIAG_EVENT_COUNT];
            let Some((loaded_seq, event)) = slot.load_reference() else {
                continue;
            };
            if loaded_seq != seq || event.at_host_nanos == 0 {
                continue;
            }
            if now_host_nanos.saturating_sub(event.at_host_nanos) <= window_ns {
                out.push(event);
            }
        }
    }
}

use deadsync_rules::timing::{
    DelaySegment, FakeSegment, ScrollSegment, SpeedSegment, SpeedUnit, StopSegment, TimingSegments,
    WarpSegment, default_time_signatures,
};
use rssp::timing as rssp_timing;

pub fn timing_segments_from_rssp(segments: &rssp_timing::TimingSegments) -> TimingSegments {
    let speeds = segments
        .speeds
        .iter()
        .map(|(beat, ratio, delay, unit)| SpeedSegment {
            beat: *beat,
            ratio: *ratio,
            delay: *delay,
            unit: match unit {
                rssp_timing::SpeedUnit::Beats => SpeedUnit::Beats,
                rssp_timing::SpeedUnit::Seconds => SpeedUnit::Seconds,
            },
        })
        .collect();

    TimingSegments {
        beat0_offset_adjust: segments.beat0_offset_adjust,
        bpms: segments.bpms.clone(),
        stops: segments
            .stops
            .iter()
            .map(|(beat, duration)| StopSegment {
                beat: *beat,
                duration: *duration,
            })
            .collect(),
        delays: segments
            .delays
            .iter()
            .map(|(beat, duration)| DelaySegment {
                beat: *beat,
                duration: *duration,
            })
            .collect(),
        warps: segments
            .warps
            .iter()
            .map(|(beat, length)| WarpSegment {
                beat: *beat,
                length: *length,
            })
            .collect(),
        speeds,
        scrolls: segments
            .scrolls
            .iter()
            .map(|(beat, ratio)| ScrollSegment {
                beat: *beat,
                ratio: *ratio,
            })
            .collect(),
        fakes: segments
            .fakes
            .iter()
            .map(|(beat, length)| FakeSegment {
                beat: *beat,
                length: *length,
            })
            .collect(),
        time_signatures: default_time_signatures(),
    }
}

#[cfg(test)]
mod tests {
    use super::timing_segments_from_rssp;
    use deadsync_rules::timing::{SpeedUnit, default_time_signature};
    use rssp::timing as rssp_timing;

    #[test]
    fn converts_rssp_timing_segments() {
        let source = rssp_timing::TimingSegments {
            beat0_offset_adjust: 0.25,
            bpms: vec![(0.0, 120.0), (48.0, 180.0)],
            stops: vec![(4.0, 0.5)],
            delays: vec![(8.0, 0.25)],
            warps: vec![(12.0, 4.0)],
            speeds: vec![
                (16.0, 2.0, 0.5, rssp_timing::SpeedUnit::Beats),
                (24.0, 1.5, 0.25, rssp_timing::SpeedUnit::Seconds),
            ],
            scrolls: vec![(32.0, 0.75)],
            fakes: vec![(40.0, 2.0)],
        };

        let converted = timing_segments_from_rssp(&source);

        assert_eq!(converted.beat0_offset_adjust, 0.25);
        assert_eq!(converted.bpms, vec![(0.0, 120.0), (48.0, 180.0)]);
        assert_eq!(converted.stops[0].beat, 4.0);
        assert_eq!(converted.stops[0].duration, 0.5);
        assert_eq!(converted.delays[0].beat, 8.0);
        assert_eq!(converted.delays[0].duration, 0.25);
        assert_eq!(converted.warps[0].beat, 12.0);
        assert_eq!(converted.warps[0].length, 4.0);
        assert_eq!(converted.speeds[0].unit, SpeedUnit::Beats);
        assert_eq!(converted.speeds[1].unit, SpeedUnit::Seconds);
        assert_eq!(converted.scrolls[0].ratio, 0.75);
        assert_eq!(converted.fakes[0].length, 2.0);
        let default_sig = default_time_signature();
        assert_eq!(converted.time_signatures.len(), 1);
        assert_eq!(converted.time_signatures[0].beat, default_sig.beat);
        assert_eq!(
            converted.time_signatures[0].numerator,
            default_sig.numerator
        );
        assert_eq!(
            converted.time_signatures[0].denominator,
            default_sig.denominator
        );
    }
}

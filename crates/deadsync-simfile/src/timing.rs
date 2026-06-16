use deadsync_core::timing::beat_to_note_row;
use deadsync_rules::timing::{
    DelaySegment, FakeSegment, ScrollSegment, SpeedSegment, SpeedUnit, StopSegment,
    TimeSignatureSegment, TimingSegments, WarpSegment, default_time_signatures,
};
use rssp::timing as rssp_timing;

pub fn parse_time_signatures(tag: Option<&str>) -> Vec<TimeSignatureSegment> {
    let Some(s) = tag.map(str::trim).filter(|s| !s.is_empty()) else {
        return default_time_signatures();
    };

    let mut out = Vec::new();
    for segment in s.split(',') {
        let mut parts = segment.trim().split('=');
        let (Some(beat), Some(numerator), Some(denominator)) =
            (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let (Ok(beat), Ok(numerator), Ok(denominator)) = (
            beat.trim().parse::<f32>(),
            numerator.trim().parse::<i32>(),
            denominator.trim().parse::<i32>(),
        ) else {
            continue;
        };
        if beat.is_finite() && numerator > 0 && denominator > 0 {
            out.push(TimeSignatureSegment {
                beat,
                numerator,
                denominator,
            });
        }
    }

    if out.is_empty() {
        return default_time_signatures();
    }

    out.sort_by(|a, b| {
        beat_to_note_row(a.beat)
            .cmp(&beat_to_note_row(b.beat))
            .then_with(|| a.beat.total_cmp(&b.beat))
    });
    out.dedup_by(|a, b| beat_to_note_row(a.beat) == beat_to_note_row(b.beat));
    if out.first().is_none_or(|seg| beat_to_note_row(seg.beat) > 0) {
        out.insert(0, default_time_signatures()[0]);
    }
    out
}

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

/// Inverse of [`timing_segments_from_rssp`]: convert deadsync timing segments
/// into the `rssp` timing-segment form so the parity/annotation engine can be
/// driven from deadsync chart data. Offsets are intentionally not encoded here;
/// callers that need absolute times use deadsync's own `TimingData` instead.
pub fn rssp_timing_segments_from_deadsync(
    segments: &TimingSegments,
) -> rssp_timing::TimingSegments {
    rssp_timing::TimingSegments {
        beat0_offset_adjust: segments.beat0_offset_adjust,
        bpms: segments.bpms.clone(),
        stops: segments.stops.iter().map(|s| (s.beat, s.duration)).collect(),
        delays: segments
            .delays
            .iter()
            .map(|s| (s.beat, s.duration))
            .collect(),
        warps: segments.warps.iter().map(|s| (s.beat, s.length)).collect(),
        speeds: segments
            .speeds
            .iter()
            .map(|s| {
                (
                    s.beat,
                    s.ratio,
                    s.delay,
                    match s.unit {
                        SpeedUnit::Beats => rssp_timing::SpeedUnit::Beats,
                        SpeedUnit::Seconds => rssp_timing::SpeedUnit::Seconds,
                    },
                )
            })
            .collect(),
        scrolls: segments.scrolls.iter().map(|s| (s.beat, s.ratio)).collect(),
        fakes: segments.fakes.iter().map(|s| (s.beat, s.length)).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_time_signatures, rssp_timing_segments_from_deadsync, timing_segments_from_rssp};
    use deadsync_rules::timing::{SpeedUnit, default_time_signature};
    use rssp::timing as rssp_timing;

    #[test]
    fn parse_time_signatures_filters_sorts_and_adds_default() {
        let signatures =
            parse_time_signatures(Some("8.000=3=4, bad, 4.000=7=8, 4.000=6=8, 12.000=0=4"));

        assert_eq!(signatures.len(), 3);
        assert_eq!(signatures[0].beat, 0.0);
        assert_eq!(signatures[0].numerator, 4);
        assert_eq!(signatures[1].beat, 4.0);
        assert_eq!(signatures[1].numerator, 7);
        assert_eq!(signatures[2].beat, 8.0);
        assert_eq!(signatures[2].numerator, 3);
    }

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

    #[test]
    fn deadsync_to_rssp_round_trips() {
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

        let deadsync = timing_segments_from_rssp(&source);
        let back = rssp_timing_segments_from_deadsync(&deadsync);

        assert_eq!(back.beat0_offset_adjust, source.beat0_offset_adjust);
        assert_eq!(back.bpms, source.bpms);
        assert_eq!(back.stops, source.stops);
        assert_eq!(back.delays, source.delays);
        assert_eq!(back.warps, source.warps);
        assert_eq!(back.scrolls, source.scrolls);
        assert_eq!(back.fakes, source.fakes);
        assert_eq!(back.speeds.len(), source.speeds.len());
        for (got, want) in back.speeds.iter().zip(source.speeds.iter()) {
            assert_eq!(got.0, want.0);
            assert_eq!(got.1, want.1);
            assert_eq!(got.2, want.2);
            assert_eq!(got.3, want.3);
        }
    }
}

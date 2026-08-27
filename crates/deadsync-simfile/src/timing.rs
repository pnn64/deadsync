use deadsync_core::timing::beat_to_note_row;
use deadsync_rules::timing::{
    ComboSegment, DelaySegment, FakeSegment, ScrollSegment, SpeedSegment, SpeedUnit, StopSegment,
    TickcountSegment, TimeSignatureSegment, TimingSegments, WarpSegment, default_combos,
    default_tickcounts, default_time_signatures,
};
use rssp::timing as rssp_timing;

fn parse_itg_int(value: &str) -> Option<i32> {
    // ITG uses atoi/std::stoi here, so values such as "8.000" parse as 8.
    let value = value.trim_start();
    let digit_start = usize::from(matches!(value.as_bytes().first(), Some(b'+' | b'-')));
    let digit_count = value.as_bytes()[digit_start..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    (digit_count != 0)
        .then(|| &value[..digit_start + digit_count])
        .and_then(|prefix| prefix.parse().ok())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrossoverAnnotation {
    pub beat: f32,
    pub column_mask: u8,
    pub left_foot_mask: u8,
    pub right_foot_mask: u8,
    pub crossover: bool,
    pub bracket: bool,
}

#[inline]
fn foot_masks(annotation: &rssp::RowAnnotation) -> (u8, u8) {
    let mut left = 0u8;
    let mut right = 0u8;
    for (lane, &foot) in annotation.feet().iter().enumerate() {
        let bit = 1u8 << lane;
        match foot {
            rssp::Foot::LeftHeel | rssp::Foot::LeftToe => left |= bit,
            rssp::Foot::RightHeel | rssp::Foot::RightToe => right |= bit,
            rssp::Foot::None => {}
        }
    }
    (left, right)
}

#[must_use]
pub fn parse_time_signatures(tag: Option<&str>) -> Vec<TimeSignatureSegment> {
    parse_time_signatures_as(
        tag,
        |beat, numerator, denominator| TimeSignatureSegment {
            beat,
            numerator,
            denominator,
        },
        |segment| segment.beat,
    )
}

pub(crate) fn parse_cached_time_signatures(tag: Option<&str>) -> Vec<(f32, i32, i32)> {
    parse_time_signatures_as(
        tag,
        |beat, numerator, denominator| (beat, numerator, denominator),
        |segment| segment.0,
    )
}

fn parse_time_signatures_as<T>(
    tag: Option<&str>,
    make: impl Fn(f32, i32, i32) -> T,
    beat: impl Fn(&T) -> f32,
) -> Vec<T> {
    let Some(s) = tag.map(str::trim).filter(|s| !s.is_empty()) else {
        return vec![make(0.0, 4, 4)];
    };

    let mut out = Vec::with_capacity(timing_segment_capacity(s));
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
            out.push(make(beat, numerator, denominator));
        }
    }

    if out.is_empty() {
        return vec![make(0.0, 4, 4)];
    }

    out.sort_by(|a, b| {
        beat_to_note_row(beat(a))
            .cmp(&beat_to_note_row(beat(b)))
            .then_with(|| beat(a).total_cmp(&beat(b)))
    });
    out.dedup_by(|a, b| beat_to_note_row(beat(a)) == beat_to_note_row(beat(b)));
    if out
        .first()
        .is_none_or(|segment| beat_to_note_row(beat(segment)) > 0)
    {
        out.insert(0, make(0.0, 4, 4));
    }
    out
}

#[must_use]
pub fn parse_tickcounts(tag: Option<&str>) -> Vec<TickcountSegment> {
    parse_tickcounts_as(
        tag,
        |beat, ticks| TickcountSegment { beat, ticks },
        |segment| segment.beat,
    )
}

pub(crate) fn parse_cached_tickcounts(tag: Option<&str>) -> Vec<(f32, u8)> {
    parse_tickcounts_as(tag, |beat, ticks| (beat, ticks), |segment| segment.0)
}

fn parse_tickcounts_as<T>(
    tag: Option<&str>,
    make: impl Fn(f32, u8) -> T,
    beat: impl Fn(&T) -> f32,
) -> Vec<T> {
    let Some(s) = tag.map(str::trim).filter(|s| !s.is_empty()) else {
        return vec![make(0.0, 4)];
    };

    let mut out = Vec::with_capacity(timing_segment_capacity(s));
    for segment in s.split(',') {
        let mut parts = segment.trim().split('=');
        let (Some(beat), Some(ticks)) = (parts.next(), parts.next()) else {
            continue;
        };
        let (Ok(beat), Some(ticks)) = (beat.trim().parse::<f32>(), parse_itg_int(ticks)) else {
            continue;
        };
        if beat.is_finite() {
            out.push(make(beat, ticks.clamp(0, 48) as u8));
        }
    }

    if out.is_empty() {
        return vec![make(0.0, 4)];
    }

    out.sort_by(|a, b| {
        beat_to_note_row(beat(a))
            .cmp(&beat_to_note_row(beat(b)))
            .then_with(|| beat(a).total_cmp(&beat(b)))
    });
    dedup_last_by_row(&mut out, &beat);
    if out
        .first()
        .is_none_or(|segment| beat_to_note_row(beat(segment)) > 0)
    {
        out.insert(0, make(0.0, 4));
    }
    out
}

#[must_use]
pub fn parse_combos(tag: Option<&str>) -> Vec<ComboSegment> {
    parse_combos_as(
        tag,
        |beat, combo, miss_combo| ComboSegment {
            beat,
            combo,
            miss_combo,
        },
        |segment| segment.beat,
    )
}

pub(crate) fn parse_cached_combos(tag: Option<&str>) -> Vec<(f32, u32, u32)> {
    parse_combos_as(
        tag,
        |beat, combo, miss_combo| (beat, combo, miss_combo),
        |segment| segment.0,
    )
}

fn parse_combos_as<T>(
    tag: Option<&str>,
    make: impl Fn(f32, u32, u32) -> T,
    beat: impl Fn(&T) -> f32,
) -> Vec<T> {
    let Some(s) = tag.map(str::trim).filter(|s| !s.is_empty()) else {
        return vec![make(0.0, 1, 1)];
    };

    let mut out = Vec::with_capacity(timing_segment_capacity(s));
    for segment in s.split(',') {
        let mut parts = segment.trim().split('=');
        let (Some(beat), Some(combo)) = (parts.next(), parts.next()) else {
            continue;
        };
        let (Ok(beat), Some(combo)) = (beat.trim().parse::<f32>(), parse_itg_int(combo)) else {
            continue;
        };
        let miss_combo = parts.next().and_then(parse_itg_int).unwrap_or(combo);
        if beat.is_finite() {
            out.push(make(beat, combo.max(0) as u32, miss_combo.max(0) as u32));
        }
    }

    if out.is_empty() {
        return vec![make(0.0, 1, 1)];
    }

    out.sort_by(|a, b| {
        beat_to_note_row(beat(a))
            .cmp(&beat_to_note_row(beat(b)))
            .then_with(|| beat(a).total_cmp(&beat(b)))
    });
    dedup_last_by_row(&mut out, &beat);
    if out
        .first()
        .is_none_or(|segment| beat_to_note_row(beat(segment)) > 0)
    {
        out.insert(0, make(0.0, 1, 1));
    }
    out
}

fn timing_segment_capacity(tag: &str) -> usize {
    tag.as_bytes()
        .iter()
        .filter(|&&byte| byte == b',')
        .count()
        .saturating_add(2)
}

fn dedup_last_by_row<T>(segments: &mut Vec<T>, beat: impl Fn(&T) -> f32) {
    let mut write = 0usize;
    for read in 0..segments.len() {
        if write != 0
            && beat_to_note_row(beat(&segments[write - 1]))
                == beat_to_note_row(beat(&segments[read]))
        {
            segments.swap(write - 1, read);
        } else {
            segments.swap(write, read);
            write += 1;
        }
    }
    segments.truncate(write);
}

#[cfg(any(test, feature = "bench-support"))]
pub(crate) fn parse_time_signatures_baseline(tag: Option<&str>) -> Vec<TimeSignatureSegment> {
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
    if out
        .first()
        .is_none_or(|segment| beat_to_note_row(segment.beat) > 0)
    {
        out.insert(0, default_time_signatures()[0]);
    }
    out
}

#[cfg(any(test, feature = "bench-support"))]
pub(crate) fn parse_tickcounts_baseline(tag: Option<&str>) -> Vec<TickcountSegment> {
    let Some(s) = tag.map(str::trim).filter(|s| !s.is_empty()) else {
        return default_tickcounts();
    };
    let mut out = Vec::new();
    for segment in s.split(',') {
        let mut parts = segment.trim().split('=');
        let (Some(beat), Some(ticks)) = (parts.next(), parts.next()) else {
            continue;
        };
        let (Ok(beat), Some(ticks)) = (beat.trim().parse::<f32>(), parse_itg_int(ticks)) else {
            continue;
        };
        if beat.is_finite() {
            out.push(TickcountSegment {
                beat,
                ticks: ticks.clamp(0, 48) as u8,
            });
        }
    }
    if out.is_empty() {
        return default_tickcounts();
    }
    out.sort_by(|a, b| {
        beat_to_note_row(a.beat)
            .cmp(&beat_to_note_row(b.beat))
            .then_with(|| a.beat.total_cmp(&b.beat))
    });
    let mut deduped = Vec::with_capacity(out.len());
    for segment in out {
        if deduped.last().is_some_and(|last: &TickcountSegment| {
            beat_to_note_row(last.beat) == beat_to_note_row(segment.beat)
        }) {
            let last = deduped.len() - 1;
            deduped[last] = segment;
        } else {
            deduped.push(segment);
        }
    }
    if deduped
        .first()
        .is_none_or(|segment| beat_to_note_row(segment.beat) > 0)
    {
        deduped.insert(0, default_tickcounts()[0]);
    }
    deduped
}

#[cfg(any(test, feature = "bench-support"))]
pub(crate) fn parse_combos_baseline(tag: Option<&str>) -> Vec<ComboSegment> {
    let Some(s) = tag.map(str::trim).filter(|s| !s.is_empty()) else {
        return default_combos();
    };
    let mut out = Vec::new();
    for segment in s.split(',') {
        let mut parts = segment.trim().split('=');
        let (Some(beat), Some(combo)) = (parts.next(), parts.next()) else {
            continue;
        };
        let (Ok(beat), Some(combo)) = (beat.trim().parse::<f32>(), parse_itg_int(combo)) else {
            continue;
        };
        let miss_combo = parts.next().and_then(parse_itg_int).unwrap_or(combo);
        if beat.is_finite() {
            out.push(ComboSegment {
                beat,
                combo: combo.max(0) as u32,
                miss_combo: miss_combo.max(0) as u32,
            });
        }
    }
    if out.is_empty() {
        return default_combos();
    }
    out.sort_by(|a, b| {
        beat_to_note_row(a.beat)
            .cmp(&beat_to_note_row(b.beat))
            .then_with(|| a.beat.total_cmp(&b.beat))
    });
    let mut deduped = Vec::with_capacity(out.len());
    for segment in out {
        if deduped.last().is_some_and(|last: &ComboSegment| {
            beat_to_note_row(last.beat) == beat_to_note_row(segment.beat)
        }) {
            let last = deduped.len() - 1;
            deduped[last] = segment;
        } else {
            deduped.push(segment);
        }
    }
    if deduped
        .first()
        .is_none_or(|segment| beat_to_note_row(segment.beat) > 0)
    {
        deduped.insert(0, default_combos()[0]);
    }
    deduped
}

#[cfg(feature = "bench-support")]
#[must_use]
pub fn benchmark_timing_tags_baseline(
    time_signatures: &str,
    tickcounts: &str,
    combos: &str,
    rounds: usize,
) -> u64 {
    (0..rounds).fold(0u64, |checksum, _| {
        let time_signatures: Vec<_> = parse_time_signatures_baseline(Some(time_signatures))
            .into_iter()
            .map(|segment| (segment.beat, segment.numerator, segment.denominator))
            .collect();
        let tickcounts: Vec<_> = parse_tickcounts_baseline(Some(tickcounts))
            .into_iter()
            .map(|segment| (segment.beat, segment.ticks))
            .collect();
        let combos: Vec<_> = parse_combos_baseline(Some(combos))
            .into_iter()
            .map(|segment| (segment.beat, segment.combo, segment.miss_combo))
            .collect();
        checksum.wrapping_add(timing_tag_checksum(&time_signatures, &tickcounts, &combos))
    })
}

#[cfg(feature = "bench-support")]
#[must_use]
pub fn benchmark_timing_tags_current(
    time_signatures: &str,
    tickcounts: &str,
    combos: &str,
    rounds: usize,
) -> u64 {
    (0..rounds).fold(0u64, |checksum, _| {
        let time_signatures = parse_cached_time_signatures(Some(time_signatures));
        let tickcounts = parse_cached_tickcounts(Some(tickcounts));
        let combos = parse_cached_combos(Some(combos));
        checksum.wrapping_add(timing_tag_checksum(&time_signatures, &tickcounts, &combos))
    })
}

#[cfg(feature = "bench-support")]
fn timing_tag_checksum(
    time_signatures: &[(f32, i32, i32)],
    tickcounts: &[(f32, u8)],
    combos: &[(f32, u32, u32)],
) -> u64 {
    let time_signatures = time_signatures.iter().fold(0u64, |checksum, segment| {
        checksum
            .wrapping_mul(31)
            .wrapping_add(u64::from(segment.0.to_bits()))
            .wrapping_add(segment.1 as u64)
            .wrapping_add(segment.2 as u64)
    });
    let tickcounts = tickcounts.iter().fold(0u64, |checksum, segment| {
        checksum
            .wrapping_mul(31)
            .wrapping_add(u64::from(segment.0.to_bits()))
            .wrapping_add(u64::from(segment.1))
    });
    combos
        .iter()
        .fold(time_signatures ^ tickcounts, |checksum, segment| {
            checksum
                .wrapping_mul(31)
                .wrapping_add(u64::from(segment.0.to_bits()))
                .wrapping_add(u64::from(segment.1))
                .wrapping_add(u64::from(segment.2))
        })
}

#[must_use]
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
        tickcounts: default_tickcounts(),
        combos: default_combos(),
    }
}

/// Inverse of [`timing_segments_from_rssp`]: convert deadsync timing segments
/// into the `rssp` timing-segment form so the parity/annotation engine can be
/// driven from deadsync chart data. Offsets are intentionally not encoded here;
/// callers that need absolute times use deadsync's own `TimingData` instead.
fn rssp_timing_segments_from_deadsync(segments: &TimingSegments) -> rssp_timing::TimingSegments {
    rssp_timing::TimingSegments {
        beat0_offset_adjust: segments.beat0_offset_adjust,
        bpms: segments.bpms.clone(),
        stops: segments
            .stops
            .iter()
            .map(|s| (s.beat, s.duration))
            .collect(),
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

#[must_use]
pub fn crossover_annotations<const LANES: usize>(
    rows: &[[u8; LANES]],
    row_to_beat: &[f32],
    segments: &TimingSegments,
) -> Vec<CrossoverAnnotation> {
    let rssp_segments = rssp_timing_segments_from_deadsync(segments);
    let timing = rssp_timing::timing_data_from_segments(0.0, 0.0, &rssp_segments);
    let Some(mut scratch) = rssp::step_parity::timing_rows_scratch::<LANES>() else {
        return Vec::new();
    };
    rssp::step_parity::annotate_timing_rows(rows, row_to_beat, &timing, &mut scratch)
        .into_iter()
        .map(|annotation| {
            let (left_foot_mask, right_foot_mask) = foot_masks(&annotation);
            CrossoverAnnotation {
                beat: annotation.beat,
                column_mask: annotation.column_mask,
                left_foot_mask,
                right_foot_mask,
                crossover: annotation.row_tech.crossovers > 0,
                bracket: annotation.foot_count() > 1,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        crossover_annotations, parse_cached_combos, parse_cached_tickcounts,
        parse_cached_time_signatures, parse_combos, parse_combos_baseline, parse_tickcounts,
        parse_tickcounts_baseline, parse_time_signatures, parse_time_signatures_baseline,
        rssp_timing_segments_from_deadsync, timing_segments_from_rssp,
    };
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
    fn parse_tickcounts_filters_sorts_clamps_and_adds_default() {
        let tickcounts = parse_tickcounts(Some("8.000=2, bad, 4.000=99, 4.000=3.000, 12.000=-2"));

        assert_eq!(tickcounts.len(), 4);
        assert_eq!((tickcounts[0].beat, tickcounts[0].ticks), (0.0, 4));
        assert_eq!((tickcounts[1].beat, tickcounts[1].ticks), (4.0, 3));
        assert_eq!((tickcounts[2].beat, tickcounts[2].ticks), (8.0, 2));
        assert_eq!((tickcounts[3].beat, tickcounts[3].ticks), (12.0, 0));
    }

    #[test]
    fn parse_combos_matches_itg_two_and_three_value_forms() {
        let combos = parse_combos(Some("8.000=2.000, bad, 4.000=3=5, 4.000=4.000=6.000"));

        assert_eq!(combos.len(), 3);
        assert_eq!(
            (combos[0].beat, combos[0].combo, combos[0].miss_combo),
            (0.0, 1, 1)
        );
        assert_eq!(
            (combos[1].beat, combos[1].combo, combos[1].miss_combo),
            (4.0, 4, 6)
        );
        assert_eq!(
            (combos[2].beat, combos[2].combo, combos[2].miss_combo),
            (8.0, 2, 2)
        );
    }

    #[test]
    fn cached_timing_parsers_match_baseline_for_edge_cases() {
        let tags = [
            None,
            Some(""),
            Some("bad"),
            Some("8=3=4,4=7=8,4=6=8,-2=5=16,12=0=4"),
        ];
        for tag in tags {
            let time_signatures: Vec<_> = parse_time_signatures_baseline(tag)
                .into_iter()
                .map(|segment| (segment.beat, segment.numerator, segment.denominator))
                .collect();
            assert_eq!(parse_cached_time_signatures(tag), time_signatures);
            assert_eq!(
                parse_time_signatures(tag)
                    .into_iter()
                    .map(|segment| (segment.beat, segment.numerator, segment.denominator))
                    .collect::<Vec<_>>(),
                time_signatures
            );

            let tickcounts: Vec<_> = parse_tickcounts_baseline(tag)
                .into_iter()
                .map(|segment| (segment.beat, segment.ticks))
                .collect();
            assert_eq!(parse_cached_tickcounts(tag), tickcounts);
            assert_eq!(
                parse_tickcounts(tag)
                    .into_iter()
                    .map(|segment| (segment.beat, segment.ticks))
                    .collect::<Vec<_>>(),
                tickcounts
            );

            let combos: Vec<_> = parse_combos_baseline(tag)
                .into_iter()
                .map(|segment| (segment.beat, segment.combo, segment.miss_combo))
                .collect();
            assert_eq!(parse_cached_combos(tag), combos);
            assert_eq!(
                parse_combos(tag)
                    .into_iter()
                    .map(|segment| (segment.beat, segment.combo, segment.miss_combo))
                    .collect::<Vec<_>>(),
                combos
            );
        }
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

    #[test]
    fn crossover_annotations_hide_rssp_rows_behind_domain_data() {
        let rows = [*b"1000", *b"0100"];
        let beats = [0.0, 1.0];
        let segments = deadsync_rules::timing::TimingSegments {
            bpms: vec![(0.0, 120.0)],
            ..deadsync_rules::timing::TimingSegments::default()
        };

        let annotations = crossover_annotations(&rows, &beats, &segments);

        assert_eq!(annotations.len(), 2);
        assert_eq!(annotations[0].beat, 0.0);
        assert_eq!(annotations[0].column_mask, 0b0001);
        assert_eq!(
            annotations[0].left_foot_mask | annotations[0].right_foot_mask,
            annotations[0].column_mask
        );
        assert_eq!(
            annotations[1].left_foot_mask | annotations[1].right_foot_mask,
            annotations[1].column_mask
        );
        assert_eq!(
            annotations[0].left_foot_mask & annotations[0].right_foot_mask,
            0
        );
        assert!(!annotations[0].bracket);
        assert_eq!(annotations[1].beat, 1.0);
        assert_eq!(annotations[1].column_mask, 0b0010);
        assert!(!annotations[1].bracket);
    }

    #[test]
    fn crossover_annotations_keep_each_foot_lane() {
        let rows = [*b"1001"];
        let beats = [0.0];
        let segments = deadsync_rules::timing::TimingSegments {
            bpms: vec![(0.0, 120.0)],
            ..deadsync_rules::timing::TimingSegments::default()
        };

        let annotations = crossover_annotations(&rows, &beats, &segments);

        assert_eq!(annotations.len(), 1);
        assert_ne!(annotations[0].left_foot_mask, 0);
        assert_ne!(annotations[0].right_foot_mask, 0);
        assert_eq!(
            annotations[0].left_foot_mask | annotations[0].right_foot_mask,
            0b1001
        );
    }
}

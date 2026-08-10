use deadsync_core::timing::ROWS_PER_BEAT;

const RANGE_GUARD_ROWS: i32 = ROWS_PER_BEAT * 4;

pub(crate) fn expand_range(range: Option<(i32, i32)>) -> Option<(i32, i32)> {
    range.map(|(low, high)| {
        (
            low.saturating_sub(RANGE_GUARD_ROWS),
            high.saturating_add(RANGE_GUARD_ROWS),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{RANGE_GUARD_ROWS, expand_range};
    use crate::{AccelYParams, ScrollTravelRequest, scroll_travel};
    use deadsync_core::song_time::song_time_ns_add_seconds;
    use deadsync_rules::scroll::ScrollSpeedSetting;
    use deadsync_rules::timing::{ScrollSegment, TimingData, TimingSegments};

    #[test]
    fn range_expansion_is_saturating() {
        assert_eq!(
            expand_range(Some((i32::MIN, i32::MAX))),
            Some((i32::MIN, i32::MAX))
        );
        assert_eq!(
            expand_range(Some((100, 200))),
            Some((100 - RANGE_GUARD_ROWS, 200 + RANGE_GUARD_ROWS))
        );
    }

    fn travel<'a>(
        timing: &'a TimingData,
        speed: ScrollSpeedSetting,
        accel: AccelYParams,
        time_ns: i64,
        elapsed_screen_s: f32,
    ) -> crate::ScrollTravel<'a> {
        let beat = timing.get_beat_for_time_ns(time_ns);
        scroll_travel(ScrollTravelRequest {
            timing,
            accel,
            scroll_speed: speed,
            current_time_ns: time_ns,
            visible_beat: beat,
            search_beat: beat,
            scroll_reference_bpm: 180.0,
            music_rate: 1.0,
            edit_beat_spacing: false,
            draw_distance_after_targets: 320.0,
            draw_distance_before_targets: 640.0,
            field_zoom: 1.0,
            elapsed_screen_s,
            effect_height: 640.0,
            screen_height: 720.0,
            note_count_stats: &[],
            arrow_effect_time_s: elapsed_screen_s,
            lane_tipsy: 0.0,
            lane_move_y: &[],
        })
    }

    #[test]
    fn opening_note_stays_in_expanded_range_during_zero_scroll_lead_in() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 120.0)],
                scrolls: vec![
                    ScrollSegment {
                        beat: 0.0,
                        ratio: 0.0,
                    },
                    ScrollSegment {
                        beat: 4.0,
                        ratio: 1.0,
                    },
                ],
                ..TimingSegments::default()
            },
            &[],
        );
        let start = timing.get_time_for_beat_ns(-12.0);
        for frame in 0..600 {
            let elapsed = frame as f32 / 120.0;
            let time_ns = song_time_ns_add_seconds(start, elapsed);
            let travel = travel(
                &timing,
                ScrollSpeedSetting::XMod(1.0),
                AccelYParams::default(),
                time_ns,
                elapsed,
            );
            let exact = travel.visible_row_range().expect("exact range");
            assert!(
                exact.0 <= 0 && exact.1 >= 0,
                "opening row should remain visible at frame {frame}: {exact:?}"
            );
            let expanded = expand_range(Some(exact)).expect("expanded range");
            assert!(
                expanded.0 <= 0 && expanded.1 >= 0,
                "opening row fell out of the expanded range at frame {frame}: {expanded:?}"
            );
        }
    }
}

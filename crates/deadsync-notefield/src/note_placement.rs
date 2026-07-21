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

#[cfg(feature = "bench-support")]
mod bench {
    use super::expand_range;
    use crate::{
        AccelYParams, ScrollTravel, ScrollTravelRequest, appearance_note_actor_alpha,
        appearance_note_actor_alpha_from_alpha, appearance_note_alpha, appearance_note_glow,
        appearance_note_glow_from_alpha, for_each_visible_note_index,
        for_each_visible_note_index_legacy, scroll_travel,
    };
    use deadsync_core::note::NoteType;
    use deadsync_core::song_time::song_time_ns_add_seconds;
    use deadsync_core::timing::beat_to_note_row;
    use deadsync_rules::note::Note;
    use deadsync_rules::scroll::ScrollSpeedSetting;
    use deadsync_rules::timing::{TimingData, TimingSegments};

    const LANES: usize = 4;
    const FRAME_RATE: f32 = 120.0;
    const SONG_FRAMES: usize = 120 * 90;
    const DRAW_AFTER: f32 = 320.0;
    const DRAW_BEFORE: f32 = 640.0;

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct PlacementBenchFrame {
        pub checksum: u64,
        pub placements: usize,
    }

    pub struct PlacementBench {
        timing: TimingData,
        notes: Vec<Note>,
        note_itg_rows: Vec<i32>,
        lanes: [Vec<usize>; LANES],
    }

    impl Default for PlacementBench {
        fn default() -> Self {
            let timing = TimingData::from_segments(
                0.0,
                0.0,
                &TimingSegments {
                    bpms: vec![(0.0, 120.0), (64.0, 180.0), (128.0, 90.0)],
                    ..TimingSegments::default()
                },
                &[],
            );
            let mut notes = Vec::with_capacity(256 * 16 * LANES);
            let mut lanes: [Vec<usize>; LANES] = std::array::from_fn(|_| Vec::new());
            for row in 0..256 * 16 {
                let beat = row as f32 / 16.0;
                for (lane, indices) in lanes.iter_mut().enumerate() {
                    let note_index = notes.len();
                    notes.push(Note {
                        beat,
                        quantization_idx: (row % 8) as u8,
                        column: lane,
                        note_type: NoteType::Tap,
                        row_index: beat_to_note_row(beat).max(0) as usize,
                        result: None,
                        early_result: None,
                        hold: None,
                        mine_result: None,
                        is_fake: false,
                        can_be_judged: true,
                    });
                    indices.push(note_index);
                }
            }
            let note_itg_rows = notes
                .iter()
                .map(|note| beat_to_note_row(note.beat))
                .collect();
            Self {
                timing,
                notes,
                note_itg_rows,
                lanes,
            }
        }
    }

    impl PlacementBench {
        pub fn old_frame(&self, frame: usize) -> PlacementBenchFrame {
            let travel = bench_travel(&self.timing, frame);
            let range = travel.visible_row_range();
            let _hold_range = travel.visible_row_range();
            let mut output = PlacementBenchFrame::default();
            for (local_col, indices) in self.lanes.iter().enumerate() {
                for_each_visible_note_index_legacy(indices, &self.notes, range, |note_index| {
                    let note = &self.notes[note_index];
                    let raw = travel.raw_note(note, false);
                    let adjusted = travel.adjusted(raw);
                    if !(-DRAW_AFTER..=DRAW_BEFORE).contains(&adjusted) {
                        return;
                    }
                    let lane_offset = travel.lane_offset(local_col);
                    let y = travel.lane_y(local_col, 160.0, 1.0, raw);
                    let alpha = appearance_note_actor_alpha(
                        travel.adjusted(raw) + lane_offset,
                        frame as f32 / FRAME_RATE,
                        0.0,
                        bench_alpha_params(),
                    );
                    let glow = appearance_note_glow(
                        travel.adjusted(raw) + lane_offset,
                        frame as f32 / FRAME_RATE,
                        0.0,
                        bench_alpha_params(),
                    );
                    if alpha <= f32::EPSILON && glow <= f32::EPSILON {
                        return;
                    }
                    let x = 320.0 + local_col as f32 * 64.0 + travel.adjusted(raw) * 0.1;
                    add_output(&mut output, note_index, adjusted, x, y, alpha, glow);
                });
            }
            output
        }

        pub fn new_frame(&self, frame: usize) -> PlacementBenchFrame {
            let travel = bench_travel(&self.timing, frame);
            let range = expand_range(travel.visible_row_range());
            let mut output = PlacementBenchFrame::default();
            for (local_col, indices) in self.lanes.iter().enumerate() {
                for_each_visible_note_index(indices, &self.note_itg_rows, range, |note_index| {
                    let note = &self.notes[note_index];
                    let adjusted = travel.adjusted(travel.raw_note(note, false));
                    if !(-DRAW_AFTER..=DRAW_BEFORE).contains(&adjusted) {
                        return;
                    }
                    let lane_offset = travel.lane_offset(local_col);
                    let percent = appearance_note_alpha(
                        adjusted + lane_offset,
                        frame as f32 / FRAME_RATE,
                        0.0,
                        bench_alpha_params(),
                    );
                    let actor_alpha = appearance_note_actor_alpha_from_alpha(percent);
                    let glow_alpha = appearance_note_glow_from_alpha(percent);
                    if actor_alpha <= f32::EPSILON && glow_alpha <= f32::EPSILON {
                        return;
                    }
                    add_output(
                        &mut output,
                        note_index,
                        adjusted,
                        320.0 + local_col as f32 * 64.0 + adjusted * 0.1,
                        160.0 + adjusted + lane_offset,
                        actor_alpha,
                        glow_alpha,
                    );
                });
            }
            output
        }
    }

    fn bench_travel(timing: &TimingData, frame: usize) -> ScrollTravel<'_> {
        let frame = frame % SONG_FRAMES;
        let elapsed = frame as f32 / FRAME_RATE;
        let start = timing.get_time_for_beat_ns(4.0);
        let time_ns = song_time_ns_add_seconds(start, elapsed);
        let beat = timing.get_beat_for_time_ns(time_ns);
        scroll_travel(ScrollTravelRequest {
            timing,
            accel: AccelYParams {
                boost: 0.25,
                brake: 0.1,
                wave: 0.2,
                boomerang: 0.0,
                expand: 0.0,
            },
            scroll_speed: ScrollSpeedSetting::XMod(1.75),
            current_time_ns: time_ns,
            visible_beat: beat,
            search_beat: beat,
            scroll_reference_bpm: 180.0,
            music_rate: 1.0,
            edit_beat_spacing: false,
            draw_distance_after_targets: DRAW_AFTER,
            draw_distance_before_targets: DRAW_BEFORE,
            field_zoom: 1.0,
            elapsed_screen_s: elapsed,
            effect_height: 640.0,
            screen_height: 720.0,
            note_count_stats: &[],
            arrow_effect_time_s: elapsed,
            lane_tipsy: 0.0,
            lane_move_y: &[],
        })
    }

    fn bench_alpha_params() -> crate::NoteAlphaParams {
        crate::NoteAlphaParams {
            hidden: 0.35,
            hidden_offset: 0.1,
            sudden: 0.2,
            sudden_offset: -0.1,
            stealth: 0.0,
            blink: 0.0,
            random_vanish: 0.15,
        }
    }

    fn add_output(
        output: &mut PlacementBenchFrame,
        note_index: usize,
        adjusted: f32,
        x: f32,
        y: f32,
        alpha: f32,
        glow: f32,
    ) {
        let values = [
            note_index as u64,
            u64::from(adjusted.to_bits()),
            u64::from(x.to_bits()),
            u64::from(y.to_bits()),
            u64::from(alpha.to_bits()),
            u64::from(glow.to_bits()),
        ];
        for value in values {
            output.checksum = output.checksum.rotate_left(7) ^ value;
        }
        output.placements += 1;
    }

    #[cfg(test)]
    mod tests {
        use super::{PlacementBench, SONG_FRAMES};

        #[test]
        fn old_and_direct_frames_are_bit_exact_for_full_song_sweep() {
            let bench = PlacementBench::default();
            for frame in 0..SONG_FRAMES {
                let old = bench.old_frame(frame);
                let new = bench.new_frame(frame);
                assert_eq!(new, old, "frame {frame}");
            }
        }
    }
}

#[cfg(feature = "bench-support")]
pub use bench::{PlacementBench, PlacementBenchFrame};

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

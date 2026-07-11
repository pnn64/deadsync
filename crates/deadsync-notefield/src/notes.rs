use crate::holds::song_time_ns_delta_seconds;
use crate::measure_lines::{beat_scroll_travel, edit_beat_scroll_travel};
use crate::transforms::{
    AccelYParams, apply_accel_y, apply_accel_y_with_peak, move_col_extra, tipsy_y_extra,
};
use deadsync_core::song_time::SongTimeNs;
use deadsync_core::timing::beat_to_note_row;
use deadsync_rules::note::{MineResult, Note, NoteCountStat};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::timing::TimingData;

#[derive(Clone, Copy, Debug)]
pub struct ScrollTravelRequest<'a> {
    pub timing: &'a TimingData,
    pub accel: AccelYParams,
    pub scroll_speed: ScrollSpeedSetting,
    pub current_time_ns: SongTimeNs,
    pub visible_beat: f32,
    pub search_beat: f32,
    pub scroll_reference_bpm: f32,
    pub music_rate: f32,
    pub edit_beat_spacing: bool,
    pub draw_distance_after_targets: f32,
    pub draw_distance_before_targets: f32,
    pub field_zoom: f32,
    pub elapsed_screen_s: f32,
    pub effect_height: f32,
    pub screen_height: f32,
    pub note_count_stats: &'a [NoteCountStat],
    pub arrow_effect_time_s: f32,
    pub lane_tipsy: f32,
    pub lane_move_y: &'a [f32],
}

#[derive(Clone, Copy, Debug)]
enum RawTravel {
    Edit {
        current_beat: f32,
    },
    Constant {
        current_time_ns: SongTimeNs,
        rate: f32,
        beats_per_second: f32,
    },
    Beat {
        current_displayed_beat: f32,
        displayed_speed_percent: f32,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct ScrollTravel<'a> {
    request: ScrollTravelRequest<'a>,
    raw: RawTravel,
    displayed_speed_percent: f32,
    post_accel_scale: f32,
}

pub fn scroll_travel<'a>(request: ScrollTravelRequest<'a>) -> ScrollTravel<'a> {
    let displayed_speed_percent = request
        .timing
        .get_speed_multiplier_ns(request.visible_beat, request.current_time_ns);
    let (raw, post_accel_scale) = if request.edit_beat_spacing {
        let player_multiplier = request
            .scroll_speed
            .beat_multiplier(request.scroll_reference_bpm, request.music_rate);
        (
            RawTravel::Edit {
                current_beat: request.visible_beat,
            },
            request.field_zoom * player_multiplier,
        )
    } else {
        match request.scroll_speed {
            ScrollSpeedSetting::CMod(c_bpm) => {
                let rate = if request.music_rate.is_finite() && request.music_rate > 0.0 {
                    request.music_rate
                } else {
                    1.0
                };
                (
                    RawTravel::Constant {
                        current_time_ns: request.current_time_ns,
                        rate,
                        beats_per_second: c_bpm / 60.0,
                    },
                    request.field_zoom,
                )
            }
            ScrollSpeedSetting::XMod(_) | ScrollSpeedSetting::MMod(_) => {
                let player_multiplier = request
                    .scroll_speed
                    .beat_multiplier(request.scroll_reference_bpm, request.music_rate);
                (
                    RawTravel::Beat {
                        current_displayed_beat: request
                            .timing
                            .get_displayed_beat(request.visible_beat),
                        displayed_speed_percent,
                    },
                    request.field_zoom * player_multiplier,
                )
            }
        }
    };
    ScrollTravel {
        request,
        raw,
        displayed_speed_percent,
        post_accel_scale,
    }
}

impl ScrollTravel<'_> {
    pub fn raw_beat(&self, beat: f32) -> f32 {
        match self.raw {
            RawTravel::Edit { current_beat } => edit_beat_scroll_travel(beat, current_beat),
            RawTravel::Constant {
                current_time_ns,
                rate,
                beats_per_second,
            } => {
                let note_time_ns = self.request.timing.get_time_for_beat_ns(beat);
                let real_seconds = song_time_ns_delta_seconds(note_time_ns, current_time_ns) / rate;
                real_seconds * beats_per_second * ScrollSpeedSetting::ARROW_SPACING
            }
            RawTravel::Beat {
                current_displayed_beat,
                displayed_speed_percent,
            } => beat_scroll_travel(
                self.request.timing.get_displayed_beat(beat),
                current_displayed_beat,
                displayed_speed_percent,
            ),
        }
    }

    pub fn raw_note(&self, note: &Note, use_hold_end: bool) -> f32 {
        let beat = if use_hold_end {
            note.hold.as_ref().map_or(note.beat, |hold| hold.end_beat)
        } else {
            note.beat
        };
        self.raw_beat(beat)
    }

    pub fn adjusted_with_peak(&self, raw_travel: f32) -> (f32, bool) {
        let (travel, before_peak) = apply_accel_y_with_peak(
            raw_travel,
            self.request.elapsed_screen_s,
            self.request.effect_height,
            self.request.screen_height,
            self.request.accel,
        );
        (travel * self.post_accel_scale, before_peak)
    }

    pub fn adjusted(&self, raw_travel: f32) -> f32 {
        apply_accel_y(
            raw_travel,
            self.request.elapsed_screen_s,
            self.request.effect_height,
            self.request.screen_height,
            self.request.accel,
        ) * self.post_accel_scale
    }

    pub fn lane_offset(&self, local_col: usize) -> f32 {
        tipsy_y_extra(
            local_col,
            self.request.arrow_effect_time_s,
            self.request.lane_tipsy,
        ) + move_col_extra(self.request.lane_move_y, local_col)
    }

    pub fn lane_y(
        &self,
        local_col: usize,
        receptor_y: f32,
        direction: f32,
        raw_travel: f32,
    ) -> f32 {
        receptor_y + direction * self.adjusted(raw_travel) + self.lane_offset(local_col)
    }

    pub fn lane_y_for_beat(
        &self,
        local_col: usize,
        beat: f32,
        receptor_y: f32,
        direction: f32,
    ) -> f32 {
        self.lane_y(local_col, receptor_y, direction, self.raw_beat(beat))
    }

    pub fn adjusted_from_screen_y(
        &self,
        local_col: usize,
        receptor_y: f32,
        direction: f32,
        screen_y: f32,
    ) -> f32 {
        let direction = if direction.abs() <= 0.000_1 {
            if direction < 0.0 { -0.000_1 } else { 0.000_1 }
        } else {
            direction
        };
        (screen_y - receptor_y - self.lane_offset(local_col)) / direction
    }

    pub fn visible_row_range(&self) -> Option<(i32, i32)> {
        let first = find_first_displayed_beat(
            self.request.search_beat,
            self.request.draw_distance_after_targets,
            self.request.note_count_stats,
            |beat| self.adjusted(self.raw_beat(beat)),
        );
        let last = find_last_displayed_beat(
            self.request.search_beat,
            self.request.draw_distance_before_targets,
            self.displayed_speed_percent,
            self.request.accel.boomerang > f32::EPSILON,
            |beat| self.adjusted_with_peak(self.raw_beat(beat)),
        );
        first.zip(last).map(|(first, last)| {
            let first_row = beat_to_note_row(first);
            let last_row = beat_to_note_row(last.max(first)).max(first_row);
            (first_row, last_row)
        })
    }

    pub fn arrow_effect_time_s(&self) -> f32 {
        self.request.arrow_effect_time_s
    }
}

pub fn note_itg_row(note: &Note) -> i32 {
    beat_to_note_row(note.beat)
}

pub fn lane_window_bounds_by_note_row(
    notes: &[Note],
    indices: &[usize],
    range: Option<(i32, i32)>,
) -> Option<(usize, usize)> {
    let (low, high) = range?;
    if high < 0 {
        return Some((0, 0));
    }
    let low = low.max(0);
    Some((
        indices.partition_point(|&note_index| note_itg_row(&notes[note_index]) < low),
        indices.partition_point(|&note_index| note_itg_row(&notes[note_index]) <= high),
    ))
}

pub fn lane_hold_window_bounds_by_note_row(
    notes: &[Note],
    indices: &[usize],
    range: Option<(i32, i32)>,
) -> Option<(usize, usize)> {
    let (low, _) = range?;
    let (mut start, end) = lane_window_bounds_by_note_row(notes, indices, range)?;
    let low = low.max(0);
    while start > 0 {
        let prev_note_index = indices[start - 1];
        let prev_end_row = notes[prev_note_index]
            .hold
            .as_ref()
            .map_or(note_itg_row(&notes[prev_note_index]), |hold| {
                beat_to_note_row(hold.end_beat)
            });
        if prev_end_row < low {
            break;
        }
        start -= 1;
    }
    Some((start, end))
}

pub fn for_each_visible_note_index<F: FnMut(usize)>(
    indices: &[usize],
    notes: &[Note],
    range: Option<(i32, i32)>,
    mut f: F,
) {
    let Some((low, high)) = range else {
        for &i in indices {
            f(i);
        }
        return;
    };
    let Some((start, end)) = lane_window_bounds_by_note_row(notes, indices, Some((low, high)))
    else {
        return;
    };
    for &i in &indices[start..end] {
        f(i);
    }
}

pub fn for_each_visible_hold_index<F: FnMut(usize)>(
    indices: &[usize],
    notes: &[Note],
    range: Option<(i32, i32)>,
    mut f: F,
) {
    let Some((low, high)) = range else {
        for &i in indices {
            f(i);
        }
        return;
    };
    let Some((start, end)) = lane_hold_window_bounds_by_note_row(notes, indices, Some((low, high)))
    else {
        return;
    };
    for &i in &indices[start..end] {
        f(i);
    }
}

pub fn hold_overlaps_visible_window(
    note_index: usize,
    notes: &[Note],
    range: Option<(i32, i32)>,
) -> bool {
    let Some(note) = notes.get(note_index) else {
        return false;
    };
    let Some((low, high)) = range else {
        return true;
    };
    let start = note_itg_row(note);
    let end = note
        .hold
        .as_ref()
        .map(|h| beat_to_note_row(h.end_beat))
        .unwrap_or(start);
    high >= 0 && end >= low.max(0) && start <= high
}

fn note_count_at(stats: &[NoteCountStat], beat: f32) -> NoteCountStat {
    let ix = stats
        .partition_point(|stat| stat.beat <= beat)
        .saturating_sub(1);
    stats.get(ix).copied().unwrap_or(NoteCountStat {
        beat: 0.0,
        notes_lower: 0,
        notes_upper: 0,
    })
}

fn note_count_range(stats: &[NoteCountStat], low: f32, high: f32) -> usize {
    let low = note_count_at(stats, low);
    let high = note_count_at(stats, high);
    high.notes_upper.saturating_sub(low.notes_lower)
}

pub fn find_first_displayed_beat<F: FnMut(f32) -> f32>(
    current_beat: f32,
    draw_distance: f32,
    stats: &[NoteCountStat],
    mut y_for_beat: F,
) -> Option<f32> {
    if !current_beat.is_finite() || !draw_distance.is_finite() {
        return None;
    }
    let mut high = current_beat.max(0.0);
    let has_cache = !stats.is_empty();
    let mut low = if has_cache { 0.0 } else { high - 4.0 };
    let mut first = low;
    for _ in 0..24 {
        let mid = (low + high) * 0.5;
        if y_for_beat(mid) < -draw_distance
            || (has_cache && note_count_range(stats, mid, current_beat) > MAX_NOTES_AFTER)
        {
            first = mid;
            low = mid;
        } else {
            high = mid;
        }
    }
    Some(first)
}

pub fn find_last_displayed_beat<F: FnMut(f32) -> (f32, bool)>(
    current_beat: f32,
    draw_distance: f32,
    displayed_speed_percent: f32,
    boomerang: bool,
    mut y_for_beat: F,
) -> Option<f32> {
    if !current_beat.is_finite() || !draw_distance.is_finite() {
        return None;
    }
    let mut search_distance = 10.0;
    let mut last = current_beat + search_distance;
    for _ in 0..20 {
        let (y_offset, before_peak) = y_for_beat(last);
        if boomerang && !before_peak {
            last += search_distance;
        } else if y_offset > draw_distance {
            last -= search_distance;
        } else {
            last += search_distance;
        }
        search_distance *= 0.5;
    }
    if displayed_speed_percent < 0.75 {
        last = last.min(current_beat + 16.0);
    }
    Some(last)
}

pub const fn mine_hides_after_resolution(mine_result: Option<MineResult>) -> bool {
    mine_result.is_some()
}

use crate::style::MAX_NOTES_AFTER;

#[cfg(test)]
mod tests {
    use super::{
        ScrollTravelRequest, for_each_visible_hold_index, for_each_visible_note_index,
        hold_overlaps_visible_window, scroll_travel,
    };
    use crate::{AccelYParams, apply_accel_y, move_col_extra, tipsy_y_extra};
    use deadsync_core::note::NoteType;
    use deadsync_core::timing::beat_to_note_row;
    use deadsync_rules::note::{HoldData, Note};
    use deadsync_rules::scroll::ScrollSpeedSetting;
    use deadsync_rules::timing::{
        ScrollSegment, SpeedSegment, SpeedUnit, TimingData, TimingSegments,
    };

    fn timing() -> TimingData {
        TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 120.0)],
                ..TimingSegments::default()
            },
            &[],
        )
    }

    fn request<'a>(
        timing: &'a TimingData,
        scroll_speed: ScrollSpeedSetting,
        visible_beat: f32,
    ) -> ScrollTravelRequest<'a> {
        ScrollTravelRequest {
            timing,
            accel: AccelYParams::default(),
            scroll_speed,
            current_time_ns: timing.get_time_for_beat_ns(visible_beat),
            visible_beat,
            search_beat: visible_beat,
            scroll_reference_bpm: 120.0,
            music_rate: 1.0,
            edit_beat_spacing: false,
            draw_distance_after_targets: 64.0,
            draw_distance_before_targets: 64.0,
            field_zoom: 1.0,
            elapsed_screen_s: 0.0,
            effect_height: 640.0,
            screen_height: 720.0,
            note_count_stats: &[],
            arrow_effect_time_s: 0.0,
            lane_tipsy: 0.0,
            lane_move_y: &[],
        }
    }

    fn assert_near(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 0.001,
            "expected {expected}, got {actual}"
        );
    }

    fn note(beat: f32) -> Note {
        Note {
            beat,
            quantization_idx: 0,
            column: 0,
            note_type: NoteType::Tap,
            row_index: beat_to_note_row(beat).max(0) as usize,
            result: None,
            early_result: None,
            hold: None,
            mine_result: None,
            is_fake: false,
            can_be_judged: true,
        }
    }

    fn hold(beat: f32, end_beat: f32) -> Note {
        let mut note = note(beat);
        note.note_type = NoteType::Hold;
        note.hold = Some(HoldData {
            end_row_index: beat_to_note_row(end_beat).max(0) as usize,
            end_beat,
            result: None,
            life: 1.0,
            let_go_started_at: None,
            let_go_starting_life: 1.0,
            last_held_row_index: note.row_index,
            last_held_beat: beat,
        });
        note
    }

    #[test]
    fn projects_cmod_xmod_mmod_and_edit_spacing() {
        let timing = timing();

        let mut cmod_request = request(&timing, ScrollSpeedSetting::CMod(600.0), 4.0);
        cmod_request.music_rate = 2.0;
        let cmod = scroll_travel(cmod_request);
        assert_near(cmod.raw_beat(5.0), 160.0);
        assert_near(cmod.adjusted(cmod.raw_beat(5.0)), 160.0);

        let xmod = scroll_travel(request(&timing, ScrollSpeedSetting::XMod(2.0), 4.0));
        assert_near(xmod.raw_beat(5.0), 64.0);
        assert_near(xmod.adjusted(xmod.raw_beat(5.0)), 128.0);

        let mmod = scroll_travel(request(&timing, ScrollSpeedSetting::MMod(600.0), 4.0));
        assert_near(mmod.raw_beat(5.0), 64.0);
        assert_near(mmod.adjusted(mmod.raw_beat(5.0)), 320.0);

        let scrolled_timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 120.0)],
                scrolls: vec![ScrollSegment {
                    beat: 0.0,
                    ratio: 0.25,
                }],
                ..TimingSegments::default()
            },
            &[],
        );
        let displayed = scroll_travel(request(
            &scrolled_timing,
            ScrollSpeedSetting::XMod(2.0),
            4.0,
        ));
        let mut edit_request = request(&scrolled_timing, ScrollSpeedSetting::XMod(2.0), 4.0);
        edit_request.edit_beat_spacing = true;
        let edit = scroll_travel(edit_request);
        assert_near(displayed.raw_beat(5.0), 16.0);
        assert_near(edit.raw_beat(5.0), 64.0);
        assert_near(edit.adjusted(edit.raw_beat(5.0)), 128.0);
    }

    #[test]
    fn invalid_rate_and_reference_bpm_keep_existing_fallbacks() {
        let timing = timing();
        let mut cmod_request = request(&timing, ScrollSpeedSetting::CMod(600.0), 4.0);
        cmod_request.music_rate = f32::NAN;
        assert_near(scroll_travel(cmod_request).raw_beat(5.0), 320.0);

        let mut mmod_request = request(&timing, ScrollSpeedSetting::MMod(600.0), 4.0);
        mmod_request.music_rate = 0.0;
        mmod_request.scroll_reference_bpm = f32::NAN;
        let mmod = scroll_travel(mmod_request);
        assert_near(mmod.raw_beat(5.0), 64.0);
        assert_near(mmod.adjusted(mmod.raw_beat(5.0)), 64.0);
    }

    #[test]
    fn applies_brake_and_boomerang_before_post_scroll_scale() {
        let timing = timing();
        let mut brake_request = request(&timing, ScrollSpeedSetting::XMod(2.0), 0.0);
        brake_request.accel.brake = 1.0;
        let brake = scroll_travel(brake_request);
        let raw = brake.raw_beat(1.0);
        let expected = apply_accel_y(
            raw,
            0.0,
            brake_request.effect_height,
            brake_request.screen_height,
            brake_request.accel,
        ) * 2.0;
        assert_near(brake.adjusted(raw), expected);
        assert_ne!(
            brake.adjusted(raw),
            apply_accel_y(
                raw * 2.0,
                0.0,
                brake_request.effect_height,
                brake_request.screen_height,
                brake_request.accel,
            )
        );

        let mut boomerang_request = request(&timing, ScrollSpeedSetting::XMod(2.0), 0.0);
        boomerang_request.accel.boomerang = 1.0;
        let boomerang = scroll_travel(boomerang_request);
        let raw = boomerang.raw_beat(10.0);
        let (adjusted, before_peak) = boomerang.adjusted_with_peak(raw);
        let (expected, expected_before_peak) = crate::apply_accel_y_with_peak(
            raw,
            0.0,
            boomerang_request.effect_height,
            boomerang_request.screen_height,
            boomerang_request.accel,
        );
        assert_eq!(before_peak, expected_before_peak);
        assert!(!before_peak);
        assert_near(adjusted, expected * 2.0);
    }

    #[test]
    fn zero_scroll_lead_in_preserves_visible_future_rows() {
        let timing = TimingData::from_segments(
            0.0,
            0.0,
            &TimingSegments {
                bpms: vec![(0.0, 120.0)],
                speeds: vec![SpeedSegment {
                    beat: 0.0,
                    ratio: 0.1,
                    delay: 0.0,
                    unit: SpeedUnit::Beats,
                }],
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
        let mut request = request(&timing, ScrollSpeedSetting::XMod(1.0), -12.0);
        request.draw_distance_before_targets = 120.0;
        let range = scroll_travel(request)
            .visible_row_range()
            .expect("finite lead-in range");
        assert!(range.1 >= beat_to_note_row(4.0), "range={range:?}");
    }

    #[test]
    fn planned_rows_bound_notes_and_keep_overlapping_holds() {
        let timing = timing();
        let travel = scroll_travel(request(&timing, ScrollSpeedSetting::XMod(1.0), 4.0));
        let range = travel.visible_row_range().expect("finite row range");
        let notes = vec![hold(2.0, 4.0), note(4.0), note(10.0)];

        let mut taps = Vec::new();
        for_each_visible_note_index(&[0, 1, 2], &notes, Some(range), |i| taps.push(i));
        assert_eq!(taps, vec![1]);

        let mut holds = Vec::new();
        for_each_visible_hold_index(&[0], &notes, Some(range), |i| holds.push(i));
        assert_eq!(holds, vec![0]);
        assert!(hold_overlaps_visible_window(0, &notes, Some(range)));
        assert_near(travel.raw_note(&notes[0], true), 0.0);
    }

    #[test]
    fn lane_projection_uses_supplied_arrow_effect_time() {
        let timing = timing();
        let move_y = [0.0, 5.0];
        let mut request = request(&timing, ScrollSpeedSetting::XMod(1.0), 4.0);
        request.arrow_effect_time_s = 2.25;
        request.lane_tipsy = 0.75;
        request.lane_move_y = &move_y;
        let travel = scroll_travel(request);
        let expected_offset = tipsy_y_extra(1, 2.25, 0.75) + move_col_extra(&move_y, 1);
        assert_near(travel.lane_offset(1), expected_offset);

        let raw = travel.raw_beat(5.0);
        let y = travel.lane_y(1, 100.0, -1.0, raw);
        assert_near(y, 100.0 - travel.adjusted(raw) + expected_offset);
        assert_near(
            travel.adjusted_from_screen_y(1, 100.0, -1.0, y),
            travel.adjusted(raw),
        );
        assert_near(travel.arrow_effect_time_s(), 2.25);
    }
}

use crate::measure_actors::{append_beat_bar, append_cue_bar, append_edit_measure_number};
use crate::notes::ScrollTravel;
use deadlib_present::actors::Actor;
use deadsync_core::timing::{beat_to_note_row, note_row_to_beat};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::timing::{
    DelaySegment, ScrollSegment, StopSegment, TimeSignatureSegment, default_time_signature,
};
use deadsync_theme::NotefieldStyle;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeasureLineMode {
    Off,
    Measure,
    Quarter,
    Eighth,
    Edit,
}

#[derive(Clone, Copy, Debug)]
pub struct MeasureComposeRequest<'a, 'travel> {
    pub mode: MeasureLineMode,
    pub show_cues: bool,
    pub style: NotefieldStyle,
    pub column_xs: &'a [f32],
    pub column_dirs: &'a [f32],
    pub column_receptor_ys: &'a [f32],
    pub num_cols: usize,
    pub spacing_multiplier: f32,
    pub field_zoom: f32,
    pub playfield_center_x: f32,
    pub screen_height: f32,
    pub current_beat: f32,
    pub scroll_speed: ScrollSpeedSetting,
    pub scroll_reference_bpm: f32,
    pub music_rate: f32,
    pub time_signatures: &'a [TimeSignatureSegment],
    pub bpms: &'a [(f32, f32)],
    pub stops: &'a [StopSegment],
    pub delays: &'a [DelaySegment],
    pub scrolls: &'a [ScrollSegment],
    pub travel: &'a ScrollTravel<'travel>,
}

#[derive(Clone, Copy, Debug)]
struct MeasureGroup {
    min_x: f32,
    max_x: f32,
    receptor_y: f32,
    direction: f32,
}

#[derive(Clone, Copy, Debug)]
struct MeasureLinePlan {
    edit: bool,
    alpha_measure: f32,
    alpha_quarter: f32,
    alpha_eighth: f32,
    alpha_sixteenth: f32,
    line_step: f32,
    edit_candidate_step_rows: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditBeatBarInfo {
    pub frame: u32,
    pub measure_index: Option<i64>,
}

fn valid_sig(sig: TimeSignatureSegment) -> TimeSignatureSegment {
    if sig.numerator > 0 && sig.denominator > 0 {
        sig
    } else {
        default_time_signature()
    }
}

fn sig_at(segments: &[TimeSignatureSegment], index: usize) -> TimeSignatureSegment {
    if segments.is_empty() {
        default_time_signature()
    } else {
        valid_sig(segments[index])
    }
}

fn sig_count(segments: &[TimeSignatureSegment]) -> usize {
    segments.len().max(1)
}

fn bar_step_rows(sig: TimeSignatureSegment) -> i32 {
    (beat_to_note_row(valid_sig(sig).denominator as f32 / 4.0) / 4).max(1)
}

fn measure_frequency(sig: TimeSignatureSegment) -> i32 {
    valid_sig(sig).numerator.saturating_mul(4).max(1)
}

fn bars_in_segment(start_row: i32, end_row: i32, sig: TimeSignatureSegment) -> i64 {
    if end_row <= start_row {
        return 0;
    }
    let step = i64::from(bar_step_rows(sig));
    let freq = i64::from(measure_frequency(sig));
    let bars = (i64::from(end_row) - i64::from(start_row) - 1) / step + 1;
    (bars - 1) / freq + 1
}

fn measure_index_before(segments: &[TimeSignatureSegment], index: usize) -> i64 {
    let mut total = 0;
    for i in 0..index {
        let sig = sig_at(segments, i);
        let next_sig = sig_at(segments, i + 1);
        total += bars_in_segment(
            beat_to_note_row(sig.beat),
            beat_to_note_row(next_sig.beat),
            sig,
        );
    }
    total
}

fn sig_index_at_row(segments: &[TimeSignatureSegment], row: i32) -> usize {
    if segments.is_empty() {
        return 0;
    }
    let mut idx = 0;
    for (i, sig) in segments.iter().enumerate() {
        if row >= beat_to_note_row(sig.beat) {
            idx = i;
        }
    }
    idx
}

pub fn edit_beat_bar_info_for_row(
    row: i32,
    segments: &[TimeSignatureSegment],
) -> Option<EditBeatBarInfo> {
    if row < 0 {
        return None;
    }

    let idx = sig_index_at_row(segments, row);
    let sig = sig_at(segments, idx);
    let start_row = beat_to_note_row(sig.beat);
    if row < start_row {
        return None;
    }

    let rel = row - start_row;
    let step = bar_step_rows(sig);
    if step <= 0 || rel % step != 0 {
        return None;
    }

    let bars_drawn = rel / step;
    let measure_frequency = measure_frequency(sig);
    let is_measure = bars_drawn % measure_frequency == 0;
    let frame = if is_measure {
        0
    } else if bars_drawn % 4 == 0 {
        1
    } else if bars_drawn % 2 == 0 {
        2
    } else {
        3
    };
    let measure_index = is_measure
        .then(|| measure_index_before(segments, idx) + i64::from(bars_drawn / measure_frequency));
    Some(EditBeatBarInfo {
        frame,
        measure_index,
    })
}

fn gcd(a: i32, b: i32) -> i32 {
    let mut a = i64::from(a).abs();
    let mut b = i64::from(b).abs();
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a.clamp(1, i64::from(i32::MAX)) as i32
}

pub fn edit_bar_candidate_step_rows(segments: &[TimeSignatureSegment]) -> i32 {
    let mut out = bar_step_rows(sig_at(segments, 0));
    for i in 0..sig_count(segments) {
        let sig = sig_at(segments, i);
        out = gcd(out, bar_step_rows(sig));
        out = gcd(out, beat_to_note_row(sig.beat));
    }
    out.max(1)
}

pub fn edit_bar_scroll_speed(speed: ScrollSpeedSetting, current_bpm: f32, music_rate: f32) -> f32 {
    match speed {
        ScrollSpeedSetting::XMod(multiplier) => multiplier,
        ScrollSpeedSetting::MMod(_) => speed.beat_multiplier(current_bpm, music_rate),
        ScrollSpeedSetting::CMod(_) => 4.0,
    }
    .max(0.0)
}

pub fn beat_scroll_travel(note_beat: f32, current_beat: f32, scroll_speed: f32) -> f32 {
    (note_beat - current_beat) * ScrollSpeedSetting::ARROW_SPACING * scroll_speed
}

pub fn edit_beat_scroll_travel(note_beat: f32, current_beat: f32) -> f32 {
    beat_scroll_travel(note_beat, current_beat, 1.0)
}

pub fn scaled_edit_bar_alpha(scroll_speed: f32, visible_at: f32, full_at: f32) -> f32 {
    ((scroll_speed - visible_at) / (full_at - visible_at)).clamp(0.0, 1.0)
}

fn measure_line_plan(request: &MeasureComposeRequest<'_, '_>) -> MeasureLinePlan {
    let edit_candidate_step_rows = edit_bar_candidate_step_rows(request.time_signatures);
    if request.mode == MeasureLineMode::Edit {
        let speed = edit_bar_scroll_speed(
            request.scroll_speed,
            request.scroll_reference_bpm,
            request.music_rate,
        );
        return MeasureLinePlan {
            edit: true,
            alpha_measure: 1.0,
            alpha_quarter: 1.0,
            alpha_eighth: scaled_edit_bar_alpha(speed, 1.0, 2.0),
            alpha_sixteenth: scaled_edit_bar_alpha(speed, 2.0, 4.0),
            line_step: note_row_to_beat(edit_candidate_step_rows),
            edit_candidate_step_rows,
        };
    }

    let (alpha_measure, alpha_quarter, alpha_eighth) = match request.mode {
        MeasureLineMode::Off => (0.0, 0.0, 0.0),
        MeasureLineMode::Measure => (0.75, 0.0, 0.0),
        MeasureLineMode::Quarter => (0.75, 0.5, 0.0),
        MeasureLineMode::Eighth => (0.75, 0.5, 0.125),
        MeasureLineMode::Edit => unreachable!("edit mode returned above"),
    };
    MeasureLinePlan {
        edit: false,
        alpha_measure,
        alpha_quarter,
        alpha_eighth,
        alpha_sixteenth: 0.0,
        line_step: 0.5,
        edit_candidate_step_rows,
    }
}

fn measure_groups(request: &MeasureComposeRequest<'_, '_>) -> [Option<MeasureGroup>; 2] {
    let count = request
        .num_cols
        .min(request.column_xs.len())
        .min(request.column_dirs.len())
        .min(request.column_receptor_ys.len());
    let mut groups: [Option<MeasureGroup>; 2] = [None, None];
    for i in 0..count {
        let x = request.column_xs[i] * request.spacing_multiplier;
        let group_index =
            usize::from(request.column_dirs[i] < 0.0 || request.column_dirs[i].is_nan());
        match &mut groups[group_index] {
            Some(group) => {
                group.min_x = group.min_x.min(x);
                group.max_x = group.max_x.max(x);
            }
            slot @ None => {
                *slot = Some(MeasureGroup {
                    min_x: x,
                    max_x: x,
                    receptor_y: request.column_receptor_ys[i],
                    direction: if group_index == 0 { 1.0 } else { -1.0 },
                });
            }
        }
    }
    groups
}

fn group_geometry(
    request: &MeasureComposeRequest<'_, '_>,
    group: MeasureGroup,
) -> Option<(f32, f32)> {
    let center_x_offset = 0.5 * (group.min_x + group.max_x) * request.field_zoom;
    let width =
        ((group.max_x - group.min_x) + ScrollSpeedSetting::ARROW_SPACING) * request.field_zoom;
    (width.is_finite() && width > 0.0)
        .then_some((request.playfield_center_x + center_x_offset, width))
}

fn candidate_for_unit(
    unit: i64,
    plan: MeasureLinePlan,
    time_signatures: &[TimeSignatureSegment],
) -> Option<(f32, Option<EditBeatBarInfo>)> {
    if !plan.edit {
        return Some(((unit as f32) * plan.line_step, None));
    }
    let row = unit
        .checked_mul(i64::from(plan.edit_candidate_step_rows))
        .and_then(|row| i32::try_from(row).ok())?;
    Some((
        note_row_to_beat(row),
        edit_beat_bar_info_for_row(row, time_signatures),
    ))
}

fn line_alpha(unit: i64, info: Option<EditBeatBarInfo>, plan: MeasureLinePlan) -> f32 {
    if plan.edit {
        return info.map_or(0.0, |info| match info.frame {
            0 => plan.alpha_measure,
            1 => plan.alpha_quarter,
            2 => plan.alpha_eighth,
            _ => plan.alpha_sixteenth,
        });
    }
    match unit.rem_euclid(8) {
        0 => plan.alpha_measure,
        2 | 4 | 6 => plan.alpha_quarter,
        _ => plan.alpha_eighth,
    }
}

fn line_thickness(frame: u32, plan: MeasureLinePlan, field_zoom: f32) -> f32 {
    if !plan.edit {
        return (2.0 * field_zoom).max(1.0);
    }
    match frame {
        0 => (3.0 * field_zoom).max(1.0),
        1 => (2.0 * field_zoom).max(1.0),
        _ => field_zoom.max(1.0),
    }
}

fn append_line_candidate(
    actors: &mut Vec<Actor>,
    request: &MeasureComposeRequest<'_, '_>,
    plan: MeasureLinePlan,
    unit: i64,
    x_center: f32,
    y: f32,
    width: f32,
    info: Option<EditBeatBarInfo>,
) {
    let alpha = line_alpha(unit, info, plan);
    if alpha <= 0.0 {
        return;
    }
    let frame = info.map_or(0, |info| info.frame);
    append_beat_bar(
        actors,
        plan.edit,
        frame,
        x_center,
        y,
        width,
        request.field_zoom,
        line_thickness(frame, plan, request.field_zoom),
        alpha,
        request.style.measure_line_z,
    );
    append_edit_measure_number(
        actors,
        plan.edit,
        info.and_then(|info| info.measure_index),
        x_center - width * 0.5,
        y,
        request.field_zoom,
        request.style.measure_line_z,
        request.style.edit_measure_number_font,
    );
}

fn append_group_lines(
    actors: &mut Vec<Actor>,
    request: &MeasureComposeRequest<'_, '_>,
    plan: MeasureLinePlan,
    group: MeasureGroup,
) {
    let Some((x_center, width)) = group_geometry(request, group) else {
        return;
    };
    let y_min = -request.style.measure_line_overscan_y;
    let y_max = request.screen_height + request.style.measure_line_overscan_y;
    let start = (request.current_beat / plan.line_step).floor() as i64;

    let mut unit = if plan.edit { start.max(0) } else { start };
    let mut iterations = 0;
    while iterations < 2000 {
        if plan.edit && unit < 0 {
            break;
        }
        let Some((beat, info)) = candidate_for_unit(unit, plan, request.time_signatures) else {
            break;
        };
        let y = request
            .travel
            .lane_y_for_beat(0, beat, group.receptor_y, group.direction);
        if !y.is_finite() {
            break;
        }
        if (group.direction >= 0.0 && y < y_min) || (group.direction < 0.0 && y > y_max) {
            break;
        }
        append_line_candidate(actors, request, plan, unit, x_center, y, width, info);
        unit -= 1;
        iterations += 1;
    }

    let mut unit = if plan.edit {
        start.max(0) + 1
    } else {
        start + 1
    };
    let mut iterations = 0;
    while iterations < 2000 {
        let Some((beat, info)) = candidate_for_unit(unit, plan, request.time_signatures) else {
            break;
        };
        let y = request
            .travel
            .lane_y_for_beat(0, beat, group.receptor_y, group.direction);
        if !y.is_finite() {
            break;
        }
        if (group.direction >= 0.0 && y > y_max) || (group.direction < 0.0 && y < y_min) {
            break;
        }
        append_line_candidate(actors, request, plan, unit, x_center, y, width, info);
        unit += 1;
        iterations += 1;
    }
}

fn cue_thickness(beat: f32, field_zoom: f32) -> f32 {
    let units = beat / 0.5;
    let rounded = units.round();
    let scale = if (units - rounded).abs() <= 1e-3 {
        match (rounded as i64).rem_euclid(8) {
            0 => 3.0,
            2 | 4 | 6 => 2.0,
            _ => 1.0,
        }
    } else {
        1.0
    };
    (scale * field_zoom).max(1.0)
}

fn append_group_cues(
    actors: &mut Vec<Actor>,
    request: &MeasureComposeRequest<'_, '_>,
    group: MeasureGroup,
) {
    let Some((x_center, width)) = group_geometry(request, group) else {
        return;
    };
    let y_min = -request.style.measure_line_overscan_y;
    let y_max = request.screen_height + request.style.measure_line_overscan_y;
    let mut append_cue = |beat: f32, color: [f32; 3]| {
        let y = request
            .travel
            .lane_y_for_beat(0, beat, group.receptor_y, group.direction);
        if y.is_finite() && y >= y_min && y <= y_max {
            append_cue_bar(
                actors,
                x_center,
                y,
                width,
                cue_thickness(beat, request.field_zoom),
                color,
                request.style.measure_cue_alpha,
                request.style.measure_line_z,
            );
        }
    };

    for window in request.scrolls.windows(2) {
        if window[1].ratio != window[0].ratio {
            append_cue(window[1].beat, request.style.measure_cue_scroll_color);
        }
    }
    for window in request.bpms.windows(2) {
        if window[1].1 != window[0].1 {
            append_cue(window[1].0, request.style.measure_cue_bpm_color);
        }
    }
    for delay in request.delays {
        append_cue(delay.beat, request.style.measure_cue_delay_color);
    }
    for stop in request.stops {
        append_cue(stop.beat, request.style.measure_cue_stop_color);
    }
}

pub fn compose_measure_lines(actors: &mut Vec<Actor>, request: MeasureComposeRequest<'_, '_>) {
    if request.mode == MeasureLineMode::Off && !request.show_cues {
        return;
    }
    let plan = measure_line_plan(&request);
    let groups = measure_groups(&request);
    if request.mode != MeasureLineMode::Off {
        for group in groups.into_iter().flatten() {
            append_group_lines(actors, &request, plan, group);
        }
    }
    if request.show_cues {
        for group in groups.into_iter().flatten() {
            append_group_cues(actors, &request, group);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AccelYParams, ScrollTravelRequest, scroll_travel};
    use deadlib_present::actors::{Actor, SizeSpec};
    use deadsync_rules::timing::{TimingData, TimingSegments};
    use deadsync_theme::{
        ColumnCueStyle, ColumnFlashLayoutStyle, ColumnFlashStyle, ComboFeedbackStyle,
        CounterHudStyle, ErrorBarLayers, ErrorBarPalette, ErrorBarStyle, JudgmentFeedbackStyle,
        MiniIndicatorStyle, NotefieldActorStyle, ReceptorStyle,
    };

    fn style() -> NotefieldStyle {
        NotefieldStyle {
            layout_width_min: 640.0,
            layout_width_max: 854.0,
            side_center_x_ratio: 0.25,
            receptor_normal_y: -125.0,
            receptor_reverse_y: 145.0,
            receptor: ReceptorStyle {
                target_z: 100,
                press_glow_z: 105,
                hold_explosion_z: 145,
            },
            actors: NotefieldActorStyle {
                hold_body_z: 110,
                hold_cap_z: 110,
                hold_glow_z: 111,
                tap_explosion_z: 150,
                mine_explosion_z: 101,
                note_z: 140,
                mine_core_size_ratio: 0.45,
            },
            judgment_normal_y: -30.0,
            judgment_reverse_y: 30.0,
            judgment_centered_y: 95.0,
            combo_normal_y: 30.0,
            combo_reverse_y: -30.0,
            combo_centered_y: 155.0,
            judgment_height: 40.0,
            error_bar_offset_y: 25.0,
            measure_line_overscan_y: 400.0,
            measure_line_z: 80,
            measure_cue_scroll_color: [0.824, 0.706, 0.549],
            measure_cue_bpm_color: [1.0, 1.0, 0.0],
            measure_cue_delay_color: [1.0, 0.45, 0.75],
            measure_cue_stop_color: [1.0, 0.0, 0.0],
            measure_cue_alpha: 0.7,
            edit_measure_number_font: "edit-font",
            column_cue: ColumnCueStyle {
                top_y: 80.0,
                reverse_anchor_y: 304.0,
                crossover_height_trim: 270.0,
                body_fade: 0.333,
                base_alpha: 0.12,
                normal_color: [0.3, 1.0, 1.0],
                mine_color: [1.0, 0.0, 0.0],
                countdown_normal_y: 160.0,
                countdown_reverse_y: 340.0,
                countdown_color: [1.0, 1.0, 1.0],
                countdown_zoom: 0.5,
                body_z: 90,
                countdown_z: 200,
            },
            column_flash: ColumnFlashStyle {
                default_layout: ColumnFlashLayoutStyle {
                    top_y: 80.0,
                    height_trim: 0.0,
                    reverse_trim: 0.0,
                    fade: 0.333,
                },
                compact_layout: ColumnFlashLayoutStyle {
                    top_y: 70.0,
                    height_trim: 270.0,
                    reverse_trim: 30.0,
                    fade: 0.2,
                },
                reverse_anchor_y: 304.0,
                normal_alpha: 0.66,
                dimmed_alpha: 0.3,
                miss_color: [1.0, 0.0, 0.0],
                decent_color: [0.70, 0.36, 1.0],
                way_off_color: [0.788, 0.522, 0.369],
                great_color: [0.4, 0.788, 0.333],
                excellent_color: [0.886, 0.612, 0.094],
                fantastic_color: [1.0, 1.0, 1.0],
                fantastic_blue_color: [0.129, 0.8, 0.91],
                z: 91,
            },
            counter_hud: CounterHudStyle {
                text_z: 85,
                shadow_len: 1.0,
                base_zoom: 0.35,
                lookahead_zoom_step: 0.05,
                vertical_step_y: 20.0,
                left_column_scale: 4.0 / 3.0,
                horizontal_span: 2.0,
                break_lookahead_color: [0.4, 0.4, 0.4, 1.0],
                break_current_color: [0.5, 0.5, 0.5, 1.0],
                stream_lookahead_color: [0.45, 0.45, 0.45, 1.0],
                ratio_color: [1.0, 1.0, 1.0, 1.0],
                total_color: [0.5, 0.5, 0.5, 1.0],
                broken_y_offset: 15.0,
                broken_vertical_y_offset: -15.0,
                broken_vertical_x_scale: 4.0 / 3.0,
                broken_color: [1.0, 1.0, 1.0, 0.7],
                run_active_color: [1.0, 1.0, 1.0, 1.0],
                run_inactive_color: [0.5, 0.5, 0.5, 1.0],
            },
            mini_indicator: MiniIndicatorStyle {
                column_offset: 1.0,
                under_up_x_offset: -45.0,
                unanchored_x_offset: -12.0,
                failed_color: [0.5, 0.5, 0.5],
                shadow_len: 1.0,
                text_z: 85,
            },
            judgment_feedback: JudgmentFeedbackStyle {
                tap_front_z: 200,
                tap_back_z: 95,
                split_overlay_alpha: 0.5,
                held_miss_normal_y: -50.0,
                held_miss_reverse_y: 110.0,
                held_miss_z: 196,
                hold_normal_y: -90.0,
                hold_reverse_y: 90.0,
                hold_z: 195,
                hold_initial_zoom: 25.6 / 140.0,
                hold_final_zoom: 32.0 / 140.0,
            },
            combo_feedback: ComboFeedbackStyle {
                threshold: 4,
                milestone_z: 89,
                number_z: 90,
                number_zoom: 0.75,
                shadow_len: 1.0,
                miss_color: [1.0, 0.0, 0.0, 1.0],
                burst_duration: 0.5,
                burst_start_zoom: 2.0,
                burst_end_zoom: 1.0,
                burst_start_alpha: 0.5,
                burst_rotation_deg: 90.0,
                hundred_start_zoom: 0.25,
                hundred_end_zoom: 2.0,
                hundred_start_alpha: 0.6,
                hundred_start_rotation_deg: 10.0,
                mini_duration: 0.4,
                mini_start_zoom: 0.25,
                mini_end_zoom: 1.8,
                mini_start_alpha: 1.0,
                mini_start_rotation_deg: 10.0,
                thousand_start_zoom: 0.25,
                thousand_end_zoom: 3.0,
                thousand_start_alpha: 0.7,
                thousand_x_travel: 100.0,
            },
            error_bar: ErrorBarStyle {
                colorful_width: 160.0,
                colorful_height: 10.0,
                average_width: 325.0,
                average_height: 7.0,
                monochrome_width: 240.0,
                tick_width: 2.0,
                colorful_border_size: 4.0,
                average_tick_padding: 4.0,
                monochrome_border_size: 2.0,
                monochrome_center_width: 2.0,
                monochrome_line_width: 1.0,
                colorful_tick_duration: 0.5,
                monochrome_tick_duration: 0.75,
                average_tick_extra_height: 75.0,
                monochrome_background_alpha: 0.5,
                line_alpha: 0.3,
                lines_fade_start: 2.5,
                lines_fade_duration: 0.5,
                label_fade_duration: 0.5,
                label_hold: 2.0,
                label_x_ratio: 0.25,
                label_zoom: 0.7,
                center_tick_width: 1.0,
                highlight_inactive_alpha: 0.3,
                offset_indicator_duration: 0.5,
                offset_indicator_gap: 6.0,
                offset_indicator_zoom: 0.25,
                offset_indicator_shadow_len: 1.0,
                long_average_tick_duration: 0.5,
                long_average_tick_extra_height: 65.0,
                long_average_tick_width: 1.0,
                text_duration: 0.5,
                text_x_offset: 40.0,
                text_zoom: 0.25,
                text_shadow_len: 1.0,
                background_color: [0.0, 0.0, 0.0, 1.0],
                monochrome_center_color: [0.5, 0.5, 0.5, 1.0],
                monochrome_line_color: [1.0, 1.0, 1.0, 1.0],
                label_color: [1.0, 1.0, 1.0, 1.0],
                colorful_tick_color: [0.698, 0.0, 0.0, 1.0],
                average_center_tick_color: [1.0, 1.0, 1.0, 0.3],
                long_average_tick_color: [0.0, 0.0, 1.0, 1.0],
                text_early_color: [0.024, 0.416, 0.957, 1.0],
                text_late_color: [1.0, 0.353, 0.306, 1.0],
                text_scaled_early_color: [0.0, 0.318, 0.859, 1.0],
                text_scaled_late_color: [1.0, 0.086, 0.02, 1.0],
                palette: ErrorBarPalette {
                    fantastic_blue: [0.129, 0.8, 0.91, 1.0],
                    fa_plus_white: [1.0, 1.0, 1.0, 1.0],
                    excellent: [0.886, 0.612, 0.094, 1.0],
                    great: [0.4, 0.788, 0.333, 1.0],
                    decent: [0.706, 0.361, 1.0, 1.0],
                    way_off: [0.788, 0.522, 0.369, 1.0],
                },
                label_font: "game",
                offset_indicator_font: "wendy",
                text_font: "wendy",
                early_label: "Early",
                late_label: "Late",
                front_layers: ErrorBarLayers {
                    background: 180,
                    band: 181,
                    line: 182,
                    tick: 183,
                    text: 184,
                },
                back_layers: ErrorBarLayers {
                    background: 86,
                    band: 87,
                    line: 88,
                    tick: 89,
                    text: 90,
                },
                average_z: 88,
            },
        }
    }

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

    fn travel(timing: &TimingData, speed: ScrollSpeedSetting) -> ScrollTravel<'_> {
        scroll_travel(ScrollTravelRequest {
            timing,
            accel: AccelYParams::default(),
            scroll_speed: speed,
            current_time_ns: timing.get_time_for_beat_ns(0.0),
            visible_beat: 0.0,
            search_beat: 0.0,
            scroll_reference_bpm: 120.0,
            music_rate: 1.0,
            edit_beat_spacing: matches!(speed, ScrollSpeedSetting::XMod(3.0)),
            draw_distance_after_targets: 400.0,
            draw_distance_before_targets: 400.0,
            field_zoom: 1.0,
            elapsed_screen_s: 0.0,
            effect_height: 640.0,
            screen_height: 480.0,
            note_count_stats: &[],
            arrow_effect_time_s: 0.0,
            lane_tipsy: 0.0,
            lane_move_y: &[],
        })
    }

    fn request<'a, 'travel>(
        mode: MeasureLineMode,
        travel: &'a ScrollTravel<'travel>,
        column_dirs: &'a [f32],
        column_receptor_ys: &'a [f32],
    ) -> MeasureComposeRequest<'a, 'travel> {
        static COLUMN_XS: [f32; 4] = [-96.0, -32.0, 32.0, 96.0];
        MeasureComposeRequest {
            mode,
            show_cues: false,
            style: style(),
            column_xs: &COLUMN_XS,
            column_dirs,
            column_receptor_ys,
            num_cols: 4,
            spacing_multiplier: 1.0,
            field_zoom: 1.0,
            playfield_center_x: 320.0,
            screen_height: 480.0,
            current_beat: 0.0,
            scroll_speed: ScrollSpeedSetting::XMod(1.0),
            scroll_reference_bpm: 120.0,
            music_rate: 1.0,
            time_signatures: &[],
            bpms: &[],
            stops: &[],
            delays: &[],
            scrolls: &[],
            travel,
        }
    }

    fn sprite_parts(actor: &Actor) -> Option<([f32; 2], [f32; 2], [f32; 4], i16)> {
        let Actor::Sprite {
            offset,
            size: [SizeSpec::Px(width), SizeSpec::Px(height)],
            tint,
            z,
            ..
        } = actor
        else {
            return None;
        };
        Some((*offset, [*width, *height], *tint, *z))
    }

    #[test]
    fn compose_modes_keep_measure_quarter_and_eighth_fingerprints() {
        let timing = timing();
        let travel = travel(&timing, ScrollSpeedSetting::XMod(1.0));
        let dirs = [1.0; 4];
        let receptors = [100.0; 4];
        let mut counts = Vec::new();

        for (mode, expected_alpha) in [
            (MeasureLineMode::Measure, 0.75),
            (MeasureLineMode::Quarter, 0.75),
            (MeasureLineMode::Eighth, 0.75),
        ] {
            let mut actors = Vec::new();
            compose_measure_lines(&mut actors, request(mode, &travel, &dirs, &receptors));
            counts.push(actors.len());
            let current = actors
                .iter()
                .filter_map(sprite_parts)
                .find(|(offset, _, _, _)| (offset[1] - 100.0).abs() <= 0.001)
                .expect("current-beat measure line");
            assert_eq!(current.0, [320.0, 100.0]);
            assert_eq!(current.1, [256.0, 2.0]);
            assert_eq!(current.2, [1.0, 1.0, 1.0, expected_alpha]);
            assert_eq!(current.3, 80);
        }

        assert!(counts[0] < counts[1], "counts={counts:?}");
        assert!(counts[1] < counts[2], "counts={counts:?}");
    }

    #[test]
    fn compose_edit_mode_emits_dashes_and_theme_font_measure_numbers() {
        let timing = timing();
        let travel = travel(&timing, ScrollSpeedSetting::XMod(3.0));
        let dirs = [1.0; 4];
        let receptors = [100.0; 4];
        let mut request = request(MeasureLineMode::Edit, &travel, &dirs, &receptors);
        request.scroll_speed = ScrollSpeedSetting::XMod(3.0);
        let mut actors = Vec::new();
        compose_measure_lines(&mut actors, request);

        assert!(actors.iter().any(|actor| matches!(
            actor,
            Actor::Sprite { align, .. } if *align == [0.0, 0.5]
        )));
        assert!(actors.iter().any(|actor| matches!(
            actor,
            Actor::Text { font, content, z, .. }
                if *font == "edit-font" && content.as_str() == "0" && *z == 81
        )));
    }

    #[test]
    fn compose_measure_lines_splits_mixed_scroll_directions() {
        let timing = timing();
        let travel = travel(&timing, ScrollSpeedSetting::XMod(1.0));
        let dirs = [1.0, 1.0, -1.0, -1.0];
        let receptors = [100.0, 100.0, 380.0, 380.0];
        let mut actors = Vec::new();
        compose_measure_lines(
            &mut actors,
            request(MeasureLineMode::Measure, &travel, &dirs, &receptors),
        );
        let fingerprints: Vec<_> = actors.iter().filter_map(sprite_parts).collect();

        assert!(
            fingerprints
                .iter()
                .any(|(offset, size, tint, z)| *offset == [256.0, 100.0]
                    && *size == [128.0, 2.0]
                    && *tint == [1.0, 1.0, 1.0, 0.75]
                    && *z == 80)
        );
        assert!(
            fingerprints
                .iter()
                .any(|(offset, size, tint, z)| *offset == [384.0, 380.0]
                    && *size == [128.0, 2.0]
                    && *tint == [1.0, 1.0, 1.0, 0.75]
                    && *z == 80)
        );
    }

    #[test]
    fn coincident_cues_keep_scroll_bpm_delay_stop_priority() {
        let timing = timing();
        let travel = travel(&timing, ScrollSpeedSetting::XMod(1.0));
        let dirs = [1.0; 4];
        let receptors = [100.0; 4];
        let bpms = [(0.0, 120.0), (4.0, 180.0)];
        let stops = [StopSegment {
            beat: 4.0,
            duration: 0.25,
        }];
        let delays = [DelaySegment {
            beat: 4.0,
            duration: 0.125,
        }];
        let scrolls = [
            ScrollSegment {
                beat: 0.0,
                ratio: 1.0,
            },
            ScrollSegment {
                beat: 4.0,
                ratio: 0.5,
            },
        ];
        let mut request = request(MeasureLineMode::Off, &travel, &dirs, &receptors);
        request.show_cues = true;
        request.bpms = &bpms;
        request.stops = &stops;
        request.delays = &delays;
        request.scrolls = &scrolls;
        let mut actors = Vec::new();
        compose_measure_lines(&mut actors, request);

        let tints: Vec<_> = actors
            .iter()
            .filter_map(sprite_parts)
            .map(|(_, _, tint, _)| tint)
            .collect();
        assert_eq!(
            tints,
            [
                [0.824, 0.706, 0.549, 0.7],
                [1.0, 1.0, 0.0, 0.7],
                [1.0, 0.45, 0.75, 0.7],
                [1.0, 0.0, 0.0, 0.7],
            ]
        );
    }
}

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

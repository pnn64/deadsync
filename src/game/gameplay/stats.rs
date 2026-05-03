use crate::game::chart::{ChartData, GameplayChartData};
use crate::game::judgment;
use crate::game::note::{Note, NoteType};
use crate::game::profile;
use rssp::streams::StreamSegment;

use super::{
    CourseDisplayCarry, DISPLAY_JUDGE_ORDER, HOLDS_MASK_BIT_FLOORED, HOLDS_MASK_BIT_NO_ROLLS,
    HOLDS_MASK_BIT_PLANTED, HOLDS_MASK_BIT_TWISTER, INSERT_MASK_BIT_ECHO, MAX_PLAYERS,
    REMOVE_MASK_BIT_LITTLE, REMOVE_MASK_BIT_NO_FAKES, REMOVE_MASK_BIT_NO_HANDS,
    REMOVE_MASK_BIT_NO_HOLDS, REMOVE_MASK_BIT_NO_JUMPS, REMOVE_MASK_BIT_NO_LIFTS,
    REMOVE_MASK_BIT_NO_MINES, REMOVE_MASK_BIT_NO_QUADS, ScrollSpeedSetting, State,
    display_judge_ix,
};

#[inline(always)]
fn count_total_steps_for_range(notes: &[Note], note_range: (usize, usize)) -> u32 {
    let (start, end) = note_range;
    if start >= end {
        return 0;
    }
    let mut rows = Vec::<usize>::with_capacity(end - start);
    for note in &notes[start..end] {
        if note.can_be_judged && !matches!(note.note_type, NoteType::Mine) {
            rows.push(note.row_index);
        }
    }
    rows.sort_unstable();
    rows.dedup();
    rows.len() as u32
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PlayerTotals {
    pub(crate) steps: u32,
    pub(crate) holds: u32,
    pub(crate) rolls: u32,
    pub(crate) mines: u32,
    pub(crate) jumps: u32,
    pub(crate) hands: u32,
}

#[inline(always)]
pub(crate) fn recompute_player_totals(notes: &[Note], note_range: (usize, usize)) -> PlayerTotals {
    let (start, end) = note_range;
    if start >= end {
        return PlayerTotals::default();
    }
    let mut totals = PlayerTotals {
        steps: count_total_steps_for_range(notes, note_range),
        ..PlayerTotals::default()
    };
    let mut row_cells: Vec<(usize, usize)> = Vec::with_capacity(end - start);
    let mut hold_starts: Vec<usize> = Vec::new();
    let mut hold_ends: Vec<usize> = Vec::new();
    for note in &notes[start..end] {
        if !note.can_be_judged {
            continue;
        }
        match note.note_type {
            NoteType::Tap => row_cells.push((note.row_index, note.column)),
            NoteType::Hold => {
                totals.holds = totals.holds.saturating_add(1);
                row_cells.push((note.row_index, note.column));
                if let Some(hold) = note.hold.as_ref() {
                    hold_starts.push(note.row_index);
                    hold_ends.push(hold.end_row_index);
                }
            }
            NoteType::Roll => {
                totals.rolls = totals.rolls.saturating_add(1);
                row_cells.push((note.row_index, note.column));
                if let Some(hold) = note.hold.as_ref() {
                    hold_starts.push(note.row_index);
                    hold_ends.push(hold.end_row_index);
                }
            }
            NoteType::Mine => totals.mines = totals.mines.saturating_add(1),
            NoteType::Lift | NoteType::Fake => {}
        }
    }

    row_cells.sort_unstable();
    hold_starts.sort_unstable();
    hold_ends.sort_unstable();

    let mut row_ix = 0usize;
    let mut hold_start_ix = 0usize;
    let mut hold_end_ix = 0usize;
    while row_ix < row_cells.len() {
        let row = row_cells[row_ix].0;
        let mut row_mask = 0u16;
        while row_ix < row_cells.len() && row_cells[row_ix].0 == row {
            row_mask |= 1u16 << row_cells[row_ix].1.min(15);
            row_ix += 1;
        }
        while hold_start_ix < hold_starts.len() && hold_starts[hold_start_ix] < row {
            hold_start_ix += 1;
        }
        while hold_end_ix < hold_ends.len() && hold_ends[hold_end_ix] < row {
            hold_end_ix += 1;
        }
        let notes_on_row = row_mask.count_ones();
        let carried_holds = hold_start_ix.saturating_sub(hold_end_ix) as u32;
        if notes_on_row >= 2 {
            totals.jumps = totals.jumps.saturating_add(1);
        }
        if notes_on_row + carried_holds >= 3 {
            totals.hands = totals.hands.saturating_add(1);
        }
    }

    totals
}

#[inline(always)]
fn chart_has_attacks(chart: &ChartData) -> bool {
    chart.has_chart_attacks
}

#[inline(always)]
pub(crate) fn mini_indicator_mode(profile: &profile::Profile) -> profile::MiniIndicator {
    if profile.mini_indicator != profile::MiniIndicator::None {
        profile.mini_indicator
    } else if profile.subtractive_scoring {
        profile::MiniIndicator::SubtractiveScoring
    } else if profile.pacemaker {
        profile::MiniIndicator::Pacemaker
    } else {
        profile::MiniIndicator::None
    }
}

#[inline(always)]
pub(crate) fn needs_stream_data(profile: &profile::Profile) -> bool {
    profile.measure_counter != profile::MeasureCounter::None
        || mini_indicator_mode(profile) != profile::MiniIndicator::None
}

#[inline(always)]
fn chart_stream_segments(
    gameplay_chart: &GameplayChartData,
    lanes: usize,
    constant_bpm: bool,
) -> (Vec<StreamSegment>, f32, f32) {
    let measure_densities = rssp::stats::measure_densities(&gameplay_chart.notes, lanes);
    zmod_stream_totals_full_measures(&measure_densities, constant_bpm)
}

pub fn stream_segments_for_results(state: &State, player: usize) -> Vec<StreamSegment> {
    if player >= state.num_players {
        return Vec::new();
    }
    if !state.mini_indicator_stream_segments[player].is_empty() {
        return state.mini_indicator_stream_segments[player].clone();
    }
    let constant_bpm = !state.timing_players[player].has_bpm_changes();
    let (segments, _, _) = chart_stream_segments(
        &state.gameplay_charts[player],
        state.cols_per_player,
        constant_bpm,
    );
    segments
}

pub fn score_invalid_reason_lines_for_chart(
    chart: &ChartData,
    profile: &profile::Profile,
    _scroll_speed: ScrollSpeedSetting,
    music_rate: f32,
) -> Vec<&'static str> {
    let mut reasons = Vec::with_capacity(6);
    let rate = if music_rate.is_finite() && music_rate > 0.0 {
        music_rate
    } else {
        1.0
    };
    if rate < 1.0 {
        reasons.push("music rate is below 1.0x");
    }

    let remove_mask = profile.remove_active_mask.bits();
    if (remove_mask & REMOVE_MASK_BIT_NO_HOLDS) != 0 && chart.stats.holds > 0 {
        reasons.push("No Holds is enabled on a chart with holds");
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_MINES) != 0 && chart.mines_nonfake > 0 {
        reasons.push("No Mines is enabled on a chart with mines");
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_JUMPS) != 0 && chart.stats.jumps > 0 {
        reasons.push("No Jumps is enabled on a chart with jumps");
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_HANDS) != 0 && chart.stats.hands > 0 {
        reasons.push("No Hands is enabled on a chart with hands");
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_QUADS) != 0 && chart.stats.hands > 0 {
        reasons.push("No Quads is enabled on a chart with quads");
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_LIFTS) != 0 && chart.stats.lifts > 0 {
        reasons.push("No Lifts is enabled on a chart with lifts");
    }
    if (remove_mask & REMOVE_MASK_BIT_NO_FAKES) != 0 && chart.stats.fakes > 0 {
        reasons.push("No Fakes is enabled on a chart with fakes");
    }

    let holds_mask = profile.holds_active_mask.bits();
    if (holds_mask & HOLDS_MASK_BIT_NO_ROLLS) != 0 && chart.stats.rolls > 0 {
        reasons.push("No Rolls is enabled on a chart with rolls");
    }

    if (remove_mask & REMOVE_MASK_BIT_LITTLE) != 0 {
        reasons.push("Little is enabled");
    }

    let insert_mask = profile.insert_active_mask.bits();
    if (insert_mask & INSERT_MASK_BIT_ECHO) != 0 {
        reasons.push("Echo is enabled");
    }

    if (holds_mask & HOLDS_MASK_BIT_PLANTED) != 0 {
        reasons.push("Planted is enabled");
    }
    if (holds_mask & HOLDS_MASK_BIT_FLOORED) != 0 {
        reasons.push("Floored is enabled");
    }
    if (holds_mask & HOLDS_MASK_BIT_TWISTER) != 0 {
        reasons.push("Twister is enabled");
    }

    match profile.attack_mode {
        profile::AttackMode::Off => {
            if chart_has_attacks(chart) {
                reasons.push("AttackMode=Off is enabled on a chart with attacks");
            }
        }
        profile::AttackMode::On => {}
        profile::AttackMode::Random => reasons.push("AttackMode=Random is enabled"),
    }

    reasons
}

#[inline(always)]
pub(crate) fn compute_possible_grade_points(
    notes: &[Note],
    note_range: (usize, usize),
    holds_total: u32,
    rolls_total: u32,
) -> i32 {
    let (start, end) = note_range;
    if start >= end {
        return 0;
    }

    let mut rows: Vec<usize> = Vec::with_capacity(end - start);
    for n in &notes[start..end] {
        if n.can_be_judged && !matches!(n.note_type, NoteType::Mine) {
            rows.push(n.row_index);
        }
    }
    rows.sort_unstable();
    rows.dedup();

    let num_tap_rows = rows.len() as u64;
    let pts = (num_tap_rows * 5)
        + (u64::from(holds_total) * judgment::HOLD_SCORE_HELD as u64)
        + (u64::from(rolls_total) * judgment::HOLD_SCORE_HELD as u64);
    pts as i32
}

#[inline(always)]
pub(crate) fn max_grade_points(
    notes: &[Note],
    note_range: (usize, usize),
    holds_total: u32,
    rolls_total: u32,
    base_points: i32,
) -> i32 {
    // ITGmania scores note-changing mods against max(pre, post): inserted notes
    // count, and removed notes still count as misses.
    compute_possible_grade_points(notes, note_range, holds_total, rolls_total).max(base_points)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CourseDisplayTotals {
    pub possible_grade_points: i32,
    pub total_steps: u32,
    pub holds_total: u32,
    pub rolls_total: u32,
    pub mines_total: u32,
}

pub fn course_display_totals_for_chart(chart: &ChartData) -> CourseDisplayTotals {
    CourseDisplayTotals {
        possible_grade_points: chart.possible_grade_points,
        total_steps: chart.stats.total_steps,
        holds_total: chart.holds_total,
        rolls_total: chart.rolls_total,
        mines_total: chart.mines_total,
    }
}

pub(crate) fn stream_sequences_threshold(
    measures: &[usize],
    threshold: usize,
) -> Vec<StreamSegment> {
    let streams: Vec<_> = measures
        .iter()
        .enumerate()
        .filter(|(_, n)| **n >= threshold)
        .map(|(i, _)| i + 1)
        .collect();

    if streams.is_empty() {
        return Vec::new();
    }

    let mut segs = Vec::new();
    let first_break = streams[0].saturating_sub(1);
    if first_break >= 2 {
        segs.push(StreamSegment {
            start: 0,
            end: first_break,
            is_break: true,
        });
    }

    let (mut count, mut end) = (1usize, None);
    for (i, &cur) in streams.iter().enumerate() {
        let next = streams.get(i + 1).copied().unwrap_or(usize::MAX);
        if cur + 1 == next {
            count += 1;
            end = Some(cur + 1);
            continue;
        }

        let e = end.unwrap_or(cur);
        segs.push(StreamSegment {
            start: e - count,
            end: e,
            is_break: false,
        });

        let bstart = cur;
        let bend = if next == usize::MAX {
            measures.len()
        } else {
            next - 1
        };
        if bend >= bstart + 2 {
            segs.push(StreamSegment {
                start: bstart,
                end: bend,
                is_break: true,
            });
        }
        count = 1;
        end = None;
    }
    segs
}

#[inline(always)]
pub(crate) fn target_score_setting_percent(setting: profile::TargetScoreSetting) -> Option<f64> {
    use profile::TargetScoreSetting;
    match setting {
        TargetScoreSetting::CMinus => Some(50.0),
        TargetScoreSetting::C => Some(55.0),
        TargetScoreSetting::CPlus => Some(60.0),
        TargetScoreSetting::BMinus => Some(64.0),
        TargetScoreSetting::B => Some(68.0),
        TargetScoreSetting::BPlus => Some(72.0),
        TargetScoreSetting::AMinus => Some(76.0),
        TargetScoreSetting::A => Some(80.0),
        TargetScoreSetting::APlus => Some(83.0),
        TargetScoreSetting::SMinus => Some(86.0),
        TargetScoreSetting::S => Some(89.0),
        TargetScoreSetting::SPlus => Some(92.0),
        TargetScoreSetting::MachineBest | TargetScoreSetting::PersonalBest => None,
    }
}

#[inline(always)]
fn zmod_stream_density(measures: &[usize], threshold: usize, multiplier: f32) -> f32 {
    let segs = stream_sequences_threshold(measures, threshold);
    if segs.is_empty() {
        return 0.0;
    }
    let mut total_stream = 0.0_f32;
    let mut total_measures = 0.0_f32;
    for seg in &segs {
        let seg_len = ((seg.end.saturating_sub(seg.start)) as f32 * multiplier).floor();
        if seg_len <= 0.0 {
            continue;
        }
        if !seg.is_break {
            total_stream += seg_len;
        }
        total_measures += seg_len;
    }
    if total_measures <= 0.0 {
        0.0
    } else {
        total_stream / total_measures
    }
}

#[inline(always)]
pub(crate) fn zmod_stream_totals_full_measures(
    measures: &[usize],
    constant_bpm: bool,
) -> (Vec<StreamSegment>, f32, f32) {
    let addition = 2usize;

    let mut threshold = 14 + addition;
    let mut multiplier = 1.0_f32;
    if constant_bpm {
        threshold = 30 + addition;
        multiplier = 2.0;

        let d32 = zmod_stream_density(measures, threshold, multiplier);
        if d32 < 0.2 {
            threshold = 22 + addition;
            multiplier = 1.5;
            let d24 = zmod_stream_density(measures, threshold, multiplier);
            if d24 < 0.2 {
                threshold = 18 + addition;
                multiplier = 1.25;
                let d20 = zmod_stream_density(measures, threshold, multiplier);
                if d20 < 0.2 {
                    threshold = 14 + addition;
                    multiplier = 1.0;
                }
            }
        }
    }

    let segs = stream_sequences_threshold(measures, threshold);
    if segs.is_empty() {
        return (segs, 0.0, 0.0);
    }

    let mut total_stream = 0.0_f32;
    let mut total_break = 0.0_f32;
    let mut edge_break = 0.0_f32;
    let mut last_stream = false;
    let len = segs.len();
    for (i, seg) in segs.iter().enumerate() {
        let seg_len = seg.end.saturating_sub(seg.start) as f32;
        if seg_len <= 0.0 {
            continue;
        }
        if seg.is_break && i > 0 && i + 1 < len {
            total_break += seg_len;
            last_stream = false;
        } else if seg.is_break {
            edge_break += seg_len;
            last_stream = false;
        } else {
            if last_stream {
                total_break += 1.0;
            }
            total_stream += seg_len;
            last_stream = true;
        }
    }

    if total_stream + total_break < 10.0 || total_stream + total_break < edge_break {
        total_break += edge_break;
    }

    (segs, total_stream * multiplier, total_break * multiplier)
}

pub fn course_display_carry_from_state(state: &State) -> [CourseDisplayCarry; MAX_PLAYERS] {
    let mut carry = [CourseDisplayCarry::default(); MAX_PLAYERS];
    for player in 0..state.num_players.min(MAX_PLAYERS) {
        let p = &state.players[player];
        let previous = state
            .course_display_carry
            .as_ref()
            .map_or(CourseDisplayCarry::default(), |old| old[player]);
        let mut judgment_counts = [0u32; 6];
        let mut scoring_counts = [0u32; 6];
        for grade in DISPLAY_JUDGE_ORDER {
            let ix = display_judge_ix(grade);
            let stage_judgment = p.judgment_counts[ix];
            let stage_scoring = p.scoring_counts[ix];
            judgment_counts[ix] = previous.judgment_counts[ix].saturating_add(stage_judgment);
            scoring_counts[ix] = previous.scoring_counts[ix].saturating_add(stage_scoring);
        }
        let stage_window_counts = state.live_window_counts[player];
        let stage_window_counts_10ms = state.live_window_counts_10ms_blue[player];
        let stage_window_counts_display_blue = state.live_window_counts_display_blue[player];
        let window_counts = crate::game::timing::WindowCounts {
            w0: previous
                .window_counts
                .w0
                .saturating_add(stage_window_counts.w0),
            w1: previous
                .window_counts
                .w1
                .saturating_add(stage_window_counts.w1),
            w2: previous
                .window_counts
                .w2
                .saturating_add(stage_window_counts.w2),
            w3: previous
                .window_counts
                .w3
                .saturating_add(stage_window_counts.w3),
            w4: previous
                .window_counts
                .w4
                .saturating_add(stage_window_counts.w4),
            w5: previous
                .window_counts
                .w5
                .saturating_add(stage_window_counts.w5),
            miss: previous
                .window_counts
                .miss
                .saturating_add(stage_window_counts.miss),
        };
        let window_counts_10ms_blue = crate::game::timing::WindowCounts {
            w0: previous
                .window_counts_10ms_blue
                .w0
                .saturating_add(stage_window_counts_10ms.w0),
            w1: previous
                .window_counts_10ms_blue
                .w1
                .saturating_add(stage_window_counts_10ms.w1),
            w2: previous
                .window_counts_10ms_blue
                .w2
                .saturating_add(stage_window_counts_10ms.w2),
            w3: previous
                .window_counts_10ms_blue
                .w3
                .saturating_add(stage_window_counts_10ms.w3),
            w4: previous
                .window_counts_10ms_blue
                .w4
                .saturating_add(stage_window_counts_10ms.w4),
            w5: previous
                .window_counts_10ms_blue
                .w5
                .saturating_add(stage_window_counts_10ms.w5),
            miss: previous
                .window_counts_10ms_blue
                .miss
                .saturating_add(stage_window_counts_10ms.miss),
        };
        let window_counts_display_blue = crate::game::timing::WindowCounts {
            w0: previous
                .window_counts_display_blue
                .w0
                .saturating_add(stage_window_counts_display_blue.w0),
            w1: previous
                .window_counts_display_blue
                .w1
                .saturating_add(stage_window_counts_display_blue.w1),
            w2: previous
                .window_counts_display_blue
                .w2
                .saturating_add(stage_window_counts_display_blue.w2),
            w3: previous
                .window_counts_display_blue
                .w3
                .saturating_add(stage_window_counts_display_blue.w3),
            w4: previous
                .window_counts_display_blue
                .w4
                .saturating_add(stage_window_counts_display_blue.w4),
            w5: previous
                .window_counts_display_blue
                .w5
                .saturating_add(stage_window_counts_display_blue.w5),
            miss: previous
                .window_counts_display_blue
                .miss
                .saturating_add(stage_window_counts_display_blue.miss),
        };
        carry[player] = CourseDisplayCarry {
            judgment_counts,
            scoring_counts,
            window_counts,
            window_counts_10ms_blue,
            window_counts_display_blue,
            holds_held_for_score: previous
                .holds_held_for_score
                .saturating_add(p.holds_held_for_score),
            holds_let_go_for_score: previous
                .holds_let_go_for_score
                .saturating_add(p.holds_let_go_for_score),
            rolls_held_for_score: previous
                .rolls_held_for_score
                .saturating_add(p.rolls_held_for_score),
            rolls_let_go_for_score: previous
                .rolls_let_go_for_score
                .saturating_add(p.rolls_let_go_for_score),
            mines_hit_for_score: previous
                .mines_hit_for_score
                .saturating_add(p.mines_hit_for_score),
        };
    }
    if state.num_players == 1 {
        carry[1] = carry[0];
    }
    carry
}

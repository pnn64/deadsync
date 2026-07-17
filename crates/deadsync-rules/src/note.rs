use crate::judgment::Judgment;
use crate::timing::{TIMING_WINDOW_ADD_S, TimingData};
use deadsync_core::input::MAX_COLS;
use deadsync_core::note::NoteType;
use deadsync_core::song_time::{SongTimeNs, song_time_ns_delta_seconds, song_time_ns_from_seconds};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HoldResult {
    Held,
    LetGo,
    Missed,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MineResult {
    Hit,
    Avoided,
}

pub const MAX_HOLD_LIFE: f32 = 1.0;
// Player::GetWindowSeconds applies TimingWindowAdd to hold and roll windows too.
pub const TIMING_WINDOW_SECONDS_HOLD: f32 = 0.32 + TIMING_WINDOW_ADD_S;
pub const TIMING_WINDOW_SECONDS_ROLL: f32 = 0.35 + TIMING_WINDOW_ADD_S;

#[derive(Clone, Debug)]
pub struct HoldData {
    pub end_row_index: usize,
    pub end_beat: f32,
    pub result: Option<HoldResult>,
    pub life: f32,
    pub let_go_started_at: Option<i64>,
    pub let_go_starting_life: f32,
    pub last_held_row_index: usize,
    pub last_held_beat: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HoldLifeAdvance {
    pub life_after: f32,
    pub zero_elapsed_music_ns: Option<SongTimeNs>,
}

#[inline(always)]
pub fn advance_hold_last_held(
    hold: &mut HoldData,
    timing: &TimingData,
    current_beat: f32,
    note_start_row: usize,
    note_start_beat: f32,
) {
    let prev_row = hold.last_held_row_index;
    let prev_beat = hold.last_held_beat.clamp(note_start_beat, hold.end_beat);
    let current_beat = current_beat.clamp(note_start_beat, hold.end_beat);
    let mut current_row = timing
        .get_row_for_beat(current_beat)
        .unwrap_or(note_start_row);
    current_row = current_row.clamp(note_start_row, hold.end_row_index);
    let final_row = prev_row.max(current_row);
    if final_row == prev_row {
        hold.last_held_beat = prev_beat.max(current_beat);
        return;
    }
    hold.last_held_row_index = final_row;
    hold.last_held_beat = prev_beat.max(current_beat);
}

#[inline(always)]
pub const fn hold_window_seconds(note_type: NoteType) -> f32 {
    match note_type {
        NoteType::Roll => TIMING_WINDOW_SECONDS_ROLL,
        _ => TIMING_WINDOW_SECONDS_HOLD,
    }
}

#[inline(always)]
pub fn advance_hold_life_ns(
    note_type: NoteType,
    life: f32,
    pressed: bool,
    music_elapsed_ns: SongTimeNs,
    music_rate: f32,
) -> HoldLifeAdvance {
    let life = life.clamp(0.0, MAX_HOLD_LIFE);
    if music_elapsed_ns <= 0 {
        return HoldLifeAdvance {
            life_after: life,
            zero_elapsed_music_ns: None,
        };
    }
    if matches!(note_type, NoteType::Hold) && pressed {
        return HoldLifeAdvance {
            life_after: MAX_HOLD_LIFE,
            zero_elapsed_music_ns: None,
        };
    }

    let rate = if music_rate.is_finite() && music_rate > 0.0 {
        music_rate
    } else {
        1.0
    };
    let window = hold_window_seconds(note_type);
    if !window.is_finite() || window <= 0.0 {
        return HoldLifeAdvance {
            life_after: 0.0,
            zero_elapsed_music_ns: Some(0),
        };
    }

    let music_elapsed_s = song_time_ns_delta_seconds(music_elapsed_ns, 0);
    let real_elapsed_s = music_elapsed_s / rate;
    let life_drop = real_elapsed_s / window;
    if life_drop < life {
        return HoldLifeAdvance {
            life_after: (life - life_drop).max(0.0),
            zero_elapsed_music_ns: None,
        };
    }

    HoldLifeAdvance {
        life_after: 0.0,
        zero_elapsed_music_ns: Some(song_time_ns_from_seconds(
            (life * window * rate).clamp(0.0, music_elapsed_s),
        )),
    }
}

#[derive(Clone, Debug)]
pub struct Note {
    pub beat: f32,
    pub quantization_idx: u8,
    pub column: usize,
    pub note_type: NoteType,
    pub row_index: usize,
    pub result: Option<Judgment>,
    pub early_result: Option<Judgment>,
    pub hold: Option<HoldData>,
    pub mine_result: Option<MineResult>,
    pub is_fake: bool,
    // Optimization: cached result of !is_fake && !warp && !fake_segment
    pub can_be_judged: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct NoteCountStat {
    pub beat: f32,
    pub notes_lower: usize,
    pub notes_upper: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerTotals {
    pub steps: u32,
    pub holds: u32,
    pub rolls: u32,
    pub mines: u32,
    pub hands: u32,
}

fn carried_holds_for_row(active_hold_ends: &mut [usize; MAX_COLS], row: usize) -> u32 {
    let mut count = 0u32;
    for end in active_hold_ends {
        if *end == usize::MAX || *end < row {
            *end = usize::MAX;
        } else {
            count += 1;
        }
    }
    count
}

/// Recompute post-transform chart totals from notes sorted by row.
///
/// Gameplay chart transforms establish this ordering before runtime setup, and
/// the row-index runtime built immediately afterward relies on the same
/// invariant. Keeping the scan ordered avoids rebuilding and sorting row and
/// hold collections during every transformed-chart initialization.
pub fn recompute_player_totals(notes: &[Note], note_range: (usize, usize)) -> PlayerTotals {
    let (start, end) = note_range;
    if start >= end {
        return PlayerTotals::default();
    }
    let notes = &notes[start..end];
    debug_assert!(
        notes
            .windows(2)
            .all(|pair| pair[0].row_index <= pair[1].row_index),
        "player notes must be row-sorted before totals are recomputed"
    );

    let mut totals = PlayerTotals::default();
    let mut active_hold_ends = [usize::MAX; MAX_COLS];
    let mut note_ix = 0usize;
    while note_ix < notes.len() {
        let row = notes[note_ix].row_index;
        let carried_holds = carried_holds_for_row(&mut active_hold_ends, row);
        let mut has_step = false;
        let mut row_mask = 0u16;
        while note_ix < notes.len() && notes[note_ix].row_index == row {
            let note = &notes[note_ix];
            note_ix += 1;
            if !note.can_be_judged {
                continue;
            }
            has_step |= note.note_type != NoteType::Mine;
            match note.note_type {
                NoteType::Tap => row_mask |= 1u16 << note.column.min(15),
                NoteType::Hold | NoteType::Roll => {
                    if note.note_type == NoteType::Hold {
                        totals.holds = totals.holds.saturating_add(1);
                    } else {
                        totals.rolls = totals.rolls.saturating_add(1);
                    }
                    row_mask |= 1u16 << note.column.min(15);
                    if let Some(hold) = note.hold.as_ref()
                        && let Some(end) = active_hold_ends.get_mut(note.column)
                    {
                        debug_assert!(*end == usize::MAX || *end <= row, "overlapping holds");
                        *end = hold.end_row_index;
                    }
                }
                NoteType::Mine => totals.mines = totals.mines.saturating_add(1),
                NoteType::Lift | NoteType::Fake => {}
            }
        }
        if has_step {
            totals.steps = totals.steps.wrapping_add(1);
        }
        if row_mask != 0 && row_mask.count_ones() + carried_holds >= 3 {
            totals.hands = totals.hands.saturating_add(1);
        }
    }

    totals
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timing::{TimingData, TimingSegments};
    use deadsync_core::song_time::song_time_ns_to_seconds;
    use deadsync_core::timing::ROWS_PER_BEAT;

    fn test_note(column: usize, row_index: usize, note_type: NoteType) -> Note {
        Note {
            beat: row_index as f32,
            quantization_idx: 0,
            column,
            note_type,
            row_index,
            result: None,
            early_result: None,
            hold: None,
            mine_result: None,
            is_fake: false,
            can_be_judged: true,
        }
    }

    fn test_hold(
        column: usize,
        row_index: usize,
        end_row_index: usize,
        note_type: NoteType,
    ) -> Note {
        Note {
            hold: Some(HoldData {
                end_row_index,
                end_beat: end_row_index as f32,
                result: None,
                life: 1.0,
                let_go_started_at: None,
                let_go_starting_life: 0.0,
                last_held_row_index: row_index,
                last_held_beat: row_index as f32,
            }),
            ..test_note(column, row_index, note_type)
        }
    }

    fn row_to_beat(rows: usize) -> Vec<f32> {
        (0..=rows)
            .map(|row| row as f32 / ROWS_PER_BEAT as f32)
            .collect()
    }

    #[test]
    fn totals_count_holds_rolls_mines_and_distinct_steps() {
        let notes = vec![
            test_note(0, 48, NoteType::Tap),
            test_note(1, 48, NoteType::Tap),
            test_hold(2, 96, 144, NoteType::Hold),
            test_hold(3, 192, 240, NoteType::Roll),
            test_note(0, 288, NoteType::Mine),
        ];

        let totals = recompute_player_totals(&notes, (0, notes.len()));

        assert_eq!(totals.steps, 3);
        assert_eq!(totals.holds, 1);
        assert_eq!(totals.rolls, 1);
        assert_eq!(totals.mines, 1);
    }

    #[test]
    fn totals_count_three_note_row_as_hand() {
        let notes = vec![
            test_note(0, 48, NoteType::Tap),
            test_note(1, 48, NoteType::Tap),
            test_note(2, 48, NoteType::Tap),
        ];

        let totals = recompute_player_totals(&notes, (0, notes.len()));

        assert_eq!(totals.hands, 1);
    }

    #[test]
    fn totals_count_carried_hold_as_hand_note() {
        let notes = vec![
            test_hold(0, 0, 96, NoteType::Hold),
            test_note(1, 48, NoteType::Tap),
            test_note(2, 48, NoteType::Tap),
        ];

        let totals = recompute_player_totals(&notes, (0, notes.len()));

        assert_eq!(totals.hands, 1);
    }

    #[test]
    fn totals_carry_hold_through_its_end_row() {
        let notes = vec![
            test_hold(0, 0, 48, NoteType::Hold),
            test_note(1, 48, NoteType::Tap),
            test_note(2, 48, NoteType::Tap),
        ];

        let totals = recompute_player_totals(&notes, (0, notes.len()));

        assert_eq!(totals.hands, 1);
    }

    #[test]
    fn totals_do_not_count_carried_holds_without_a_row_note() {
        let notes = vec![
            test_hold(0, 0, 96, NoteType::Hold),
            test_hold(1, 0, 96, NoteType::Hold),
            test_hold(2, 0, 96, NoteType::Hold),
            test_note(3, 48, NoteType::Mine),
        ];

        let totals = recompute_player_totals(&notes, (0, notes.len()));

        assert_eq!(totals.hands, 1);
    }

    #[test]
    fn totals_count_judgable_lift_and_fake_rows_as_steps() {
        let notes = vec![
            test_note(0, 48, NoteType::Lift),
            test_note(1, 96, NoteType::Fake),
        ];

        let totals = recompute_player_totals(&notes, (0, notes.len()));

        assert_eq!(totals.steps, 2);
        assert_eq!(totals.hands, 0);
    }

    #[test]
    fn totals_ignore_unjudgable_notes() {
        let mut notes = vec![
            test_note(0, 48, NoteType::Tap),
            test_hold(1, 96, 144, NoteType::Hold),
            test_note(2, 192, NoteType::Mine),
        ];
        for note in &mut notes {
            note.can_be_judged = false;
        }

        let totals = recompute_player_totals(&notes, (0, notes.len()));

        assert_eq!(totals, PlayerTotals::default());
    }

    #[test]
    fn hold_life_advance_keeps_pressed_holds_full() {
        let advanced = advance_hold_life_ns(
            NoteType::Hold,
            0.25,
            true,
            song_time_ns_from_seconds(0.2),
            1.0,
        );

        assert_eq!(
            advanced,
            HoldLifeAdvance {
                life_after: MAX_HOLD_LIFE,
                zero_elapsed_music_ns: None,
            }
        );
    }

    #[test]
    fn hold_life_advance_reports_exact_zero_cross_time() {
        let advanced = advance_hold_life_ns(
            NoteType::Hold,
            0.25,
            false,
            song_time_ns_from_seconds(0.2),
            1.0,
        );

        assert_eq!(advanced.life_after, 0.0);
        let zero_elapsed = advanced
            .zero_elapsed_music_ns
            .expect("hold should cross zero");
        assert!((song_time_ns_to_seconds(zero_elapsed) - 0.080375).abs() <= 1e-6);
    }

    #[test]
    fn hold_life_advance_split_intervals_match_single_interval() {
        let whole = advance_hold_life_ns(
            NoteType::Hold,
            1.0,
            false,
            song_time_ns_from_seconds(0.16),
            1.0,
        );
        let first = advance_hold_life_ns(
            NoteType::Hold,
            1.0,
            false,
            song_time_ns_from_seconds(0.05),
            1.0,
        );
        let split = advance_hold_life_ns(
            NoteType::Hold,
            first.life_after,
            false,
            song_time_ns_from_seconds(0.11),
            1.0,
        );

        assert!((whole.life_after - split.life_after).abs() <= 1e-6);
        assert_eq!(whole.zero_elapsed_music_ns, split.zero_elapsed_music_ns);
    }

    #[test]
    fn roll_life_advance_scales_zero_cross_with_music_rate() {
        let advanced = advance_hold_life_ns(
            NoteType::Roll,
            0.5,
            false,
            song_time_ns_from_seconds(0.4),
            2.0,
        );

        assert_eq!(advanced.life_after, 0.0);
        let zero_elapsed = advanced
            .zero_elapsed_music_ns
            .expect("roll should cross zero");
        assert!((song_time_ns_to_seconds(zero_elapsed) - 0.3515).abs() <= 1e-6);
    }

    #[test]
    fn advance_hold_last_held_keeps_exact_progress_beat() {
        let timing =
            TimingData::from_segments(0.0, 0.0, &TimingSegments::default(), &row_to_beat(96));
        let mut hold = test_hold(0, 0, 96, NoteType::Hold)
            .hold
            .expect("test hold has hold data");
        hold.end_beat = 96.0 / ROWS_PER_BEAT as f32;
        hold.last_held_row_index = 24;
        hold.last_held_beat = 24.0 / ROWS_PER_BEAT as f32;

        advance_hold_last_held(&mut hold, &timing, 0.99, 0, 0.0);

        assert_eq!(hold.last_held_row_index, 48);
        assert!((hold.last_held_beat - 0.99).abs() <= 1e-6);
    }

    #[test]
    fn advance_hold_last_held_progresses_to_row_boundary() {
        let timing =
            TimingData::from_segments(0.0, 0.0, &TimingSegments::default(), &row_to_beat(96));
        let mut hold = test_hold(0, 0, 96, NoteType::Hold)
            .hold
            .expect("test hold has hold data");
        hold.end_beat = 96.0 / ROWS_PER_BEAT as f32;
        hold.last_held_row_index = 24;
        hold.last_held_beat = 24.0 / ROWS_PER_BEAT as f32;

        advance_hold_last_held(&mut hold, &timing, 1.0, 0, 0.0);

        assert_eq!(hold.last_held_row_index, 48);
        assert!((hold.last_held_beat - 1.0).abs() <= 1e-6);
    }
}

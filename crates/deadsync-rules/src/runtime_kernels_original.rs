//! Frozen production kernels from a4d53398c.
use super::*;

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
        let mut row_mask = 0u32;
        while note_ix < notes.len() && notes[note_ix].row_index == row {
            let note = &notes[note_ix];
            note_ix += 1;
            if !note.can_be_judged {
                continue;
            }
            has_step |= note.note_type != NoteType::Mine;
            match note.note_type {
                NoteType::Tap => row_mask |= 1u32 << note.column.min(31),
                NoteType::Hold | NoteType::Roll => {
                    if note.note_type == NoteType::Hold {
                        totals.holds = totals.holds.saturating_add(1);
                    } else {
                        totals.rolls = totals.rolls.saturating_add(1);
                    }
                    row_mask |= 1u32 << note.column.min(31);
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

pub fn live_timing_stats_snapshot(stats: &LiveTimingStats) -> LiveTimingSnapshot {
    let mut recent_max_abs = 0.0_f32;
    for e in stats.recent_errors_ms.iter().take(stats.recent_len) {
        recent_max_abs = recent_max_abs.max(e.abs());
    }
    LiveTimingSnapshot {
        recent: stats_from_sums(
            stats.recent_len as u32,
            stats.recent_sum_ms,
            stats.recent_sum_abs_ms,
            stats.recent_sum_sq_ms,
            recent_max_abs,
        ),
        all: stats_from_sums(
            stats.all_count,
            stats.all_sum_ms,
            stats.all_sum_abs_ms,
            stats.all_sum_sq_ms,
            stats.all_max_abs_ms,
        ),
    }
}

pub fn record_live_timing_stats(stats: &mut LiveTimingStats, judgment: &Judgment) {
    if judgment.grade == JudgeGrade::Miss {
        return;
    }

    let e = judgment.time_error_ms;
    let abs = e.abs();
    let sq = e * e;
    stats.all_sum_ms += e;
    stats.all_sum_abs_ms += abs;
    stats.all_sum_sq_ms += sq;
    stats.all_max_abs_ms = stats.all_max_abs_ms.max(abs);
    stats.all_count = stats.all_count.saturating_add(1);

    if stats.recent_len == LIVE_TIMING_RECENT {
        let old = stats.recent_errors_ms[stats.recent_next];
        stats.recent_sum_ms -= old;
        stats.recent_sum_abs_ms -= old.abs();
        stats.recent_sum_sq_ms = old.mul_add(-old, stats.recent_sum_sq_ms);
    } else {
        stats.recent_len += 1;
    }
    stats.recent_errors_ms[stats.recent_next] = e;
    stats.recent_sum_ms += e;
    stats.recent_sum_abs_ms += abs;
    stats.recent_sum_sq_ms += sq;
    stats.recent_next = (stats.recent_next + 1) % LIVE_TIMING_RECENT;
}

pub fn pixels_per_second(
    setting: ScrollSpeedSetting,
    current_chart_bpm: f32,
    reference_bpm: f32,
    music_rate: f32,
) -> f32 {
    let bpm = setting.effective_bpm(current_chart_bpm, reference_bpm, music_rate);
    if !bpm.is_finite() || bpm <= 0.0 {
        0.0
    } else {
        (bpm / 60.0) * ScrollSpeedSetting::ARROW_SPACING
    }
}

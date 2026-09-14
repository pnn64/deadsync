// Frozen from e4ed8302c28ea18e06cccf425a30d8f15f1d7482 (0.5.1209).
// Only row-visitor visibility differs; types and unchanged selection/math helpers are shared.
use super::*;

#[inline]
pub fn record_life_history(history: &mut Vec<(f32, f32)>, t: f32, life: f32) {
    const EPS: f32 = 0.000_001_f32;

    let life = life.clamp(0.0_f32, 1.0_f32);
    let Some(&(last_t, last_life)) = history.last() else {
        history.push((t, life));
        return;
    };

    if t > last_t {
        history.push((t, life));
        if history.len() >= 3 {
            let len = history.len();
            let a = history[len - 3].1;
            let b = history[len - 2].1;
            let c = history[len - 1].1;
            if a == b && b == c {
                history.remove(len - 2);
            }
        }
        return;
    }

    if (t - last_t).abs() <= EPS {
        if life == last_life {
            return;
        }
        let shifted_t = t - LIFE_HISTORY_SAME_TIME_SHIFT;
        let last_ix = history.len() - 1;
        if last_ix > 0 && (history[last_ix - 1].0 - shifted_t).abs() <= EPS {
            history[last_ix - 1] = (shifted_t, last_life);
            history[last_ix] = (t, life);
        } else {
            history[last_ix].0 = shifted_t;
            history.push((t, life));
        }
    }
}

#[inline(always)]
pub(super) fn for_each_row_final_judgment<F>(notes: &[Note], mut f: F)
where
    F: FnMut(&Judgment),
{
    let mut idx: usize = 0;
    while idx < notes.len() {
        let row_start = idx;
        let row_index = notes[idx].row_index;
        while idx < notes.len() && notes[idx].row_index == row_index {
            idx += 1;
        }

        if let Some(j) = judgment::aggregate_row_final_judgment(
            notes[row_start..idx].iter().filter_map(judgeable_result),
        ) {
            f(j);
        }
    }
}

#[inline(always)]
const fn judgeable_result(note: &Note) -> Option<&Judgment> {
    if note.is_fake || !note.can_be_judged || matches!(note.note_type, NoteType::Mine) {
        None
    } else {
        note.result.as_ref()
    }
}

#[inline(always)]
#[must_use]
pub fn compute_note_timing_stats(notes: &[Note]) -> TimingStats {
    let mut stats = StatsAccum::default();
    for_each_row_final_judgment(notes, |j| {
        if j.grade != JudgeGrade::Miss {
            stats.add(j.time_error_ms);
        }
    });
    stats.finish_stats()
}

#[inline(always)]
fn local_direction_code(note: &Note, col_offset: usize, cols_per_player: usize) -> Option<u8> {
    if note.column < col_offset {
        return None;
    }
    let local = note.column - col_offset;
    if local >= cols_per_player {
        return None;
    }
    let code = local.saturating_add(1).min(u8::MAX as usize) as u8;
    Some(code)
}

#[inline(always)]
#[must_use]
pub fn build_scatter_points(
    notes: &[Note],
    note_time_cache_ns: &[i64],
    col_offset: usize,
    cols_per_player: usize,
    foot_by_row: Option<&[(usize, ScatterFoot)]>,
) -> Vec<ScatterPoint> {
    let mut out = Vec::with_capacity(notes.len());
    visit_scatter_points(
        notes,
        note_time_cache_ns,
        col_offset,
        cols_per_player,
        foot_by_row,
        |point| out.push(point),
    );
    out
}

#[inline(always)]
pub fn visit_scatter_points(
    notes: &[Note],
    note_time_cache_ns: &[i64],
    col_offset: usize,
    cols_per_player: usize,
    foot_by_row: Option<&[(usize, ScatterFoot)]>,
    mut visit: impl FnMut(ScatterPoint),
) {
    debug_assert!(foot_by_row.is_none_or(|rows| rows.windows(2).all(|pair| pair[0].0 < pair[1].0)));
    let mut row_start = 0usize;
    let mut foot_row_idx = 0usize;

    while row_start < notes.len() {
        let row = notes[row_start].row_index;
        let mut row_end = row_start + 1;
        while row_end < notes.len() && notes[row_end].row_index == row {
            row_end += 1;
        }
        let parity_foot = if let Some(rows) = foot_by_row {
            while foot_row_idx < rows.len() && rows[foot_row_idx].0 < row {
                foot_row_idx += 1;
            }
            rows.get(foot_row_idx)
                .filter(|(foot_row, _)| *foot_row == row)
                .map(|(_, foot)| *foot)
                .unwrap_or_default()
        } else {
            ScatterFoot::Unknown
        };

        let mut row_judgment = None;
        let mut representative_ix: Option<usize> = None;
        let mut direction_code = 0u8;
        for (offset, n) in notes[row_start..row_end].iter().enumerate() {
            let i = row_start + offset;
            if n.is_fake || !n.can_be_judged || matches!(n.note_type, NoteType::Mine) {
                continue;
            }
            if let Some(judgment) = n.result.as_ref() {
                judgment::select_row_final_judgment(&mut row_judgment, judgment);
                representative_ix.get_or_insert(i);
            }
            if let Some(code) = local_direction_code(n, col_offset, cols_per_player) {
                direction_code = direction_code.saturating_add(code);
            }
        }

        let Some(judgment) = row_judgment else {
            row_start = row_end;
            continue;
        };
        let Some(idx) = representative_ix else {
            row_start = row_end;
            continue;
        };
        let t = note_time_cache_ns
            .get(idx)
            .copied()
            .map(|time_ns| (time_ns as f64 * 1.0e-9) as f32)
            .unwrap_or(0.0);
        let offset_ms = if judgment.grade == JudgeGrade::Miss {
            None
        } else {
            Some(judgment.time_error_ms)
        };

        visit(ScatterPoint {
            time_sec: t,
            offset_ms,
            direction_code,
            miss_because_held: judgment.grade == JudgeGrade::Miss && judgment.miss_because_held,
            row_index: row,
            quantization_idx: notes[idx].quantization_idx,
            parity_foot,
        });

        row_start = row_end;
    }
}

#[inline(always)]
#[must_use]
pub fn compute_window_counts_blue_ms(notes: &[Note], blue_window_ms: f32) -> WindowCounts {
    let mut out = WindowCounts::default();
    let split_ms = if blue_window_ms.is_finite() && blue_window_ms > 0.0 {
        blue_window_ms
    } else {
        FA_PLUS_W010_MS
    };

    for_each_row_final_judgment(notes, |j| match j.grade {
        JudgeGrade::Fantastic => {
            if j.time_error_ms.abs() <= split_ms {
                out.w0 = out.w0.saturating_add(1);
            } else {
                out.w1 = out.w1.saturating_add(1);
            }
        }
        JudgeGrade::Excellent => out.w2 = out.w2.saturating_add(1),
        JudgeGrade::Great => out.w3 = out.w3.saturating_add(1),
        JudgeGrade::Decent => out.w4 = out.w4.saturating_add(1),
        JudgeGrade::WayOff => out.w5 = out.w5.saturating_add(1),
        JudgeGrade::Miss => out.miss = out.miss.saturating_add(1),
    });

    out
}

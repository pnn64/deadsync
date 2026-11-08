use super::*;

#[inline(always)]
#[must_use]
pub const fn combo_continues_on_grade(grade: JudgeGrade) -> bool {
    matches!(
        grade,
        JudgeGrade::Fantastic | JudgeGrade::Excellent | JudgeGrade::Great
    )
}

#[inline(always)]
#[must_use]
pub const fn combo_increments_miss_combo(grade: JudgeGrade) -> bool {
    matches!(grade, JudgeGrade::Miss)
}

#[inline(always)]
pub const fn clear_full_combo_state(state: &mut ComboState) {
    state.first_fc_attempt_broken = true;
    state.full_combo_grade = None;
}

#[inline(always)]
pub fn break_combo_state(state: &mut ComboState, miss_combo_delta: u32) -> ComboUpdate {
    state.combo = 0;
    if miss_combo_delta > 0 {
        state.miss_combo = state.miss_combo.saturating_add(miss_combo_delta);
    }
    clear_full_combo_state(state);
    state.current_combo_grade = None;
    ComboUpdate {
        combo_broken: true,
        ..ComboUpdate::default()
    }
}

#[inline(always)]
fn apply_successful_row_combo_state(
    state: &mut ComboState,
    final_grade: JudgeGrade,
    row_combo_count: u32,
) -> ComboUpdate {
    state.miss_combo = 0;
    let old_combo = state.combo;
    state.combo = old_combo.saturating_add(row_combo_count);
    let combo = state.combo;
    // Jumps and combo multipliers can cross a milestone without landing on it.
    let hit_thousand_milestone = combo / 1000 > old_combo / 1000;
    let hit_hundred_milestone = combo / 100 > old_combo / 100;

    if !state.first_fc_attempt_broken {
        let new_grade = if let Some(current_fc_grade) = state.full_combo_grade {
            final_grade.max(current_fc_grade)
        } else {
            final_grade
        };
        state.full_combo_grade = Some(new_grade);
    }
    let current_combo_grade = if let Some(curr_grade) = state.current_combo_grade {
        final_grade.max(curr_grade)
    } else {
        final_grade
    };
    state.current_combo_grade = Some(current_combo_grade);

    ComboUpdate {
        combo_broken: false,
        hit_hundred_milestone,
        hit_thousand_milestone,
    }
}

#[inline(always)]
pub fn apply_row_combo_state(
    state: &mut ComboState,
    final_grade: JudgeGrade,
    row_combo_count: u32,
    miss_combo_count: u32,
) -> ComboUpdate {
    if combo_continues_on_grade(final_grade) {
        return apply_successful_row_combo_state(state, final_grade, row_combo_count);
    }

    let miss_combo_delta = if combo_increments_miss_combo(final_grade) {
        miss_combo_count
    } else {
        0
    };
    break_combo_state(state, miss_combo_delta)
}

#[inline(always)]
pub fn apply_life_delta(
    meter: &mut LifeMeter,
    current_music_time: f32,
    delta: f32,
) -> LifeDeltaResult {
    apply_life_delta_mode(meter, current_music_time, delta, true)
}

pub fn apply_life_delta_mode(
    meter: &mut LifeMeter,
    current_music_time: f32,
    delta: f32,
    fail_at_zero: bool,
) -> LifeDeltaResult {
    if meter.is_failing || (fail_at_zero && meter.life <= 0.0) {
        let old_life = meter.life;
        meter.life = 0.0;
        meter.is_failing = true;
        return LifeDeltaResult {
            old_life,
            new_life: 0.0,
            failed_now: false,
        };
    }

    let old_life = meter.life;

    let mut final_delta = delta;
    if old_life >= 1.0 && final_delta < 0.0 {
        final_delta = final_delta.min(HOT_LIFE_MIN_NEGATIVE_DELTA);
    }
    if final_delta >= 0.0 {
        if meter.combo_after_miss > 0 {
            meter.combo_after_miss -= 1;
            if meter.combo_after_miss > 0 {
                final_delta = 0.0;
            }
        }
    } else if final_delta < 0.0 {
        let stacked_lock = meter
            .combo_after_miss
            .saturating_add(REGEN_COMBO_AFTER_MISS);
        meter.combo_after_miss = meter
            .combo_after_miss
            .max(stacked_lock.min(MAX_REGEN_COMBO_AFTER_MISS));
    }

    let mut new_life = (meter.life + final_delta).clamp(0.0, 1.0);
    let failed_now = fail_at_zero && new_life <= 0.0 && !meter.is_failing;

    if fail_at_zero && new_life <= 0.0 {
        if failed_now {
            meter.fail_time = Some(current_music_time);
        }
        new_life = 0.0;
        meter.is_failing = true;
    }

    meter.life = new_life;
    LifeDeltaResult {
        old_life,
        new_life,
        failed_now,
    }
}

#[must_use]
pub fn tap_strokes(offset_ms: Option<f32>) -> u32 {
    strokes_from_ms(offset_ms.map(|ms| f64::from(ms).abs()))
}

#[must_use]
pub fn tap_strokes_seconds(offset: Option<f64>) -> u32 {
    strokes_from_ms(offset.map(|offset| offset.abs() * 1000.0))
}

// The curve floors absolute milliseconds before scoring, so its finite domain
// has only 104 results. Keep the 103.5 ms cutoff before truncating to an index.
const STROKES: [u8; 104] = [
    0, 0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 12, 12, 13,
    14, 14, 15, 16, 17, 17, 18, 19, 19, 20, 21, 21, 22, 23, 24, 24, 25, 26, 27, 28, 29, 30, 31, 32,
    32, 33, 34, 35, 36, 37, 37, 38, 39, 40, 41, 41, 42, 43, 44, 44, 45, 46, 47, 47, 48, 49, 49, 50,
    51, 51, 52, 52, 53, 54, 54, 55, 56, 56, 57, 57, 58, 58, 59, 60, 60, 61, 61, 62, 62, 63, 63, 64,
    64, 65, 65,
];

pub(super) fn strokes_from_ms(ms: Option<f64>) -> u32 {
    match ms {
        Some(ms) if (0.0..=103.5).contains(&ms) => u32::from(STROKES[ms as usize]),
        _ => MISS_STROKES,
    }
}

pub fn record_golf_tap(score: &mut GolfScore, offset_ms: Option<f32>) -> u32 {
    let strokes = tap_strokes(offset_ms);
    score.raw_strokes = score.raw_strokes.saturating_add(u64::from(strokes));
    score.judged_rows = score.judged_rows.saturating_add(1);
    strokes
}

pub(super) fn parse_scroll(s: &str) -> Result<ScrollSpeedSetting, String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err("ScrollSpeed value is empty".to_string());
    }

    let (variant, value_str) = if let Some(rest) = trimmed.strip_prefix('C') {
        ("C", rest)
    } else if let Some(rest) = trimmed.strip_prefix('c') {
        ("C", rest)
    } else if let Some(rest) = trimmed.strip_prefix('X') {
        ("X", rest)
    } else if let Some(rest) = trimmed.strip_prefix('x') {
        ("X", rest)
    } else if let Some(rest) = trimmed.strip_prefix('M') {
        ("M", rest)
    } else if let Some(rest) = trimmed.strip_prefix('m') {
        ("M", rest)
    } else {
        return Err(format!(
            "ScrollSpeed '{trimmed}' must start with 'C', 'X', or 'M'"
        ));
    };

    let value: f32 = value_str
        .trim()
        .parse()
        .map_err(|_| format!("ScrollSpeed '{trimmed}' is not a valid number"))?;

    if value <= 0.0 {
        return Err(format!("ScrollSpeed '{trimmed}' must be greater than zero"));
    }

    match variant {
        "C" => Ok(ScrollSpeedSetting::CMod(value)),
        "X" => Ok(ScrollSpeedSetting::XMod(value)),
        "M" => Ok(ScrollSpeedSetting::MMod(value)),
        _ => Err(format!(
            "ScrollSpeed '{trimmed}' has an unsupported modifier"
        )),
    }
}

#[inline(always)]
#[must_use]
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

pub(super) fn strokes_lower_u32(ms: Option<f64>) -> u32 {
    match ms {
        Some(ms) if (0.0..=103.5).contains(&ms) => u32::from(STROKES[ms as u32 as usize]),
        _ => MISS_STROKES,
    }
}

pub(super) fn strokes_upper_usize(ms: Option<f64>) -> u32 {
    match ms {
        Some(ms) if ms <= 103.5 => u32::from(STROKES[ms as usize]),
        _ => MISS_STROKES,
    }
}

pub(super) fn seconds_lower_u32(offset: Option<f64>) -> u32 {
    strokes_lower_u32(offset.map(|offset| offset.abs() * 1000.0))
}

pub(super) fn seconds_upper_usize(offset: Option<f64>) -> u32 {
    strokes_upper_usize(offset.map(|offset| offset.abs() * 1000.0))
}

use crate::judgment::JudgeGrade;

// Match Simply Love ITG / FA+: repeated negative life events add a 5-hit lock
// back up to a 10-hit ceiling before life can regenerate again.
pub const REGEN_COMBO_AFTER_MISS: u32 = 5;
pub const MAX_REGEN_COMBO_AFTER_MISS: u32 = 10;

// Simply Love enables HarshHotLifePenalty, so negative events from a full bar
// should cost at least 10% life.
pub const HOT_LIFE_MIN_NEGATIVE_DELTA: f32 = -0.10;

// ITGmania _fallback and Simply Love life deltas.
pub const LIFE_FANTASTIC: f32 = 0.008;
pub const LIFE_EXCELLENT: f32 = 0.008;
pub const LIFE_GREAT: f32 = 0.004;
pub const LIFE_DECENT: f32 = 0.0;
pub const LIFE_WAY_OFF: f32 = -0.050;
pub const LIFE_MISS: f32 = -0.100;
pub const LIFE_HIT_MINE: f32 = -0.050;
pub const LIFE_HELD: f32 = 0.008;
pub const LIFE_LET_GO: f32 = -0.080;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LifeMeter {
    pub life: f32,
    pub combo_after_miss: u32,
    pub is_failing: bool,
    pub fail_time: Option<f32>,
}

impl LifeMeter {
    #[inline(always)]
    pub const fn new(life: f32) -> Self {
        Self {
            life,
            combo_after_miss: 0,
            is_failing: false,
            fail_time: None,
        }
    }

    #[inline(always)]
    pub const fn course_submit_start() -> Self {
        Self::new(0.5)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LifeDeltaResult {
    pub old_life: f32,
    pub new_life: f32,
    pub failed_now: bool,
}

pub const LIFE_HISTORY_SAME_TIME_SHIFT: f32 = 0.003_906_25_f32;

#[inline(always)]
pub const fn judge_life_delta(grade: JudgeGrade) -> f32 {
    match grade {
        JudgeGrade::Fantastic => LIFE_FANTASTIC,
        JudgeGrade::Excellent => LIFE_EXCELLENT,
        JudgeGrade::Great => LIFE_GREAT,
        JudgeGrade::Decent => LIFE_DECENT,
        JudgeGrade::WayOff => LIFE_WAY_OFF,
        JudgeGrade::Miss => LIFE_MISS,
    }
}

#[inline(always)]
pub fn record_life_history(history: &mut Vec<(f32, f32)>, t: f32, life: f32) {
    let life = life.clamp(0.0_f32, 1.0_f32);
    let Some(&(last_t, last_life)) = history.last() else {
        history.push((t, life));
        return;
    };

    if t > last_t {
        if (life - last_life).abs() > 0.000_001_f32 {
            history.push((t, life));
        }
        return;
    }

    if (t - last_t).abs() <= 0.000_001_f32 {
        if (life - last_life).abs() <= 0.000_001_f32 {
            return;
        }
        let last_ix = history.len() - 1;
        history[last_ix].0 = t - LIFE_HISTORY_SAME_TIME_SHIFT;
        history.push((t, life));
    }
}

#[inline(always)]
pub fn apply_life_delta(
    meter: &mut LifeMeter,
    current_music_time: f32,
    delta: f32,
) -> LifeDeltaResult {
    if meter.is_failing || meter.life <= 0.0 {
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
    let failed_now = new_life <= 0.0 && !meter.is_failing;

    if new_life <= 0.0 {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_life_events_burn_down_regen_lock_before_healing() {
        let mut meter = LifeMeter::new(0.5);
        meter.combo_after_miss = REGEN_COMBO_AFTER_MISS;

        for _ in 0..REGEN_COMBO_AFTER_MISS {
            apply_life_delta(&mut meter, 0.0, LIFE_DECENT);
        }

        assert_eq!(meter.combo_after_miss, 0);
        assert!((meter.life - 0.5).abs() <= 1e-6);

        apply_life_delta(&mut meter, 0.0, LIFE_GREAT);

        assert!((meter.life - 0.504).abs() <= 1e-6);
    }

    #[test]
    fn repeated_negative_events_stack_regen_lock_to_maximum() {
        let mut meter = LifeMeter::new(0.5);
        meter.combo_after_miss = REGEN_COMBO_AFTER_MISS;

        apply_life_delta(&mut meter, 0.0, LIFE_HIT_MINE);
        assert_eq!(meter.combo_after_miss, MAX_REGEN_COMBO_AFTER_MISS);

        apply_life_delta(&mut meter, 0.0, LIFE_HIT_MINE);
        assert_eq!(meter.combo_after_miss, MAX_REGEN_COMBO_AFTER_MISS);
    }

    #[test]
    fn hot_life_penalty_clamps_negative_events_to_ten_percent() {
        let mut meter = LifeMeter::new(1.0);

        apply_life_delta(&mut meter, 0.0, LIFE_HIT_MINE);

        assert!((meter.life - 0.9).abs() <= 1e-6);
    }

    #[test]
    fn fail_transition_records_first_fail_time() {
        let mut meter = LifeMeter::new(0.03);

        let result = apply_life_delta(&mut meter, 12.5, LIFE_MISS);

        assert!(result.failed_now);
        assert!(meter.is_failing);
        assert_eq!(meter.fail_time, Some(12.5));
        assert_eq!(meter.life, 0.0);
    }

    #[test]
    fn judge_life_deltas_match_itg_values() {
        assert_eq!(judge_life_delta(JudgeGrade::Fantastic), LIFE_FANTASTIC);
        assert_eq!(judge_life_delta(JudgeGrade::Excellent), LIFE_EXCELLENT);
        assert_eq!(judge_life_delta(JudgeGrade::Great), LIFE_GREAT);
        assert_eq!(judge_life_delta(JudgeGrade::Decent), LIFE_DECENT);
        assert_eq!(judge_life_delta(JudgeGrade::WayOff), LIFE_WAY_OFF);
        assert_eq!(judge_life_delta(JudgeGrade::Miss), LIFE_MISS);
    }

    #[test]
    fn life_history_records_changes_only() {
        let mut history = Vec::new();

        record_life_history(&mut history, 1.0, 0.8);
        record_life_history(&mut history, 2.0, 0.8);
        record_life_history(&mut history, 3.0, 0.7);

        assert_eq!(history, vec![(1.0, 0.8), (3.0, 0.7)]);
    }

    #[test]
    fn life_history_shifts_same_time_changes() {
        let mut history = Vec::new();

        record_life_history(&mut history, 4.0, 0.8);
        record_life_history(&mut history, 4.0, 0.7);

        assert_eq!(
            history,
            vec![(4.0 - LIFE_HISTORY_SAME_TIME_SHIFT, 0.8), (4.0, 0.7)]
        );
    }

    #[test]
    fn life_history_clamps_life_values() {
        let mut history = Vec::new();

        record_life_history(&mut history, 1.0, 2.0);
        record_life_history(&mut history, 2.0, -1.0);

        assert_eq!(history, vec![(1.0, 1.0), (2.0, 0.0)]);
    }
}

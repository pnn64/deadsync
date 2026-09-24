use crate::judgment::JudgeGrade;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ComboState {
    pub combo: u32,
    pub miss_combo: u32,
    pub full_combo_grade: Option<JudgeGrade>,
    pub current_combo_grade: Option<JudgeGrade>,
    pub first_fc_attempt_broken: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ComboUpdate {
    pub combo_broken: bool,
    pub hit_hundred_milestone: bool,
    pub hit_thousand_milestone: bool,
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_rows_clear_miss_combo_and_extend_combo() {
        let mut state = ComboState {
            combo: 20,
            miss_combo: 4,
            ..ComboState::default()
        };

        let update = apply_row_combo_state(&mut state, JudgeGrade::Great, 2, 1);

        assert_eq!(state.combo, 22);
        assert_eq!(state.miss_combo, 0);
        assert_eq!(state.full_combo_grade, Some(JudgeGrade::Great));
        assert_eq!(state.current_combo_grade, Some(JudgeGrade::Great));
        assert!(!update.combo_broken);
    }

    #[test]
    fn worse_successful_rows_degrade_combo_grade() {
        let mut state = ComboState {
            combo: 20,
            full_combo_grade: Some(JudgeGrade::Fantastic),
            current_combo_grade: Some(JudgeGrade::Fantastic),
            ..ComboState::default()
        };

        apply_row_combo_state(&mut state, JudgeGrade::Great, 1, 0);

        assert_eq!(state.full_combo_grade, Some(JudgeGrade::Great));
        assert_eq!(state.current_combo_grade, Some(JudgeGrade::Great));
    }

    #[test]
    fn bad_first_row_breaks_full_combo_attempt() {
        let mut state = ComboState::default();

        apply_row_combo_state(&mut state, JudgeGrade::Decent, 1, 1);

        assert_eq!(state.combo, 0);
        assert!(state.full_combo_grade.is_none());
        assert!(state.current_combo_grade.is_none());
        assert!(state.first_fc_attempt_broken);

        apply_row_combo_state(&mut state, JudgeGrade::Fantastic, 1, 0);

        assert_eq!(state.combo, 1);
        assert!(state.full_combo_grade.is_none());
        assert_eq!(state.current_combo_grade, Some(JudgeGrade::Fantastic));
    }

    #[test]
    fn decent_rows_break_combo_without_clearing_existing_miss_combo() {
        let mut state = ComboState {
            combo: 20,
            miss_combo: 4,
            full_combo_grade: Some(JudgeGrade::Great),
            current_combo_grade: Some(JudgeGrade::Great),
            ..ComboState::default()
        };

        let update = apply_row_combo_state(&mut state, JudgeGrade::Decent, 2, 1);

        assert_eq!(state.combo, 0);
        assert_eq!(state.miss_combo, 4);
        assert!(state.full_combo_grade.is_none());
        assert!(state.current_combo_grade.is_none());
        assert!(state.first_fc_attempt_broken);
        assert!(update.combo_broken);
    }

    #[test]
    fn miss_rows_increment_existing_miss_combo() {
        let mut state = ComboState {
            combo: 20,
            miss_combo: 4,
            ..ComboState::default()
        };

        apply_row_combo_state(&mut state, JudgeGrade::Miss, 2, 1);

        assert_eq!(state.combo, 0);
        assert_eq!(state.miss_combo, 5);
    }

    #[test]
    fn combo_milestone_crossing() {
        for (combo, added, hundred, thousand) in [
            (0, 0, false, false),
            (98, 1, false, false),
            (99, 1, true, false),
            (99, 2, true, false),
            (100, 0, false, false),
            (100, 1, false, false),
            (199, 4, true, false),
            (699, 2, true, false),
            (999, 1, true, true),
            (999, 2, true, true),
            (1000, 0, false, false),
            (1000, 1, false, false),
            (1999, 4, true, true),
            (99, 2002, true, true),
            (u32::MAX - 1, 2, false, false),
            (u32::MAX, 1, false, false),
        ] {
            let mut state = ComboState {
                combo,
                ..ComboState::default()
            };

            let update = apply_row_combo_state(&mut state, JudgeGrade::Fantastic, added, 0);

            assert_eq!(state.combo, combo.saturating_add(added));
            assert_eq!(
                update,
                ComboUpdate {
                    combo_broken: false,
                    hit_hundred_milestone: hundred,
                    hit_thousand_milestone: thousand,
                },
                "combo={combo}, added={added}"
            );
        }
    }
}

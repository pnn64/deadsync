use deadsync_gameplay::{GameplayInputPlayStyle as Style, player_index_for_column};

#[test]
fn couple_assigns_each_authored_note_to_exactly_one_participant() {
    for style in [Style::Couple, Style::PumpCouple] {
        let width = style.cols_per_player();
        for column in 0..style.total_cols() {
            let owner = column / width;
            assert_eq!(style.runtime_column(column, owner), Some(column));
            assert_eq!(style.runtime_column(column, 1 - owner), None);
            assert_eq!(player_index_for_column(2, width, column), owner);
        }
        assert_eq!(style.runtime_column(style.total_cols(), 1), None);
    }
}

#[test]
fn routine_uses_one_shared_score_and_accepts_the_entire_two_pad_field() {
    for style in [Style::Routine, Style::PumpRoutine] {
        assert_eq!(style.participant_count(), 2);
        assert_eq!(style.player_count(), 1);
        for column in 0..style.total_cols() {
            assert_eq!(style.runtime_column(column, 0), Some(column));
            assert_eq!(style.runtime_column(column, 1), None);
        }
        assert_eq!(style.runtime_column(style.total_cols(), 0), None);
    }
}

#[test]
fn versus_keeps_separate_chart_lane_offsets() {
    for style in [Style::Versus, Style::PumpVersus] {
        let width = style.cols_per_player();
        for column in 0..width {
            assert_eq!(style.runtime_column(column, 0), Some(column));
            assert_eq!(style.runtime_column(column, 1), Some(column + width));
        }
    }
}

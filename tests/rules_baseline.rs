use deadsync::game::judgment::{self, JudgeGrade, TimingWindow};
use deadsync::game::note::NoteType;
use deadsync::game::parsing::notes::{ParsedNote, parse_chart_notes};
use deadsync::game::timing::{
    ROWS_PER_BEAT, TimingProfile, TimingProfileNs, beat_to_note_row,
    classify_offset_ns_with_disabled_windows, note_row_to_beat,
};

#[test]
fn timing_window_edges_stay_stable() {
    let profile = TimingProfile::default_itg_with_fa_plus();
    let profile_ns = TimingProfileNs::from_profile_scaled(&profile, 1.0);
    let disabled = [false; 5];
    let w0 = profile_ns
        .fa_plus_window_ns
        .expect("default profile has FA+ W0");

    assert_eq!(
        classify_offset_ns_with_disabled_windows(w0, &profile_ns, &disabled),
        Some((JudgeGrade::Fantastic, TimingWindow::W0))
    );
    assert_eq!(
        classify_offset_ns_with_disabled_windows(w0 + 1, &profile_ns, &disabled),
        Some((JudgeGrade::Fantastic, TimingWindow::W1))
    );
    assert_eq!(
        classify_offset_ns_with_disabled_windows(
            profile_ns.windows_ns[4] + 1,
            &profile_ns,
            &disabled
        ),
        None
    );
}

#[test]
fn disabled_top_windows_demote_exact_hits() {
    let profile = TimingProfile::default_itg_with_fa_plus();
    let profile_ns = TimingProfileNs::from_profile_scaled(&profile, 1.0);
    let disabled = [true, true, false, false, false];

    assert_eq!(
        classify_offset_ns_with_disabled_windows(0, &profile_ns, &disabled),
        Some((JudgeGrade::Great, TimingWindow::W3))
    );
}

#[test]
fn itg_percent_keeps_truncation_semantics() {
    assert_eq!(
        judgment::calculate_itg_score_percent_from_points(-1, 100),
        0.0
    );
    assert_eq!(
        judgment::calculate_itg_score_percent_from_points(100, 100),
        1.0
    );

    let boundary = judgment::calculate_itg_score_percent_from_points(848_199, 1_000_000);
    assert!((boundary - 0.8482).abs() <= f64::EPSILON);
}

#[test]
fn minimized_chart_notes_drop_invalid_holds() {
    let notes = parse_chart_notes(b"2000\n1000\n3000\nL000\n", 4);

    assert_eq!(
        notes,
        vec![
            ParsedNote {
                row_index: 1,
                column: 0,
                note_type: NoteType::Tap,
                tail_row_index: None,
            },
            ParsedNote {
                row_index: 3,
                column: 0,
                note_type: NoteType::Lift,
                tail_row_index: None,
            },
        ]
    );
}

#[test]
fn note_row_beat_conversions_use_itg_rows_per_beat() {
    assert_eq!(beat_to_note_row(1.0), ROWS_PER_BEAT);
    assert_eq!(note_row_to_beat(ROWS_PER_BEAT * 2), 2.0);
}

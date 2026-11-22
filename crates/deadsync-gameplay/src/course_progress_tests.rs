use super::{CourseProgress, CourseRenewal};

#[test]
fn finite_progress_selects_only_prepared_stages_and_finishes_at_the_boundary() {
    for (cursor, count, selected, finished) in [
        (0, 0, None, true),
        (0, 1, Some(0), false),
        (1, 1, None, true),
        (2, 1, None, true),
        (0, 3, Some(0), false),
        (2, 3, Some(2), false),
        (3, 3, None, true),
        (4, 3, None, true),
        (usize::MAX - 1, usize::MAX, Some(usize::MAX - 1), false),
        (usize::MAX, usize::MAX, None, true),
    ] {
        let progress = CourseProgress::new(cursor);
        assert_eq!(progress.stage_index(count, false), selected);
        assert_eq!(progress.has_next_stage(count, false), selected.is_some());
        assert_eq!(progress.is_finished(count, false, false), finished);
        assert_eq!(progress.next_stage_index(), cursor);
        assert!(!progress.is_interrupted());
    }
}

#[test]
fn endless_progress_wraps_nonempty_stages_without_finishing() {
    for (cursor, count, selected) in [
        (0, 3, 0),
        (2, 3, 2),
        (3, 3, 0),
        (4, 3, 1),
        (usize::MAX, 2, 1),
        (usize::MAX, usize::MAX, 0),
    ] {
        let progress = CourseProgress::new(cursor);
        assert!(progress.has_next_stage(count, true));
        assert_eq!(progress.stage_index(count, true), Some(selected));
        assert!(!progress.is_finished(count, true, false));
        assert_eq!(progress.next_stage_index(), cursor);
    }
}

#[test]
fn empty_endless_has_no_available_stage_but_is_not_finished() {
    for cursor in [0, 1, usize::MAX] {
        let progress = CourseProgress::new(cursor);
        assert!(!progress.has_next_stage(0, true));
        assert_eq!(progress.stage_index(0, true), None);
        assert!(!progress.is_finished(0, true, false));
        assert!(progress.is_finished(0, true, true));
    }
}

#[test]
fn player_failure_finishes_either_mode_without_mutating_stage_availability() {
    for endless in [false, true] {
        let progress = CourseProgress::new(1);
        assert!(!progress.is_finished(3, endless, false));
        assert!(progress.is_finished(3, endless, true));
        assert_eq!(progress.stage_index(3, endless), Some(1));
        assert!(progress.has_next_stage(3, endless));
        assert_eq!(progress, CourseProgress::new(1));
    }
}

#[test]
fn interruption_stops_stage_selection_and_finishes_without_player_failure() {
    for endless in [false, true] {
        for count in [0, 3] {
            for cursor in [0, 3, usize::MAX] {
                let mut progress = CourseProgress::new(cursor);
                progress.interrupt();
                assert!(progress.is_interrupted());
                assert!(!progress.has_next_stage(count, endless));
                assert_eq!(progress.stage_index(count, endless), None);
                assert!(progress.is_finished(count, endless, false));
                assert!(!progress.can_append_endless_cycle(endless));
                assert_eq!(progress.next_stage_index(), cursor);
                let interrupted = progress;
                progress.interrupt();
                assert_eq!(progress, interrupted);
            }
        }
    }
}

#[test]
fn renewal_distinguishes_prepared_boundaries_from_modulo_stage_availability() {
    for (cursor, count, renewal) in [
        (0, 3, CourseRenewal::NotRequired),
        (2, 3, CourseRenewal::NotRequired),
        (3, 3, CourseRenewal::AppendCycle),
        (4, 3, CourseRenewal::InvalidCursor),
        (usize::MAX, 3, CourseRenewal::InvalidCursor),
        (0, 0, CourseRenewal::AppendCycle),
        (1, 0, CourseRenewal::InvalidCursor),
        (usize::MAX, usize::MAX, CourseRenewal::AppendCycle),
    ] {
        for interrupted in [false, true] {
            let mut progress = CourseProgress::new(cursor);
            if interrupted {
                progress.interrupt();
            }
            assert_eq!(progress.renewal(count, true), renewal);
            assert_eq!(progress.renewal(count, false), CourseRenewal::NotRequired);
        }
    }

    // A modulo lookup remains valid for callers inspecting an Endless run,
    // while entry must reject skipped, unprepared history.
    let skipped = CourseProgress::new(4);
    assert_eq!(skipped.stage_index(3, true), Some(1));
    assert!(skipped.has_next_stage(3, true));
    assert_eq!(skipped.renewal(3, true), CourseRenewal::InvalidCursor);
}

#[test]
fn append_authorization_uses_mode_and_interruption_independently_of_cursor() {
    for cursor in [0, 1, usize::MAX] {
        for (endless, interrupted, allowed) in [
            (false, false, false),
            (true, false, true),
            (false, true, false),
            (true, true, false),
        ] {
            let mut progress = CourseProgress::new(cursor);
            if interrupted {
                progress.interrupt();
            }
            assert_eq!(progress.can_append_endless_cycle(endless), allowed);
        }
    }
}

#[test]
fn loading_interruption_requires_recorded_stages_and_preserves_existing_state() {
    for (was_interrupted, completed, applied, is_interrupted) in [
        (false, 0, false, false),
        (true, 0, false, true),
        (false, 1, true, true),
        (true, 1, true, true),
        (false, usize::MAX, true, true),
        (true, usize::MAX, true, true),
    ] {
        let mut progress = CourseProgress::new(2);
        if was_interrupted {
            progress.interrupt();
        }
        assert_eq!(progress.interrupt_loading(completed), applied);
        assert_eq!(progress.is_interrupted(), is_interrupted);
        assert_eq!(progress.next_stage_index(), 2);
    }
}

#[test]
fn successful_entry_advances_with_saturation_while_reload_never_advances() {
    let mut progress = CourseProgress::default();
    assert_eq!(progress, CourseProgress::new(0));
    progress.finish_entry(false);
    assert_eq!(progress.next_stage_index(), 1);
    progress.finish_entry(true);
    assert_eq!(progress.next_stage_index(), 1);
    progress.finish_entry(false);
    assert_eq!(progress.next_stage_index(), 2);

    for interrupted in [false, true] {
        let mut progress = CourseProgress::new(usize::MAX - 1);
        if interrupted {
            progress.interrupt();
        }
        progress.finish_entry(false);
        assert_eq!(progress.next_stage_index(), usize::MAX);
        progress.finish_entry(false);
        assert_eq!(progress.next_stage_index(), usize::MAX);
        progress.finish_entry(true);
        assert_eq!(progress.next_stage_index(), usize::MAX);
        assert_eq!(progress.is_interrupted(), interrupted);
    }
}

#[test]
fn reload_entry_views_rewind_without_mutating_live_progress_or_interruption() {
    for (cursor, reload_cursor, normal_lead_in, reload_lead_in) in [
        (0, 0, false, false),
        (1, 0, true, false),
        (4, 3, true, true),
        (usize::MAX, usize::MAX - 1, true, true),
    ] {
        for interrupted in [false, true] {
            let mut progress = CourseProgress::new(cursor);
            if interrupted {
                progress.interrupt();
            }
            let original = progress;
            let normal = progress.for_entry(false);
            let mut reload = progress.for_entry(true);
            assert_eq!(normal, original);
            assert_eq!(reload.next_stage_index(), reload_cursor);
            assert_eq!(reload.is_interrupted(), interrupted);
            assert_eq!(normal.uses_next_stage_lead_in(), normal_lead_in);
            assert_eq!(reload.uses_next_stage_lead_in(), reload_lead_in);
            assert_eq!(progress, original);

            reload.finish_entry(false);
            assert_eq!(
                progress, original,
                "an entry view must be an independent value"
            );
        }
    }
}

#[test]
fn queries_observe_current_catalog_length_and_mode_on_the_same_progress() {
    let progress = CourseProgress::new(3);
    assert_eq!(progress.stage_index(3, true), Some(0));
    assert_eq!(progress.renewal(3, true), CourseRenewal::AppendCycle);
    assert!(!progress.is_finished(3, true, false));

    // Renewal extends the prepared catalog without resetting its live cursor.
    assert_eq!(progress.stage_index(5, true), Some(3));
    assert_eq!(progress.renewal(5, true), CourseRenewal::NotRequired);
    assert!(progress.has_next_stage(5, true));

    // A changed course mode must immediately affect availability and completion.
    assert_eq!(progress.stage_index(3, false), None);
    assert!(!progress.has_next_stage(3, false));
    assert!(progress.is_finished(3, false, false));
    assert_eq!(progress.renewal(3, false), CourseRenewal::NotRequired);
    assert_eq!(progress.stage_index(5, false), Some(3));
    assert!(!progress.is_finished(5, false, false));
    assert_eq!(progress, CourseProgress::new(3));
}

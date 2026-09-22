#[test]
fn zero_tare_stops_a_preexisting_direction_hold() {
    for code in [KeyCode::Digit0, KeyCode::Numpad0] {
        for advanced in [false, true] {
            for action in [VirtualAction::p1_up, VirtualAction::p2_down] {
                let mut s = with_pad();
                set_fsr_enabled(&mut s, true);
                if advanced {
                    apply_edit(&mut s, &ev(VirtualAction::p1_start), false);
                }
                s.pads[0].buttons[0].sensors[0].raw_value = 66;
                apply_edit(&mut s, &ev(action), false);
                assert!(handle_raw_key_event(&mut s, &tare_key(code)));
                assert_eq!(pending_threshold_value(&s), Some(66));
                take_commands(&mut s);
                update(&mut s, 0.301);
                assert!(take_commands(&mut s).is_empty());
                apply_edit(&mut s, &ev_release(action), false);
                update(&mut s, 1.0);
                assert!(take_commands(&mut s).is_empty());
                // A fresh press starts stepping again after the late release.
                apply_edit(&mut s, &ev(action), false);
                assert!(!take_commands(&mut s).is_empty());
            }
        }
    }
}

#[test]
fn horizontal_navigation_does_not_retrigger_the_tare_chord() {
    let mut s = with_pad();
    for sv in &mut s.pads[0].buttons[0].sensors {
        sv.raw_value = 100;
    }
    apply_edit(&mut s, &ev(VirtualAction::p1_up), false);
    apply_edit(&mut s, &ev(VirtualAction::p1_down), false);
    take_commands(&mut s);
    for sv in &mut s.pads[0].buttons[0].sensors {
        sv.raw_threshold = 100;
        sv.raw_value = 110;
    }
    apply_edit(&mut s, &ev(VirtualAction::p1_right), false);
    assert!(
        s.pending.is_empty(),
        "cursor movement unexpectedly recalibrated the panel: {:?}",
        s.pending
    );
    assert_eq!(s.selected, 1);
    update(&mut s, 0.301);
    assert!(s.pending.is_empty());
    assert_eq!(s.selected, 2);
}

#[test]
fn load_cell_tare_chord_matches_zero_regardless_of_key_order() {
    let mut zero = init();
    set_pads(&mut zero, vec![load_cell_pad(0)]);
    zero.pads[0].buttons[0].aggregate_value = 50;
    assert!(tare_focused(&mut zero));
    let expected = take_commands(&mut zero);
    assert!(matches!(
        expected.as_slice(),
        [PadCommand::ThresholdPair {
            press: 80,
            release: 50,
            ..
        }]
    ));

    for order in [
        [VirtualAction::p1_up, VirtualAction::p1_down],
        [VirtualAction::p1_down, VirtualAction::p1_up],
    ] {
        let mut s = init();
        set_pads(&mut s, vec![load_cell_pad(0)]);
        s.pads[0].buttons[0].aggregate_value = 50;
        for action in order {
            apply_edit(&mut s, &ev(action), false);
        }
        assert_eq!(
            take_commands(&mut s),
            expected,
            "tare chord should match the zero shortcut"
        );
    }
}

fn tare_key(code: KeyCode) -> RawKeyboardEvent {
    RawKeyboardEvent {
        code,
        pressed: true,
        repeat: false,
        timestamp: Instant::now(),
        host_nanos: 0,
    }
}

/// Model a shell frame that drains the edit and refreshes the hardware view.
fn flush_load_cell_edits(s: &mut State) {
    for command in take_commands(s) {
        let PadCommand::ThresholdPair {
            button,
            press,
            release,
            ..
        } = command
        else {
            panic!("unexpected {command:?}");
        };
        s.pads[0].buttons[button].aggregate_threshold = press;
        s.pads[0].buttons[button].release_threshold = Some(release);
    }
    let pads = take_pads(s);
    set_pads(s, pads);
}

fn load_cell_pair(s: &State) -> (u16, u16) {
    pending_threshold_pair(s, s.pads[0].device_id, 0).unwrap_or((
        s.pads[0].buttons[0].aggregate_threshold,
        s.pads[0].buttons[0].release_threshold.unwrap(),
    ))
}

#[test]
fn load_cell_tare_preserves_the_partner_across_frames_and_at_bounds() {
    for selected in [0, 1] {
        for lock_off in [false, true] {
            for reading in [0, 50, 70, 79, 80, 100, u16::MAX] {
                let fixture = || {
                    let mut s = init();
                    set_pads(&mut s, vec![load_cell_pad(0)]);
                    set_fsr_enabled(&mut s, true);
                    s.selected = selected;
                    s.threshold_lock_off = lock_off;
                    s.pads[0].buttons[0].aggregate_value = reading;
                    if lock_off {
                        s.pads[0].buttons[0].release_threshold = Some(79);
                    }
                    s
                };
                let mut zero = fixture();
                assert!(handle_raw_key_event(&mut zero, &tare_key(KeyCode::Digit0)));
                let expected = load_cell_pair(&zero);
                for order in [
                    [VirtualAction::p1_up, VirtualAction::p1_down],
                    [VirtualAction::p2_down, VirtualAction::p2_up],
                ] {
                    for flush in [false, true] {
                        let mut s = fixture();
                        apply_edit(&mut s, &ev(order[0]), false);
                        if flush {
                            flush_load_cell_edits(&mut s);
                        }
                        apply_edit(&mut s, &ev(order[1]), false);
                        assert_eq!(
                            load_cell_pair(&s),
                            expected,
                            "selected={selected}, lock_off={lock_off}, reading={reading}, flush={flush}"
                        );
                        flush_load_cell_edits(&mut s);
                        update(&mut s, 1.0);
                        apply_edit(&mut s, &ev_release(order[0]), false);
                        update(&mut s, 1.0);
                        apply_edit(&mut s, &ev_release(order[1]), false);
                        assert!(take_commands(&mut s).is_empty());
                    }
                }
            }
        }
    }
}

#[test]
fn duplicate_chord_presses_do_not_recalibrate() {
    let mut s = with_pad();
    apply_edit(&mut s, &ev(VirtualAction::p1_up), false);
    apply_edit(&mut s, &ev(VirtualAction::p1_down), false);
    take_commands(&mut s);
    s.pads[0].buttons[0].sensors[0].raw_value = 110;
    apply_edit(&mut s, &ev(VirtualAction::p1_up), false);
    apply_edit(&mut s, &ev(VirtualAction::p1_down), false);
    update(&mut s, 1.0);
    assert!(take_commands(&mut s).is_empty());
}

#[test]
fn load_cell_tare_keeps_completed_edits_and_uses_the_current_cursor() {
    let mut s = init();
    set_pads(&mut s, vec![load_cell_pad(0)]);
    set_fsr_enabled(&mut s, true);
    s.pads[0].buttons[0].aggregate_value = 50;
    apply_edit(&mut s, &ev(VirtualAction::p1_up), false);
    apply_edit(&mut s, &ev_release(VirtualAction::p1_up), false);
    flush_load_cell_edits(&mut s);
    assert!(handle_raw_key_event(&mut s, &tare_key(KeyCode::Digit0)));
    assert_eq!(load_cell_pair(&s), (85, 50));

    flush_load_cell_edits(&mut s);
    apply_edit(&mut s, &ev(VirtualAction::p1_up), false);
    // Moving to the press side ends the previous release-side gesture.
    apply_edit(&mut s, &ev(VirtualAction::p1_right), false);
    apply_edit(&mut s, &ev(VirtualAction::p1_down), false);
    assert_eq!(load_cell_pair(&s), (50, 40));
}

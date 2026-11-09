//! Host-calibrated pad controls and independently editable sensor release lines.
use super::*;

pub(super) fn sensor_pair(
    state: &State,
    device: PadDeviceId,
    button: usize,
    sensor: usize,
) -> Option<(u16, u16)> {
    if let Some(pair) = state.pending.iter().rev().find_map(|c| match *c {
        PadCommand::SensorThresholdPair {
            device: d,
            button: b,
            sensor: s,
            press,
            release,
        } if d == device && b == button && s == sensor => Some((press, release)),
        _ => None,
    }) {
        return Some(pair);
    }
    let s = pad_by_device(state, device)?
        .buttons
        .get(button)?
        .sensors
        .iter()
        .find(|s| s.firmware_index == sensor)?;
    Some((s.raw_threshold, s.release_threshold?))
}
pub(super) fn adjust_release(
    state: &mut State,
    device: PadDeviceId,
    button: usize,
    display: usize,
    delta: i32,
) {
    let Some(pad) = pad_by_device(state, device) else {
        return;
    };
    if !pad.thresholds_editable {
        return;
    }
    let Some(s) = pad.buttons.get(button).and_then(|b| b.sensors.get(display)) else {
        return;
    };
    let sensor = s.firmware_index;
    let Some((press, release)) = sensor_pair(state, device, button, sensor) else {
        return;
    };
    let next = (i32::from(release) + delta).clamp(0, i32::from(press.saturating_sub(1))) as u16;
    if next != release {
        queue_unique(
            state,
            PadCommand::SensorThresholdPair {
                device,
                button,
                sensor,
                press,
                release: next,
            },
        );
    }
}

pub(super) fn handle_key(state: &mut State, key: &RawKeyboardEvent) -> bool {
    if !state.fsr_enabled || state.profiles_mode || state.saving.is_some() {
        state.calibration_armed = None;
        return false;
    }
    let Some(device) = state.advanced else {
        state.calibration_armed = None;
        return false;
    };
    if pad_by_device(state, device).is_none_or(|p| p.host_controls.is_none()) {
        state.calibration_armed = None;
        return false;
    }
    let action = match key.code {
        KeyCode::F5 => HostPadAction::Calibrate,
        KeyCode::F6 => HostPadAction::SwitchPlayer,
        KeyCode::F7 => HostPadAction::CycleLights,
        KeyCode::F8 => HostPadAction::TestLights,
        KeyCode::Minus | KeyCode::NumpadSubtract => HostPadAction::Brightness(-5),
        KeyCode::Equal | KeyCode::NumpadAdd => HostPadAction::Brightness(5),
        _ => {
            if key.pressed {
                state.calibration_armed = None;
            }
            return false;
        }
    };
    if !key.pressed || key.repeat {
        return true;
    }
    if action == HostPadAction::Calibrate && state.calibration_armed != Some(device) {
        state.calibration_armed = Some(device);
        clear_hold(state);
        return true;
    }
    state.calibration_armed = None;
    queue_unique(state, PadCommand::HostControl { device, action });
    true
}

#[allow(clippy::too_many_arguments)]
pub(super) fn push_release_line(
    actors: &mut Vec<Actor>,
    state: &State,
    device: PadDeviceId,
    button: usize,
    sensor: &SensorView,
    x: f32,
    y: f32,
    curve: ValueCurve,
    selected: bool,
    zb: f32,
) {
    let Some((_, release)) = sensor_pair(state, device, button, sensor.firmware_index) else {
        return;
    };
    let y = y + (1.0 - curve.normalize(release)) * ADV_BAR_HEIGHT;
    let color = if selected {
        SELECTED_TEXT
    } else {
        [0.4, 0.75, 1.0, 1.0]
    };
    push_quad(actors, x, y, ADV_BAR_W, 2.0, color, 27.0 + zb);
}

pub(super) fn push_controls(actors: &mut Vec<Actor>, state: &State, zb: f32) {
    if state.profiles_mode || state.saving.is_some() {
        return;
    }
    let Some(device) = state.advanced else { return };
    let Some(pad) = pad_by_device(state, device) else {
        return;
    };
    let Some(host) = pad.host_controls else {
        return;
    };
    let targets = advanced_targets(state);
    if let Some(
        AdvTarget::Sensor { button, sensor } | AdvTarget::SensorRelease { button, sensor },
    ) = targets.get(state.adv_sel)
    {
        let s = &pad.buttons[*button].sensors[*sensor];
        let (press, release) = sensor_pair(state, device, *button, s.firmware_index).unwrap();
        let editing = if matches!(targets[state.adv_sel], AdvTarget::SensorRelease { .. }) {
            "Release"
        } else {
            "Press"
        };
        let baseline = s
            .baseline
            .map_or_else(|| "uncalibrated".to_owned(), |v| v.to_string());
        let text = format!(
            "Sensor {}  |  Press {press}  Release {release}  |  Baseline {baseline}  |  Editing {editing}",
            s.firmware_index + 1
        );
        actors.push(act!(text: font("miso"): settext(text): align(0.5,0.5): xy(screen_center_x(),screen_height()-177.0): zoom(0.52): z(29.0+zb)));
    }
    let settings = format!(
        "P{}  |  Lights: {}  |  Brightness: {}%",
        host.player + 1,
        host.lights,
        host.brightness
    );
    actors.push(act!(text: font("miso"): settext(settings): align(0.5,0.5): xy(screen_center_x(),screen_height()-156.0): zoom(0.55): z(29.0+zb)));
    let hint = if state.calibration_armed == Some(device) {
        "STEP OFF ALL PANELS. Press F5 again to calibrate; another key cancels."
    } else {
        "F5: calibrate  |  F6: swap players  |  F7: light mode  |  -/+: brightness  |  F8: test lights"
    };
    actors.push(act!(text: font("miso"): settext(hint): align(0.5,0.5): xy(screen_center_x(),screen_height()-138.0): zoom(0.48): diffuse(1.0,0.75,0.4,1.0): z(29.0+zb)));
}

#[cfg(test)]
mod tests {
    use super::super::tests::{fsrio_pad, mk_sensor};
    use super::*;
    use std::time::Instant;
    fn state() -> State {
        let mut state = init();
        set_fsr_enabled(&mut state, true);
        let mut pad = fsrio_pad(&[1]);
        pad.device_id.backend = BackendKind::Reflex;
        pad.host_controls = Some(deadsync_input::fsr::HostPadControls {
            player: 0,
            brightness: 40,
            lights: "Input",
        });
        let mut sensor = mk_sensor(0, 30);
        sensor.release_threshold = Some(25);
        sensor.baseline = Some(1000);
        pad.buttons[0].sensors = [sensor].into_iter().collect();
        pad.buttons[0].min_raw_threshold = 1;
        set_pads(&mut state, vec![pad]);
        enter_advanced(&mut state);
        state
    }
    #[test]
    fn calibration_is_explicit_and_requires_confirmation() {
        let mut s = state();
        let key = |code| RawKeyboardEvent {
            code,
            pressed: true,
            repeat: false,
            timestamp: Instant::now(),
            host_nanos: 0,
        };
        assert!(handle_key(&mut s, &key(KeyCode::F5)));
        assert!(take_commands(&mut s).is_empty());
        assert!(!handle_key(&mut s, &key(KeyCode::Escape)));
        assert!(handle_key(&mut s, &key(KeyCode::F5)));
        assert!(take_commands(&mut s).is_empty());
        assert!(handle_key(&mut s, &key(KeyCode::F5)));
        assert!(matches!(
            take_commands(&mut s).as_slice(),
            [PadCommand::HostControl {
                action: HostPadAction::Calibrate,
                ..
            }]
        ));
    }
    #[test]
    fn press_adjustment_preserves_gap_and_release_edits_remain_independent() {
        let mut s = state();
        let d = s.advanced.unwrap();
        assert_eq!(advanced_targets(&s).len(), 2);
        adjust_sensor_threshold(&mut s, d, 0, 0, 5);
        assert_eq!(sensor_pair(&s, d, 0, 0), Some((35, 30)));
        adjust_release(&mut s, d, 0, 0, -5);
        assert_eq!(sensor_pair(&s, d, 0, 0), Some((35, 25)));
        adjust_release(&mut s, d, 0, 0, 100);
        assert_eq!(sensor_pair(&s, d, 0, 0), Some((35, 34)));
        assert_eq!(take_commands(&mut s).len(), 1);
    }

    #[test]
    fn release_selection_uses_fine_steps_and_tares_without_moving_press() {
        let mut s = state();
        let d = s.advanced.unwrap();
        s.adv_sel = 1;
        let target = advanced_targets(&s)[s.adv_sel];
        edit_focused(&mut s, d, target, false, false);
        assert_eq!(sensor_pair(&s, d, 0, 0), Some((30, 20)));
        edit_focused(&mut s, d, target, true, true);
        assert_eq!(sensor_pair(&s, d, 0, 0), Some((30, 21)));
        s.pads[0].buttons[0].sensors[0].raw_value = 12;
        assert!(tare_focused(&mut s));
        assert_eq!(sensor_pair(&s, d, 0, 0), Some((30, 12)));
    }

    #[test]
    fn keyboard_mode_panel_letters_cannot_calibrate_or_change_assignment() {
        let mut s = state();
        for code in [KeyCode::KeyC, KeyCode::KeyP, KeyCode::KeyL, KeyCode::KeyT] {
            assert!(!handle_key(
                &mut s,
                &RawKeyboardEvent {
                    code,
                    pressed: true,
                    repeat: false,
                    timestamp: Instant::now(),
                    host_nanos: 0
                }
            ));
        }
        assert!(s.calibration_armed.is_none());
        assert!(take_commands(&mut s).is_empty());
    }
}

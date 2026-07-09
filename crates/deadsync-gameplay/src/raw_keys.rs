use deadsync_input::RawKeyboardEvent;
use winit::keyboard::KeyCode;

#[derive(Clone, Copy, Debug)]
pub struct GameplayRawKeyEvent {
    pub code: KeyCode,
    pub pressed: bool,
    pub timestamp: Instant,
}

#[derive(Clone, Copy, Debug)]
pub enum GameplayQueuedEvent {
    Input(deadsync_input::InputEvent),
    RawKey(GameplayRawKeyEvent),
}

#[inline(always)]
pub fn gameplay_raw_key_event(raw_key: &RawKeyboardEvent) -> Option<GameplayQueuedEvent> {
    if raw_key.repeat {
        return None;
    }
    match raw_key.code {
        KeyCode::ShiftLeft
        | KeyCode::ShiftRight
        | KeyCode::ControlLeft
        | KeyCode::ControlRight
        | KeyCode::KeyR
        | KeyCode::F6
        | KeyCode::F7
        | KeyCode::F8
        | KeyCode::F11
        | KeyCode::F12 => {}
        _ => return None,
    }
    Some(GameplayQueuedEvent::RawKey(GameplayRawKeyEvent {
        code: raw_key.code,
        pressed: raw_key.pressed,
        timestamp: raw_key.timestamp,
    }))
}

#[inline(always)]
pub const fn gameplay_raw_modifier_key(code: KeyCode) -> Option<GameplayRawModifierKey> {
    match code {
        KeyCode::ShiftLeft | KeyCode::ShiftRight => Some(GameplayRawModifierKey::Shift),
        KeyCode::ControlLeft | KeyCode::ControlRight => Some(GameplayRawModifierKey::Ctrl),
        _ => None,
    }
}

#[inline(always)]
pub const fn gameplay_raw_key_input(code: KeyCode) -> GameplayRawKeyInput {
    match code {
        KeyCode::KeyR => GameplayRawKeyInput::Restart,
        KeyCode::F6 => GameplayRawKeyInput::Autosync,
        KeyCode::F7 => GameplayRawKeyInput::TimingTick,
        KeyCode::F8 => GameplayRawKeyInput::Autoplay,
        KeyCode::F11 => GameplayRawKeyInput::OffsetAdjust(GameplayOffsetAdjustKey::Decrease),
        KeyCode::F12 => GameplayRawKeyInput::OffsetAdjust(GameplayOffsetAdjustKey::Increase),
        _ => GameplayRawKeyInput::Other,
    }
}

#[cfg(test)]
mod raw_key_tests {
    use super::*;

    #[test]
    fn gameplay_raw_key_event_ignores_repeats_and_unhandled_keys() {
        let mut event = RawKeyboardEvent {
            code: KeyCode::KeyA,
            pressed: true,
            repeat: false,
            timestamp: Instant::now(),
            host_nanos: 0,
        };

        assert!(gameplay_raw_key_event(&event).is_none());
        event.code = KeyCode::KeyR;
        event.repeat = true;
        assert!(gameplay_raw_key_event(&event).is_none());
    }

    #[test]
    fn gameplay_raw_key_event_accepts_gameplay_control_keys() {
        let event = RawKeyboardEvent {
            code: KeyCode::F12,
            pressed: true,
            repeat: false,
            timestamp: Instant::now(),
            host_nanos: 0,
        };

        match gameplay_raw_key_event(&event) {
            Some(GameplayQueuedEvent::RawKey(ev)) => {
                assert_eq!(ev.code, KeyCode::F12);
                assert!(ev.pressed);
            }
            _ => panic!("expected raw gameplay key event"),
        }
    }

    #[test]
    fn gameplay_raw_key_mapping_preserves_offset_keys() {
        assert_eq!(
            gameplay_raw_key_input(KeyCode::F11),
            GameplayRawKeyInput::OffsetAdjust(GameplayOffsetAdjustKey::Decrease)
        );
        assert_eq!(
            gameplay_raw_key_input(KeyCode::F12),
            GameplayRawKeyInput::OffsetAdjust(GameplayOffsetAdjustKey::Increase)
        );
        assert_eq!(
            gameplay_raw_modifier_key(KeyCode::ControlLeft),
            Some(GameplayRawModifierKey::Ctrl)
        );
    }
}

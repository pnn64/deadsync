//! Physical-key translation for the game-owned practice editor.
//!
//! Only fixed key roles and aliases are translated here. Modifier state, mode
//! precedence, repeats, selection, playback and gameplay effects remain in the
//! practice controller; consumers do not need platform event types.

use deadlib_platform::input::{KeyCode, RawKeyboardEvent};
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PracticeKey {
    Shift,
    Control,
    FastScroll,
    RateDecrease,
    RateIncrease,
    Autosync,
    TimingTick,
    Autoplay,
    OffsetDecrease,
    OffsetIncrease,
    Cancel,
    Accept,
    ScrollSpeedIncrease,
    ScrollSpeedDecrease,
    Play,
    AreaMarker,
    PageUp,
    PageDown,
    PreviousNote,
    NextNote,
    SongStart,
    SongEnd,
    Help,
    Other,
}

/// An unfiltered key edge. Releases and operating-system repeats reach the
/// controller unchanged, including keys that have no practice role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PracticeKeyEvent {
    pub key: PracticeKey,
    pub pressed: bool,
    pub repeat: bool,
    pub timestamp: Instant,
}

impl PracticeKeyEvent {
    #[inline]
    #[must_use]
    pub const fn from_raw(event: &RawKeyboardEvent) -> Self {
        let key = match event.code {
            KeyCode::ShiftLeft | KeyCode::ShiftRight => PracticeKey::Shift,
            KeyCode::ControlLeft | KeyCode::ControlRight => PracticeKey::Control,
            KeyCode::Tab => PracticeKey::FastScroll,
            KeyCode::BracketLeft => PracticeKey::RateDecrease,
            KeyCode::BracketRight => PracticeKey::RateIncrease,
            KeyCode::F6 => PracticeKey::Autosync,
            KeyCode::F7 => PracticeKey::TimingTick,
            KeyCode::F8 => PracticeKey::Autoplay,
            KeyCode::F11 => PracticeKey::OffsetDecrease,
            KeyCode::F12 => PracticeKey::OffsetIncrease,
            KeyCode::Escape => PracticeKey::Cancel,
            KeyCode::Enter => PracticeKey::Accept,
            KeyCode::ArrowUp => PracticeKey::ScrollSpeedIncrease,
            KeyCode::ArrowDown => PracticeKey::ScrollSpeedDecrease,
            KeyCode::KeyP => PracticeKey::Play,
            KeyCode::Space => PracticeKey::AreaMarker,
            KeyCode::Semicolon | KeyCode::PageUp => PracticeKey::PageUp,
            KeyCode::Quote | KeyCode::PageDown => PracticeKey::PageDown,
            KeyCode::Comma => PracticeKey::PreviousNote,
            KeyCode::Period => PracticeKey::NextNote,
            KeyCode::Home => PracticeKey::SongStart,
            KeyCode::End => PracticeKey::SongEnd,
            KeyCode::F1 => PracticeKey::Help,
            _ => PracticeKey::Other,
        };
        Self {
            key,
            pressed: event.pressed,
            repeat: event.repeat,
            timestamp: event.timestamp,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn practice_key_aliases_keep_every_edge_and_its_repeat_metadata() {
        let timestamp = Instant::now();
        for (first, alias, expected) in [
            (KeyCode::ShiftLeft, KeyCode::ShiftRight, PracticeKey::Shift),
            (
                KeyCode::ControlLeft,
                KeyCode::ControlRight,
                PracticeKey::Control,
            ),
            (KeyCode::Semicolon, KeyCode::PageUp, PracticeKey::PageUp),
            (KeyCode::Quote, KeyCode::PageDown, PracticeKey::PageDown),
        ] {
            for (pressed, repeat) in [(true, false), (true, true), (false, false)] {
                let first = PracticeKeyEvent::from_raw(&RawKeyboardEvent {
                    code: first,
                    pressed,
                    repeat,
                    timestamp,
                    host_nanos: 123,
                });
                let alias = PracticeKeyEvent::from_raw(&RawKeyboardEvent {
                    code: alias,
                    pressed,
                    repeat,
                    timestamp,
                    host_nanos: 456,
                });
                assert_eq!(first, alias);
                assert_eq!(first.key, expected);
                assert_eq!(first.pressed, pressed);
                assert_eq!(first.repeat, repeat);
                assert_eq!(first.timestamp, timestamp);
            }
        }
    }

    #[test]
    fn practice_unmapped_keys_remain_available_to_other_input_handlers() {
        let timestamp = Instant::now();
        for pressed in [true, false] {
            let event = PracticeKeyEvent::from_raw(&RawKeyboardEvent {
                code: KeyCode::KeyA,
                pressed,
                repeat: false,
                timestamp,
                host_nanos: 123,
            });
            assert_eq!(event.key, PracticeKey::Other);
            assert_eq!(event.pressed, pressed);
            assert_eq!(event.timestamp, timestamp);
        }
    }
}

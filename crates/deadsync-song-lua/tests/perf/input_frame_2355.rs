//! Frozen frame from 48719519a; only private enum-name access is adapted.
use crate::input::{SongLuaInputEvent, SongLuaInputType};
use mlua::program::Datum;
use smallvec::SmallVec;
fn kind_name(kind: SongLuaInputType) -> &'static str {
    match kind {
        SongLuaInputType::FirstPress => "InputEventType_FirstPress",
        SongLuaInputType::Repeat => "InputEventType_Repeat",
        SongLuaInputType::Release => "InputEventType_Release",
    }
}
enum Name {
    Static(&'static [u8]),
    Formatted(SmallVec<[u8; 64]>),
}

impl std::ops::Deref for Name {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            Self::Static(value) => value,
            Self::Formatted(value) => value,
        }
    }
}

pub(crate) struct Frame {
    controller: Option<Name>,
    player: Option<Name>,
}

impl Frame {
    pub fn new(input: &SongLuaInputEvent<'_>) -> Self {
        fn name(prefix: &[u8], common: [&'static [u8]; 2], index: Option<usize>) -> Option<Name> {
            index.map(|index| {
                if let Some(value) = common.get(index) {
                    return Name::Static(value);
                }
                let mut bytes = SmallVec::from_slice(prefix);
                let mut number = itoa::Buffer::new();
                bytes.extend_from_slice(number.format(index + 1).as_bytes());
                Name::Formatted(bytes)
            })
        }
        Self {
            controller: name(
                b"GameController_",
                [b"GameController_1", b"GameController_2"],
                input.controller,
            ),
            player: name(
                b"PlayerNumber_P",
                [b"PlayerNumber_P1", b"PlayerNumber_P2"],
                input.player,
            ),
        }
    }

    pub fn values<'a>(&'a self, input: &'a SongLuaInputEvent<'_>) -> [Datum<'a>; 13] {
        [
            Datum::Bytes(input.button.as_bytes()),
            Datum::Bytes(input.game_button.as_bytes()),
            Datum::Bytes(kind_name(input.kind).as_bytes()),
            self.controller.as_deref().map_or(Datum::Nil, Datum::Bytes),
            self.player.as_deref().map_or(Datum::Nil, Datum::Bytes),
            Datum::Bytes(input.device.as_bytes()),
            Datum::Bytes(input.device_button.as_bytes()),
            Datum::Number(f64::from(input.level)),
            Datum::Number(f64::from(input.z)),
            Datum::Boolean(input.down),
            Datum::Number(input.ago),
            Datum::Boolean(input.is_joystick),
            Datum::Boolean(input.is_mouse),
        ]
    }
}

//! Frozen frame construction from 94dd65ca1; only private enum-name access is adapted.
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
pub(crate) struct Frame {
    controller: Option<SmallVec<[u8; 64]>>,
    player: Option<SmallVec<[u8; 64]>>,
}

impl Frame {
    pub fn new(input: &SongLuaInputEvent<'_>) -> Self {
        fn name(prefix: &[u8], index: Option<usize>) -> Option<SmallVec<[u8; 64]>> {
            index.map(|index| {
                let mut bytes = SmallVec::from_slice(prefix);
                let mut number = itoa::Buffer::new();
                bytes.extend_from_slice(number.format(index + 1).as_bytes());
                bytes
            })
        }
        Self {
            controller: name(b"GameController_", input.controller),
            player: name(b"PlayerNumber_P", input.player),
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

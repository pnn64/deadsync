//! Closed input-only callbacks. Actor actions, captures and event mutation stay
//! in the ordinary Lua dispatcher until their ownership contracts are proven.
use super::SongLuaInputEvent;
use mlua::Function;
use mlua::program::{self, Binding, Bindings, Datum, Field, FieldValue, Machine, Types};
use smallvec::SmallVec;

/// A compiled screen input predicate. Its result controls whether later screen
/// callbacks run; normal gameplay input is independent of this result.
///
/// Only input reads and primitive expressions are accepted. Captures, globals,
/// calls, event writes and unsupported bytecode reject compilation without
/// executing the body. The result is Lua truthiness of the first return value.
/// This artifact owns no Lua handle, and evaluation needs no VM. The live screen
/// registry separately roots original functions for fallback and Lua identity.
pub struct SongLuaCompiledInput(Machine);

impl SongLuaCompiledInput {
    /// Try to bind a Lua 5.1 callback to the native input schema. Unsupported
    /// behavior returns `None` so the screen can retain its ordinary Lua path.
    pub fn compile(callback: &Function) -> Option<Self> {
        let bindings = bindings();
        // No upvalue/environment/action bindings: their reads/writes cannot be
        // mistaken for immutable inputs. Every input object is fresh and has no
        // metatable; these callbacks cannot mutate or leak it to another body.
        Some(Self(program::compile(callback, &bindings).ok()?.machine()))
    }

    /// Process actual runtime input without Lua or allocation.
    pub fn evaluate(&mut self, input: &SongLuaInputEvent<'_>) -> bool {
        let frame = Frame::new(input);
        self.evaluate_frame(&frame.values(input))
    }

    /// Native owned payload, excluding allocator/Arc bookkeeping.
    pub fn retained_bytes(&self) -> usize {
        self.0.retained_bytes()
    }

    pub(super) fn evaluate_frame(&mut self, values: &[Datum<'_>]) -> bool {
        self.0
            .run(values, |_, _| {
                unreachable!("input-only schema has no actions")
            })
            .expect("complete typed input frame")
            .is_truthy()
    }
}

pub(crate) fn bindings() -> Bindings {
    let mut fields = Vec::with_capacity(15);
    for (object, names) in [
        (
            0,
            &[
                ("button", 0, Types::BYTES),
                ("GameButton", 1, Types::BYTES),
                ("type", 2, Types::BYTES),
                ("controller", 3, Types::NIL | Types::BYTES),
                ("PlayerNumber", 4, Types::NIL | Types::BYTES),
            ][..],
        ),
        (
            1,
            &[
                ("device", 5, Types::BYTES),
                ("button", 6, Types::BYTES),
                ("level", 7, Types::NUMBER),
                ("z", 8, Types::NUMBER),
                ("down", 9, Types::BOOLEAN),
                ("ago", 10, Types::NUMBER),
                ("is_joystick", 11, Types::BOOLEAN),
                ("is_mouse", 12, Types::BOOLEAN),
            ][..],
        ),
    ] {
        fields.extend(names.iter().map(|&(name, slot, types)| Field {
            object,
            name: name.as_bytes().into(),
            value: FieldValue::Value(Binding::Input { slot, types }),
        }));
    }
    fields.extend([
        Field {
            object: 0,
            name: b"DeviceInput".as_slice().into(),
            value: FieldValue::Value(Binding::Object(1)),
        },
        Field {
            object: 0,
            name: b"MultiPlayer".as_slice().into(),
            value: FieldValue::Value(Binding::Literal(program::Literal::Nil)),
        },
    ]);
    Bindings {
        arguments: vec![Binding::Object(0)],
        fields,
        ..Bindings::default()
    }
}

pub(crate) struct Frame {
    controller: SmallVec<[u8; 64]>,
    player: SmallVec<[u8; 64]>,
}

impl Frame {
    pub fn new(input: &SongLuaInputEvent<'_>) -> Self {
        fn name(prefix: &[u8], index: Option<usize>) -> SmallVec<[u8; 64]> {
            if let Some(index) = index.filter(|&index| index >= 2) {
                let mut bytes = SmallVec::from_slice(prefix);
                let mut number = itoa::Buffer::new();
                bytes.extend_from_slice(number.format(index + 1).as_bytes());
                bytes
            } else {
                SmallVec::new()
            }
        }
        Self {
            controller: name(b"GameController_", input.controller),
            player: name(b"PlayerNumber_P", input.player),
        }
    }

    pub fn values<'a>(&'a self, input: &'a SongLuaInputEvent<'_>) -> [Datum<'a>; 13] {
        fn name<'a>(
            index: Option<usize>,
            common: [&'static [u8]; 2],
            formatted: &'a [u8],
        ) -> Datum<'a> {
            match index {
                None => Datum::Nil,
                Some(0) => Datum::Bytes(common[0]),
                Some(1) => Datum::Bytes(common[1]),
                Some(_) => Datum::Bytes(formatted),
            }
        }
        [
            Datum::Bytes(input.button.as_bytes()),
            Datum::Bytes(input.game_button.as_bytes()),
            Datum::Bytes(input.kind.native_name().as_bytes()),
            name(
                input.controller,
                [b"GameController_1", b"GameController_2"],
                &self.controller,
            ),
            name(
                input.player,
                [b"PlayerNumber_P1", b"PlayerNumber_P2"],
                &self.player,
            ),
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

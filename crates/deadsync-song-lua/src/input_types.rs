//! Engine-independent screen input data and authored actor writes.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SongLuaInputType {
    FirstPress,
    Repeat,
    Release,
}

/// Inputs supplied by the input owner, never inferred from expected Lua output.
#[derive(Clone, Debug)]
pub struct SongLuaInputEvent<'a> {
    pub player: Option<usize>,
    pub controller: Option<usize>,
    pub button: &'a str,
    pub game_button: &'a str,
    pub kind: SongLuaInputType,
    pub device: &'a str,
    pub device_button: &'a str,
    pub level: f32,
    pub z: i32,
    pub down: bool,
    pub ago: f64,
    pub is_joystick: bool,
    pub is_mouse: bool,
}

/// An authored native actor write, in callback execution order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SongLuaActorInputAction {
    /// Actor indices address overlays first, followed by tracked actors.
    Visible {
        actor: usize,
        visible: bool,
    },
    /// The exact f32 bits written by the authored numeric setter.
    X {
        actor: usize,
        bits: u32,
    },
    Y {
        actor: usize,
        bits: u32,
    },
}

/// Primitive state captured from a raw scalar table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SongLuaInputScalar<'a> {
    Boolean(bool),
    Number(f64),
    Bytes(&'a [u8]),
}

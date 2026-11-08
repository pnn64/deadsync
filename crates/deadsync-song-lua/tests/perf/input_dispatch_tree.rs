//! Frozen native-prefix BTreeMap dispatcher before contiguous callback storage.
//! Screen-owned native Lua input callbacks. ScreenManager sends these after
//! normal screen input; a truthy callback stops other callbacks, not gameplay.
use mlua::{Function, Lua, Table, Value};
use std::collections::BTreeMap;
use std::ops::Bound::{Excluded, Unbounded};

#[path = "../../src/input/compiled.rs"]
mod compiled;
pub use compiled::SongLuaCompiledInput;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SongLuaInputType {
    FirstPress,
    Repeat,
    Release,
}

impl SongLuaInputType {
    fn native_name(self) -> &'static str {
        match self {
            Self::FirstPress => "InputEventType_FirstPress",
            Self::Repeat => "InputEventType_Repeat",
            Self::Release => "InputEventType_Release",
        }
    }
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

#[derive(Default)]
struct Callbacks {
    functions: BTreeMap<usize, Callback>,
    dispatching: bool,
    removals: Vec<usize>,
}

struct Callback {
    function: Function,
    native: Option<SongLuaCompiledInput>,
}

// Sticky across callback removal and screen replacement: an offline sample
// cannot represent external input, even if its discovery probe later removes it.
#[derive(Default)]
struct RequiresRuntime;

pub(crate) fn require_offline_compatible(lua: &Lua) -> Result<(), String> {
    if lua.app_data_ref::<RequiresRuntime>().is_some() {
        Err(
            "song Lua registers input callbacks and requires runtime Lua (context.live = true)"
                .into(),
        )
    } else {
        Ok(())
    }
}

pub(crate) fn install(lua: &Lua, screen: &Table) -> mlua::Result<()> {
    // A gameplay reload retains the VM but replaces Screen's callback map.
    lua.set_app_data(Callbacks::default());
    screen.set(
        "AddInputCallback",
        lua.create_function(|lua, (screen, callback): (Table, Function)| {
            lua.set_app_data(RequiresRuntime);
            let key = callback.to_pointer() as usize;
            let native = SongLuaCompiledInput::compile(&callback);
            let mut callbacks = lua
                .app_data_mut::<Callbacks>()
                .expect("screen callback registry");
            callbacks.functions.insert(
                key,
                Callback {
                    function: callback,
                    native,
                },
            );
            Ok(screen)
        })?,
    )?;
    screen.set(
        "RemoveInputCallback",
        lua.create_function(|lua, (screen, callback): (Table, Function)| {
            let mut callbacks = lua
                .app_data_mut::<Callbacks>()
                .expect("screen callback registry");
            let key = callback.to_pointer() as usize;
            if callbacks.dispatching {
                callbacks.removals.push(key);
            } else {
                callbacks.functions.remove(&key);
            }
            Ok(screen)
        })?,
    )
}

pub(crate) fn dispatch(
    lua: &Lua,
    input: &SongLuaInputEvent<'_>,
) -> mlua::Result<(bool, Vec<String>)> {
    let mut previous = None;
    if !lua.has_hook() {
        let mut callbacks = lua
            .app_data_mut::<Callbacks>()
            .expect("screen callback registry");
        // Reentrant dispatch retains the existing deferred-removal protocol.
        if !callbacks.dispatching {
            if callbacks.functions.is_empty() {
                return Ok((false, Vec::new()));
            }
            if callbacks
                .functions
                .first_key_value()
                .is_some_and(|(_, callback)| callback.native.is_some())
            {
                let frame = compiled::Frame::new(input);
                let values = frame.values(input);
                let mut all_native = true;
                for (&key, callback) in &mut callbacks.functions {
                    let Some(native) = callback.native.as_mut() else {
                        all_native = false;
                        break;
                    };
                    previous = Some(key);
                    if native.evaluate_frame(&values) {
                        return Ok((true, Vec::new()));
                    }
                }
                if all_native {
                    return Ok((false, Vec::new()));
                }
            }
        }
    }
    // Input-only callbacks cannot mutate the event or registry. A false native
    // prefix leaves the original input untouched. Once Lua runs, all remaining
    // callbacks use its shared mutable event, including otherwise compiled ones.
    let event = create_event(lua, input)?;
    lua.app_data_mut::<Callbacks>()
        .expect("screen callback registry")
        .dispatching = true;
    let mut handled = false;
    let mut errors = Vec::new();
    loop {
        let next = {
            let callbacks = lua
                .app_data_ref::<Callbacks>()
                .expect("screen callback registry");
            match previous {
                Some(key) => callbacks.functions.range((Excluded(key), Unbounded)).next(),
                None => callbacks.functions.iter().next(),
            }
            .map(|(&key, callback)| (key, callback.function.clone()))
        };
        let Some((key, callback)) = next else {
            break;
        };
        previous = Some(key);
        match callback.call::<Value>(event.clone()) {
            Ok(value) => handled = !matches!(value, Value::Nil | Value::Boolean(false)),
            Err(error) => errors.push(format!("Error running input callback: {error}")),
        }
        if handled {
            break;
        }
    }
    let mut callbacks = lua
        .app_data_mut::<Callbacks>()
        .expect("screen callback registry");
    callbacks.dispatching = false;
    let removals = std::mem::take(&mut callbacks.removals);
    for key in removals {
        callbacks.functions.remove(&key);
    }
    Ok((handled, errors))
}

// Fresh tables cannot have authored metamethods. Keep the original insertion
// and conversion order, including nil writes, without changing hash capacity.
pub(crate) fn create_event(lua: &Lua, input: &SongLuaInputEvent<'_>) -> mlua::Result<Table> {
    use mlua::Either::{Left, Right};
    let event = lua.create_table()?;
    let device = lua.create_table()?;
    device.raw_set_many([
        ("device", Left(input.device)),
        ("button", Left(input.device_button)),
        ("level", Right(Value::Number(f64::from(input.level)))),
        ("z", Right(Value::Integer(input.z.into()))),
        ("down", Right(Value::Boolean(input.down))),
        ("ago", Right(Value::Number(input.ago))),
        ("is_joystick", Right(Value::Boolean(input.is_joystick))),
        ("is_mouse", Right(Value::Boolean(input.is_mouse))),
    ])?;
    let mut device = Some(device);
    event.raw_set_many((0..7).map(|index| match index {
        0 => (
            "DeviceInput",
            Right(Value::Table(device.take().expect("first event field"))),
        ),
        1 => (
            "controller",
            input.controller.map_or(Right(Value::Nil), |p| {
                Left(Right(format!("GameController_{}", p + 1)))
            }),
        ),
        2 => ("button", Left(Left(input.button))),
        3 => ("GameButton", Left(Left(input.game_button))),
        4 => ("type", Left(Left(input.kind.native_name()))),
        5 => (
            "PlayerNumber",
            input.player.map_or(Right(Value::Nil), |p| {
                Left(Right(format!("PlayerNumber_P{}", p + 1)))
            }),
        ),
        _ => ("MultiPlayer", Right(Value::Nil)),
    }))?;
    Ok(event)
}

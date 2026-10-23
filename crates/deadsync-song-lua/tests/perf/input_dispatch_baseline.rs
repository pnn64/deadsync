//! Frozen input registry/dispatcher from 5f6cf66777b1.
//! Screen-owned native Lua input callbacks. ScreenManager sends these after
//! normal screen input; a truthy callback stops other callbacks, not gameplay.
use mlua::{Function, Lua, Table, Value};
use std::collections::BTreeMap;
use std::ops::Bound::{Excluded, Unbounded};

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
    functions: BTreeMap<usize, Function>,
    dispatching: bool,
    removals: Vec<usize>,
}

// Sticky across callback removal and screen replacement: an offline sample
// cannot represent external input, even if its discovery probe later removes it.
#[derive(Default)]
struct RequiresRuntime;

pub(crate) fn require_offline_compatible(lua: &Lua) -> Result<(), String> {
    if lua.app_data_ref::<RequiresRuntime>().is_some() {
        Err("song Lua registers input callbacks and requires runtime Lua (context.live = true)".into())
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
            lua.app_data_mut::<Callbacks>()
                .expect("screen callback registry")
                .functions
                .insert(callback.to_pointer() as usize, callback);
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
    let event = lua.create_table()?;
    let device = lua.create_table()?;
    device.set("device", input.device)?;
    device.set("button", input.device_button)?;
    device.set("level", input.level)?;
    device.set("z", input.z)?;
    device.set("down", input.down)?;
    device.set("ago", input.ago)?;
    device.set("is_joystick", input.is_joystick)?;
    device.set("is_mouse", input.is_mouse)?;
    event.set("DeviceInput", device)?;
    event.set(
        "controller",
        input
            .controller
            .map(|p| format!("GameController_{}", p + 1)),
    )?;
    event.set("button", input.button)?;
    event.set("GameButton", input.game_button)?;
    event.set("type", input.kind.native_name())?;
    event.set(
        "PlayerNumber",
        input.player.map(|p| format!("PlayerNumber_P{}", p + 1)),
    )?;
    event.set("MultiPlayer", Value::Nil)?;
    lua.app_data_mut::<Callbacks>()
        .expect("screen callback registry")
        .dispatching = true;
    let mut previous = None;
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
            .map(|(&key, function)| (key, function.clone()))
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


// Event construction extracted verbatim for the isolated boundary measurement.
pub(crate) fn create_event(lua: &Lua, input: &SongLuaInputEvent<'_>) -> mlua::Result<Table> {
    let event = lua.create_table()?;
    let device = lua.create_table()?;
    device.set("device", input.device)?;
    device.set("button", input.device_button)?;
    device.set("level", input.level)?;
    device.set("z", input.z)?;
    device.set("down", input.down)?;
    device.set("ago", input.ago)?;
    device.set("is_joystick", input.is_joystick)?;
    device.set("is_mouse", input.is_mouse)?;
    event.set("DeviceInput", device)?;
    event.set(
        "controller",
        input
            .controller
            .map(|p| format!("GameController_{}", p + 1)),
    )?;
    event.set("button", input.button)?;
    event.set("GameButton", input.game_button)?;
    event.set("type", input.kind.native_name())?;
    event.set(
        "PlayerNumber",
        input.player.map(|p| format!("PlayerNumber_P{}", p + 1)),
    )?;
    event.set("MultiPlayer", Value::Nil)?;
    Ok(event)
}

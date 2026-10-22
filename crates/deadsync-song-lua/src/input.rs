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

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: SongLuaInputType) -> SongLuaInputEvent<'static> {
        SongLuaInputEvent {
            player: Some(1),
            controller: Some(1),
            button: "Up",
            game_button: "MenuUp",
            kind,
            device: "InputDevice_Key",
            device_button: "DeviceButton_up",
            level: 1.0,
            z: 0,
            down: kind != SongLuaInputType::Release,
            ago: 0.0,
            is_joystick: false,
            is_mouse: false,
        }
    }

    #[test]
    fn screen_callbacks_receive_press_repeat_release_and_deduplicate_registration() {
        let lua = Lua::new();
        let screen = lua.create_table().unwrap();
        install(&lua, &screen).unwrap();
        lua.globals().set("SCREEN", screen).unwrap();
        lua.load(
            r#"
SEEN = {}
CALLBACK = function(event)
    assert(event.button == 'Up' and event.GameButton == 'MenuUp')
    assert(event.PlayerNumber == 'PlayerNumber_P2' and event.controller == 'GameController_2')
    assert(event.DeviceInput.button == 'DeviceButton_up')
    assert(event.DeviceInput.down == (event.type ~= 'InputEventType_Release'))
    SEEN[#SEEN+1] = event.type
end
SCREEN:AddInputCallback(CALLBACK):AddInputCallback(CALLBACK)
"#,
        )
        .exec()
        .unwrap();
        for kind in [
            SongLuaInputType::FirstPress,
            SongLuaInputType::Repeat,
            SongLuaInputType::Release,
        ] {
            let (handled, errors) = dispatch(&lua, &event(kind)).unwrap();
            assert!(!handled && errors.is_empty(), "{errors:?}");
        }
        assert_eq!(lua.load("return #SEEN").eval::<usize>().unwrap(), 3);
        lua.load("SCREEN:RemoveInputCallback(CALLBACK)")
            .exec()
            .unwrap();
        dispatch(&lua, &event(SongLuaInputType::FirstPress)).unwrap();
        assert_eq!(lua.load("return #SEEN").eval::<usize>().unwrap(), 3);
        install(&lua, &lua.create_table().unwrap()).unwrap();
        assert!(
            lua.app_data_ref::<Callbacks>()
                .unwrap()
                .functions
                .is_empty(),
            "new screen replaces callbacks"
        );
    }

    #[test]
    fn screen_callbacks_defer_removals_share_event_and_use_lua_truthiness() {
        let lua = Lua::new();
        let screen = lua.create_table().unwrap();
        install(&lua, &screen).unwrap();
        lua.globals().set("SCREEN", screen).unwrap();
        let create = lua
            .load(
                r#"return function()
    local callback
    callback = function(e)
        if callback == FIRST then
            COUNT=COUNT+1; e.mark=true
            SCREEN:RemoveInputCallback(SECOND)
            return HANDLED
        end
        assert(e.mark); COUNT=COUNT+10
    end
    return callback
end"#,
            )
            .eval::<Function>()
            .unwrap();
        let first = create.call::<Function>(()).unwrap();
        let second = create.call::<Function>(()).unwrap();
        // Map order is native function-pointer order, independent of registration.
        let (low, high) = if first.to_pointer() < second.to_pointer() {
            (first, second)
        } else {
            (second, first)
        };
        lua.globals().set("FIRST", low).unwrap();
        lua.globals().set("SECOND", high.clone()).unwrap();
        lua.globals().set("COUNT", 0).unwrap();
        lua.globals().set("HANDLED", false).unwrap();
        lua.load("SCREEN:AddInputCallback(SECOND):AddInputCallback(FIRST)")
            .exec()
            .unwrap();
        let (handled, errors) = dispatch(&lua, &event(SongLuaInputType::FirstPress)).unwrap();
        assert!(!handled && errors.is_empty(), "{errors:?}");
        assert_eq!(lua.globals().get::<i32>("COUNT").unwrap(), 11);
        lua.globals().set("HANDLED", 0).unwrap();
        assert!(
            dispatch(&lua, &event(SongLuaInputType::Repeat)).unwrap().0,
            "zero is truthy in Lua"
        );
        assert_eq!(lua.globals().get::<i32>("COUNT").unwrap(), 12);
    }
}

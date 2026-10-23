//! Screen-owned native Lua input callbacks. ScreenManager sends these after
//! normal screen input; a truthy callback stops other callbacks, not gameplay.
use mlua::{Function, Lua, Table, Value};

#[path = "input/compiled.rs"]
pub(crate) mod compiled;
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
    functions: Vec<Callback>,
    dispatching: bool,
    removals: Vec<usize>,
}

struct Callback {
    key: usize,
    function: Function,
    native: Option<SongLuaCompiledInput>,
}

pub(crate) fn functions(lua: &Lua) -> Option<Vec<Function>> {
    let callbacks = lua.app_data_ref::<Callbacks>()?;
    if callbacks.dispatching { return None; }
    Some(callbacks.functions.iter().map(|entry| entry.function.clone()).collect())
}

impl Callbacks {
    fn remove(&mut self, key: usize) {
        if let Ok(index) = self
            .functions
            .binary_search_by_key(&key, |callback| callback.key)
        {
            self.functions.remove(index);
        }
    }
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
            let entry = Callback {
                key,
                function: callback,
                native,
            };
            match callbacks
                .functions
                .binary_search_by_key(&key, |callback| callback.key)
            {
                Ok(index) => callbacks.functions[index] = entry,
                Err(index) => callbacks.functions.insert(index, entry),
            }
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
                callbacks.remove(key);
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
    let mut next_index = 0;
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
                .first()
                .is_some_and(|callback| callback.native.is_some())
            {
                let frame = compiled::Frame::new(input);
                let values = frame.values(input);
                let mut all_native = true;
                for callback in &mut callbacks.functions {
                    let Some(native) = callback.native.as_mut() else {
                        all_native = false;
                        break;
                    };
                    previous = Some(callback.key);
                    next_index += 1;
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
            // Most callbacks keep the registry unchanged. Validate the prior
            // key at its old index, then advance in O(1). If callback additions,
            // nested dispatch or screen replacement moved it, repair from the
            // previous identity exactly as the original ordered range search.
            let index = match previous {
                None => 0,
                Some(key)
                    if next_index > 0
                        && callbacks
                            .functions
                            .get(next_index - 1)
                            .is_some_and(|callback| callback.key == key) =>
                {
                    next_index
                }
                Some(key) => callbacks
                    .functions
                    .partition_point(|callback| callback.key <= key),
            };
            next_index = index + 1;
            callbacks
                .functions
                .get(index)
                .map(|callback| (callback.key, callback.function.clone()))
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
        callbacks.remove(key);
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

    #[test]
    fn closed_callbacks_use_native_dispatch_and_debug_hooks_keep_lua_execution() {
        let lua = Lua::new();
        let screen = lua.create_table().unwrap();
        install(&lua, &screen).unwrap();
        lua.globals().set("SCREEN", screen).unwrap();
        lua.load("SCREEN:AddInputCallback(function(e) return e.GameButton=='MenuUp' end)")
            .exec()
            .unwrap();
        assert_eq!(
            lua.app_data_ref::<Callbacks>()
                .unwrap()
                .functions
                .iter()
                .filter(|v| v.native.is_none())
                .count(),
            0
        );
        assert!(
            lua.app_data_ref::<Callbacks>()
                .unwrap()
                .functions
                .iter()
                .all(|v| v.native.is_some())
        );
        for _ in 0..3 {
            let (handled, errors) = dispatch(&lua, &event(SongLuaInputType::Repeat)).unwrap();
            assert!(handled && errors.is_empty());
        }
        lua.set_hook(mlua::HookTriggers::ON_CALLS, |_, _| {
            Err(mlua::Error::runtime("observed callback hook"))
        })
        .unwrap();
        let (handled, errors) = dispatch(&lua, &event(SongLuaInputType::Repeat)).unwrap();
        assert!(!handled);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("observed callback hook"));
        lua.remove_hook();
        assert!(dispatch(&lua, &event(SongLuaInputType::Repeat)).unwrap().0);
    }

    #[test]
    fn mixed_callbacks_see_mutated_events_and_removals_restore_native_eligibility() {
        let lua = Lua::new();
        let screen = lua.create_table().unwrap();
        install(&lua, &screen).unwrap();
        lua.globals().set("SCREEN", screen.clone()).unwrap();
        let reader: Function = lua
            .load("return function(e) return e.button=='changed' end")
            .eval()
            .unwrap();
        let mutator: Function = lua
            .load("return function(e) e.button='changed';SCREEN:RemoveInputCallback(MUTATOR) end")
            .eval()
            .unwrap();
        let mutated_before_read = mutator.to_pointer() < reader.to_pointer();
        lua.globals().set("MUTATOR", mutator.clone()).unwrap();
        screen
            .get::<Function>("AddInputCallback")
            .unwrap()
            .call::<()>((screen.clone(), reader))
            .unwrap();
        screen
            .get::<Function>("AddInputCallback")
            .unwrap()
            .call::<()>((screen, mutator))
            .unwrap();
        assert_eq!(
            lua.app_data_ref::<Callbacks>()
                .unwrap()
                .functions
                .iter()
                .filter(|v| v.native.is_none())
                .count(),
            1
        );
        let (handled, errors) = dispatch(&lua, &event(SongLuaInputType::FirstPress)).unwrap();
        assert_eq!(handled, mutated_before_read);
        assert!(errors.is_empty());
        assert_eq!(
            lua.app_data_ref::<Callbacks>()
                .unwrap()
                .functions
                .iter()
                .filter(|v| v.native.is_none())
                .count(),
            0
        );
        assert!(!dispatch(&lua, &event(SongLuaInputType::Repeat)).unwrap().0);
    }

    #[test]
    fn native_prefix_stopping_preserves_later_lua_action_order() {
        let lua = Lua::new();
        let screen = lua.create_table().unwrap();
        install(&lua, &screen).unwrap();
        lua.globals().set("SCREEN", screen.clone()).unwrap();
        lua.globals()
            .set(
                "TARGET",
                lua.load("{visible=true}").eval::<Table>().unwrap(),
            )
            .unwrap();
        let predicate: Function = lua
            .load("return function(e) return e.GameButton=='MenuUp' end")
            .eval()
            .unwrap();
        let action: Function = lua
            .load("return function(e) TARGET.visible=false end")
            .eval()
            .unwrap();
        let predicate_first = predicate.to_pointer() < action.to_pointer();
        let add = screen.get::<Function>("AddInputCallback").unwrap();
        add.call::<()>((screen.clone(), predicate)).unwrap();
        add.call::<()>((screen, action)).unwrap();
        let (handled, errors) = dispatch(&lua, &event(SongLuaInputType::FirstPress)).unwrap();
        assert!(handled && errors.is_empty());
        assert_eq!(
            lua.load("return TARGET.visible").eval::<bool>().unwrap(),
            predicate_first
        );
        let mut alternate = event(SongLuaInputType::Release);
        alternate.game_button = "MenuLeft";
        let (handled, errors) = dispatch(&lua, &alternate).unwrap();
        assert!(!handled && errors.is_empty());
        assert!(!lua.load("return TARGET.visible").eval::<bool>().unwrap());
    }

    #[test]
    fn reentrant_dispatch_keeps_existing_pending_removal_protocol() {
        let lua = Lua::new();
        let screen = lua.create_table().unwrap();
        install(&lua, &screen).unwrap();
        let f: Function = lua
            .load("return function(e) return true end")
            .eval()
            .unwrap();
        let key = f.to_pointer() as usize;
        screen
            .get::<Function>("AddInputCallback")
            .unwrap()
            .call::<()>((screen, f))
            .unwrap();
        {
            let mut callbacks = lua.app_data_mut::<Callbacks>().unwrap();
            callbacks.dispatching = true;
            callbacks.removals.push(key);
        }
        assert!(
            dispatch(&lua, &event(SongLuaInputType::FirstPress))
                .unwrap()
                .0
        );
        let callbacks = lua.app_data_ref::<Callbacks>().unwrap();
        assert!(!callbacks.dispatching && callbacks.removals.is_empty());
        assert!(callbacks.functions.is_empty());
    }

    #[test]
    fn additions_on_both_sides_of_current_callback_keep_native_identity_order() {
        let lua = Lua::new();
        let screen = lua.create_table().unwrap();
        install(&lua, &screen).unwrap();
        lua.globals().set("SCREEN", screen.clone()).unwrap();
        lua.globals()
            .set("SEEN", lua.create_table().unwrap())
            .unwrap();
        let mut functions = (0..12).map(|id| {
            let f:Function=lua.load(format!("return function(e) SEEN[#SEEN+1]={id};if START_ID=={id} and not ADDED then ADDED=true;SCREEN:AddInputCallback(LOW):AddInputCallback(HIGH) end;return false end")).eval().unwrap();
            (id,f)
        }).collect::<Vec<_>>();
        functions.sort_by_key(|(_, f)| f.to_pointer() as usize);
        let (low_id, low) = functions[0].clone();
        let (mid_id, mid) = functions[6].clone();
        let (high_id, high) = functions[11].clone();
        lua.globals().set("LOW", low).unwrap();
        lua.globals().set("HIGH", high).unwrap();
        lua.globals().set("START_ID", mid_id).unwrap();
        screen
            .get::<Function>("AddInputCallback")
            .unwrap()
            .call::<()>((screen, mid))
            .unwrap();
        assert!(
            !dispatch(&lua, &event(SongLuaInputType::FirstPress))
                .unwrap()
                .0
        );
        let seen: Vec<usize> = lua.globals().get("SEEN").unwrap();
        assert_eq!(seen, [mid_id, high_id]);
        lua.globals()
            .set("SEEN", lua.create_table().unwrap())
            .unwrap();
        assert!(!dispatch(&lua, &event(SongLuaInputType::Repeat)).unwrap().0);
        let seen: Vec<usize> = lua.globals().get("SEEN").unwrap();
        assert_eq!(seen, [low_id, mid_id, high_id]);
    }

    #[test]
    fn nested_dispatch_removing_earlier_entries_repairs_outer_position() {
        let lua = Lua::new();
        let screen = lua.create_table().unwrap();
        install(&lua, &screen).unwrap();
        lua.globals().set("SCREEN", screen.clone()).unwrap();
        lua.globals()
            .set("SEEN", lua.create_table().unwrap())
            .unwrap();
        lua.globals()
            .set(
                "NESTED",
                lua.create_function(|lua, ()| {
                    dispatch(lua, &event(SongLuaInputType::Repeat))?;
                    Ok(())
                })
                .unwrap(),
            )
            .unwrap();
        let mut functions=(0..12).map(|id|{
            let f:Function=lua.load(format!("return function(e) SEEN[#SEEN+1]={id};if START_ID=={id} and not IN_NEST then IN_NEST=true;SCREEN:RemoveInputCallback(LOW);NESTED() end;return false end")).eval().unwrap();(id,f)
        }).collect::<Vec<_>>();
        functions.sort_by_key(|(_, f)| f.to_pointer() as usize);
        let (low_id, low) = functions[0].clone();
        let (mid_id, mid) = functions[6].clone();
        let (high_id, high) = functions[11].clone();
        lua.globals().set("LOW", low.clone()).unwrap();
        lua.globals().set("START_ID", mid_id).unwrap();
        let add = screen.get::<Function>("AddInputCallback").unwrap();
        for f in [low, mid, high] {
            add.call::<()>((screen.clone(), f)).unwrap();
        }
        assert!(
            !dispatch(&lua, &event(SongLuaInputType::FirstPress))
                .unwrap()
                .0
        );
        let seen: Vec<usize> = lua.globals().get("SEEN").unwrap();
        assert_eq!(seen, [low_id, mid_id, low_id, mid_id, high_id, high_id]);
    }

    #[test]
    fn compiled_registry_keeps_function_identity_rooted_until_removal() {
        let lua = Lua::new();
        let screen = lua.create_table().unwrap();
        install(&lua, &screen).unwrap();
        lua.globals().set("SCREEN", screen).unwrap();
        lua.load("WEAK=setmetatable({}, {__mode='v'});local f=function(e) return e.button=='Up' end;WEAK[1]=f;SCREEN:AddInputCallback(f)").exec().unwrap();
        lua.gc_collect().unwrap();
        assert!(lua.load("return WEAK[1]~=nil").eval::<bool>().unwrap());
        lua.load("SCREEN:RemoveInputCallback(WEAK[1])")
            .exec()
            .unwrap();
        lua.gc_collect().unwrap();
        assert!(lua.load("return WEAK[1]==nil").eval::<bool>().unwrap());
        assert!(
            lua.app_data_ref::<Callbacks>()
                .unwrap()
                .functions
                .is_empty()
        );
    }
}

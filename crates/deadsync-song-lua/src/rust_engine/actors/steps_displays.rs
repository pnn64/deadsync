//! Existing StepsDisplay children, SetInternal ordering and gameplay metrics.
use super::*;
use native_children::{run_command, with_children};
use sprites::field;

const COMMANDS: &[u8] = br#"return function()
return {
    FrameLoadCommand = function(self,param) local bFlip = param.PlayerState and param.PlayerState:GetPlayerNumber() ~= PLAYER_1; self:zoomx(bFlip and -1 or 1); end,
    FrameSetCommand = function(self,param) if param.CustomDifficulty and param.CustomDifficulty ~= "" then self:diffuse(CustomDifficultyToColor(param.CustomDifficulty)) end end,
    MeterOnCommand = function(self) self:visible(false) end,
    MeterSetCommand = function(self,param) MESSAGEMAN:Broadcast("MeterSet") end,
    DescriptionOnCommand = function(self) self:zoom(0.8):horizalign("left"):vertspacing(-9):_wrapwidthpixels(345) end,
    DescriptionSetCommand = function(self,params) self:visible(params.Steps ~= nil and params.Steps:GetDifficulty()=="Difficulty_Edit") end,
}
end"#;

#[derive(Clone)]
struct Binding {
    store: Shared,
    method: &'static str,
}

impl Actors {
    pub(in crate::rust_engine) fn initialize_steps_displays(
        &self,
        vm: &mut Vm,
        screen: Value,
    ) -> Result<(), Error> {
        ensure_helpers(vm, self.helpers)?;
        let function = vm.new_host_function_with_data(
            initialize_callback,
            &[Value::Table(self.registry), Value::Table(self.helpers)],
            self.store.clone(),
        )?;
        vm.call(function, &[screen]).map(|_| ())
    }
}

pub(super) fn ensure_helpers(vm: &mut Vm, helpers: TableId) -> Result<(), Error> {
    let key = vm.try_intern(b"steps_display_commands")?;
    if vm.raw_table_get(helpers, key)? == Value::Nil {
        let code = compile_named(COMMANDS, b"@theme:/metrics.ini:StepsDisplayGameplay")?;
        let factory = vm.execute(&code)?[0];
        super::super::put(vm, helpers, "steps_display_commands", factory)?;
    }
    Ok(())
}

fn raw(call: &mut HostCall<'_>, actor: TableId, name: &str, value: Value) -> Result<(), Error> {
    let key = call.try_intern(name.as_bytes())?;
    call.raw_table_set(actor, key, value)
}

pub(super) fn create(
    call: &mut HostCall<'_>,
    store: &Shared,
    scene: u64,
    name: &str,
) -> Result<TableId, Error> {
    let actor = generated_players::dummy(call, store, scene, "StepsDisplay", name, None)?;
    raw(
        call,
        actor,
        "__songlua_owned_child_lookup",
        Value::Boolean(true),
    )?;
    for (name, kind) in [
        ("Frame", "Sprite"),
        ("Meter", "BitmapText"),
        ("Description", "BitmapText"),
        ("Autogen", "Sprite"),
    ] {
        let child = generated_players::dummy(call, store, scene, kind, name, Some(actor))?;
        if kind == "Sprite" {
            screen_parts::blank_texture(call, store, child)?;
        } else {
            let font = call.try_intern(b"Common Normal")?;
            raw(call, child, "Font", font)?;
            let empty = call.try_intern(b"")?;
            raw(call, child, "Text", empty)?;
        }
        screen_parts::append(call, store, actor, child)?;
        screen_parts::named(call, store, actor, name, child)?;
    }
    let bound = get(call, actor, b"__songlua_bound_methods")?;
    let bound = table(call, bound)?;
    for method in ["SetFromSteps", "SetFromTrail", "SetFromGameState"] {
        let function = call.new_host_function_with_data(
            invoke,
            &[call.captures()[0], call.captures()[1]],
            Binding {
                store: store.clone(),
                method,
            },
        )?;
        raw(call, bound, method, function)?;
    }
    Ok(actor)
}

fn value_call(
    call: &mut HostCall<'_>,
    receiver: TableId,
    method: &str,
    arguments: &[Value],
) -> Result<Value, Error> {
    let function = field(call, receiver, method)?;
    if !matches!(function, Value::Function(_) | Value::Native(_)) {
        return Err(call.error(b"function expected"));
    }
    let mut args = Vec::with_capacity(arguments.len() + 1);
    args.push(Value::Table(receiver));
    args.extend_from_slice(arguments);
    Ok(call
        .call(function, &args)?
        .first()
        .copied()
        .unwrap_or(Value::Nil))
}

fn truthy(value: Value) -> bool {
    !matches!(value, Value::Nil | Value::Boolean(false))
}

fn optional_table(call: &HostCall<'_>, value: Value) -> Result<Option<TableId>, Error> {
    if value == Value::Nil {
        Ok(None)
    } else {
        table(call, value).map(Some)
    }
}

fn visibility(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    visible: bool,
) -> Result<(), Error> {
    raw(call, actor, "__songlua_visible", Value::Boolean(visible))?;
    if let Some(actor) = store.lock().unwrap().actors.get_mut(&actor) {
        actor.state.visible = visible;
    }
    Ok(())
}

fn registry(call: &mut HostCall<'_>, actor: TableId) -> Result<Value, Error> {
    let function = native_children::helper_function(call, b"native_registry")?;
    Ok(call.call(function, &[Value::Table(actor)])?[0])
}

fn child(call: &mut HostCall<'_>, registry: TableId, name: &str) -> Result<TableId, Error> {
    let value = get(call, registry, name.as_bytes())?;
    table(call, value)
}

fn direct_message(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    name: &[u8],
    params: Value,
) -> Result<(), Error> {
    call.with_roots(&[Value::Table(actor), params], |call| {
        run_command(call, store, actor, name, Some(params))?;
        let kind = assets::field(call, actor, b"__songlua_actor_type")?.unwrap_or_default();
        if ["ActorFrame", "ActorFrameTexture", "TopScreen"]
            .iter()
            .any(|k| kind.eq_ignore_ascii_case(k))
        {
            with_children(call, actor, |call, child| {
                direct_message(call, store, child, name, params)
            })?;
        }
        Ok(())
    })
}

fn set_value(
    call: &mut HostCall<'_>,
    store: &Shared,
    display: TableId,
    value: Option<TableId>,
    trail: bool,
) -> Result<(), Error> {
    call.with_roots(&[value.map_or(Value::Nil, Value::Table)], |call| {
        let (difficulty, steps_type, meter) = if let Some(value) = value {
            let difficulty = value_call(call, value, "GetDifficulty", &[])?;
            let difficulty = assets::text(call, difficulty)?;
            let steps_type = value_call(call, value, "GetStepsType", &[])?;
            let steps_type = assets::text(call, steps_type)?;
            let meter = value_call(call, value, "GetMeter", &[])?;
            let meter = sprites::number(call, meter)?.trunc();
            if !(i32::MIN as f64..=i32::MAX as f64).contains(&meter) {
                return Err(call.error(b"steps meter is out of range"));
            }
            (difficulty, steps_type, meter as i32)
        } else {
            let difficulty = get(call, display, b"__songlua_preferred_difficulty")?;
            (
                assets::text(call, difficulty)?,
                "StepsType_Invalid".into(),
                0,
            )
        };
        let short = difficulty
            .strip_prefix("Difficulty_")
            .unwrap_or(&difficulty);
        let custom = match (steps_type.as_str(), short) {
            ("StepsType_Pump_Single", "Hard") => "Crazy",
            ("StepsType_Pump_Double", "Medium") => "Freestyle",
            ("StepsType_Pump_Double", "Hard") => "Nightmare",
            ("StepsType_Pump_Halfdouble", "Medium") => "HalfDouble",
            (_, "Invalid") => "",
            _ => short,
        };
        let description = if !trail
            && short == "Edit"
            && let Some(value) = value
        {
            let text = value_call(call, value, "GetDescription", &[])?;
            assets::text(call, text)?
        } else {
            match custom {
                "Beginner" => "Novice",
                "Challenge" => "Expert",
                _ => custom,
            }
            .into()
        };
        let params = call.try_new_table()?;
        call.with_roots(&[Value::Table(params)], |call| {
            for (key, text) in [
                ("CustomDifficulty", custom),
                ("DisplayDescription", description.as_str()),
                ("StepsType", steps_type.as_str()),
            ] {
                let value = call.try_intern(text.as_bytes())?;
                raw(call, params, key, value)?;
            }
            raw(call, params, "Meter", Value::Number(meter as f64))?;
            if let Some(value) = value {
                raw(
                    call,
                    params,
                    if trail { "Trail" } else { "Steps" },
                    Value::Table(value),
                )?;
            }
            let params = Value::Table(params);
            visibility(call, store, display, true)?;
            let children = registry(call, display)?;
            call.with_roots(&[children], |call| {
                let children = table(call, children)?;
                // Native Table handles keep every selected child alive until
                // SetInternal returns, even after callbacks replace its registry.
                let held = call.try_new_table()?;
                call.with_roots(&[Value::Table(held)], |call| {
                    let frame = child(call, children, "Frame")?;
                    call.raw_table_set(held, Value::Number(1.), Value::Table(frame))?;
                    direct_message(call, store, frame, b"Set", params)?;
                    let text = child(call, children, "Meter")?;
                    call.raw_table_set(held, Value::Number(2.), Value::Table(text))?;
                    let meter_text = if meter == 0 {
                        "?".into()
                    } else {
                        meter.to_string()
                    };
                    let meter_text = call.try_intern(meter_text.as_bytes())?;
                    raw(call, text, "Text", meter_text)?;
                    if meter != 0 {
                        direct_message(call, store, text, b"Set", params)?;
                    }
                    let text = child(call, children, "Description")?;
                    call.raw_table_set(held, Value::Number(3.), Value::Table(text))?;
                    let description = call.try_intern(description.as_bytes())?;
                    raw(call, text, "Text", description)?;
                    direct_message(call, store, text, b"Set", params)?;
                    let autogen = child(call, children, "Autogen")?;
                    call.raw_table_set(held, Value::Number(4.), Value::Table(autogen))?;
                    call.with_roots(&[Value::Table(autogen)], |call| {
                        direct_message(call, store, autogen, b"Set", params)?;
                        let autogen_value = if !trail && let Some(value) = value {
                            truthy(value_call(call, value, "IsAutogen", &[])?)
                        } else {
                            false
                        };
                        visibility(call, store, autogen, autogen_value)
                    })?;
                    run_command(call, store, display, b"Set", Some(params))?;
                    with_children(call, display, |call, child| {
                        direct_message(call, store, child, b"Set", params)
                    })
                })
            })
        })
    })
}

fn invoke(call: &mut HostCall<'_>) -> Result<(), Error> {
    let Binding { store, method } = call.function_data::<Binding>().unwrap().clone();
    let display = table(call, arg(call, 0))?;
    let value = arg(call, 1);
    if method == "SetFromGameState" {
        let globals = table(call, helper(call, 5)?)?;
        let game = field(call, globals, "GAMESTATE")?;
        call.with_roots(&[game], |call| {
            let game = table(call, game)?;
            let trail = truthy(value_call(call, game, "IsCourseMode", &[])?);
            let value = value_call(
                call,
                game,
                if trail {
                    "GetCurrentTrail"
                } else {
                    "GetCurrentSteps"
                },
                &[value],
            )?;
            set_value(call, &store, display, optional_table(call, value)?, trail)
        })?;
    } else if let Some(value) = optional_table(call, value)? {
        set_value(call, &store, display, Some(value), method == "SetFromTrail")?;
    } else {
        visibility(call, &store, display, false)?;
    }
    // The typed counter conversion and increment happen only after all callbacks.
    let globals = table(call, helper(call, 5)?)?;
    let count = field(call, globals, crate::SONG_LUA_SIDE_EFFECT_COUNT_KEY)?;
    let count = if count == Value::Nil {
        0
    } else {
        let count = sprites::number(call, count)?;
        if !(i64::MIN as f64..-(i64::MIN as f64)).contains(&count) {
            return Err(call.error(b"side effect count is out of range"));
        }
        count as i64
    };
    sprites::set(
        call,
        globals,
        crate::SONG_LUA_SIDE_EFFECT_COUNT_KEY,
        Value::Number(count.saturating_add(1) as f64),
    )?;
    call.push(Value::Table(display))
}

fn initialize_callback(call: &mut HostCall<'_>) -> Result<(), Error> {
    let store = call.function_data::<Shared>().unwrap().clone();
    let screen = table(call, arg(call, 0))?;
    initialize(call, &store, screen)
}

pub(super) fn initialize(
    call: &mut HostCall<'_>,
    store: &Shared,
    screen: TableId,
) -> Result<(), Error> {
    let globals = table(call, helper(call, 5)?)?;
    let game = field(call, globals, "GAMESTATE")?;
    call.with_roots(&[game], |call| {
        let game = table(call, game)?;
        let factory = native_children::helper_function(call, b"steps_display_commands")?;
        let commands = call.call(factory, &[])?[0];
        call.with_roots(&[commands], |call| {
            let commands = table(call, commands)?;
            for index in 0..crate::LUA_PLAYERS {
                let children = registry(call, screen)?;
                let display = call.with_roots(&[children], |call| {
                    let value = get(
                        call,
                        table(call, children)?,
                        format!("StepsDisplayP{}", index + 1).as_bytes(),
                    )?;
                    optional_table(call, value)
                })?;
                let Some(display) = display else { continue };
                for (name, x) in [
                    ("Frame", 0.),
                    ("Meter", 30.),
                    ("Description", -265.),
                    ("Autogen", 40.),
                ] {
                    let children = registry(call, display)?;
                    let actor = call.with_roots(&[children], |call| {
                        child(call, table(call, children)?, name)
                    })?;
                    {
                        let mut guard = store.lock().unwrap();
                        let actor = guard.actors.get_mut(&actor).ok_or_else(|| {
                            call.error(b"steps display child is not owned by this runtime")
                        })?;
                        let pose = actor.destination_mut();
                        pose.x = x;
                        pose.y = 0.;
                    }
                    for command in ["Load", "On", "Set"] {
                        let callback =
                            get(call, commands, format!("{name}{command}Command").as_bytes())?;
                        if callback != Value::Nil {
                            raw(call, actor, &format!("{command}Command"), callback)?;
                        }
                    }
                    run_named(call, store, actor, b"On", None)?;
                }
                let player = call.try_intern(crate::player_number_name(index).as_bytes())?;
                let params = Value::Table(call.try_new_table()?);
                call.with_roots(&[player, params], |call| {
                    let state = value_call(call, game, "GetPlayerState", &[player])?;
                    table(call, state)?;
                    raw(call, table(call, params)?, "PlayerState", state)?;
                    with_children(call, display, |call, child| {
                        direct_message(call, store, child, b"Load", params)
                    })?;
                    visibility(call, store, display, false)?;
                    let steps = value_call(call, game, "GetCurrentSteps", &[player])?;
                    if let Some(steps) = optional_table(call, steps)? {
                        set_value(call, store, display, Some(steps), false)?;
                    }
                    Ok(())
                })?;
            }
            Ok(())
        })
    })
}

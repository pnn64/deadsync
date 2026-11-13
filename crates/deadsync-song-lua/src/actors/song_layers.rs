//! The production compiler's authored file, ownership, Init and attach boundary.
use super::super::song_layers::{Entry, SongLayerPlan};
use super::*;

#[derive(Clone)]
struct Load {
    scene: u64,
    plan: SongLayerPlan,
}
impl Actors {
    pub(crate) fn song_initial_states(
        &self,
        vm: &Vm,
        screen: ActorScene,
    ) -> Option<super::super::song_layers::SongInitialStates> {
        self.root(vm, screen)?;
        vm.store_ref()
            .scenes
            .get(&screen.id)?
            .initial_states
            .clone()
    }

    pub(crate) fn initialize_song_layers(
        &self,
        vm: &mut Vm,
        screen: ActorScene,
        plan: &SongLayerPlan,
    ) -> Result<Option<Value>, Error> {
        if self.root(vm, screen).is_none() {
            return Ok(None);
        }
        for (key, source, name) in [
            (
                "layer_upper_left",
                UPPER_LEFT,
                b"@theme:/BackgroundEffects/UpperLeft.lua".as_slice(),
            ),
            (
                "layer_compose",
                COMPOSE,
                b"=song layer command merge".as_slice(),
            ),
            (
                "layer_wrap",
                WRAP,
                b"@theme:/BackgroundEffects/UpperLeft.lua".as_slice(),
            ),
        ] {
            let key_value = vm.try_intern(key.as_bytes())?;
            if vm.raw_table_get(self.helpers, key_value)? == Value::Nil {
                let code = compile_named(source, name)?;
                let function = vm.execute(&code)?[0];
                super::super::put(vm, self.helpers, key, function)?;
            }
        }
        let function = vm.new_host_function_with_data(
            load,
            &[Value::Table(self.registry), Value::Table(self.helpers)],
            Load {
                scene: screen.id,
                plan: plan.clone(),
            },
        )?;
        Ok(Some(vm.call(function, &[])?[0]))
    }
}
fn load(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Load>().unwrap().clone();
    if data.plan.live
        != call
            .store()
            .clock
            .live
            .load(std::sync::atomic::Ordering::Relaxed)
    {
        return Err(call.error(b"song layer plan and runtime modes differ"));
    }
    let top = call.store().scenes[&data.scene].root.unwrap();
    let first = call.store().next;
    let result = load_layers(call, &data, top);
    // Constructors can leave implicit templates or throw after publishing an
    // actor. Transfer every new owner even when execution/Init failed.
    let created = call
        .store()
        .scenes
        .keys()
        .copied()
        .filter(|id| *id > first)
        .collect::<Vec<_>>();
    for scene in created {
        gameplay_theme::adopt_owned_scene(call.store(), data.scene, scene);
    }
    let roots: Vec<_> = match &result {
        Ok(value) => vec![*value],
        Err(error) => error.value().into_iter().collect(),
    };
    call.with_roots(&roots, |call| {
        lifecycle::invalidate_plans(call)?;
        call.push(result?)
    })
}
fn load_layers(call: &mut HostCall<'_>, data: &Load, top: TableId) -> Result<Value, Error> {
    let globals = table(call, helper(call, 5)?)?;
    raw_set(
        call,
        globals,
        "__songlua_screen_loading",
        Value::Boolean(data.plan.screen_loading),
    )?;
    let roots = call.try_new_table()?;
    call.with_roots(&[Value::Table(roots)], |call| {
        // An active screen owns the scalar loader results too. Retirement keeps
        // this private edge as part of the scene graph, without an extra pin.
        let state = call.try_new_table()?;
        call.raw_table_set(state, Value::Number(1.), Value::Table(roots))?;
        if let Some(previous) = call.table_host_state(top)? {
            call.raw_table_set(state, Value::Number(2.), Value::Table(previous))?;
        }
        call.set_table_host_state(top, Some(state))?;
        let retained = call.try_new_table()?;
        call.raw_table_set(state, Value::Number(3.), Value::Table(retained))?;
        call.store().scenes.get_mut(&data.scene).unwrap().initial_states = Some(Default::default());
        call.store().scenes.get_mut(&data.scene).unwrap().song_session = Some(song_activation::Session { roots, ready: false, activated: false, discovery_started: false, locals: None, queued: HashMap::new(), messages: None });
        capture_screen(call, data, retained, globals)?;
        for (index, entry) in data.plan.entries.iter().enumerate() {
            let result = execute(call, entry);
            let definition = match result {
                Ok(value) if !data.plan.live || matches!(value, Value::Table(_)) => value,
                Ok(_) => {
                    log::warn!("Failed to load song actor '{}': must return a table; using native dummy Actor", entry.path.display());
                    Value::Table(offline::dummy(call, "Actor")?)
                }
                Err(error) if data.plan.live => {
                    log::warn!("Failed to load song actor '{}': {error}; using native dummy Actor", entry.path.display());
                    Value::Table(offline::dummy(call, "Actor")?)
                }
                Err(error) => return Err(call.error(format!("failed to execute '{}': {error}", entry.path.display()).as_bytes())),
            };
            let value = call.with_roots(&[definition], |call| initialize_layer(call, data, entry, index, definition, retained))?;
            call.raw_table_set(roots, Value::Number((index+1) as f64), value)?;
        }
        call.store().scenes.get_mut(&data.scene).unwrap().song_session.as_mut().unwrap().ready = true;
        Ok(Value::Table(roots))
    })
}
fn execute(call: &mut HostCall<'_>, entry: &Entry) -> Result<Value, Error> {
    let path = call.try_intern(entry.path.to_string_lossy().as_bytes())?;
    call.with_roots(&[path], |call| {
        let source = call.try_intern(entry.source.to_string_lossy().as_bytes())?;
        let loader = native_children::helper_function(call, b"source_loader")?;
        let function = call.call(loader, &[path, Value::Nil, source])?[0];
        Ok(call
            .call(function, &[])?
            .first()
            .copied()
            .unwrap_or(Value::Nil))
    })
}
fn initialize_layer(
    call: &mut HostCall<'_>,
    data: &Load,
    entry: &Entry,
    index: usize,
    value: Value,
    retained: TableId,
) -> Result<Value, Error> {
    let Value::Table(actor) = value else {
        return Ok(value);
    };
    let actor = if let Some(color) = entry.color {
        upper_left(call, actor, color)?
    } else {
        actor
    };
    call.with_roots(&[Value::Table(actor)], |call| {
        let scene = materialize(call, actor, Materialization::Construct)?;
        let actor = call.store().scenes[&scene].root.unwrap();
        gameplay_theme::adopt_owned_scene(call.store(), data.scene, scene);
        #[cfg(feature = "test-support")]
        {
            let path = call.try_intern((index + 1).to_string().as_bytes())?;
            raw_set(call, actor, "__songlua_message_path", path)?;
        }
        #[cfg(not(feature = "test-support"))]
        let _ = index;
        let globals = table(call, helper(call, 5)?)?;
        let screen = sprites::field(call, globals, "__songlua_top_screen")?;
        let screen = table(call, screen)?;
        let children = sprites::field(call, screen, "__songlua_children")?;
        let children = table(call, children)?;
        let parent = if entry.background {
            None
        } else {
            {
                let parent = sprites::field(call, children, "SongForeground")?;
                Some(table(call, parent)?)
            }
        };
        if let Some(beat) = entry.beat {
            let key = if entry.background {
                "__songlua_background_start_beat"
            } else {
                "__songlua_foreground_start_beat"
            };
            raw_set(call, actor, key, Value::Number(f64::from(beat)))?;
            raw_set(call, actor, "__songlua_defer_on", Value::Boolean(true))?;
            raw_set(call, actor, "__songlua_live_inactive", Value::Boolean(true))?;
        }
        if let Some(parent) = parent {
            parent_scene(call, actor, Value::Table(parent), false)?;
        }
        collect_initial(call, data, retained, Value::Table(actor))?;
        prepare_init(call.store());
        initialize(call, actor).map_err(|error| {
            call.error(
                format!(
                    "failed to run actor init commands for '{}': {error}",
                    entry.path.display()
                )
                .as_bytes(),
            )
        })?;
        if let Some(parent) = parent {
            // Init may change the activation flag. Read its actual post-Init value.
            let beat = get(call, actor, b"__songlua_foreground_start_beat")?;
            if beat != Value::Nil {
                sprites::number(call, beat)?;
                if let Some(name) = &entry.name {
                    let value = call.try_intern(name.as_bytes())?;
                    if get(call, actor, b"__songlua_load_name")? != Value::Nil {
                        raw_set(call, actor, "__songlua_load_name", value)?;
                    }
                    raw_set(call, actor, "__songlua_native_name", value)?;
                    raw_set(call, actor, "Name", value)?;
                    call.store().actors.get_mut(&actor).unwrap().state.name =
                        name.as_bytes().to_vec();
                }
                let visible = sprites::field(call, actor, "visible")?;
                call.call(visible, &[Value::Table(actor), Value::Boolean(false)])?;
            }
            parent_scene(call, actor, Value::Table(parent), true)?;
        }
        Ok(Value::Table(actor))
    })
}
fn upper_left(call: &mut HostCall<'_>, actor: TableId, color: [f32; 4]) -> Result<TableId, Error> {
    let colors = call.try_new_table()?;
    for (i, value) in color.into_iter().enumerate() {
        call.raw_table_set(
            colors,
            Value::Number((i + 1) as f64),
            Value::Number(f64::from(value)),
        )?;
    }
    let factory = native_children::helper_function(call, b"layer_upper_left")?;
    let commands = call.call(factory, &[Value::Table(colors)])?[0];
    call.with_roots(&[commands], |call| {
        let commands = table(call, commands)?;
        for name in ["OnCommand", "GainFocusCommand", "LoseFocusCommand"] {
            let key = call.try_intern(name.as_bytes())?;
            let second = call.raw_table_get(commands, key)?;
            let first = call.raw_table_get(actor, key)?;
            let value = if matches!(first, Value::Function(_) | Value::Native(_)) {
                let compose = native_children::helper_function(call, b"layer_compose")?;
                call.call(compose, &[first, second])?[0]
            } else {
                second
            };
            call.table_set(Value::Table(actor), key, value)?;
        }
        let wrap = native_children::helper_function(call, b"layer_wrap")?;
        let value = call.call(wrap, &[Value::Table(actor)])?[0];
        table(call, value)
    })
}
const UPPER_LEFT: &[u8] = br#"
return function(color)
    return {
        OnCommand = function(self)
            self:diffuse(color):effectclock('music')
            if self.GetTexture then self:GetTexture():rate(self:GetParent():GetUpdateRate()) end
        end,
        GainFocusCommand = function(self) self:play() end,
        LoseFocusCommand = function(self) self:pause() end,
    }
end
"#;
const COMPOSE: &[u8] =
    b"return function(first, second) return function(...) first(...); return second(...) end end";
const WRAP: &[u8] = b"return function(child)\n\n\nreturn Def.ActorFrame{child}\nend";

fn raw_set(call: &mut HostCall<'_>, actor: TableId, name: &str, value: Value) -> Result<(), Error> {
    let key = call.try_intern(name.as_bytes())?;
    call.raw_table_set(actor, key, value)
}

fn save_initial(
    call: &mut HostCall<'_>,
    data: &Load,
    retained: TableId,
    actor: TableId,
    screen_layer: bool,
) -> Result<(), Error> {
    call.with_roots(&[Value::Table(actor)], |call| {
        let state = initial_state::read(call, actor)?;
        call.raw_table_set(retained, Value::Table(actor), Value::Boolean(true))?;
        let guard = call.store();
        let initial = guard
            .scenes
            .get_mut(&data.scene)
            .unwrap()
            .initial_states
            .as_mut()
            .unwrap();
        if screen_layer {
            initial.screen_layers.insert(actor, state);
        } else {
            initial.actors.insert(actor, state);
        }
        Ok(())
    })
}
fn collect_initial(
    call: &mut HostCall<'_>,
    data: &Load,
    retained: TableId,
    value: Value,
) -> Result<(), Error> {
    let Value::Table(actor) = value else {
        return Ok(());
    };
    call.with_roots(&[value], |call| {
        let class = sprites::field(call, actor, "__songlua_actor_type")?;
        if initial_state::value_text(call, class)?.is_some() {
            save_initial(call, data, retained, actor, false)?;
        }
        // The compiler walks raw sequence slots: aliases are visited again,
        // holes stop the walk, and wrappers are not sequence children.
        let mut index = 1;
        loop {
            let value = call.raw_table_get(actor, Value::Number(index as f64))?;
            if value == Value::Nil {
                break;
            }
            collect_initial(call, data, retained, value)?;
            index += 1;
        }
        Ok(())
    })
}
fn capture_screen(
    call: &mut HostCall<'_>,
    data: &Load,
    retained: TableId,
    globals: TableId,
) -> Result<(), Error> {
    let screen = sprites::field(call, globals, "__songlua_top_screen")?;
    call.with_roots(&[screen], |call| {
        let children = sprites::field(call, table(call, screen)?, "__songlua_children")?;
        call.with_roots(&[children], |call| {
            let children = table(call, children)?;
            for name in ["Underlay", "Overlay", "SongBackground", "In"] {
                let actor = sprites::field(call, children, name)?;
                if actor != Value::Nil {
                    save_initial(call, data, retained, table(call, actor)?, true)?;
                }
            }
            Ok(())
        })
    })?;
    for name in [
        "__songlua_top_screen_player_1",
        "__songlua_top_screen_player_2",
    ] {
        let actor = sprites::field(call, globals, name)?;
        table(call, actor)?;
        collect_initial(call, data, retained, actor)?;
    }
    Ok(())
}

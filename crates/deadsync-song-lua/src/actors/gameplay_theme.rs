//! Bundled gameplay actors, loaded in the production host's separate phases.
use super::*;

mod overlay;
mod source;
mod underlay;

#[derive(Clone, Copy)]
pub(crate) enum Phase {
    Combo,
    Underlay,
    Overlay { capture_probe: bool },
    Decorations,
    Transitions,
}

#[derive(Clone)]
struct Load {
    scene: u64,
    phase: Phase,
}

impl Actors {
    pub(crate) fn load_gameplay_theme_phase(
        &self,
        vm: &mut Vm,
        scene: ActorScene,
        phase: Phase,
    ) -> Result<bool, Error> {
        if self.root(vm, scene).is_none() {
            return Ok(false);
        }
        let function = vm.new_host_function_with_data(
            load,
            &[Value::Table(self.registry), Value::Table(self.helpers)],
            Load {
                scene: scene.id,
                phase,
            },
        )?;
        vm.call(function, &[])?;
        Ok(true)
    }
}

fn load(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Load>().unwrap().clone();
    let first = call.store().next;
    let result = match data.phase {
        Phase::Combo => combo(call, &data),
        Phase::Underlay => {
            song_capture::without_live_broadcasts(call, |call| underlay::load(call, &data))
        }
        Phase::Overlay { capture_probe } => overlay::load(call, &data, capture_probe),
        Phase::Decorations => overlay::decorations(call, &data),
        Phase::Transitions => transitions(call, &data),
    };
    // Def/LoadFont concatenation can leave an implicit template owner behind.
    // Its callbacks share Lua upvalues with the loaded copy, and can therefore
    // retain the screen after Init stores an actor in one of those upvalues.
    // All objects constructed in this theme phase belong to its screen, even
    // when a file or callback fails before the returned root can be attached.
    let created = call
        .store()
        .scenes
        .keys()
        .copied()
        .filter(|id| *id > first)
        .collect::<Vec<_>>();
    for scene in created {
        adopt_scene(call, &data, scene);
    }
    result?;
    lifecycle::invalidate_plans(call)
}

fn field(call: &mut HostCall<'_>, actor: TableId, name: &str) -> Result<Value, Error> {
    call.with_roots(&[Value::Table(actor)], |call| {
        sprites::field(call, actor, name)
    })
}

fn set(call: &mut HostCall<'_>, actor: TableId, name: &str, value: Value) -> Result<(), Error> {
    call.with_roots(&[Value::Table(actor), value], |call| {
        let key = call.try_intern(name.as_bytes())?;
        call.raw_table_set(actor, key, value)
    })
}

fn parent(call: &mut HostCall<'_>, actor: TableId, owner: TableId) -> Result<(), Error> {
    set(call, actor, "__songlua_parent", Value::Table(owner))?;
    if let Some(actor) = call.store().actors.get_mut(&actor) {
        actor.parent = Some(owner);
    }
    Ok(())
}

fn member(call: &mut HostCall<'_>, owner: TableId, name: &str) -> Result<TableId, Error> {
    let value = field(call, owner, name)?;
    table(call, value)
}

// Materialization owns a separate scene, including implicit offline Def owners.
// Transfer all members before Init: a callback may fail or replace a child, but
// the screen must still release every actor registered by this load.
fn own(call: &mut HostCall<'_>, data: &Load, definition: Value) -> Result<TableId, Error> {
    call.with_roots(&[definition], |call| {
        let scene = materialize(call, table(call, definition)?, Materialization::Construct)?;
        let actor = call.store().scenes[&scene]
            .root
            .expect("constructed actor root");
        adopt_scene(call, data, scene);
        Ok(actor)
    })
}

fn adopt_scene(call: &mut HostCall<'_>, data: &Load, scene: u64) {
    adopt_owned_scene(call.store(), data.scene, scene);
}

pub(super) fn adopt_owned_scene(store: &mut Store, owner: u64, scene: u64) {
    if scene == owner {
        return;
    }
    let guard = &mut *store;
    guard.constructor_scenes.remove(&scene);
    let loaded = guard
        .scenes
        .remove(&scene)
        .expect("theme construction owner");
    for actor in loaded.actors {
        if !guard.scenes[&owner].actors.contains(&actor) {
            guard.scenes.get_mut(&owner).unwrap().actors.push(actor);
        }
        if guard.actors[&actor].scene == scene {
            guard.actors.get_mut(&actor).unwrap().scene = owner;
        }
    }
}

fn owned_lookup(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    let kind = field(call, actor, "__songlua_actor_type")?;
    if kind != Value::Nil && assets::text(call, kind)?.eq_ignore_ascii_case("ActorFrame") {
        set(
            call,
            actor,
            "__songlua_owned_child_lookup",
            Value::Boolean(true),
        )?;
    }
    native_children::with_children(call, actor, owned_lookup)
}

fn combo(call: &mut HostCall<'_>, data: &Load) -> Result<(), Error> {
    let globals = table(call, helper(call, 5)?)?;
    let sl = member(call, globals, "SL")?;
    call.with_roots(&[Value::Table(sl)], |call| {
        // Native gameplay constructs explosions for both players, including a
        // disabled player's retained subtree. HideComboExplosions is Lua truthy.
        for index in 1..=crate::LUA_PLAYERS {
            let player = member(
                call,
                globals,
                &format!("__songlua_top_screen_player_{index}"),
            )?;
            let registry = native_children::helper_function(call, b"native_registry")?;
            let children = call.call(registry, &[Value::Table(player)])?[0];
            call.with_roots(&[children], |call| {
                let combo = member(call, table(call, children)?, "Combo")?;
                call.with_roots(&[Value::Table(combo)], |call| {
                    let mods = member(call, sl, &format!("P{index}"))?;
                    let mods = member(call, mods, "ActiveModifiers")?;
                    let kind = field(call, combo, "__songlua_actor_type")?;
                    let hidden = field(call, mods, "HideComboExplosions")?;
                    if kind == Value::Nil
                        || !assets::text(call, kind)?.eq_ignore_ascii_case("ActorFrame")
                        || !matches!(hidden, Value::Nil | Value::Boolean(false))
                    {
                        return Ok(());
                    }
                    let children = call.call(registry, &[Value::Table(combo)])?[0];
                    call.with_roots(&[children], |call| {
                        let children = table(call, children)?;
                        let number = member(call, children, "Number")?;
                        call.with_roots(&[Value::Table(number)], |call| {
                            for (ordinal, name, thousand) in [
                                (1, "OneHundredMilestone", false),
                                (2, "OneThousandMilestone", true),
                            ] {
                                let definition = load_combo(call, thousand)?;
                                let child = own(call, data, definition)?;
                                let name_value = call.try_intern(name.as_bytes())?;
                                set(call, child, "Name", name_value)?;
                                owned_lookup(call, child)?;
                                initialize(call, child)?;
                                // Init runs while the milestone has no Combo parent.
                                parent(call, child, combo)?;
                                call.raw_table_set(
                                    combo,
                                    Value::Number(ordinal as f64),
                                    Value::Table(child),
                                )?;
                                set(call, children, name, Value::Table(child))?;
                            }
                            call.raw_table_set(combo, Value::Number(3.), Value::Table(number))?;
                            let mut declared = Vec::new();
                            for index in 1.. {
                                match call.raw_table_get(combo, Value::Number(index as f64))? {
                                    Value::Nil => break,
                                    Value::Table(child) => declared.push(child),
                                    _ => {}
                                }
                            }
                            call.store().actors.get_mut(&combo).unwrap().children = declared;
                            Ok(())
                        })
                    })
                })
            })?;
        }
        Ok(())
    })
}

fn load_combo(call: &mut HostCall<'_>, thousand: bool) -> Result<Value, Error> {
    let globals = helper(call, 5)?;
    let environment = source::environment(call, globals, false)?;
    call.with_roots(&[environment], |call| {
        let loader = call.new_host_function(combo_resource, call.captures().to_vec().as_slice())?;
        set(call, table(call, environment)?, "LoadActor", loader)?;
        source::evaluate(
            call,
            if thousand {
                source::THOUSAND
            } else {
                source::HUNDRED
            },
            environment,
            &[],
        )
    })
}

fn combo_resource(call: &mut HostCall<'_>) -> Result<(), Error> {
    let path = assets::text(call, arg(call, 0))?;
    if path == "../Combo 100milestone" {
        let value = load_combo(call, false)?;
        return call.push(value);
    }
    let key = if path == "explosion.png" {
        "Combo 100milestone/explosion.png"
    } else {
        path.strip_prefix(crate::SONG_LUA_THEME_PATH_PREFIX)
            .and_then(|p| p.strip_prefix("G/"))
            .unwrap_or(&path)
    };
    let path = gameplay_bitmaps::load(key).map_err(|error| call.error(error.as_bytes()))?;
    let definition = call.try_new_table()?;
    call.with_roots(&[Value::Table(definition)], |call| {
        let path = call.try_intern(path.as_bytes())?;
        set(call, definition, "Texture", path)?;
        let globals = table(call, helper(call, 5)?)?;
        let def = member(call, globals, "Def")?;
        let function = field(call, def, "Sprite")?;
        call.call_and_return(function, &[Value::Table(definition)])
    })
}

fn transitions(call: &mut HostCall<'_>, data: &Load) -> Result<(), Error> {
    let globals = table(call, helper(call, 5)?)?;
    let top = get_static(call, globals, b"__songlua_top_screen")?;
    let top = table(call, top)?;
    native_children::with_children(call, top, |call, wrapper| {
        let role = get_static(call, wrapper, b"__songlua_screen_actor_role")?;
        if role == Value::Nil {
            return Ok(());
        }
        let source = match assets::text(call, role)?.as_str() {
            "Toasty" => source::TOASTY,
            "NextSong" => source::NEXT_SONG,
            "Ready" | "Go" | "Failed" => source::BLANK,
            _ => return Ok(()),
        };
        let definition = source::evaluate(call, source, Value::Table(globals), &[])?;
        let child = own(call, data, definition)?;
        set(
            call,
            wrapper,
            "__songlua_transition_child",
            Value::Table(child),
        )?;
        parent(call, child, wrapper)?;
        let mut found = false;
        for i in 1..=call.raw_table_len(wrapper)? {
            found |= call.raw_table_get(wrapper, Value::Number(i as f64))? == Value::Table(child);
        }
        if !found {
            call.raw_table_set(
                wrapper,
                Value::Number((call.raw_table_len(wrapper)? + 1) as f64),
                Value::Table(child),
            )?;
            call.store()
                .actors
                .get_mut(&wrapper)
                .unwrap()
                .children
                .push(child);
        }
        let registry = native_children::helper_function(call, b"native_registry")?;
        let registry = call.call(registry, &[Value::Table(wrapper)])?[0];
        call.with_roots(&[registry], |call| {
            set(call, table(call, registry)?, "", Value::Table(child))
        })?;
        let visible = field(call, wrapper, "visible")?;
        call.call(visible, &[Value::Table(wrapper), Value::Boolean(true)])?;
        owned_lookup(call, wrapper)?;
        initialize(call, child)
    })
}

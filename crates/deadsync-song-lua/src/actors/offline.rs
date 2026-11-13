//! Existing offline Def constructors: in-place instances before Init.
use super::*;

const CLASSES: &[&str] = &[
    "Actor",
    "ActorFrame",
    "Sprite",
    "Banner",
    "ActorMultiVertex",
    "Sound",
    "BitmapText",
    "BPMDisplay",
    "RollingNumbers",
    "GraphDisplay",
    "SongMeterDisplay",
    "CourseContentsList",
    "DeviceList",
    "InputList",
    "Model",
    "Quad",
    "ActorProxy",
    "ActorFrameTexture",
];

pub(super) fn install(call: &mut HostCall<'_>) -> Result<(), Error> {
    let def = Value::Table(call.try_new_table()?);
    call.with_roots(&[def], |call| {
        let captures = call.captures().to_vec();
        for &class in CLASSES {
            let function = call.new_host_function_with_data(
                callback,
                &captures,
                Op::OfflineConstructor(class),
            )?;
            let key = call.try_intern(class.as_bytes())?;
            call.raw_table_set(table(call, def)?, key, function)?;
        }
        let globals = table(call, helper(call, 5)?)?;
        sprites::set(
            call,
            globals,
            "__songlua_production_loaders",
            Value::Boolean(true),
        )?;
        sprites::set(call, globals, "Def", def)?;
        let loader =
            call.new_host_function_with_data(callback, &captures, Op::LoadPreparationActor)?;
        sprites::set(call, globals, "LoadActor", loader)?;
        let null = get_static(call, globals, b"NullActor")?;
        let null = table(call, null)?;
        sprites::set(call, null, "__songlua_def_owned", Value::Boolean(true))?;
        let guard = call.store();
        guard.offline_definitions = true;
        let null = guard.actors.get_mut(&null).expect("owned NullActor");
        null.offline_instance = true;
        null.capture.get_or_insert_with(capture::Capture::default);
        Ok(())
    })
}

pub(super) fn construct(call: &mut HostCall<'_>, class: &'static str) -> Result<(), Error> {
    let supplied = match arg(call, 0) {
        Value::Table(actor) => Some(actor),
        _ => None,
    };
    let actor = create(call, class, supplied, false)?;
    call.push(Value::Table(actor))
}

pub(super) fn dummy(call: &mut HostCall<'_>, class: &'static str) -> Result<TableId, Error> {
    create(call, class, None, true)
}

fn create(
    call: &mut HostCall<'_>,
    class: &'static str,
    supplied: Option<TableId>,
    dummy: bool,
) -> Result<TableId, Error> {
    let actor = match supplied {
        Some(actor) => actor,
        None => call.try_new_table()?,
    };
    call.with_roots(&[Value::Table(actor)], |call| {
        let existing = call.store().actors.get(&actor).map(|a| {
            (
                a.scene,
                TemplateState::from(a),
                a.initialized,
                a.registered,
                a.parent,
            )
        });
        let scene = if let Some((scene, ..)) = &existing {
            *scene
        } else {
            new_scene(call)?
        };
        if existing.is_none() {
            call.store().constructor_scenes.insert(scene);
        }
        let kind = call.try_intern(class.as_bytes())?;
        call.call(
            helper(call, 1)?,
            &[
                kind,
                Value::Nil,
                Value::Table(actor),
                Value::Boolean(false),
                Value::Boolean(true),
            ],
        )?;
        let globals = table(call, helper(call, 5)?)?;
        for key in ["__songlua_script_dir", "__songlua_song_dir"] {
            let current = get(call, actor, key.as_bytes())?;
            let value = if key == "__songlua_script_dir" && current != Value::Nil {
                current
            } else {
                get(call, globals, key.as_bytes())?
            };
            if value != Value::Nil {
                sprites::set(call, actor, key, value)?;
            }
        }
        let name = get_static(call, actor, b"Name")?;
        let load_name = if matches!(name, Value::Number(_) | Value::String(_)) {
            Some(bytes(call, name)?)
        } else {
            None
        };
        let mut instance = Actor::new(scene, class, None, load_name);
        instance.offline_instance = true;
        instance.pending_init = true;
        instance.registered = dummy;
        instance.capture = call
            .store()
            .constructing_capture
            .then(capture::Capture::default);
        if let Some((_, previous, initialized, registered, parent)) = existing {
            restore_pose(&mut instance, &previous);
            instance.state.name = previous.state.name;
            instance.initialized = initialized;
            instance.pending_init = !initialized;
            instance.registered = registered;
            instance.parent = parent;
        }
        let registry = table(call, call.captures()[0])?;
        retirement::retain(call, scene, actor, registry)?;
        {
            let guard = call.store();
            let members = &mut guard.scenes.get_mut(&scene).unwrap().actors;
            if !members.contains(&actor) {
                members.push(actor);
            }
            guard
                .scenes
                .get_mut(&scene)
                .unwrap()
                .root
                .get_or_insert(actor);
            guard.actors.insert(actor, instance);
            if dummy {
                guard.actor_order.push(actor);
            }
        }
        if matches!(class, "Quad" | "ActorMultiVertex") {
            sprites::set(
                call,
                actor,
                "__songlua_default_texture",
                Value::Boolean(true),
            )?;
        }
        initialize_resources(
            call,
            scene,
            actor,
            class,
            if dummy {
                ResourceMode::Dummy
            } else {
                ResourceMode::Constructor
            },
        )?;
        call.call(helper(call, 13)?, &[Value::Table(actor)])?;
        own_children(call, actor)?;
        Ok(actor)
    })
}

fn restore_pose(actor: &mut Actor, previous: &TemplateState) {
    actor.current = previous.pose;
    actor.hibernate = previous.hibernate;
    actor.update_rate = previous.update_rate;
    actor.propagate = previous.propagate;
    actor.update_error_reported = previous.update_error_reported;
    actor.state.size = previous.state.size;
    actor.state.visible = previous.state.visible;
    actor.state.draw_order = previous.state.draw_order;
    actor.state.render = previous.state.render;
    actor.base_transform = previous.base_transform;
    actor.pose_exported = previous.pose_exported;
    actor.state.rotation = previous.state.rotation;
    actor.effects = previous.effects;
    actor.preparation_getters = previous.preparation_getters;
}

fn already_owned(call: &mut HostCall<'_>, actor: TableId) -> Result<bool, Error> {
    Ok(!matches!(
        get_static(call, actor, b"__songlua_def_owned")?,
        Value::Nil | Value::Boolean(false)
    ))
}

// Native cloning copies Lua state and rebinds methods; it does not rerun the
// resource constructor. Concatenation additionally leaves child identities shared.
pub(super) fn clone_actor(
    call: &mut HostCall<'_>,
    source: TableId,
    own: bool,
) -> Result<TableId, Error> {
    let previous = TemplateState::from(&call.store().actors[&source]);
    let value = call.call(helper(call, 12)?, &[Value::Table(source)])?[0];
    call.with_roots(&[value], |call| {
        let actor = table(call, value)?;
        let scene = new_scene(call)?;
        let class = previous.state.class;
        let kind = call.try_intern(class.as_bytes())?;
        call.call(
            helper(call, 1)?,
            &[
                kind,
                Value::Nil,
                value,
                Value::Boolean(true),
                Value::Boolean(true),
            ],
        )?;
        let name = get_static(call, actor, b"Name")?;
        let name = if matches!(name, Value::String(_) | Value::Number(_)) {
            Some(bytes(call, name)?)
        } else {
            None
        };
        let mut instance = Actor::new(scene, class, None, name);
        restore_pose(&mut instance, &previous);
        instance.offline_instance = true;
        instance.pending_init = true;
        instance.registered = false;
        instance.capture = previous.capture;
        let registry = table(call, call.captures()[0])?;
        retirement::retain(call, scene, actor, registry)?;
        {
            let guard = call.store();
            guard.constructor_scenes.insert(scene);
            let owner = guard.scenes.get_mut(&scene).unwrap();
            owner.root = Some(actor);
            owner.actors.push(actor);
            guard.actors.insert(actor, instance);
        }
        initialize_resources(call, scene, actor, class, ResourceMode::OfflineClone)?;
        call.call(helper(call, 13)?, &[value])?;
        if own {
            own_children(call, actor)?;
        } else {
            share_children(call, actor)?;
        }
        Ok(actor)
    })
}

fn share_children(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    let scene = call.store().actors[&actor].scene;
    let mut declared = Vec::new();
    for i in 1.. {
        match call.raw_table_get(actor, Value::Number(i as f64))? {
            Value::Nil => break,
            Value::Table(child) => {
                courses::attach(call, scene, child)?;
                declared.push(child);
            }
            _ => {}
        }
    }
    call.store().actors.get_mut(&actor).unwrap().children = declared;
    for child in children(call, actor)? {
        courses::attach(call, scene, child)?;
    }
    Ok(())
}

// Transfer the implicit constructor owner after claiming its root. The native
// host keeps abandoned constructor objects until VM teardown; an explicitly
// owned scene instead supplies the existing release boundary for its subtree.
fn transfer(call: &mut HostCall<'_>, scene: u64, actor: TableId) -> Result<(), Error> {
    courses::attach(call, scene, actor)?;
    let old = {
        let guard = call.store();
        guard
            .constructor_scenes
            .iter()
            .copied()
            .filter(|id| *id != scene && guard.scenes[id].root == Some(actor))
            .collect::<Vec<_>>()
    };
    let guard = call.store();
    for id in old {
        guard.constructor_scenes.remove(&id);
        let previous = guard.scenes.remove(&id).unwrap();
        for member in previous.actors {
            if !guard.scenes[&scene].actors.contains(&member) {
                guard.scenes.get_mut(&scene).unwrap().actors.push(member);
            }
            if guard.actors[&member].scene == id {
                guard.actors.get_mut(&member).unwrap().scene = scene;
            }
        }
    }
    Ok(())
}

fn own_children(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    let scene = call.store().actors[&actor].scene;
    let values = call
        .call(helper(call, 11)?, &[Value::Table(actor)])?
        .to_vec();
    call.with_roots(&values, |call| {
        let source = table(call, values[0])?;
        let entries = table(call, values[1])?;
        for i in 1..=call.raw_table_len(entries)? {
            let entry = table(call, call.raw_table_get(entries, Value::Number(i as f64))?)?;
            let key = call.raw_table_get(entry, Value::Number(1.))?;
            let child = table(call, call.raw_table_get(entry, Value::Number(2.))?)?;
            let typed = get_static(call, child, b"__songlua_actor_type")? != Value::Nil;
            let child = if typed && already_owned(call, child)? {
                clone_actor(call, child, true)?
            } else {
                child
            };
            if typed {
                sprites::set(call, child, "__songlua_def_owned", Value::Boolean(true))?;
                call.raw_table_set(source, key, Value::Table(child))?;
            }
            transfer(call, scene, child)?;
        }
        let mut children = Vec::new();
        for i in 1.. {
            match call.raw_table_get(actor, Value::Number(i as f64))? {
                Value::Nil => break,
                Value::Table(child) => {
                    transfer(call, scene, child)?;
                    children.push(child);
                }
                _ => {}
            }
        }
        call.store().actors.get_mut(&actor).unwrap().children = children;
        // Native children (for example a supplied SongMeter Stream) also
        // leave their temporary constructor owner when this instance claims them.
        for child in super::children(call, actor)? {
            transfer(call, scene, child)?;
        }
        Ok(())
    })
}

pub(super) fn own_root(call: &mut HostCall<'_>, actor: TableId, init: bool) -> Result<u64, Error> {
    let actor = if already_owned(call, actor)? {
        clone_actor(call, actor, true)?
    } else {
        actor
    };
    let existing = call.store().actors[&actor].scene;
    let scene = if call.store().scenes[&existing].root == Some(actor) {
        existing
    } else {
        let scene = new_scene(call)?;
        transfer(call, scene, actor)?;
        call.store().scenes.get_mut(&scene).unwrap().root = Some(actor);
        scene
    };
    call.store().constructor_scenes.remove(&scene);
    sprites::set(call, actor, "__songlua_def_owned", Value::Boolean(true))?;
    if init {
        super::prepare_init(call.store());
        if let Err(error) = initialize(call, actor) {
            release(call, scene)?;
            return Err(error);
        }
    }
    Ok(scene)
}

pub(super) fn prepare_init(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    let initialized = !matches!(
        get_static(call, actor, b"__songlua_init_commands_ran")?,
        Value::Nil | Value::Boolean(false)
    );
    call.store().actors.get_mut(&actor).unwrap().initialized = initialized;
    if initialized {
        return Ok(());
    }
    courses::refresh_raw_children(call, actor, true)?;
    let name = get_static(call, actor, b"Name")?;
    let name = if matches!(name, Value::Number(_) | Value::String(_)) {
        Some(bytes(call, name)?)
    } else {
        None
    };
    call.store().actors.get_mut(&actor).unwrap().load_name = name;
    let empty = call.try_intern(b"")?;
    for key in ["__songlua_native_name", "__songlua_load_name"] {
        sprites::set(call, actor, key, empty)?;
    }
    Ok(())
}

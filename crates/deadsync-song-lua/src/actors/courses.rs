//! CourseContentsList display ownership, scroller fields and SetSong callbacks.
use super::*;
use sprites::{field, set};

pub(super) const METHODS: &[&str] = &[
    "Load",
    "SetFromGameState",
    "SetCurrentAndDestinationItem",
    "SetCurrentItem",
    "GetCurrentItem",
    "SetDestinationItem",
    "GetDestinationItem",
    "GetNumItems",
    "SetTransformFromFunction",
    "SetTransformFromHeight",
    "SetTransformFromWidth",
    "PositionItems",
    "GetSecondsToDestination",
    "SetSecondsPerItem",
    "SetLoop",
    "GetLoop",
    "SetPauseCountdownSeconds",
    "SetSecondsPauseBetweenItems",
    "GetSecondsPauseBetweenItems",
    "SetNumSubdivisions",
    "ScrollThroughAllItems",
    "ScrollWithPadding",
    "SetFastCatchup",
    "SetWrap",
    "SetMask",
    "SetNumItemsToDraw",
    "GetFullScrollLengthSeconds",
];

pub(super) fn install(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    let methods = native_children::helper_function(call, b"course_methods")?;
    let methods = table(call, methods)?;
    let bound = get(call, actor, b"__songlua_bound_methods")?;
    let bound = table(call, bound)?;
    for &name in METHODS {
        let method = get(call, methods, name.as_bytes())?;
        let captures = [
            call.captures()[0],
            call.captures()[1],
            Value::Table(actor),
            method,
        ];
        let function = call.new_host_function_with_data(invoke_bound, &captures, name)?;
        set(call, bound, name, function)?;
    }
    Ok(())
}

pub(super) fn initialize(
    call: &mut HostCall<'_>,
    store: &Shared,
    scene: u64,
    actor: TableId,
) -> Result<(), Error> {
    for (key, value) in [
        ("current_item", 0.),
        ("destination_item", 0.),
        ("num_items", 1.),
    ] {
        put(call, actor, key, Value::Number(value))?;
    }
    let display = field(call, actor, "Display")?;
    if display == Value::Nil {
        return Ok(());
    }
    let display = table(call, display)?;
    let name = field(call, display, "Name")?;
    if name == Value::Nil {
        let name = call.try_intern(b"Display")?;
        set(call, display, "Name", name)?;
    } else {
        assets::text(call, name)?;
    }
    native_children::adopt(call, store, display, actor)?;
    if store.lock().unwrap().actors[&actor].offline_instance
        || get(call, display, b"Condition")? != Value::Boolean(false)
    {
        attach(call, store, scene, display)?;
        store
            .lock()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap()
            .children
            .push(display);
        let len = call.raw_table_len(actor)?;
        let mut present = false;
        for i in 1..=len {
            present |= call.raw_table_get(actor, Value::Number(i as f64))? == Value::Table(display);
        }
        if !present {
            call.raw_table_set(
                actor,
                Value::Number((len + 1) as f64),
                Value::Table(display),
            )?;
        }
    }
    Ok(())
}

// The production constructor appends Display itself, including a plain Def
// table. Track it without adding instance methods or replacing its metatable.
pub(super) fn attach(
    call: &mut HostCall<'_>,
    store: &Shared,
    scene: u64,
    actor: TableId,
) -> Result<(), Error> {
    {
        let guard = store.lock().unwrap();
        let actors = &guard.scenes[&scene].actors;
        if actors.contains(&actor) {
            return Ok(());
        }
    }
    if store.lock().unwrap().actors.contains_key(&actor) {
        if store.lock().unwrap().retired_scenes.contains_key(&scene) {
            let registry = table(call, call.captures()[0])?;
            retirement::retain(call, store, scene, actor, registry)?;
        }
        store
            .lock()
            .unwrap()
            .scenes
            .get_mut(&scene)
            .unwrap()
            .actors
            .push(actor);
        if !store.lock().unwrap().actors[&actor].initialized {
            let name = get(call, actor, b"Name")?;
            if matches!(name, Value::Number(_) | Value::String(_)) {
                let name = bytes(call, name)?;
                store
                    .lock()
                    .unwrap()
                    .actors
                    .get_mut(&actor)
                    .unwrap()
                    .load_name = Some(name);
            }
        }
        let mut descendants = children(call, store, actor)?;
        descendants.extend(
            store.lock().unwrap().actors[&actor]
                .wrappers
                .iter()
                .copied(),
        );
        for child in descendants {
            attach(call, store, scene, child)?;
        }
        return Ok(());
    }
    let name = get(call, actor, b"Name")?;
    let name = if matches!(name, Value::Number(_) | Value::String(_)) {
        Some(bytes(call, name)?)
    } else {
        None
    };
    let mut instance = Actor::new(scene, "Actor", None, name);
    instance.raw_instance = true;
    instance.initialized = boolean(call, actor, "__songlua_init_commands_ran")?;
    instance.started = boolean(call, actor, "__songlua_startup_commands_ran")?;
    if instance.initialized {
        let name = get(call, actor, b"__songlua_native_name")?;
        if matches!(name, Value::Number(_) | Value::String(_)) {
            instance.state.name = bytes(call, name)?;
        }
    }
    instance.registered = instance.initialized;
    let registry = table(call, call.captures()[0])?;
    retirement::retain(call, store, scene, actor, registry)?;
    store.lock().unwrap().actors.insert(actor, instance);
    store
        .lock()
        .unwrap()
        .scenes
        .get_mut(&scene)
        .unwrap()
        .actors
        .push(actor);
    refresh_raw_children(call, store, actor, false)
}

pub(super) fn refresh_raw_children(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    compact: bool,
) -> Result<(), Error> {
    let selected = call.call(helper(call, 10)?, &[Value::Table(actor)])?[0];
    call.with_roots(&[selected], |call| {
        let selected = table(call, selected)?;
        let owners: Vec<_> = store
            .lock()
            .unwrap()
            .scenes
            .iter()
            .filter_map(|(id, scene)| scene.actors.contains(&actor).then_some(*id))
            .collect();
        let mut children = Vec::new();
        for i in 1..=call.raw_table_len(selected)? {
            let child = table(call, call.raw_table_get(selected, Value::Number(i as f64))?)?;
            for &scene in &owners {
                attach(call, store, scene, child)?;
            }
            children.push(child);
        }
        if compact {
            let entries = call.call(helper(call, 7)?, &[Value::Table(actor)])?[0];
            call.with_roots(&[entries], |call| {
                let entries = table(call, entries)?;
                for i in 1..=call.raw_table_len(entries)? {
                    let pair = table(call, call.raw_table_get(entries, Value::Number(i as f64))?)?;
                    let key = call.raw_table_get(pair, Value::Number(1.))?;
                    if matches!(key, Value::Number(n) if n >= 1. && n.fract()==0.) {
                        call.raw_table_set(actor, key, Value::Nil)?;
                    }
                }
                Ok(())
            })?;
            for (i, child) in children.iter().enumerate() {
                call.raw_table_set(actor, Value::Number((i + 1) as f64), Value::Table(*child))?;
            }
        }
        store
            .lock()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap()
            .children = children;
        Ok(())
    })
}

pub(super) fn prepare_raw_init(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
) -> Result<(), Error> {
    if !store.lock().unwrap().actors[&actor].raw_instance {
        return Ok(());
    }
    let initialized = boolean(call, actor, "__songlua_init_commands_ran")?;
    store
        .lock()
        .unwrap()
        .actors
        .get_mut(&actor)
        .unwrap()
        .initialized = initialized;
    if initialized {
        return Ok(());
    }
    refresh_raw_children(call, store, actor, true)?;
    let name = get(call, actor, b"Name")?;
    let name = if matches!(name, Value::Number(_) | Value::String(_)) {
        Some(bytes(call, name)?)
    } else {
        None
    };
    store
        .lock()
        .unwrap()
        .actors
        .get_mut(&actor)
        .unwrap()
        .load_name = name;
    let empty = call.try_intern(b"")?;
    for name in [b"__songlua_native_name".as_slice(), b"__songlua_load_name"] {
        let key = call.try_intern(name)?;
        call.raw_table_set(actor, key, empty)?;
    }
    Ok(())
}

pub(super) fn share_child(
    call: &mut HostCall<'_>,
    store: &Shared,
    owner: TableId,
    child: TableId,
) -> Result<(), Error> {
    let owners: Vec<_> = store
        .lock()
        .unwrap()
        .scenes
        .iter()
        .filter_map(|(id, scene)| scene.actors.contains(&owner).then_some(*id))
        .collect();
    for scene in owners {
        attach(call, store, scene, child)?;
    }
    Ok(())
}

pub(super) fn publish_raw_name(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    original: Value,
) -> Result<(), Error> {
    let name = {
        let guard = store.lock().unwrap();
        let a = &guard.actors[&actor];
        (a.raw_instance || a.offline_instance).then(|| a.state.name.clone())
    };
    if let Some(name) = name {
        let value = if original == Value::Nil {
            call.try_intern(&name)?
        } else {
            original
        };
        let key = call.try_intern(b"__songlua_native_name")?;
        call.raw_table_set(actor, key, value)?;
        if original != Value::Nil {
            let key = call.try_intern(b"Name")?;
            call.raw_table_set(actor, key, original)?;
        }
        let key = call.try_intern(b"__songlua_load_name")?;
        call.raw_table_set(actor, key, Value::Nil)?;
    }
    Ok(())
}
pub(super) fn publish_raw_flag(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    key: &str,
) -> Result<(), Error> {
    let publish = {
        let guard = store.lock().unwrap();
        let a = &guard.actors[&actor];
        a.raw_instance || a.offline_instance
    };
    if publish {
        set(call, actor, key, Value::Boolean(true))?;
    }
    Ok(())
}
fn boolean(call: &mut HostCall<'_>, actor: TableId, key: &str) -> Result<bool, Error> {
    // Lua's bool conversion uses truthiness; nil is the optional default.
    Ok(!matches!(
        field(call, actor, key)?,
        Value::Nil | Value::Boolean(false)
    ))
}
fn put(call: &mut HostCall<'_>, actor: TableId, key: &str, value: Value) -> Result<(), Error> {
    set(call, actor, &format!("__songlua_scroller_{key}"), value)
}
fn float(call: &mut HostCall<'_>, actor: TableId, key: &str, default: f32) -> Result<f32, Error> {
    let value = field(call, actor, &format!("__songlua_scroller_{key}"))?;
    if value == Value::Nil {
        Ok(default)
    } else {
        Ok(sprites::number(call, value)? as f32)
    }
}
fn count(call: &mut HostCall<'_>, actor: TableId) -> Result<i64, Error> {
    let value = field(call, actor, "__songlua_scroller_num_items")?;
    if value == Value::Nil {
        return Ok(1);
    }
    let n = sprites::number(call, value)?;
    if !(i64::MIN as f64..9223372036854775808.0).contains(&n) {
        return Err(call.error(b"scroller item count out of range"));
    }
    Ok(n as i64)
}
fn function(call: &HostCall<'_>, value: Value) -> Result<Option<Value>, Error> {
    match value {
        Value::Nil => Ok(None),
        Value::Function(_) | Value::Native(_) => Ok(Some(value)),
        _ => Err(call.error(b"function expected")),
    }
}
fn display(call: &mut HostCall<'_>, actor: TableId) -> Result<Option<TableId>, Error> {
    let named = native_children::named_children(call, actor)?;
    call.with_roots(&[named], |call| {
        let value = field(call, table(call, named)?, "Display")?;
        if value == Value::Nil {
            Ok(None)
        } else {
            Ok(Some(table(call, value)?))
        }
    })
}
fn position(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    let Some(display) = display(call, actor)? else {
        return Ok(());
    };
    call.with_roots(&[Value::Table(display)], |call| {
        let count = count(call, actor)?.max(1);
        let transform = field(call, actor, "__songlua_scroller_transform_function")?;
        if let Some(transform) = function(call, transform)? {
            call.call(
                transform,
                &[
                    Value::Table(display),
                    Value::Number(0.),
                    Value::Number(0.),
                    Value::Number(count as f64),
                ],
            )?;
        } else {
            for (key, axis) in [("item_height", "Y"), ("item_width", "X")] {
                let value = field(call, actor, &format!("__songlua_scroller_{key}"))?;
                if value != Value::Nil {
                    let offset = 0.0_f32 * sprites::number(call, value)? as f32;
                    set(call, display, axis, Value::Number(offset as f64))?;
                    break;
                }
            }
        }
        Ok(())
    })
}
fn call_method(
    call: &mut HostCall<'_>,
    object: TableId,
    name: &str,
    extra: Option<Value>,
) -> Result<Value, Error> {
    call.with_roots(
        &[Value::Table(object), extra.unwrap_or(Value::Nil)],
        |call| {
            let method = field(call, object, name)?;
            let Some(method) = function(call, method)? else {
                return Ok(Value::Nil);
            };
            let args = [Value::Table(object), extra.unwrap_or(Value::Nil)];
            Ok(call
                .call(method, &args[..if extra.is_some() { 2 } else { 1 }])?
                .first()
                .copied()
                .unwrap_or(Value::Nil))
        },
    )
}
fn gamestate(call: &mut HostCall<'_>, name: &str, extra: Option<Value>) -> Result<Value, Error> {
    let globals = table(call, helper(call, 5)?)?;
    let value = field(call, globals, "GAMESTATE")?;
    if value == Value::Nil {
        Ok(Value::Nil)
    } else {
        call_method(call, table(call, value)?, name, extra)
    }
}
fn set_song(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    params: Value,
    leaf: bool,
) -> Result<(), Error> {
    if !store.lock().unwrap().actors.contains_key(&actor) {
        if !leaf && let Some((command, _)) = command(call, actor, b"SetSong")? {
            run_actor_function(call, store, actor, command, Some(params))?;
        }
        if leaf {
            for child in native_children::group_members(call, actor)? {
                set_song(call, store, child, params, true)?;
            }
        }
    } else if leaf {
        leaves(call, store, actor, &mut |call, child| {
            run_named(call, store, child, b"SetSong", Some(params))
        })?;
    } else {
        run_named(call, store, actor, b"SetSong", Some(params))?;
    }
    Ok(())
}
fn populate(call: &mut HostCall<'_>, store: &Shared, actor: TableId) -> Result<(), Error> {
    let Some(display) = display(call, actor)? else {
        return Ok(());
    };
    call.with_roots(&[Value::Table(display)], |call| {
        for player in [b"PlayerNumber_P1", b"PlayerNumber_P2"] {
            let params = call.try_new_table()?;
            call.with_roots(&[Value::Table(params)], |call| {
                set(call, params, "Number", Value::Number(0.))?;
                let player = call.try_intern(player)?;
                set(call, params, "PlayerNumber", player)?;
                let song = gamestate(call, "GetCurrentSong", None)?;
                set(call, params, "Song", song)?;
                let steps = gamestate(call, "GetCurrentSteps", Some(player))?;
                set(call, params, "Steps", steps)?;
                for (name, method, default) in [
                    ("Meter", "GetMeter", "?"),
                    ("Difficulty", "GetDifficulty", "Difficulty_Medium"),
                ] {
                    let value = if let Value::Table(steps) = steps {
                        call_method(call, steps, method, None)?
                    } else {
                        Value::Nil
                    };
                    let value = if value == Value::Nil {
                        call.try_intern(default.as_bytes())?
                    } else {
                        value
                    };
                    set(call, params, name, value)?;
                }
                let params = Value::Table(params);
                set_song(call, store, display, params, false)?;
                set_song(call, store, display, params, true)
            })?;
        }
        Ok(())
    })
}

pub(super) fn method(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    name: &str,
) -> Result<(), Error> {
    let value = arg(call, 1);
    match name {
        "Load" => return sprites::method(call, store, actor, name),
        "SetFromGameState" => {
            if assets::field(call, actor, b"__songlua_actor_type")?
                .is_some_and(|s| s.eq_ignore_ascii_case("CourseContentsList"))
            {
                set(
                    call,
                    actor,
                    "__songlua_course_contents_from_gamestate",
                    Value::Boolean(true),
                )?;
                put(call, actor, "num_items", Value::Number(1.))?;
                populate(call, store, actor)?;
            }
        }
        "PositionItems" => position(call, actor)?,
        "GetCurrentItem" | "GetDestinationItem" | "GetSecondsPauseBetweenItems" => {
            let key = match name {
                "GetCurrentItem" => "current_item",
                "GetDestinationItem" => "destination_item",
                _ => "pause_between",
            };
            let value = float(call, actor, key, 0.)?;
            return call.push(Value::Number(value as f64));
        }
        "GetNumItems" => {
            let n = count(call, actor)?.max(0);
            return call.push(Value::Number(n as f64));
        }
        "GetLoop" => {
            let value = boolean(call, actor, "__songlua_scroller_loop")?;
            return call.push(Value::Boolean(value));
        }
        "GetSecondsToDestination" => {
            let current = float(call, actor, "current_item", 0.)?;
            let destination = float(call, actor, "destination_item", current)?;
            let seconds = float(call, actor, "seconds_per_item", 0.)?;
            return call.push(Value::Number(
                ((destination - current).abs() * seconds) as f64,
            ));
        }
        "GetFullScrollLengthSeconds" => {
            let count = count(call, actor)?.max(0) as f32;
            let seconds = float(call, actor, "seconds_per_item", 0.)?;
            let pause = float(call, actor, "pause_between", 0.)?;
            return call.push(Value::Number(
                (count - 1.).max(0.).mul_add(pause, count * seconds) as f64,
            ));
        }
        "SetTransformFromFunction" => {
            if matches!(value, Value::Function(_) | Value::Native(_)) {
                put(call, actor, "transform_function", value)?;
            }
        }
        "SetLoop" | "SetFastCatchup" | "SetWrap" | "SetMask" => {
            let key = match name {
                "SetLoop" => "loop",
                "SetFastCatchup" => "fast_catchup",
                "SetWrap" => "wrap",
                _ => "mask",
            };
            if name != "SetLoop" || call.arguments().len() > 1 {
                put(
                    call,
                    actor,
                    key,
                    Value::Boolean(!matches!(value, Value::Nil | Value::Boolean(false))),
                )?;
            }
        }
        "SetNumSubdivisions" | "SetNumItemsToDraw" => {
            if let Some(n) = integer_scalar(call, value) {
                put(
                    call,
                    actor,
                    if name == "SetNumSubdivisions" {
                        "num_subdivisions"
                    } else {
                        "num_items_to_draw"
                    },
                    Value::Number(n.max(0) as f64),
                )?;
            }
        }
        "ScrollThroughAllItems" | "ScrollWithPadding" => {
            let before = float_scalar(call, value, false).unwrap_or(0.).max(0.);
            let after = float_scalar(call, arg(call, 2), false)
                .unwrap_or(0.)
                .max(0.);
            let last = count(call, actor)?.saturating_sub(1).max(0) as f32;
            let destination = if name == "ScrollWithPadding" {
                put(call, actor, "current_item", Value::Number(-before as f64))?;
                last + after
            } else {
                last
            };
            put(
                call,
                actor,
                "destination_item",
                Value::Number(destination as f64),
            )?;
        }
        _ => {
            if let Some(value) = float_scalar(call, value, false) {
                let (key, clamp) = match name {
                    "SetCurrentAndDestinationItem" | "SetCurrentItem" => ("current_item", false),
                    "SetDestinationItem" => ("destination_item", true),
                    "SetTransformFromHeight" => ("item_height", false),
                    "SetTransformFromWidth" => ("item_width", false),
                    "SetSecondsPerItem" => ("seconds_per_item", true),
                    "SetPauseCountdownSeconds" => ("pause_countdown", true),
                    "SetSecondsPauseBetweenItems" => ("pause_between", true),
                    _ => unreachable!(),
                };
                let value = Value::Number(if clamp { value.max(0.) } else { value } as f64);
                put(call, actor, key, value)?;
                if name == "SetCurrentAndDestinationItem" {
                    put(call, actor, "destination_item", value)?;
                }
            }
        }
    }
    call.push(Value::Table(actor))
}

//! Original overlay logic and screen decorations, in native replacement order.
use super::*;
use underlay::{command, named, registry};

const ENVIRONMENT: &[u8] = b"__songlua_overlay_environment";

fn resource(call: &mut HostCall<'_>) -> Result<(), Error> {
    let path = assets::text(call, arg(call, 0))
        .map_err(|_| call.error(b"native LoadActor requires a path"))?;
    let globals = table(call, helper(call, 5)?)?;
    let environment = get(call, globals, ENVIRONMENT)?;
    table(call, environment)?;
    let arguments = call.arguments().iter().skip(1).copied().collect::<Vec<_>>();
    let value = evaluate(call, environment, &path, &arguments)?;
    call.push(value)
}

fn evaluate(
    call: &mut HostCall<'_>,
    environment: Value,
    path: &str,
    arguments: &[Value],
) -> Result<Value, Error> {
    let name = path.strip_prefix("./").unwrap_or(path);
    let source = source::overlay(name)
        .ok_or_else(|| call.error(format!("unknown native Overlay resource {path}").as_bytes()))?;
    source::evaluate(call, source, environment, arguments)
}

fn order(call: &mut HostCall<'_>, actor: TableId) -> Result<i32, Error> {
    let value = get(call, actor, b"__songlua_state_draw_order")?;
    if value == Value::Nil {
        return Ok(0);
    }
    let value = sprites::number(call, value)?.trunc();
    if !(i32::MIN as f64..=i32::MAX as f64).contains(&value) {
        return Err(call.error(b"draw order out of range"));
    }
    Ok(value as i32)
}

fn replace(
    call: &mut HostCall<'_>,
    data: &Load,
    screen: TableId,
    previous: TableId,
    actor: TableId,
    name: &str,
) -> Result<(), Error> {
    for index in 1..=call.raw_table_len(screen)? {
        let key = Value::Number(index as f64);
        if call.raw_table_get(screen, key)? == Value::Table(previous) {
            call.raw_table_set(screen, key, Value::Table(actor))?;
        }
    }
    let children = registry(call, screen)?;
    call.with_roots(&[children], |call| {
        set(call, table(call, children)?, name, Value::Table(actor))
    })?;
    for child in &mut data
        .store
        .lock()
        .unwrap()
        .actors
        .get_mut(&screen)
        .unwrap()
        .children
    {
        if *child == previous {
            *child = actor;
        }
    }
    Ok(())
}

pub(super) fn load(call: &mut HostCall<'_>, data: &Load, capture_probe: bool) -> Result<(), Error> {
    let globals = table(call, helper(call, 5)?)?;
    let environment = source::environment(call, Value::Table(globals), true)?;
    call.with_roots(&[environment], |call| {
        let key = call.try_intern(ENVIRONMENT)?;
        call.raw_table_set(globals, key, environment)?;
        let captures = call.captures().to_vec();
        let loader = call.new_host_function(resource, &captures)?;
        set(call, table(call, environment)?, "LoadActor", loader)?;
        let definition = evaluate(call, environment, "default.lua", &[])?;
        let actor = own(call, data, definition)?;
        let screen = get(call, globals, b"__songlua_top_screen")?;
        let screen = table(call, screen)?;
        let overlay = named(call, screen, "Overlay")?;
        if capture_probe {
            parent(call, &data.store, actor, overlay)?;
            screen_parts::append(call, &data.store, overlay, actor)?;
            let children = registry(call, overlay)?;
            set(call, table(call, children)?, "", Value::Table(actor))?;
        } else {
            parent(call, &data.store, actor, screen)?;
            let name = call.try_intern(b"Overlay")?;
            call.with_roots(&[name], |call| {
                if get(call, actor, b"__songlua_load_name")? != Value::Nil {
                    set(call, actor, "__songlua_load_name", name)?;
                }
                set(call, actor, "__songlua_native_name", name)?;
                set(call, actor, "Name", name)?;
                set(call, actor, "__songlua_top_screen_child_name", name)
            })?;
            let order = order(call, overlay)?;
            set(
                call,
                actor,
                "__songlua_state_draw_order",
                Value::Number(order as f64),
            )?;
            {
                let mut guard = data.store.lock().unwrap();
                let state = &mut guard.actors.get_mut(&actor).unwrap().state;
                state.name = b"Overlay".to_vec();
                state.draw_order = order;
                state.render.draw_order = order;
            }
            replace(call, data, screen, overlay, actor, "Overlay")?;
        }
        owned_lookup(call, actor)?;
        initialize(call, &data.store, actor)?;
        command(call, actor, b"BeginCommand")?;
        command(call, actor, b"OnCommand")?;
        if capture_probe {
            // The diagnostic probe uses ActorDefinitionHost, which the native
            // host installs only in live mode. Its error occurs after On and
            // leaves the loaded theme attached to the original wrapper.
            let live = data
                .store
                .lock()
                .unwrap()
                .clock
                .live
                .load(std::sync::atomic::Ordering::Relaxed);
            if !live {
                return Err(call.error(b"Overlay requires its actor host"));
            }
            let probe = generated_players::dummy(
                call,
                &data.store,
                data.scene,
                "ActorFrame",
                "ITGmaniaHarnessTraceProbe",
                Some(overlay),
            )?;
            screen_parts::append(call, &data.store, overlay, probe)?;
            let children = registry(call, overlay)?;
            set(
                call,
                table(call, children)?,
                "ITGmaniaHarnessTraceProbe",
                Value::Table(probe),
            )?;
        }
        Ok(())
    })
}

fn screen_string(call: &mut HostCall<'_>) -> Result<(), Error> {
    let name = call.function_data::<String>().unwrap().clone();
    let key = assets::text(call, arg(call, 0))?;
    let text = crate::theme_string(&name, &key);
    let text = call.try_intern(text.as_bytes())?;
    call.push(text)
}

pub(super) fn decorations(call: &mut HostCall<'_>, data: &Load) -> Result<(), Error> {
    let globals = table(call, helper(call, 5)?)?;
    let screen = get(call, globals, b"__songlua_top_screen")?;
    let screen = table(call, screen)?;
    let name = actor_name(call, &data.store, screen)?;
    let name =
        String::from_utf8(name).map_err(|_| call.error(b"screen name must be valid UTF-8"))?;
    let environment = source::environment(call, Value::Table(globals), false)?;
    call.with_roots(&[environment], |call| {
        let function = call.new_host_function_with_data(screen_string, &[], name)?;
        set(call, table(call, environment)?, "ScreenString", function)?;
        for (name, source) in [("Header", source::HEADER), ("Footer", source::FOOTER)] {
            let definition = source::evaluate(call, source, environment, &[])?;
            let actor = own(call, data, definition)?;
            let previous = named(call, screen, name)?;
            parent(call, &data.store, actor, screen)?;
            owned_lookup(call, actor)?;
            initialize(call, &data.store, actor)?;
            let on = source::evaluate(call, source::DECORATION_ON, Value::Table(globals), &[])?;
            set(call, actor, "OnCommand", on)?;
            replace(call, data, screen, previous, actor, name)?;
            command(call, actor, b"BeginCommand")?;
            command(call, actor, b"OnCommand")?;
        }
        let mut children = Vec::new();
        for index in 1.. {
            let child = call.raw_table_get(screen, Value::Number(index as f64))?;
            if child == Value::Nil {
                break;
            }
            let child = table(call, child)?;
            children.push((order(call, child).unwrap_or(0), child));
        }
        children.sort_by_key(|&(order, _)| order);
        for (index, &(_, child)) in children.iter().enumerate() {
            call.raw_table_set(
                screen,
                Value::Number((index + 1) as f64),
                Value::Table(child),
            )?;
        }
        data.store
            .lock()
            .unwrap()
            .actors
            .get_mut(&screen)
            .unwrap()
            .children = children.into_iter().map(|(_, child)| child).collect();
        Ok(())
    })
}

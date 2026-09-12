// Frozen from 02df6cae276557eb6ddc7db182c93dd5cb24704f; only test visibility/formatting differs.

use super::*;

pub(super) fn install_actor_command_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set(
        "SetTarget",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(Value::Table(target)) = args.get(1) {
                    set_proxy_target_fields(&actor, target)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "visible",
        lua.create_function({
            let actor = actor.clone();
            move |lua, (_self, value): (Option<Value>, Option<Value>)| {
                if let Some(value) = value.and_then(read_boolish) {
                    prepare_capture_scope_actor(lua, &actor)?;
                    actor.set("__songlua_visible", value)?;
                    capture_block_set_bool(lua, &actor, "visible", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "addcommand",
        lua.create_function({
            let actor = actor.clone();
            move |_, (_self, name, function): (Table, String, Function)| {
                actor.set(format!("{name}Command"), function)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "removecommand",
        lua.create_function({
            let actor = actor.clone();
            move |_, (_self, name): (Table, String)| {
                actor.set(format!("{name}Command"), Value::Nil)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetCommand",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(Value::Nil);
                };
                Ok(actor
                    .get::<Option<Function>>(format!("{name}Command"))?
                    .map_or(Value::Nil, Value::Function))
            }
        })?,
    )?;
    actor.set(
        "GetTweenTimeLeft",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: MultiValue| actor_tween_time_left(&actor)
        })?,
    )?;
    actor.set(
        "sleep",
        lua.create_function({
            let actor = actor.clone();
            move |lua, (_self, duration): (Option<Value>, Option<Value>)| {
                prepare_capture_scope_actor(lua, &actor)?;
                flush_actor_capture(&actor)?;
                let exact_duration = duration
                    .as_ref()
                    .cloned()
                    .and_then(read_f64)
                    .unwrap_or(0.0)
                    .max(0.0);
                let duration = duration.and_then(read_f32).unwrap_or(0.0).max(0.0);
                let cursor = actor
                    .get::<Option<f32>>("__songlua_capture_cursor")?
                    .unwrap_or(0.0);
                actor.set("__songlua_capture_cursor", cursor + duration)?;
                actor.set("__songlua_capture_tween_time_left", cursor + duration)?;
                if actor_has_active_command(lua, &actor)? {
                    let interval = actor
                        .get::<Option<f64>>("__songlua_recurring_update_exact_interval")?
                        .unwrap_or(0.0);
                    actor.set(
                        "__songlua_recurring_update_exact_interval",
                        interval + exact_duration,
                    )?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "hibernate",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                prepare_capture_scope_actor(lua, &actor)?;
                flush_actor_capture(&actor)?;
                let duration = args
                    .get(1)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0)
                    .max(0.0);
                if duration <= f32::EPSILON {
                    return Ok(actor.clone());
                }
                let restore_visible = actor
                    .get::<Option<bool>>("__songlua_visible")?
                    .unwrap_or(true);
                capture_block_set_bool(lua, &actor, "visible", false)?;
                flush_actor_capture(&actor)?;
                let cursor = actor
                    .get::<Option<f32>>("__songlua_capture_cursor")?
                    .unwrap_or(0.0);
                actor.set("__songlua_capture_cursor", cursor + duration)?;
                actor.set("__songlua_capture_tween_time_left", cursor + duration)?;
                capture_block_set_bool(lua, &actor, "visible", restore_visible)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "linear",
        make_actor_tween_method(lua, actor, Some("linear"))?,
    )?;
    actor.set(
        "accelerate",
        make_actor_tween_method(lua, actor, Some("inQuad"))?,
    )?;
    actor.set(
        "decelerate",
        make_actor_tween_method(lua, actor, Some("outQuad"))?,
    )?;
    actor.set(
        "smooth",
        make_actor_tween_method(lua, actor, Some("inOutQuad"))?,
    )?;
    actor.set(
        "spring",
        make_actor_tween_method(lua, actor, Some("spring"))?,
    )?;
    actor.set(
        "bouncebegin",
        make_actor_tween_method(lua, actor, Some("inBounce"))?,
    )?;
    actor.set(
        "bounceend",
        make_actor_tween_method(lua, actor, Some("outBounce"))?,
    )?;
    actor.set(
        "queuecommand",
        lua.create_function({
            let actor = actor.clone();
            move |lua, (_self, name): (Option<Value>, Option<Value>)| {
                let Some(name) = name.and_then(read_string) else {
                    return Ok(actor.clone());
                };
                let active = actor_active_commands(lua, &actor)?;
                let command = format!("{name}Command");
                if active
                    .get::<Option<bool>>(command.as_str())?
                    .unwrap_or(false)
                {
                    actor.set("__songlua_recurring_update_command", command)?;
                    invalidate_compile_update_plan(lua);
                    let cursor = actor
                        .get::<Option<f32>>("__songlua_capture_cursor")?
                        .unwrap_or(0.0);
                    let start = actor
                        .get::<Option<f32>>("__songlua_recurring_update_start_cursor")?
                        .unwrap_or(cursor);
                    let interval = actor
                        .get::<Option<f64>>("__songlua_recurring_update_exact_interval")?
                        .filter(|interval| *interval > f64::EPSILON)
                        .unwrap_or_else(|| f64::from((cursor - start).max(0.0)));
                    if interval > f64::EPSILON {
                        actor.set("__songlua_recurring_update_interval", interval)?;
                    }
                    return Ok(actor.clone());
                }
                let queue = actor_command_queue(lua, &actor)?;
                queue.raw_set(queue.raw_len() + 1, name)?;
                if lua.app_data_ref::<SongLuaStartupQueues>().is_some()
                    || !actor_has_active_command(lua, &actor)?
                {
                    drain_actor_command_queue(lua, &actor)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "queuemessage",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(message) = args.get(1).cloned().and_then(read_string) {
                    note_song_lua_side_effect(lua)?;
                    crate::record_song_lua_broadcast(lua, &message, false)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "playcommand",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                let params = method_arg(&args, 1).cloned();
                let command_name = format!("{name}Command");
                if actor
                    .get::<Option<bool>>("__songlua_propagate_commands")?
                    .unwrap_or(false)
                {
                    run_named_command_on_children_recursively(lua, &actor, &command_name, params)?;
                } else {
                    run_actor_message_with_params(lua, &actor, &command_name, params)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "propagate",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                actor.set(
                    "__songlua_propagate_commands",
                    method_arg(&args, 0).is_some_and(truthy),
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "propagatecommand",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                let command = format!("{name}Command");
                run_named_command_on_children_recursively(
                    lua,
                    &actor,
                    &command,
                    method_arg(&args, 1).cloned(),
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "playcommandonchildren",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                let params = method_arg(&args, 1).cloned();
                let command = format!("{name}Command");
                for child in actor_direct_children(lua, &actor)? {
                    run_actor_named_command_with_drain_and_params(
                        lua,
                        &child,
                        &command,
                        true,
                        params.clone(),
                    )?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "playcommandonleaves",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                let params = method_arg(&args, 1).cloned();
                run_named_command_on_leaves(lua, &actor, &format!("{name}Command"), params)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "RemoveChild",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                remove_actor_child(lua, &actor, &name)?;
                invalidate_compile_update_plan(lua);
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "RemoveAllChildren",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: MultiValue| {
                remove_all_actor_children(lua, &actor)?;
                invalidate_compile_update_plan(lua);
                Ok(actor.clone())
            }
        })?,
    )?;
    Ok(())
}

pub(super) fn drain_actor_command_queue(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let queue = actor_command_queue(lua, actor)?;
    if let Some(mut startup) = lua.app_data_mut::<SongLuaStartupQueues>() {
        if queue.raw_len() > 0
            && !startup
                .0
                .iter()
                .any(|queued| queued.to_pointer() == actor.to_pointer())
        {
            startup.0.push(actor.clone());
        }
        return Ok(());
    }
    while queue.raw_len() > 0 {
        let Some(name) = queue.raw_get::<Option<String>>(1)? else {
            break;
        };
        // Shift inside Lua, avoiding a Rust handle and two Lua API crossings
        // per remaining item. Remove before dispatch so callbacks observe the
        // same queue and can append, replace, or recursively drain it.
        if queue.raw_len() == 1 {
            queue.raw_set(1, Value::Nil)?;
        } else {
            queue.raw_remove(1_i64)?;
        }
        // Effect setters bypass the tween queue, but a queued command itself
        // begins at the current queue cursor. Preserve that command-local now.
        let immediate_start_key = "__songlua_capture_immediate_start";
        let previous_immediate_start = actor.get::<Value>(immediate_start_key)?;
        actor.set(
            immediate_start_key,
            actor
                .get::<Option<f32>>("__songlua_capture_cursor")?
                .unwrap_or(0.0),
        )?;
        let result = run_actor_message_with_params(lua, actor, &format!("{name}Command"), None);
        actor.set(immediate_start_key, previous_immediate_start)?;
        result?;
    }
    Ok(())
}

pub(super) fn broadcast_song_lua_message(
    lua: &Lua,
    message: &str,
    params: Option<Value>,
) -> mlua::Result<()> {
    if message.trim().is_empty() {
        return Ok(());
    }
    let command = format!("{message}MessageCommand");
    let globals = lua.globals();
    let beat = compile_song_runtime_values(lua).map_or(0.0, |(beat, _)| beat);
    if let Some(mut capture) = lua.app_data_mut::<SongLuaOverlayUpdateCapture>() {
        capture
            .runtime_broadcasts
            .push((beat, message.to_string(), params.is_some()));
    }
    // Keep the Lua key itself: converting a Rust string for each property
    // lookup also allocates for long names. Prepare it before changing scope.
    let command_key = if lua.app_data_ref::<SongLuaOverlayUpdateCapture>().is_some() {
        Some(lua.create_string(&command)?)
    } else {
        None
    };
    let previous_broadcast = globals.raw_get::<Value>(ACTIVE_BROADCAST_KEY)?;
    globals.raw_set(ACTIVE_BROADCAST_KEY, message)?;
    let capture_broadcast = lua
        .app_data_mut::<SongLuaOverlayUpdateCapture>()
        .map(|mut capture| {
            let previous = capture.active_broadcast.replace(message.to_string());
            let previous_command =
                std::mem::replace(&mut capture.active_broadcast_command, command_key);
            (previous, previous_command)
        });
    let result = || {
        let registry = song_lua_actor_registry(lua)?;
        let mut actors = Vec::with_capacity(registry.raw_len());
        for value in registry.sequence_values::<Value>() {
            let Value::Table(actor) = value? else {
                continue;
            };
            actors.push(actor);
        }
        let params = normalize_broadcast_params(lua, message, params)?;
        actors.into_iter().try_for_each(|actor| {
            run_actor_named_command_with_drain_and_params(
                lua,
                &actor,
                &command,
                true,
                params.clone(),
            )
        })
    };
    let result = result();
    if let Some((previous, previous_command)) = capture_broadcast
        && let Some(mut capture) = lua.app_data_mut::<SongLuaOverlayUpdateCapture>()
    {
        capture.active_broadcast = previous;
        capture.active_broadcast_command = previous_command;
    }
    let restore = globals.raw_set(ACTIVE_BROADCAST_KEY, previous_broadcast);
    restore?;
    result
}

pub(super) fn run_actor_message_with_params(
    lua: &Lua,
    actor: &Table,
    name: &str,
    params: Option<Value>,
) -> mlua::Result<()> {
    run_actor_named_command_with_drain_and_params(lua, actor, name, true, params.clone())?;
    if actor_type_is(actor, "ActorFrame")? || actor_type_is(actor, "ActorFrameTexture")? {
        for child in actor_direct_children(lua, actor)? {
            run_actor_message_with_params(lua, &child, name, params.clone())?;
        }
    }
    Ok(())
}

pub(super) fn run_actor_named_command_with_drain_and_params(
    lua: &Lua,
    actor: &Table,
    name: &str,
    drain_queue: bool,
    params: Option<Value>,
) -> mlua::Result<()> {
    let Some(command) = actor.get::<Option<Function>>(name)? else {
        return Ok(());
    };
    run_guarded_actor_command(lua, actor, name, &command, drain_queue, params)
}

pub(super) fn run_guarded_actor_command(
    lua: &Lua,
    actor: &Table,
    name: &str,
    command: &Function,
    drain_queue: bool,
    params: Option<Value>,
) -> mlua::Result<()> {
    let active = actor_active_commands(lua, actor)?;
    if active.get::<Option<bool>>(name)?.unwrap_or(false) {
        return Ok(());
    }
    active.set(name, true)?;
    let recurring_cursor_key = "__songlua_recurring_update_start_cursor";
    let recurring_exact_key = "__songlua_recurring_update_exact_interval";
    let previous_recurring_state = {
        let previous_cursor = actor.get::<Value>(recurring_cursor_key)?;
        let previous_exact = actor.get::<Value>(recurring_exact_key)?;
        actor.set(
            recurring_cursor_key,
            actor
                .get::<Option<f32>>("__songlua_capture_cursor")?
                .unwrap_or(0.0),
        )?;
        actor.set(recurring_exact_key, 0.0_f64)?;
        (previous_cursor, previous_exact)
    };
    let result = call_actor_function(lua, actor, command, params)
        .map_err(|err| {
            mlua::Error::external(format!(
                "{} failed for {}: {err}",
                name,
                actor_debug_label(actor)
            ))
        })
        .and_then(|()| {
            if drain_queue {
                drain_actor_command_queue(lua, actor)?;
            }
            Ok(())
        });
    actor.set(recurring_cursor_key, previous_recurring_state.0)?;
    actor.set(recurring_exact_key, previous_recurring_state.1)?;
    active.set(name, Value::Nil)?;
    result
}

// Exact formatting expressions used by the parent callers above.
pub(super) fn command_name(name: &str) -> String {
    format!("{name}Command")
}
pub(super) fn message_name(message: &str) -> String {
    format!("{message}MessageCommand")
}

// Frozen from 01769fb969fb015cffe71b0b9d095e718d8e1d7d. Only visibility, method receivers, and calls to frozen methods are adapted.
#![allow(clippy::too_many_arguments)]
use super::*;

pub(super) fn record(
    capture: &mut SongLuaOverlayUpdateCapture,
    actor: &Table,
    beat: f32,
    target: SongLuaOverlayUpdateTarget,
    value: SongLuaOverlayUpdateValue,
) -> bool {
    capture.record_stateful_message(actor, beat, target, value.clone(), 0.0, 0.0, None, None);
    let Some(index) = capture.touch(actor) else {
        return false;
    };
    SongLuaOverlayUpdateCapture::replace_value(
        &mut capture.final_values[index],
        target,
        value.clone(),
    );
    SongLuaOverlayUpdateCapture::replace_value(&mut capture.values[index], target, value);
    true
}

pub(super) fn record_scheduled(
    capture: &mut SongLuaOverlayUpdateCapture,
    actor: &Table,
    beat: f32,
    delay_seconds: f32,
    duration_seconds: f32,
    easing: Option<String>,
    opt1: Option<f32>,
    target: SongLuaOverlayUpdateTarget,
    value: SongLuaOverlayUpdateValue,
) -> bool {
    capture.record_stateful_message(
        actor,
        beat,
        target,
        value.clone(),
        delay_seconds,
        duration_seconds,
        easing.clone(),
        opt1,
    );
    let Some(index) = capture.touch(actor) else {
        return false;
    };
    SongLuaOverlayUpdateCapture::replace_value(
        &mut capture.final_values[index],
        target,
        value.clone(),
    );
    capture.scheduled[index].push(SongLuaScheduledOverlayUpdate {
        delay_seconds,
        duration_seconds,
        easing,
        opt1,
        target,
        value,
    });
    true
}

pub(super) fn record_overlay_update_capture(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value: SongLuaOverlayUpdateValue,
) -> bool {
    let Some(target) = capture_target_for_key(key) else {
        return false;
    };
    let direct_message_actor = {
        let Some(capture) = lua.app_data_ref::<SongLuaOverlayUpdateCapture>() else {
            return false;
        };
        if !capture
            .actor_indices
            .contains_key(&(actor.to_pointer() as usize))
        {
            return false;
        }
        capture.active_broadcast.as_deref().is_some_and(|message| {
            actor
                .get::<Option<Function>>(format!("{message}MessageCommand"))
                .ok()
                .flatten()
                .is_some()
        })
    };
    // The receiving actor's own command is compiled as a timed message block.
    // Mutations to other actors must remain in the sequential capture because
    // stateful commands can select a different target on every broadcast.
    if direct_message_actor {
        return lua
            .app_data_mut::<SongLuaOverlayUpdateCapture>()
            .is_some_and(|mut capture| capture.touch(actor).is_some());
    }
    let cursor = actor
        .get::<Option<f32>>("__songlua_capture_cursor")
        .ok()
        .flatten()
        .unwrap_or(0.0)
        .max(0.0);
    let duration = actor
        .get::<Option<f32>>("__songlua_capture_duration")
        .ok()
        .flatten()
        .unwrap_or(0.0)
        .max(0.0);
    let easing = actor
        .get::<Option<String>>("__songlua_capture_easing")
        .ok()
        .flatten();
    let opt1 = actor
        .get::<Option<f32>>("__songlua_capture_opt1")
        .ok()
        .flatten();
    let beat = compile_song_runtime_values(lua).map_or(0.0, |(beat, _)| beat);
    lua.app_data_mut::<SongLuaOverlayUpdateCapture>()
        .is_some_and(|mut capture| {
            if cursor > f32::EPSILON || duration > f32::EPSILON {
                record_scheduled(
                    &mut capture,
                    actor,
                    beat,
                    cursor,
                    duration,
                    easing,
                    opt1,
                    target,
                    value,
                )
            } else {
                record(&mut capture, actor, beat, target, value)
            }
        })
}

pub(super) fn record_overlay_update_capture_immediate(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value: SongLuaOverlayUpdateValue,
) -> bool {
    let Some(target) = capture_target_for_key(key) else {
        return false;
    };
    let direct_message_actor = {
        let Some(capture) = lua.app_data_ref::<SongLuaOverlayUpdateCapture>() else {
            return false;
        };
        if !capture
            .actor_indices
            .contains_key(&(actor.to_pointer() as usize))
        {
            return false;
        }
        capture.active_broadcast.as_deref().is_some_and(|message| {
            actor
                .get::<Option<Function>>(format!("{message}MessageCommand"))
                .ok()
                .flatten()
                .is_some()
        })
    };
    if direct_message_actor {
        return true;
    }
    let beat = compile_song_runtime_values(lua).map_or(0.0, |(beat, _)| beat);
    lua.app_data_mut::<SongLuaOverlayUpdateCapture>()
        .is_some_and(|mut capture| record(&mut capture, actor, beat, target, value))
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
    let previous_broadcast = globals.raw_get::<Value>(ACTIVE_BROADCAST_KEY)?;
    globals.raw_set(ACTIVE_BROADCAST_KEY, message)?;
    let capture_broadcast = lua
        .app_data_mut::<SongLuaOverlayUpdateCapture>()
        .map(|mut capture| capture.active_broadcast.replace(message.to_string()));
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
    if let Some(previous) = capture_broadcast
        && let Some(mut capture) = lua.app_data_mut::<SongLuaOverlayUpdateCapture>()
    {
        capture.active_broadcast = previous;
    }
    let restore = globals.raw_set(ACTIVE_BROADCAST_KEY, previous_broadcast);
    restore?;
    result
}

pub(super) fn reset_overlay_compile_actor_capture_tables<Kind>(
    lua: &Lua,
    overlays: &[SongLuaOverlayCompileActor<Kind>],
) -> Result<(), String> {
    let indices: Vec<_> = (0..overlays.len()).collect();
    let tables = overlay_compile_actor_tables_for_indices(overlays, &indices);
    reset_indexed_actor_capture_tables(lua, &tables)
}

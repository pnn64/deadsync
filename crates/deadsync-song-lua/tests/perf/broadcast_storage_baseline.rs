// Frozen broadcast and getter functions from d95a183b6 (0.5.1632).
use super::*;

pub(super) fn normalize_broadcast_params(
    lua: &Lua,
    message: &str,
    params: Option<Value>,
) -> mlua::Result<Option<Value>> {
    match params {
        Some(Value::Table(table)) => {
            if message.eq_ignore_ascii_case("Judgment") {
                normalize_judgment_params(lua, &table)?;
            }
            Ok(Some(Value::Table(table)))
        }
        params => Ok(params),
    }
}

pub(super) fn broadcast_song_lua_message(
    lua: &Lua,
    message: &str,
    params: Option<Value>,
) -> mlua::Result<()> {
    if message.trim().is_empty() {
        return Ok(());
    }
    let command = ActorCommandName::new(message, "MessageCommand");
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
        Some(lua.create_string(command.as_str())?)
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

pub(super) fn normalize_judgment_params(lua: &Lua, params: &Table) -> mlua::Result<()> {
    if matches!(params.get::<Value>("Player")?, Value::Nil) {
        params.set("Player", player_number_name(0))?;
    }
    if matches!(params.get::<Value>("TapNoteScore")?, Value::Nil) {
        params.set("TapNoteScore", "TapNoteScore_W3")?;
    }
    if matches!(params.get::<Value>("HoldNoteScore")?, Value::Nil) {
        params.set("HoldNoteScore", "HoldNoteScore_None")?;
    }
    if matches!(params.get::<Value>("TapNoteOffset")?, Value::Nil) {
        params.set("TapNoteOffset", 0.0_f32)?;
    }

    let notes = match params.get::<Value>("Notes")? {
        Value::Table(notes) => notes,
        _ => {
            let notes = lua.create_table()?;
            let first_track = table_i32_field(params, &["FirstTrack", "Column"])?.unwrap_or(0);
            notes.raw_set(
                i64::from(first_track.max(0)) + 1,
                create_tap_note_table(lua, params, None)?,
            )?;
            params.set("Notes", notes.clone())?;
            notes
        }
    };
    if table_has_entries(&notes)? {
        let entries = notes
            .pairs::<Value, Value>()
            .collect::<mlua::Result<Vec<_>>>()?;
        for (key, value) in entries {
            let note = match value {
                Value::Table(note) => normalize_tap_note_table(lua, params, note)?,
                value => create_tap_note_table(lua, params, Some(value))?,
            };
            notes.set(key, note)?;
        }
    } else {
        notes.raw_set(1, create_tap_note_table(lua, params, None)?)?;
    }
    Ok(())
}

pub(super) fn normalize_tap_note_table(
    lua: &Lua,
    params: &Table,
    note: Table,
) -> mlua::Result<Table> {
    if !matches!(note.get::<Value>("GetTapNoteType")?, Value::Nil) {
        return Ok(note);
    }
    let result = match note.get::<Value>("TapNoteResult")? {
        Value::Table(result) => normalize_tap_note_result_table(lua, params, result, Some(&note))?,
        _ => create_tap_note_result_table(lua, params, Some(&note))?,
    };
    note.set("TapNoteResult", result.clone())?;
    note.set("HoldNoteResult", result.clone())?;
    install_tap_note_methods(lua, params, &note, result)?;
    Ok(note)
}

pub(super) fn create_tap_note_table(
    lua: &Lua,
    params: &Table,
    note_value: Option<Value>,
) -> mlua::Result<Table> {
    let note = lua.create_table()?;
    if let Some(value) = note_value {
        note.set("TapNoteType", value)?;
    }
    let result = create_tap_note_result_table(lua, params, Some(&note))?;
    note.set("TapNoteResult", result.clone())?;
    note.set("HoldNoteResult", result.clone())?;
    install_tap_note_methods(lua, params, &note, result)?;
    Ok(note)
}

pub(super) fn install_tap_note_methods(
    lua: &Lua,
    params: &Table,
    note: &Table,
    result: Table,
) -> mlua::Result<()> {
    let note_type = table_string_field(note, &["TapNoteType", "Type", "NoteType"])?
        .or(table_string_field(
            params,
            &["TapNoteType", "Type", "NoteType"],
        )?)
        .unwrap_or_else(|| "TapNoteType_Tap".to_string());
    let source = table_string_field(note, &["TapNoteSource", "Source"])?
        .unwrap_or_else(|| "TapNoteSource_Original".to_string());
    let subtype = table_string_field(note, &["TapNoteSubType", "SubType"])?
        .unwrap_or_else(|| "TapNoteSubType_Hold".to_string());
    let player = table_string_field(params, &["Player"])?
        .unwrap_or_else(|| player_number_name(0).to_string());
    let hold_duration = table_f32_field(note, &["HoldDuration"])?.unwrap_or(0.0);
    let attack_duration = table_f32_field(note, &["AttackDuration"])?.unwrap_or(0.0);
    let attack_mods = table_string_field(note, &["AttackModifiers"])?.unwrap_or_default();
    let keysound = table_i32_field(note, &["KeysoundIndex"])?.unwrap_or(0);

    set_string_method(lua, note, "GetTapNoteType", &note_type)?;
    set_string_method(lua, note, "GetTapNoteSource", &source)?;
    set_string_method(lua, note, "GetTapNoteSubType", &subtype)?;
    set_string_method(lua, note, "GetPlayerNumber", &player)?;
    set_string_method(lua, note, "GetAttackModifiers", &attack_mods)?;
    note.set(
        "GetTapNoteResult",
        lua.create_function({
            let result = result.clone();
            move |_, _args: MultiValue| Ok(result.clone())
        })?,
    )?;
    note.set(
        "GetHoldNoteResult",
        lua.create_function({
            let result = result;
            move |_, _args: MultiValue| Ok(result.clone())
        })?,
    )?;
    note.set(
        "GetHoldDuration",
        lua.create_function(move |_, _args: MultiValue| Ok(hold_duration))?,
    )?;
    note.set(
        "GetAttackDuration",
        lua.create_function(move |_, _args: MultiValue| Ok(attack_duration))?,
    )?;
    note.set(
        "GetKeysoundIndex",
        lua.create_function(move |_, _args: MultiValue| Ok(keysound))?,
    )?;
    Ok(())
}

pub(super) fn create_tap_note_result_table(
    lua: &Lua,
    params: &Table,
    note: Option<&Table>,
) -> mlua::Result<Table> {
    let result = lua.create_table()?;
    install_tap_note_result_methods(lua, params, note, &result)?;
    Ok(result)
}

pub(super) fn normalize_tap_note_result_table(
    lua: &Lua,
    params: &Table,
    result: Table,
    note: Option<&Table>,
) -> mlua::Result<Table> {
    if matches!(result.get::<Value>("GetHeld")?, Value::Nil) {
        install_tap_note_result_methods(lua, params, note, &result)?;
    }
    Ok(result)
}

pub(super) fn install_tap_note_result_methods(
    lua: &Lua,
    params: &Table,
    note: Option<&Table>,
    result: &Table,
) -> mlua::Result<()> {
    let held = match note {
        Some(note) => table_bool_field(note, &["Held", "held"])?,
        None => None,
    }
    .unwrap_or(false);
    let hidden = match note {
        Some(note) => table_bool_field(note, &["Hidden", "hidden"])?,
        None => None,
    }
    .unwrap_or(false);
    let offset = match note {
        Some(note) => table_f32_field(note, &["TapNoteOffset", "Offset"])?,
        None => None,
    }
    .or(table_f32_field(params, &["TapNoteOffset", "Offset"])?)
    .unwrap_or(0.0);
    let score = match note {
        Some(note) => table_string_field(note, &["TapNoteScore", "Score"])?,
        None => None,
    }
    .or(table_string_field(params, &["TapNoteScore", "Score"])?)
    .unwrap_or_else(|| "TapNoteScore_None".to_string());

    result.set(
        "GetHeld",
        lua.create_function(move |_, _args: MultiValue| Ok(held))?,
    )?;
    result.set(
        "GetHidden",
        lua.create_function(move |_, _args: MultiValue| Ok(hidden))?,
    )?;
    result.set(
        "GetTapNoteOffset",
        lua.create_function(move |_, _args: MultiValue| Ok(offset))?,
    )?;
    set_string_method(lua, result, "GetTapNoteScore", &score)?;
    Ok(())
}

pub(super) fn set_string_method(
    lua: &Lua,
    table: &Table,
    name: &str,
    value: &str,
) -> mlua::Result<()> {
    let value = value.to_string();
    table.set(
        name,
        lua.create_function(move |lua, _args: MultiValue| {
            Ok(Value::String(lua.create_string(&value)?))
        })?,
    )
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

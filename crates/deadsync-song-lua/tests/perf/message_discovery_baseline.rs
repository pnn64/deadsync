// Frozen from ff42a7005d492d6bf2b732ea8a80bcbfa40654b4; only test visibility/formatting differs.

use super::*;

pub(super) fn capture_actor_message_commands(
    lua: &Lua,
    actor: &Table,
) -> Result<SongLuaCapturedMessageCommands, String> {
    let mut out = SongLuaCapturedMessageCommands::default();
    let mut command_names = Vec::new();
    for pair in actor.clone().pairs::<Value, Value>() {
        let (key, value) = pair.map_err(|err| err.to_string())?;
        let Some(name) = read_string(key) else {
            continue;
        };
        if !name.ends_with("MessageCommand") || !matches!(value, Value::Function(_)) {
            continue;
        }
        command_names.push(name);
    }
    command_names.sort_unstable();

    // Capturing a command resets and restores fields on the actor table. Do not
    // mutate that table while a Lua `pairs` iterator is still traversing it:
    // Lua's next-key order is unspecified and can otherwise skip commands.
    for name in command_names {
        let message = name.trim_end_matches("MessageCommand").to_string();
        let globals = lua.globals();
        let previous_sound_calls = globals
            .get::<Value>(SONG_LUA_SOUND_CALLS_KEY)
            .map_err(|err| err.to_string())?;
        let sound_calls = lua.create_table().map_err(|err| err.to_string())?;
        globals
            .set(SONG_LUA_SOUND_CALLS_KEY, sound_calls.clone())
            .map_err(|err| err.to_string())?;
        let blocks = capture_actor_command_preserving_state(lua, actor, name.as_str());
        let sounds = read_path_table(&sound_calls);
        globals
            .set(SONG_LUA_SOUND_CALLS_KEY, previous_sound_calls)
            .map_err(|err| err.to_string())?;
        let blocks = match blocks {
            Ok(blocks) => blocks,
            Err(err) => {
                push_unique_compile_detail(
                    &mut out.skipped,
                    format!("{}.{}: {err}", actor_debug_label(actor), name),
                );
                continue;
            }
        };
        out.sounds
            .extend(sounds?.into_iter().map(|path| (message.clone(), path)));
        if !blocks.is_empty() {
            out.commands.push(SongLuaOverlayMessageCommand {
                message,
                blocks,
                aux: None,
            });
        }
    }
    flush_actor_capture(actor).map_err(|err| err.to_string())?;
    let startup_sound_blocks: Vec<_> = read_actor_capture_blocks(actor)?
        .into_iter()
        .filter(|block| block.delta.sound_play == Some(true))
        .collect();
    if !startup_sound_blocks.is_empty() {
        out.commands.push(SongLuaOverlayMessageCommand {
            message: SONG_LUA_STARTUP_MESSAGE.to_string(),
            blocks: startup_sound_blocks,
            aux: None,
        });
    }
    Ok(out)
}

pub(super) fn capture_stable_cross_actor_message_commands<Kind>(
    lua: &Lua,
    overlays: &mut [SongLuaOverlayCompileActor<Kind>],
    mut on_dynamic: impl FnMut(String),
) -> Result<(), String> {
    let overlay_tables = overlays
        .iter()
        .enumerate()
        .map(|(index, overlay)| (index, overlay.table.clone()))
        .collect::<Vec<_>>();
    let drain_tables = overlay_tables
        .iter()
        .map(|(_, table)| table.clone())
        .collect::<Vec<_>>();
    let mut commands = Vec::new();
    for (source_index, source) in overlays.iter().enumerate() {
        for pair in source.table.clone().pairs::<Value, Value>() {
            let (key, value) = pair.map_err(|err| err.to_string())?;
            let (Some(name), Value::Function(function)) = (read_string(key), value) else {
                continue;
            };
            let Some(message) = name.strip_suffix("MessageCommand") else {
                continue;
            };
            let message = message.to_string();
            commands.push((source_index, source.table.clone(), name, message, function));
        }
    }

    let mut additions = Vec::new();
    for (source_index, source, command_name, message, command) in commands {
        let table_snapshots =
            snapshot_function_action_tables(lua, &command).map_err(|err| err.to_string())?;
        let params =
            default_message_command_params(lua, &command_name).map_err(|err| err.to_string())?;
        let source_for_call = source.clone();
        let command_for_call = command.clone();
        let command_name_for_call = command_name.clone();
        let drain_for_call = drain_tables.clone();
        let runner = lua
            .create_function(move |lua, ()| {
                run_guarded_actor_command(
                    lua,
                    &source_for_call,
                    &command_name_for_call,
                    &command_for_call,
                    true,
                    params.clone(),
                )?;
                for actor in &drain_for_call {
                    drain_actor_command_queue(lua, actor)?;
                }
                Ok(())
            })
            .map_err(|err| err.to_string())?;
        let first =
            capture_function_action_blocks_inner(lua, &overlay_tables, &[], &runner, 0.0, false);
        let second = first.as_ref().ok().map(|_| {
            capture_function_action_blocks_inner(lua, &overlay_tables, &[], &runner, 0.0, false)
        });
        restore_function_action_tables(table_snapshots).map_err(|err| err.to_string())?;
        let Ok(first) = first else {
            // The ordinary per-actor capture already records this command as
            // skipped. Cross-actor probing must not turn that into a hard error.
            continue;
        };
        let second = match second.expect("second capture exists after a successful first capture") {
            Ok(second) => second,
            Err(err) => {
                on_dynamic(format!(
                    "{}.{} cannot be replayed for stable cross-actor capture: {err}",
                    actor_debug_label(&source),
                    command_name
                ));
                continue;
            }
        };
        let first = cross_actor_effects(&first, source_index);
        let second = cross_actor_effects(&second, source_index);
        if first.is_empty() {
            continue;
        }
        if first != second {
            on_dynamic(format!(
                "{}.{} changes cross-actor targets or effects between runs",
                actor_debug_label(&source),
                command_name
            ));
            continue;
        }
        additions.extend(first.into_iter().map(|(target, blocks, aux)| {
            (
                target,
                SongLuaOverlayMessageCommand {
                    message: message.clone(),
                    blocks,
                    aux,
                },
            )
        }));
    }
    for (target, command) in additions {
        overlays[target].actor.message_commands.push(command);
    }
    Ok(())
}

// Exact discovery prefix of the frozen ordinary capture function above.
pub(super) fn command_names(actor: &Table) -> Result<Vec<String>, String> {
    let mut command_names = Vec::new();
    for pair in actor.clone().pairs::<Value, Value>() {
        let (key, value) = pair.map_err(|err| err.to_string())?;
        let Some(name) = read_string(key) else {
            continue;
        };
        if !name.ends_with("MessageCommand") || !matches!(value, Value::Function(_)) {
            continue;
        }
        command_names.push(name);
    }
    command_names.sort_unstable();
    Ok(command_names)
}

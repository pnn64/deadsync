use super::*;

pub fn capture_overlay_compile_actor_function_action_blocks<Kind>(
    lua: &Lua,
    overlays: &[SongLuaOverlayCompileActor<Kind>],
    tracked_actors: &[SongLuaTrackedActor],
    function: &Function,
    beat: f32,
) -> Result<SongLuaFunctionActionCapture, String> {
    let indices: Vec<_> = (0..overlays.len()).collect();
    let overlay_tables = overlay_compile_actor_tables_for_indices(overlays, &indices);
    capture_function_action_blocks(lua, &overlay_tables, tracked_actors, function, beat)
}

pub fn capture_stable_cross_actor_message_commands<Kind>(
    lua: &Lua,
    overlays: &mut [SongLuaOverlayCompileActor<Kind>],
    mut on_dynamic: impl FnMut(String),
) -> Result<(), String> {
    let overlay_tables = overlays
        .iter()
        .enumerate()
        .map(|(index, overlay)| (index, overlay.table.clone()))
        .collect::<Vec<_>>();
    let mut commands = Vec::new();
    for (source_index, source) in overlays.iter().enumerate() {
        for_each_actor_message_command(&source.table, |name, function| {
            let message = name
                .strip_suffix("MessageCommand")
                .expect("filtered suffix")
                .to_owned();
            commands.push((source_index, source.table.clone(), name, message, function));
        })
        .map_err(|err| err.to_string())?;
    }

    let mut additions = Vec::new();
    for (source_index, source, command_name, message, command) in commands {
        let table_snapshots = lua
            .create_sequence_from([command.clone()])
            .and_then(|functions| snapshot_function_action_tables(lua, &functions))
            .map_err(|err| err.to_string())?;
        let runner = message_capture_runner(lua, &source, &command_name, &command)
            .map_err(|err| err.to_string())?;
        let local_scope = begin_action_capture_scope(lua).map_err(|err| err.to_string())?;
        let first =
            capture_function_action_blocks_inner(lua, &overlay_tables, &[], &runner, 0.0, false);
        let second = first.as_ref().ok().map(|_| {
            capture_function_action_blocks_inner(lua, &overlay_tables, &[], &runner, 0.0, false)
        });
        restore_action_capture_scope(lua, local_scope).map_err(|err| err.to_string())?;
        restore_function_action_tables(lua, table_snapshots).map_err(|err| err.to_string())?;
        let Ok(first) = first else {
            // The ordinary per-actor capture already records this command as
            // skipped. Cross-actor probing must not turn that into a hard error.
            continue;
        };
        let second = match second.expect("second capture exists after a successful first capture") {
            Ok(second) => second,
            Err(err) => {
                let detail = defer_actor_message(
                    lua,
                    &source,
                    &command_name,
                    format!(
                        "{}.{} cannot be replayed for stable cross-actor capture: {err}",
                        actor_debug_label(&source),
                        command_name
                    ),
                );
                on_dynamic(detail);
                continue;
            }
        };
        let first = actor_capture_effects(&first.overlay_blocks, source_index);
        let second = actor_capture_effects(&second.overlay_blocks, source_index);
        if first.is_empty() {
            continue;
        }
        if first != second {
            let detail = defer_actor_message(
                lua,
                &source,
                &command_name,
                format!(
                    "{}.{} changes cross-actor targets or effects between runs",
                    actor_debug_label(&source),
                    command_name
                ),
            );
            on_dynamic(detail);
            continue;
        }
        additions.extend(first.into_iter().map(|(target, blocks)| {
            (
                target,
                SongLuaOverlayMessageCommand {
                    frame_advance: 0.0,
                    message: message.clone(),
                    blocks,
                },
            )
        }));
    }
    for (target, command) in additions {
        overlays[target].actor.message_commands.push(command);
    }
    Ok(())
}

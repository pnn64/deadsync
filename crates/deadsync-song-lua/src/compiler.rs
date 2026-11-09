//! Assembly of the existing preparation pipeline over the shared Rust VM.
use super::*;
use crate::{
    SongLuaCompileTimer, SongLuaMessageEvent, SongLuaNoteskinResolver, SongLuaOverlayActor,
    SongLuaOverlayCommandBlock, SongLuaOverlayKind, SongLuaOverlayMessageCommand,
    SongLuaOverlayModelLayer, SongLuaOverlayState, SongLuaOverlayStateDelta, SongLuaTimeUnit,
    SongLuaTrackedActorTarget as Target,
};
use actors::compiler::{Kind as Late, Query};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
type Kind<S, V> = SongLuaOverlayKind<S, V, deadlib_present::actors::TextAttribute>;
type Output<S, V> = crate::CompiledSongLua<SongLuaOverlayActor<Kind<S, V>>>;
mod live;
pub use live::{compile_song_lua_layers, reload_song_lua};
type ModelSlots<S> = fn(&Path, &Path, &Path) -> Result<Arc<[S]>, String>;

/// Compile finite preparation output. A live session retains its runtime at the
/// activation boundary instead.

pub fn compile_prepared_song_lua_layers<S, V>(
    paths: &[&Path],
    primary: usize,
    context: &SongLuaCompileContext,
    noteskins: SongLuaNoteskinResolver,
    slots: ModelSlots<S>,
    layer: fn(&S) -> Option<SongLuaOverlayModelLayer<V>>,
    mut visual: impl FnMut(&SongLuaCompileContext, &str) -> Option<(Kind<S, V>, SongLuaOverlayState)>,
) -> Result<Vec<Output<S, V>>, String> {
    if paths.is_empty() && !context.live {
        return Ok(Vec::new());
    }
    if context.live {
        return Err("finite song preparation requires an offline context".into());
    }
    let mut timer = SongLuaCompileTimer::start();
    let plan = SongLayerPlan::new(paths, primary, context)?;
    let paths = plan.paths().map(Path::to_path_buf).collect::<Vec<_>>();
    let trace = paths
        .get(primary)
        .cloned()
        .unwrap_or_else(|| context.song_dir.clone());
    let mut runtime =
        SongRuntime::new_with_noteskin_resolver(context, noteskins).map_err(|e| e.to_string())?;
    runtime
        .begin_song_compilation()
        .map_err(|e| e.to_string())?;
    let (mut screen, mut selected) = runtime
        .construct_gameplay_screen(context)
        .map_err(|e| e.to_string())?;
    runtime
        .load_gameplay_theme(screen, &selected)
        .map_err(|e| e.to_string())?;
    timer.push_stage("host");
    #[cfg(feature = "test-support")]
    runtime.observations.start();
    let mut reloads = 0;
    let mut prior = Vec::new();
    let (root, mut activation) = loop {
        #[cfg(feature = "test-support")]
        runtime.observations.skin_active(true);
        let root = runtime
            .initialize_song_layers(screen, &plan)
            .map_err(|e| e.to_string())?
            .ok_or("song screen was released during initialization")?;
        timer.push_stage("execute_init");
        let mut activation = runtime
            .activate_song_layers(screen, &selected)
            .map_err(|e| e.to_string())?
            .ok_or("song screen was released during activation")?;
        if let Some(request) = &activation.new_screen {
            if request != "ScreenGameplay" {
                return Err(format!(
                    "song Lua startup requested unsupported screen '{request}'"
                ));
            }
            if reloads == 128 {
                return Err("song Lua exceeded 128 startup gameplay reloads".into());
            }
            prior.append(&mut activation.broadcasts);
            runtime
                .actors
                .compiler_read(&mut runtime.vm, root, Query::End)
                .map_err(|e| e.to_string())?;
            runtime
                .retire_actor_scene(screen)
                .map_err(|e| e.to_string())?;
            (screen, selected) = runtime
                .reconstruct_gameplay_screen(context)
                .map_err(|e| e.to_string())?;
            runtime
                .load_gameplay_theme(screen, &selected)
                .map_err(|e| e.to_string())?;
            reloads += 1;
            continue;
        }
        prior.append(&mut activation.broadcasts);
        activation.broadcasts = prior;
        break (root, activation);
    };
    timer.push_stage("startup_commands");
    runtime.require_input_offline_compatible()?;
    runtime
        .discover_song_startup_commands(screen, &activation)
        .map_err(|e| e.to_string())?;
    #[cfg(feature = "test-support")]
    let startup_observations = runtime.observations.take();
    let queued = runtime
        .song_startup_queued_states(screen)
        .ok_or("song startup state was released")?;
    let mut screen_startup = runtime
        .actors
        .screen_startup_states(&mut runtime.vm, screen)
        .map_err(|e| e.to_string())?;
    crate::lua_util::bake_startup_tweens(
        activation
            .tweens
            .values_mut()
            .chain(screen_startup.values_mut())
            .chain(activation.updates.values_mut()),
        crate::perframe::update_function_replay_beats(
            &selected,
            0.,
            crate::perframe::update_function_end_beat(&selected),
        )
        .into_iter(),
        crate::perframe::reference_actor_frames(&selected).as_deref(),
    );
    timer.push_stage("update_functions");
    runtime
        .actors
        .compiler_read(&mut runtime.vm, root, Query::Draw)
        .map_err(|e| e.to_string())?;
    timer.push_stage("draw_functions");
    let mut discovered = runtime.actors.take_song_broadcasts();
    let mut sounds = runtime.take_prepared_sounds();
    let mut judgments = runtime.take_judgment_textures();
    let mut sprites = runtime
        .take_sprite_textures()
        .into_iter()
        .map(|(id, event)| (id as usize, event))
        .collect();
    runtime.end_actor_capture();
    let startup_beat = crate::song_beat_at_elapsed_seconds(
        crate::perframe::first_update_seconds(&selected),
        &selected,
    );
    for (beat, _, _) in &mut discovered {
        *beat = startup_beat;
    }
    for event in &mut sounds {
        if event.queued_startup {
            event.beat = startup_beat;
        }
    }
    let mut names = runtime.new_song_easing_names().map_err(|e| e.to_string())?;
    runtime.register_loaded_song_easing_names(&mut names)?;
    timer.push_stage("easing_names");
    runtime
        .actors
        .compiler_read(&mut runtime.vm, root, Query::MarkLayers)
        .map_err(|e| e.to_string())?;
    let mut out = Output::<S, V> {
        startup: activation.options.clone(),
        loaded_steps: Some(selected.player_step_indices.map(|index| {
            index.and_then(|index| {
                if selected.song_step_chart_indices.is_empty() {
                    Some(index)
                } else {
                    selected
                        .song_step_chart_indices
                        .get(index)
                        .copied()
                        .flatten()
                }
            })
        })),
        loaded_style: Some(selected.style_name.clone()),
        startup_reloads: reloads,
        entry_path: paths[primary].clone(),
        screen_width: selected.screen_width,
        screen_height: selected.screen_height,
        ..Default::default()
    };
    #[cfg(feature = "test-support")]
    startup_observations.apply(&mut out);
    crate::compiled_layers::merge_runtime_messages(&mut out.messages, 0, &activation.broadcasts);
    let runtime_message_start = out.messages.len();
    crate::compiled_layers::merge_runtime_messages(
        &mut out.messages,
        runtime_message_start,
        &discovered,
    );
    let mut rows =
        runtime.read_song_preparation_overlay_actors(root, &selected, slots, layer, |detail| {
            crate::push_unique_compile_detail(
                &mut out.info.skipped_message_command_captures,
                detail,
            )
        })?;
    let screen_value = runtime
        .actor_scene_root(screen)
        .ok_or("released song screen")?;
    runtime
        .actors
        .compiler_read(&mut runtime.vm, screen_value, Query::MarkScreen(primary))
        .map_err(|e| e.to_string())?;
    let state = runtime
        .actors
        .compiler_read(&mut runtime.vm, screen_value, Query::State)
        .map_err(|e| e.to_string())?
        .state
        .unwrap();
    let screen_index = rows.actors.len();
    runtime.append_compiler_row(
        &mut rows,
        screen_value,
        SongLuaOverlayActor {
            kind: SongLuaOverlayKind::ActorFrame,
            name: Some("ScreenGameplay".into()),
            parent_index: None,
            initial_state: state,
            message_commands: Vec::new(),
        },
    )?;
    out.screen_overlay_index = Some(screen_index);
    let mut targets = runtime.compiler_targets(&mut rows)?;
    runtime.capture_song_stable_cross_actor_message_commands(&mut targets, |detail| {
        crate::push_unique_compile_detail(&mut out.info.skipped_message_command_captures, detail)
    })?;
    let mut message_sounds = Vec::new();
    for row in &rows.actors {
        let value = runtime
            .song_overlay_actor_value(row)
            .map_err(|e| e.to_string())?;
        let tag = runtime
            .actors
            .compiler_read(&mut runtime.vm, value, Query::Layer(primary))
            .map_err(|e| e.to_string())?
            .layer;
        message_sounds.extend(
            row.message_sounds
                .iter()
                .cloned()
                .map(|(message, path)| (tag, message, path)),
        );
    }
    timer.push_stage("read_overlays");
    let mut tracked = runtime
        .read_song_tracked_actors(screen)
        .map_err(|e| format!("failed to read tracked actors: {e}"))?
        .ok_or("released song screen")?;
    let mut hidden = std::array::from_fn(|player| {
        tracked
            .actors
            .get(player)
            .is_some_and(|a| !a.actor.initial_state.visible)
    });
    let static_overlay = crate::runtime_static_overlay_index_by_path(rows.actors.len(), |i| {
        if let Kind::Sprite { texture_path, .. } = &rows.actors[i].actor.kind {
            Some(texture_path.as_path())
        } else {
            None
        }
    });
    let mut schedule = SongStaticSchedule {
        messages: std::mem::take(&mut out.messages),
        sound_events: sounds,
        info: std::mem::take(&mut out.info),
        ..Default::default()
    };
    let mut targets = runtime.compiler_targets(&mut rows)?;
    let mut tracked_targets = runtime.compiler_tracked_targets(&mut tracked)?;
    let mut tables = runtime.compile_song_static_schedule(
        SongStaticInputs {
            root,
            initialized_globals: activation.initialized_globals,
            names: &names,
            context: &selected,
            static_overlay,
        },
        &mut targets,
        &mut tracked_targets,
        &mut schedule,
        |stage| timer.push_stage(stage),
    )?;
    out.beat_mods = schedule.beat_mods;
    out.time_mods = schedule.time_mods;
    out.eases = schedule.eases;
    out.overlay_eases = schedule.overlay_eases;
    out.column_offsets = schedule.column_offsets;
    out.messages = schedule.messages;
    out.info = schedule.info;
    sounds = schedule.sound_events;
    runtime.apply_compiler_startup(
        &mut rows,
        &queued,
        &activation,
        &mut out.messages,
        &selected,
    )?;
    let targets = runtime.compiler_targets(&mut rows)?;
    let (eases, overlay_eases, info) = runtime.compile_song_static_perframes(
        &mut tables,
        &selected,
        &targets,
        &tracked,
        &out.messages,
    )?;
    out.eases.extend(eases);
    out.overlay_eases.extend(overlay_eases);
    crate::merge_compile_info(&mut out.info, info);
    timer.push_stage("perframes");
    out.note_hides = runtime.read_song_note_column_zoom_hides()?;
    timer.push_stage("note_hides");
    let update = runtime.compile_song_update_stage(
        screen,
        root,
        &selected,
        &mut rows,
        &mut tracked,
        &out.messages,
        &activation,
        &mut sounds,
        &mut judgments,
        &mut sprites,
        noteskins,
        layer,
        |skin| visual(&selected, skin),
    )?;
    #[cfg(feature = "test-support")]
    {
        runtime.observations.take().apply(&mut out);
        out.overlay_writes = update.overlay_writes;
    }
    out.eases.extend(update.eases);
    out.overlay_eases.extend(update.overlay_eases);
    out.overlay_updates = update.overlay_tracks;
    out.column_offsets.extend(update.column_transforms);
    out.column_splines = update.column_splines;
    out.overlay_update_unit = if selected.song_timing.is_some() {
        SongLuaTimeUnit::Second
    } else {
        SongLuaTimeUnit::Beat
    };
    crate::compiled_layers::merge_runtime_messages(
        &mut out.messages,
        runtime_message_start,
        &update.broadcasts,
    );
    for capture in update.stateful_messages {
        out.info.skipped_message_command_captures.retain(|detail| {
            !detail.contains(&format!(
                ".{}MessageCommand changes cross-actor targets or effects",
                capture.message
            ))
        });
        out.stateful_message_captures.push(capture);
    }
    out.column_spline_origin = selected
        .song_timing
        .as_ref()
        .map(|t| t.get_time_for_beat_exact(0.));
    for detail in runtime.actors.take_callback_errors() {
        out.info.unsupported_perframes += 1;
        crate::push_unique_compile_detail(&mut out.info.unsupported_perframe_captures, detail);
    }
    timer.push_stage("update_overlays");
    let mut targets = runtime.compiler_targets(&mut rows)?;
    let mut tracked_targets = runtime.compiler_tracked_targets(&mut tracked)?;
    let deferred = runtime.capture_song_deferred_messages(
        &mut targets,
        &mut tracked_targets,
        &out.stateful_message_captures,
        &mut out.info.skipped_message_command_captures,
    )?;
    for (index, message, path) in deferred {
        let value = runtime
            .song_overlay_actor_value(&rows.actors[index])
            .map_err(|e| e.to_string())?;
        let tag = runtime
            .actors
            .compiler_read(&mut runtime.vm, value, Query::Layer(primary))
            .map_err(|e| e.to_string())?
            .layer;
        message_sounds.push((tag, message, path));
    }
    timer.push_stage("deferred_messages");
    for row in &mut rows.actors {
        for command in &mut row.actor.message_commands {
            if update
                .broadcasts
                .iter()
                .any(|(_, message, _)| message == &command.message)
            {
                command.blocks.retain(|block| !block.queued);
            }
        }
    }
    runtime.finalize_compiler_kinds(&mut rows, &mut hidden)?;
    runtime.finalize_compiler_sprites(&mut rows, sprites)?;
    let seconds = crate::perframe::first_update_seconds(&selected);
    let time = selected.song_timing.as_ref().map_or_else(
        || crate::song_beat_at_elapsed_seconds(seconds, &selected),
        |timing| timing.get_time_for_beat_exact(0.) + seconds * crate::song_music_rate(&selected),
    );
    for track in &mut out.overlay_updates {
        let value = runtime
            .song_overlay_actor_value(&rows.actors[track.overlay_index])
            .map_err(|e| e.to_string())?;
        if track.target != crate::SongLuaOverlayUpdateTarget::EffectTime
            && matches!(value,Value::Table(id) if queued.contains_key(&id))
        {
            for sample in track.samples.iter_mut().take_while(|s| s.time < time) {
                sample.time = time;
            }
        }
    }
    runtime.apply_compiler_screen_startup(&mut tracked, &screen_startup, &mut out.messages)?;
    for row in &mut rows.actors {
        let value = runtime
            .song_overlay_actor_value(row)
            .map_err(|e| e.to_string())?;
        row.actor.initial_state.spin_baked = runtime
            .actors
            .compiler_read(&mut runtime.vm, value, Query::Spin)
            .map_err(|e| e.to_string())?
            .spin;
    }
    for row in &mut tracked.actors {
        let value = runtime
            .song_tracked_actor_value(row)
            .map_err(|e| e.to_string())?;
        row.actor.initial_state.spin_baked = runtime
            .actors
            .compiler_read(&mut runtime.vm, value, Query::Spin)
            .map_err(|e| e.to_string())?
            .spin;
    }
    crate::push_startup_message_if_listened(
        &mut out.messages,
        rows.actors
            .iter()
            .map(|r| r.actor.message_commands.as_slice()),
    );
    remove_untouched_screen(&mut out, &mut rows, screen_index);
    runtime.take_compiler_materials(&mut rows)?;
    let mut sources = Vec::new();
    for (i, row) in rows.actors.iter().enumerate() {
        sources.push((
            runtime
                .song_overlay_actor_value(row)
                .map_err(|e| e.to_string())?,
            crate::SongLuaDrawSource::Overlay(i),
        ));
    }
    for row in &tracked.actors {
        sources.push((
            runtime
                .song_tracked_actor_value(row)
                .map_err(|e| e.to_string())?,
            draw_source(row.target),
        ));
    }
    out.draw_frames = runtime
        .take_actor_draw_frames(&sources)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(_, frame)| frame)
        .collect();
    let mut tags = Vec::new();
    for row in &rows.actors {
        let value = runtime
            .song_overlay_actor_value(row)
            .map_err(|e| e.to_string())?;
        tags.push(
            runtime
                .actors
                .compiler_read(&mut runtime.vm, value, Query::Layer(primary))
                .map_err(|e| e.to_string())?
                .layer,
        );
    }
    out.overlays = std::mem::take(&mut rows.actors)
        .into_iter()
        .map(|r| r.actor)
        .collect();
    for (tag, message, path) in message_sounds {
        if !sounds
            .iter()
            .any(|e| e.source_message.as_deref() == Some(message.as_str()))
        {
            out.overlays.push(sound_actor(message, path));
            tags.push(tag);
        }
    }
    for (i, event) in sounds.into_iter().enumerate() {
        let message = format!("__songlua_sound_call_{i}");
        out.messages.push(SongLuaMessageEvent {
            beat: event.beat,
            message: message.clone(),
            persists: false,
        });
        out.overlays.push(sound_actor(message, event.path));
        tags.push(primary);
    }
    finalize_tracked(&mut out, std::mem::take(&mut tracked.actors), judgments);
    out.hidden_players = hidden;
    crate::sort_compiled_song_lua(&mut out);
    out.sound_paths = runtime
        .actors
        .compiler_read(&mut runtime.vm, root, Query::SoundPaths)
        .map_err(|e| e.to_string())?
        .paths;
    let mut seen = out
        .sound_paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    crate::push_song_lua_overlay_sound_paths(&out.overlays, &mut seen, &mut out.sound_paths);
    runtime.require_input_offline_compatible()?;
    runtime
        .release_song_static_tables(&mut tables)
        .map_err(|e| e.to_string())?;
    runtime
        .release_song_easing_names(&mut names)
        .map_err(|e| e.to_string())?;
    runtime
        .release_song_overlay_actors(&mut rows)
        .map_err(|e| e.to_string())?;
    runtime
        .release_song_tracked_actors(&mut tracked)
        .map_err(|e| e.to_string())?;
    timer.push_stage("finalize");
    crate::log_song_lua_compile_timing(&trace, &timer);
    crate::compiled_layers::split_compiled_song_lua(out, tags, &paths, primary)
}

impl SongRuntime {
    fn append_compiler_row<K>(
        &mut self,
        rows: &mut SongOverlayActors<K>,
        value: Value,
        actor: SongLuaOverlayActor<K>,
    ) -> Result<(), String> {
        let owner = rows.owner.ok_or("released song overlays")?;
        let Value::Table(array) = self.vm.rooted(owner).map_err(|e| e.to_string())? else {
            unreachable!()
        };
        let slot = self.vm.raw_table_len(array).map_err(|e| e.to_string())? + 1;
        self.vm
            .raw_table_set(array, Value::Number(slot as f64), value)
            .map_err(|e| e.to_string())?;
        rows.actors.push(SongOverlayActor {
            actor,
            message_sounds: Vec::new(),
            owner,
            slot,
        });
        Ok(())
    }
    fn compiler_targets<'a, K>(
        &self,
        rows: &'a mut SongOverlayActors<K>,
    ) -> Result<Vec<SongMessageTarget<'a>>, String> {
        let values = rows
            .actors
            .iter()
            .map(|r| self.song_overlay_actor_value(r).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .actors
            .iter_mut()
            .zip(values)
            .map(|(r, actor)| SongMessageTarget {
                actor,
                commands: &mut r.actor.message_commands,
            })
            .collect())
    }
    fn compiler_tracked_targets<'a>(
        &self,
        rows: &'a mut SongTrackedActors,
    ) -> Result<Vec<SongMessageTarget<'a>>, String> {
        let values = rows
            .actors
            .iter()
            .map(|r| self.song_tracked_actor_value(r).map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .actors
            .iter_mut()
            .zip(values)
            .map(|(r, actor)| SongMessageTarget {
                actor,
                commands: &mut r.actor.message_commands,
            })
            .collect())
    }
    fn apply_compiler_startup<K>(
        &self,
        rows: &mut SongOverlayActors<K>,
        queued: &HashMap<TableId, SongStartupState>,
        activation: &SongActivation,
        messages: &mut Vec<SongLuaMessageEvent>,
        context: &SongLuaCompileContext,
    ) -> Result<(), String> {
        let mut changed = false;
        for row in &mut rows.actors {
            let Value::Table(id) = self
                .song_overlay_actor_value(row)
                .map_err(|e| e.to_string())?
            else {
                unreachable!()
            };
            if let Some(state) = queued.get(&id)
                && let Some(command) =
                    crate::perframe::startup_command(state, row.actor.initial_state)
            {
                row.actor.initial_state = state.initial;
                row.actor.message_commands.push(command);
                changed = true;
            }
        }
        if changed {
            messages.push(SongLuaMessageEvent {
                beat: crate::song_beat_at_elapsed_seconds(
                    crate::perframe::first_update_seconds(context),
                    context,
                ),
                message: "__songlua_queued_startup".into(),
                persists: true,
            });
        }
        changed = false;
        for row in &mut rows.actors {
            let Value::Table(id) = self
                .song_overlay_actor_value(row)
                .map_err(|e| e.to_string())?
            else {
                unreachable!()
            };
            if let Some(state) = activation.tweens.get(&id)
                && state.blocks.iter().any(|b| {
                    (b.duration > 0.
                        || b.progress
                            .as_ref()
                            .is_some_and(|s| s.first().is_some_and(|s| s[0] > 0.)))
                        && b.delta != SongLuaOverlayStateDelta::default()
                })
            {
                row.actor.initial_state =
                    crate::overlay_state_after_blocks(state.initial, &state.blocks, 0.);
                row.actor
                    .message_commands
                    .push(SongLuaOverlayMessageCommand {
                        frame_advance: 0.,
                        message: "__songlua_actor_startup".into(),
                        blocks: state.blocks.clone(),
                    });
                changed = true;
            }
        }
        if changed {
            messages.push(SongLuaMessageEvent {
                beat: 0.,
                message: "__songlua_actor_startup".into(),
                persists: true,
            });
        }
        for row in &mut rows.actors {
            let Value::Table(id) = self
                .song_overlay_actor_value(row)
                .map_err(|e| e.to_string())?
            else {
                unreachable!()
            };
            if let Some(state) = activation.updates.get(&id) {
                for block in &state.blocks {
                    if block.duration == 0.
                        && block.start <= 0.
                        && !block.queued
                        && block.progress.is_none()
                    {
                        crate::apply_overlay_delta(&mut row.actor.initial_state, &block.delta);
                    }
                }
            }
        }
        Ok(())
    }
    fn apply_compiler_screen_startup(
        &self,
        rows: &mut SongTrackedActors,
        states: &HashMap<TableId, SongStartupState>,
        messages: &mut Vec<SongLuaMessageEvent>,
    ) -> Result<(), String> {
        let mut changed = false;
        for row in &mut rows.actors {
            if !matches!(row.target, Target::ScreenLayer(_)) {
                continue;
            }
            let Value::Table(id) = self
                .song_tracked_actor_value(row)
                .map_err(|e| e.to_string())?
            else {
                unreachable!()
            };
            if let Some(state) = states.get(&id)
                && !state.blocks.is_empty()
            {
                row.actor.initial_state =
                    crate::overlay_state_after_blocks(state.initial, &state.blocks, 0.);
                row.actor
                    .message_commands
                    .push(SongLuaOverlayMessageCommand {
                        frame_advance: 0.,
                        message: "__songlua_screen_startup".into(),
                        blocks: state.blocks.clone(),
                    });
                changed = true;
            }
        }
        if changed
            && !messages
                .iter()
                .any(|m| m.message == "__songlua_screen_startup")
        {
            messages.push(SongLuaMessageEvent {
                beat: 0.,
                message: "__songlua_screen_startup".into(),
                persists: true,
            });
        }
        Ok(())
    }
    fn finalize_compiler_kinds<S, V>(
        &mut self,
        rows: &mut SongOverlayActors<Kind<S, V>>,
        hidden: &mut [bool; LUA_PLAYERS],
    ) -> Result<(), String> {
        let mut indices = HashMap::new();
        if rows
            .actors
            .iter()
            .any(|r| matches!(r.actor.kind, Kind::ActorProxy { .. }))
        {
            for (i, row) in rows.actors.iter().enumerate() {
                let value = self
                    .song_overlay_actor_value(row)
                    .map_err(|e| e.to_string())?;
                indices.insert(
                    self.vm
                        .object_pointer(value)
                        .ok_or("overlay identity is missing")?,
                    i,
                );
            }
        }
        for pass in 0..2 {
            for row in &mut rows.actors {
                let tag = match (pass, &row.actor.kind) {
                    (0, Kind::Actor) => 0,
                    (0, Kind::ActorProxy { .. }) => 1,
                    (1, Kind::ActorFrameTexture { .. }) => 2,
                    (1, Kind::Actor) => 3,
                    _ => continue,
                };
                let actor = self
                    .song_overlay_actor_value(row)
                    .map_err(|e| e.to_string())?;
                let read = self
                    .actors
                    .compiler_read(&mut self.vm, actor, Query::Late(tag))
                    .map_err(|e| e.to_string())?;
                match read.kind {
                    Some(Late::Sprite(texture_path, states)) => {
                        row.actor.kind = Kind::Sprite {
                            texture_key: crate::lua_util::texture_path_key(&texture_path),
                            texture_path,
                            states: states.into(),
                            textures: Arc::from([]),
                        }
                    }
                    Some(Late::AftSprite(capture_name)) => {
                        row.actor.kind = Kind::AftSprite { capture_name }
                    }
                    Some(Late::Aft(frame)) => {
                        row.actor.kind = Kind::ActorFrameTexture {
                            capture_name: frame.capture_name,
                            capture_size: frame.capture_size,
                            alpha_buffer: frame.alpha_buffer,
                            depth_buffer: frame.depth_buffer,
                            float_buffer: frame.float_buffer,
                        }
                    }
                    Some(Late::Proxy(mut target, player)) => {
                        if let crate::SongLuaProxyTarget::Actor { overlay_index } = &mut target {
                            *overlay_index =
                                indices.get(overlay_index).copied().unwrap_or(usize::MAX);
                        }
                        if let Some(player) = player.and_then(|p| hidden.get_mut(p)) {
                            *player = true;
                        }
                        row.actor.kind = Kind::ActorProxy { target };
                    }
                    None => (),
                }
            }
        }
        Ok(())
    }
    fn finalize_compiler_sprites<S, V>(
        &mut self,
        rows: &mut SongOverlayActors<Kind<S, V>>,
        events: Vec<(usize, crate::SongLuaSpriteTexture)>,
    ) -> Result<(), String> {
        let mut groups: HashMap<usize, (Vec<crate::SongLuaSpriteTexture>, usize)> = HashMap::new();
        if !events.is_empty() {
            for row in &rows.actors {
                if matches!(row.actor.kind, Kind::Sprite { .. }) {
                    let value = self
                        .song_overlay_actor_value(row)
                        .map_err(|e| e.to_string())?;
                    groups
                        .entry(self.vm.object_pointer(value).unwrap())
                        .or_default()
                        .1 += 1;
                }
            }
            for (id, event) in events {
                if let Some((events, _)) = groups.get_mut(&id) {
                    events.push(event);
                }
            }
            for (events, _) in groups.values_mut() {
                events.sort_by(|a, b| a.second.total_cmp(&b.second));
            }
        }
        for row in &mut rows.actors {
            let value = self
                .song_overlay_actor_value(row)
                .map_err(|e| e.to_string())?;
            if let Kind::Sprite { textures, .. } = &mut row.actor.kind {
                *textures = if groups.is_empty() {
                    Vec::new()
                } else {
                    let (events, remaining) = groups
                        .get_mut(&self.vm.object_pointer(value).unwrap())
                        .unwrap();
                    take_events(events, remaining)
                }
                .into();
            }
        }
        Ok(())
    }
    fn take_compiler_materials<S, V>(
        &mut self,
        rows: &mut SongOverlayActors<Kind<S, V>>,
    ) -> Result<(), String> {
        for row in &mut rows.actors {
            if !matches!(
                row.actor.kind,
                Kind::Model { .. } | Kind::NoteskinActor { .. }
            ) {
                continue;
            }
            let value = self
                .song_overlay_actor_value(row)
                .map_err(|e| e.to_string())?;
            let samples = self.take_actor_model_material_samples(value)?;
            if samples.is_empty() {
                continue;
            }
            match &mut row.actor.kind {
                Kind::Model { layers } => {
                    for (layer, samples) in Arc::make_mut(layers).iter_mut().zip(samples) {
                        layer.texture_samples = samples;
                    }
                }
                Kind::NoteskinActor {
                    texture_samples, ..
                } => *texture_samples = samples.into(),
                _ => (),
            }
        }
        Ok(())
    }
}

fn take_events<T: Clone>(events: &mut Vec<T>, remaining: &mut usize) -> Vec<T> {
    if events.is_empty() {
        return Vec::new();
    }
    *remaining -= 1;
    if *remaining == 0 {
        std::mem::take(events)
    } else {
        events.clone()
    }
}
fn finalize_tracked<S, V>(
    out: &mut Output<S, V>,
    rows: Vec<SongTrackedActor>,
    events: Vec<(usize, crate::SongLuaJudgmentTexture)>,
) {
    let mut groups: [(Vec<crate::SongLuaJudgmentTexture>, usize); LUA_PLAYERS] =
        std::array::from_fn(|_| (Vec::new(), 0));
    if !events.is_empty() {
        for row in &rows {
            if let Target::PlayerJudgment(player) = row.target {
                groups[player].1 += 1;
            }
        }
        for (player, event) in events {
            if let Some((events, n)) = groups.get_mut(player)
                && *n != 0
            {
                events.push(event);
            }
        }
        for (events, _) in &mut groups {
            events.sort_by(|a, b| a.second.total_cmp(&b.second));
            events.dedup_by(|a, b| a.path == b.path);
        }
    }
    for row in rows {
        match row.target {
            Target::Player(player) => out.player_actors[player] = row.actor,
            Target::PlayerJudgment(player) => {
                let (events, n) = &mut groups[player];
                out.player_actors[player].judgment = crate::SongLuaCapturedChildActor {
                    initial_state: row.actor.initial_state,
                    message_commands: row.actor.message_commands,
                    textures: take_events(events, n),
                };
            }
            Target::PlayerCombo(player) => {
                out.player_actors[player].combo = crate::SongLuaCapturedChildActor {
                    initial_state: row.actor.initial_state,
                    message_commands: row.actor.message_commands,
                    ..Default::default()
                }
            }
            Target::SongForeground => out.song_foreground = row.actor,
            Target::ScreenLayer(index) => out.screen_layers[index] = row.actor,
        }
    }
}
fn sound_actor<S, V, A>(
    message: String,
    sound_path: PathBuf,
) -> SongLuaOverlayActor<SongLuaOverlayKind<S, V, A>> {
    SongLuaOverlayActor {
        kind: SongLuaOverlayKind::Sound { sound_path },
        name: None,
        parent_index: None,
        initial_state: Default::default(),
        message_commands: vec![SongLuaOverlayMessageCommand {
            frame_advance: 0.,
            message,
            blocks: vec![SongLuaOverlayCommandBlock {
                progress: None,
                queued: false,
                start: 0.,
                duration: 0.,
                easing: None,
                opt1: None,
                opt2: None,
                delta: SongLuaOverlayStateDelta {
                    sound_play: Some(true),
                    ..Default::default()
                },
            }],
        }],
    }
}
fn draw_source(target: Target) -> crate::SongLuaDrawSource {
    use crate::SongLuaDrawSource as Draw;
    match target {
        Target::Player(p) => Draw::Player(p),
        Target::PlayerJudgment(p) => Draw::Judgment(p),
        Target::PlayerCombo(p) => Draw::Combo(p),
        Target::SongForeground => Draw::SongForeground,
        Target::ScreenLayer(i) => Draw::ScreenLayer(i),
    }
}

fn remove_untouched_screen<S, V>(
    out: &mut Output<S, V>,
    rows: &mut SongOverlayActors<Kind<S, V>>,
    screen: usize,
) {
    if rows.actors[screen].actor.initial_state != SongLuaOverlayState::default()
        || !rows.actors[screen].actor.message_commands.is_empty()
        || out.overlay_eases.iter().any(|e| e.overlay_index == screen)
        || out
            .overlay_updates
            .iter()
            .any(|e| e.overlay_index == screen)
    {
        return;
    }
    rows.actors.remove(screen);
    out.screen_overlay_index = None;
    let remap = |index: &mut usize| {
        if *index > screen {
            *index -= 1;
        }
    };
    for row in &mut rows.actors {
        if let Some(index) = &mut row.actor.parent_index {
            remap(index);
        }
        if let Kind::ActorProxy {
            target: crate::SongLuaProxyTarget::Actor { overlay_index },
        } = &mut row.actor.kind
        {
            remap(overlay_index);
        }
    }
    for ease in &mut out.overlay_eases {
        remap(&mut ease.overlay_index);
    }
    for track in &mut out.overlay_updates {
        remap(&mut track.overlay_index);
    }
    #[cfg(feature = "test-support")]
    for track in &mut out.overlay_writes {
        remap(&mut track.overlay_index);
    }
    for capture in &mut out.stateful_message_captures {
        for (index, _) in &mut capture.overlay_targets {
            remap(index);
        }
        for write in &mut capture.writes {
            remap(&mut write.overlay_index);
        }
    }
}

pub fn compile_song_lua<S, V, F>(
    path: &Path,
    context: &SongLuaCompileContext,
    noteskins: SongLuaNoteskinResolver,
    slots: ModelSlots<S>,
    layer: fn(&S) -> Option<SongLuaOverlayModelLayer<V>>,
    visual: F,
) -> Result<Output<S, V>, String>
where
    F: FnMut(&SongLuaCompileContext, &str) -> Option<(Kind<S, V>, SongLuaOverlayState)>,
{
    compile_song_lua_layers(&[path], 0, context, noteskins, slots, layer, visual)?
        .into_iter()
        .next()
        .ok_or_else(|| "song lua compiler returned no result for one entry".to_string())
}

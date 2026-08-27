use deadlib_present::actors::TextAttribute;
use mlua::{Lua, Table, Value};
use std::path::Path;
use std::sync::Arc;

use crate::{
    CompiledSongLua, SongLuaCompileContext, SongLuaCompileTimer, SongLuaHostState,
    SongLuaNoteskinResolver, SongLuaOverlayActor, SongLuaOverlayKind, SongLuaOverlayModelLayer,
    SongLuaOverlayState, SongLuaTimeUnit, SongLuaTrackedActorTarget as TrackedCompileActorTarget,
    add_actor_child_from_path as add_host_actor_child_from_path,
    compile_multitap_update_overlays_for_actors, compile_perframes, compile_update_functions,
    create_dummy_actor as create_host_dummy_actor,
    create_named_child_actor as create_host_named_child_actor, ensure_overlay_arrow_visual,
    entry_file_path, execute_script_file, install_actor_methods as install_host_actor_methods,
    install_compile_host, log_song_lua_compile_timing, merge_compile_info,
    note_field_column_actors as host_note_field_column_actors, push_startup_message_if_listened,
    push_unique_compile_detail, read_actor_model_layers, read_eases_for_overlay_actors,
    read_global_function_nested_tables, read_mod_windows, read_note_column_zoom_hides,
    read_noteskin_tap_actor_slots, read_overlay_compile_actor_actions, read_overlay_compile_actors,
    read_proxy_target_kind, read_runtime_mod_eases, read_song_lua_sound_paths,
    read_tracked_compile_actors, read_update_function_nested_tables,
    read_update_function_overlay_compile_actor_actions, read_update_function_tables,
    read_xero_runtime_mod_eases_for_overlay_actors, register_loaded_easing_names,
    restore_compile_globals, run_actor_draw_functions, run_actor_init_commands,
    run_actor_startup_commands, run_actor_update_functions,
    runtime_static_overlay_index_for_actors, snapshot_compile_globals, sort_compiled_song_lua,
};

const COMPILE_LAYER_KEY: &str = "__songlua_compile_layer";

type DefaultCompiledSongLua<NoteskinSlot, ModelVertex> = CompiledSongLua<
    SongLuaOverlayActor<SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>>,
>;

pub fn compile_song_lua_with_default_host<NoteskinSlot, ModelVertex, MultitapArrowVisualSpec>(
    entry_path: &Path,
    context: &SongLuaCompileContext,
    noteskin_resolver: SongLuaNoteskinResolver,
    read_model_slots: fn(&Path) -> Result<Arc<[NoteskinSlot]>, String>,
    model_layer_from_slot: fn(&NoteskinSlot) -> Option<SongLuaOverlayModelLayer<ModelVertex>>,
    multitap_arrow_visual_spec: MultitapArrowVisualSpec,
) -> Result<
    CompiledSongLua<
        SongLuaOverlayActor<SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>>,
    >,
    String,
>
where
    MultitapArrowVisualSpec: FnMut(
        &SongLuaCompileContext,
        &str,
    ) -> Option<(
        SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>,
        SongLuaOverlayState,
    )>,
{
    compile_song_lua_with_actors(
        entry_path,
        context,
        noteskin_resolver,
        create_default_dummy_actor,
        create_default_named_child_actor,
        install_default_actor_methods,
        read_model_slots,
        model_layer_from_slot,
        multitap_arrow_visual_spec,
    )
}

pub fn compile_song_lua_layers_with_default_host<
    NoteskinSlot,
    ModelVertex,
    MultitapArrowVisualSpec,
>(
    entry_paths: &[&Path],
    primary_index: usize,
    context: &SongLuaCompileContext,
    noteskin_resolver: SongLuaNoteskinResolver,
    read_model_slots: fn(&Path) -> Result<Arc<[NoteskinSlot]>, String>,
    model_layer_from_slot: fn(&NoteskinSlot) -> Option<SongLuaOverlayModelLayer<ModelVertex>>,
    multitap_arrow_visual_spec: MultitapArrowVisualSpec,
) -> Result<Vec<DefaultCompiledSongLua<NoteskinSlot, ModelVertex>>, String>
where
    MultitapArrowVisualSpec: FnMut(
        &SongLuaCompileContext,
        &str,
    ) -> Option<(
        SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>,
        SongLuaOverlayState,
    )>,
{
    compile_song_lua_layers_with_actors(
        entry_paths,
        primary_index,
        context,
        noteskin_resolver,
        create_default_dummy_actor,
        create_default_named_child_actor,
        install_default_actor_methods,
        read_model_slots,
        model_layer_from_slot,
        multitap_arrow_visual_spec,
    )
}

fn create_default_named_child_actor(lua: &Lua, parent: &Table, name: &str) -> mlua::Result<Table> {
    create_host_named_child_actor(
        lua,
        parent,
        name,
        create_default_dummy_actor,
        create_default_named_child_actor,
    )
}

fn default_note_field_column_actors(lua: &Lua, note_field: &Table) -> mlua::Result<Table> {
    host_note_field_column_actors(lua, note_field, create_default_dummy_actor)
}

fn create_default_dummy_actor(lua: &Lua, actor_type: &'static str) -> mlua::Result<Table> {
    create_host_dummy_actor(lua, actor_type, install_default_actor_methods)
}

fn install_default_actor_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    install_host_actor_methods(
        lua,
        actor,
        add_default_actor_child_from_path,
        default_note_field_column_actors,
        create_default_named_child_actor,
        create_default_dummy_actor,
    )
}

fn add_default_actor_child_from_path(lua: &Lua, actor: &Table, path: &str) -> mlua::Result<()> {
    add_host_actor_child_from_path(lua, actor, path, create_default_dummy_actor)
}

pub fn compile_song_lua_with_actors<NoteskinSlot, ModelVertex, MultitapArrowVisualSpec>(
    entry_path: &Path,
    context: &SongLuaCompileContext,
    noteskin_resolver: SongLuaNoteskinResolver,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
    create_named_child_actor: fn(&Lua, &Table, &str) -> mlua::Result<Table>,
    install_actor_methods: fn(&Lua, &Table) -> mlua::Result<()>,
    read_model_slots: fn(&Path) -> Result<Arc<[NoteskinSlot]>, String>,
    model_layer_from_slot: fn(&NoteskinSlot) -> Option<SongLuaOverlayModelLayer<ModelVertex>>,
    multitap_arrow_visual_spec: MultitapArrowVisualSpec,
) -> Result<
    CompiledSongLua<
        SongLuaOverlayActor<SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>>,
    >,
    String,
>
where
    MultitapArrowVisualSpec: FnMut(
        &SongLuaCompileContext,
        &str,
    ) -> Option<(
        SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>,
        SongLuaOverlayState,
    )>,
{
    compile_song_lua_layers_with_actors(
        &[entry_path],
        0,
        context,
        noteskin_resolver,
        create_dummy_actor,
        create_named_child_actor,
        install_actor_methods,
        read_model_slots,
        model_layer_from_slot,
        multitap_arrow_visual_spec,
    )?
    .into_iter()
    .next()
    .ok_or_else(|| "song lua compiler returned no result for one entry".to_string())
}

pub fn compile_song_lua_layers_with_actors<NoteskinSlot, ModelVertex, MultitapArrowVisualSpec>(
    entry_paths: &[&Path],
    primary_index: usize,
    context: &SongLuaCompileContext,
    noteskin_resolver: SongLuaNoteskinResolver,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
    create_named_child_actor: fn(&Lua, &Table, &str) -> mlua::Result<Table>,
    install_actor_methods: fn(&Lua, &Table) -> mlua::Result<()>,
    read_model_slots: fn(&Path) -> Result<Arc<[NoteskinSlot]>, String>,
    model_layer_from_slot: fn(&NoteskinSlot) -> Option<SongLuaOverlayModelLayer<ModelVertex>>,
    mut multitap_arrow_visual_spec: MultitapArrowVisualSpec,
) -> Result<Vec<DefaultCompiledSongLua<NoteskinSlot, ModelVertex>>, String>
where
    MultitapArrowVisualSpec: FnMut(
        &SongLuaCompileContext,
        &str,
    ) -> Option<(
        SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>,
        SongLuaOverlayState,
    )>,
{
    if entry_paths.is_empty() {
        return Ok(Vec::new());
    }
    if primary_index >= entry_paths.len() {
        return Err(format!(
            "song lua primary index {primary_index} is outside {} entries",
            entry_paths.len()
        ));
    }
    let mut compile_timer = SongLuaCompileTimer::start();
    let entry_paths = entry_paths
        .iter()
        .map(|path| {
            entry_file_path(path)
                .ok_or_else(|| format!("song lua entry '{}' does not exist", path.display()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let trace_entry_path = entry_paths[primary_index].clone();
    let lua = Lua::new();
    let mut host = SongLuaHostState::default();
    install_compile_host(
        &lua,
        context,
        &mut host,
        noteskin_resolver,
        create_dummy_actor,
        create_named_child_actor,
        install_actor_methods,
    )
    .map_err(|err| err.to_string())?;
    compile_timer.push_stage("host");
    let roots = lua.create_table().map_err(|err| err.to_string())?;
    for (index, entry_path) in entry_paths.iter().enumerate() {
        let root = execute_script_file(&lua, entry_path, context.song_dir.as_path())
            .map_err(|err| format!("failed to execute '{}': {err}", entry_path.display()))?;
        run_actor_init_commands(&lua, &root).map_err(|err| {
            format!(
                "failed to run actor init commands for '{}': {err}",
                entry_path.display()
            )
        })?;
        roots
            .raw_set(index + 1, root)
            .map_err(|err| err.to_string())?;
    }
    compile_timer.push_stage("execute_init");
    let root = Value::Table(roots);
    run_actor_startup_commands(&lua, &root).map_err(|err| {
        format!(
            "failed to run actor startup commands for song lua session '{}': {err}",
            trace_entry_path.display()
        )
    })?;
    compile_timer.push_stage("startup_commands");
    run_actor_update_functions(&lua, &root).map_err(|err| {
        format!(
            "failed to run actor update functions for song lua session '{}': {err}",
            trace_entry_path.display()
        )
    })?;
    compile_timer.push_stage("update_functions");
    run_actor_draw_functions(&lua, &root);
    compile_timer.push_stage("draw_functions");
    register_loaded_easing_names(&lua, &mut host).map_err(|err| err.to_string())?;
    compile_timer.push_stage("easing_names");
    mark_compile_layers(&root).map_err(|err| err.to_string())?;

    let globals = lua.globals();
    let mut out = CompiledSongLua {
        entry_path: entry_paths[primary_index].clone(),
        screen_width: context.screen_width,
        screen_height: context.screen_height,
        ..CompiledSongLua::default()
    };
    let compile_globals =
        snapshot_compile_globals(&lua, &globals).map_err(|err| err.to_string())?;
    let overlays = read_overlay_compile_actors(
        &lua,
        &root,
        context,
        |actor| read_actor_model_layers(actor, read_model_slots, model_layer_from_slot),
        |actor, _context| read_noteskin_tap_actor_slots(actor, read_model_slots),
        |skipped| {
            push_unique_compile_detail(&mut out.info.skipped_message_command_captures, skipped);
        },
    );
    restore_compile_globals(&globals, compile_globals).map_err(|err| err.to_string())?;
    let mut overlays = overlays?;
    compile_timer.push_stage("read_overlays");
    let mut tracked_actors = read_tracked_compile_actors(&lua, create_named_child_actor)?;
    let mut hidden_players = std::array::from_fn(|player| {
        tracked_actors
            .get(player)
            .is_some_and(|tracked| !tracked.actor.initial_state.visible)
    });
    let mut overlay_trigger_counter = 0usize;
    let prefix_perframes = globals
        .get::<Option<Table>>("prefix_globals")
        .map_err(|err| err.to_string())?
        .and_then(|table| table.get::<Option<Table>>("perframes").ok().flatten());
    let global_perframes = globals
        .get::<Option<Table>>("mod_perframes")
        .map_err(|err| err.to_string())?;
    compile_timer.push_stage("read_globals");

    if let Some(prefix_globals) = globals
        .get::<Option<Table>>("prefix_globals")
        .map_err(|err| err.to_string())?
    {
        out.beat_mods.extend(read_mod_windows(
            prefix_globals
                .get::<Option<Table>>("mods")
                .map_err(|err| err.to_string())?,
            SongLuaTimeUnit::Beat,
        )?);
        compile_timer.push_stage("prefix_mods");
        let (eases, overlay_eases, column_offsets, info) = read_eases_for_overlay_actors(
            &lua,
            prefix_globals
                .get::<Option<Table>>("ease")
                .map_err(|err| err.to_string())?,
            SongLuaTimeUnit::Beat,
            &host.easing_names,
            &mut overlays,
        )?;
        out.eases.extend(eases);
        out.overlay_eases.extend(overlay_eases);
        out.column_offsets.extend(column_offsets);
        merge_compile_info(&mut out.info, info);
        compile_timer.push_stage("prefix_eases");
        read_overlay_compile_actor_actions(
            &lua,
            prefix_globals
                .get::<Option<Table>>("actions")
                .map_err(|err| err.to_string())?,
            &mut overlays,
            &mut tracked_actors,
            &mut out.messages,
            &mut overlay_trigger_counter,
            &mut out.info,
        )?;
        compile_timer.push_stage("prefix_actions");
    }

    let global_mods = globals
        .get::<Option<Table>>("mods")
        .map_err(|err| err.to_string())?;
    out.beat_mods.extend(read_mod_windows(
        global_mods.clone(),
        SongLuaTimeUnit::Beat,
    )?);
    let (runtime_eases, runtime_overlay_eases) = read_runtime_mod_eases(
        global_mods,
        &host.easing_names,
        runtime_static_overlay_index_for_actors(&overlays),
        context,
    )?;
    out.eases.extend(runtime_eases);
    out.overlay_eases.extend(runtime_overlay_eases);
    out.time_mods.extend(read_mod_windows(
        globals
            .get::<Option<Table>>("mod_time")
            .map_err(|err| err.to_string())?,
        SongLuaTimeUnit::Second,
    )?);
    for table in read_update_function_tables(&lua, &root, &["mod_time"])? {
        out.time_mods
            .extend(read_mod_windows(Some(table), SongLuaTimeUnit::Second)?);
    }
    compile_timer.push_stage("global_mods");
    let (global_eases, global_overlay_eases, global_column_offsets, global_info) =
        read_eases_for_overlay_actors(
            &lua,
            globals
                .get::<Option<Table>>("mods_ease")
                .map_err(|err| err.to_string())?,
            SongLuaTimeUnit::Beat,
            &host.easing_names,
            &mut overlays,
        )?;
    out.eases.extend(global_eases);
    out.overlay_eases.extend(global_overlay_eases);
    out.column_offsets.extend(global_column_offsets);
    merge_compile_info(&mut out.info, global_info);
    compile_timer.push_stage("global_eases");
    let mut xero_node_tables = read_update_function_nested_tables(&lua, &root, &["nodes"])?;
    xero_node_tables.extend(read_global_function_nested_tables(
        &lua,
        "xero",
        &["definemod", "node"],
        &["nodes"],
    )?);
    let (xero_eases, xero_overlay_eases, xero_info) =
        read_xero_runtime_mod_eases_for_overlay_actors(
            &lua,
            read_update_function_nested_tables(&lua, &root, &["eases"])?,
            xero_node_tables,
            &host.easing_names,
            &overlays,
        )?;
    out.eases.extend(xero_eases);
    out.overlay_eases.extend(xero_overlay_eases);
    merge_compile_info(&mut out.info, xero_info);
    compile_timer.push_stage("xero_eases");
    read_overlay_compile_actor_actions(
        &lua,
        globals
            .get::<Option<Table>>("mod_actions")
            .map_err(|err| err.to_string())?,
        &mut overlays,
        &mut tracked_actors,
        &mut out.messages,
        &mut overlay_trigger_counter,
        &mut out.info,
    )?;
    compile_timer.push_stage("global_actions");
    read_update_function_overlay_compile_actor_actions(
        &lua,
        &root,
        &mut overlays,
        &mut tracked_actors,
        &mut out.messages,
        &mut overlay_trigger_counter,
        &mut out.info,
    )?;
    compile_timer.push_stage("update_actions");
    let (perframe_eases, perframe_overlay_eases, perframe_info) = compile_perframes(
        &lua,
        prefix_perframes,
        global_perframes,
        context,
        &mut overlays,
        &tracked_actors,
        &out.messages,
    )?;
    out.eases.extend(perframe_eases);
    out.overlay_eases.extend(perframe_overlay_eases);
    merge_compile_info(&mut out.info, perframe_info);
    compile_timer.push_stage("perframes");
    out.note_hides = read_note_column_zoom_hides(&lua)?;
    compile_timer.push_stage("note_hides");
    let (update_eases, update_overlay_eases, update_overlay_tracks, update_column_transforms) =
        match compile_multitap_update_overlays_for_actors(
            &lua,
            context,
            &mut overlays,
            &mut out.messages,
            noteskin_resolver,
            |overlays, arrow_index, noteskin| {
                ensure_overlay_arrow_visual(
                    &lua,
                    overlays,
                    arrow_index,
                    noteskin,
                    create_dummy_actor,
                    |noteskin| multitap_arrow_visual_spec(context, noteskin),
                )
            },
        )? {
            Some(eases) => (Vec::new(), eases, Vec::new(), Vec::new()),
            None => compile_update_functions(&lua, &root, context, &mut overlays, &tracked_actors)?,
        };
    out.eases.extend(update_eases);
    out.overlay_eases.extend(update_overlay_eases);
    out.overlay_updates.extend(update_overlay_tracks);
    out.column_offsets.extend(update_column_transforms);
    compile_timer.push_stage("update_overlays");
    resolve_late_proxy_targets(&mut overlays, &mut hidden_players)?;
    push_startup_message_if_listened(
        &mut out.messages,
        overlays
            .iter()
            .map(|overlay| overlay.actor.message_commands.as_slice()),
    );
    let overlay_layers = overlays
        .iter()
        .map(|overlay| {
            overlay
                .table
                .get::<Option<usize>>(COMPILE_LAYER_KEY)
                .map_err(|err| err.to_string())
                .map(|index| index.unwrap_or(primary_index))
        })
        .collect::<Result<Vec<_>, _>>()?;
    out.overlays = overlays.into_iter().map(|overlay| overlay.actor).collect();
    for tracked in tracked_actors {
        match tracked.target {
            TrackedCompileActorTarget::Player(player) => out.player_actors[player] = tracked.actor,
            TrackedCompileActorTarget::SongForeground => out.song_foreground = tracked.actor,
        }
    }
    out.hidden_players = hidden_players;

    sort_compiled_song_lua(&mut out);
    out.sound_paths = read_song_lua_sound_paths(&lua)?;
    compile_timer.push_stage("finalize");
    log_song_lua_compile_timing(&trace_entry_path, &compile_timer);
    split_compiled_song_lua(out, overlay_layers, &entry_paths, primary_index)
}

fn resolve_late_proxy_targets<NoteskinSlot, ModelVertex>(
    overlays: &mut [crate::SongLuaOverlayCompileActor<
        SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>,
    >],
    hidden_players: &mut [bool; crate::LUA_PLAYERS],
) -> Result<(), String> {
    let actor_indices = overlays
        .iter()
        .enumerate()
        .map(|(index, overlay)| (overlay.table.to_pointer() as usize, index))
        .collect::<std::collections::HashMap<_, _>>();
    for overlay in overlays {
        let SongLuaOverlayKind::ActorProxy { target } = &mut overlay.actor.kind else {
            continue;
        };
        let Some(mut resolved) = read_proxy_target_kind(&overlay.table)? else {
            continue;
        };
        if let crate::SongLuaProxyTarget::Actor { overlay_index } = &mut resolved {
            *overlay_index = actor_indices
                .get(overlay_index)
                .copied()
                .unwrap_or(usize::MAX);
        }
        if let crate::SongLuaProxyTarget::Player { player_index } = resolved {
            let target_actor = overlay
                .table
                .get::<Option<Table>>("__songlua_proxy_target_actor")
                .map_err(|err| err.to_string())?;
            let target_hidden = target_actor
                .and_then(|actor| {
                    actor
                        .get::<Option<bool>>("__songlua_visible")
                        .ok()
                        .flatten()
                })
                .is_some_and(|visible| !visible);
            if target_hidden {
                if let Some(hidden) = hidden_players.get_mut(player_index) {
                    *hidden = true;
                }
            }
        }
        *target = resolved;
    }
    Ok(())
}

fn mark_compile_layers(root: &Value) -> mlua::Result<()> {
    let Value::Table(root) = root else {
        return Ok(());
    };
    for (index, actor) in root.sequence_values::<Value>().enumerate() {
        mark_actor_layer(&actor?, index)?;
    }
    Ok(())
}

fn mark_actor_layer(actor: &Value, index: usize) -> mlua::Result<()> {
    let Value::Table(actor) = actor else {
        return Ok(());
    };
    actor.set(COMPILE_LAYER_KEY, index)?;
    for child in actor.sequence_values::<Value>() {
        mark_actor_layer(&child?, index)?;
    }
    Ok(())
}

fn split_compiled_song_lua<NoteskinSlot, ModelVertex>(
    mut compiled: DefaultCompiledSongLua<NoteskinSlot, ModelVertex>,
    overlay_layers: Vec<usize>,
    entry_paths: &[std::path::PathBuf],
    primary_index: usize,
) -> Result<Vec<DefaultCompiledSongLua<NoteskinSlot, ModelVertex>>, String> {
    if overlay_layers.len() != compiled.overlays.len() {
        return Err("song lua overlay ownership did not match compiled overlays".to_string());
    }
    let mut local_counts = vec![0usize; entry_paths.len()];
    let overlay_map = overlay_layers
        .iter()
        .map(|&layer| {
            if layer >= local_counts.len() {
                return Err(format!("song lua overlay has invalid layer index {layer}"));
            }
            let local = local_counts[layer];
            local_counts[layer] += 1;
            Ok((layer, local))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut outputs = entry_paths
        .iter()
        .map(|entry_path| DefaultCompiledSongLua {
            entry_path: entry_path.clone(),
            screen_width: compiled.screen_width,
            screen_height: compiled.screen_height,
            messages: compiled.messages.clone(),
            sound_paths: compiled.sound_paths.clone(),
            ..DefaultCompiledSongLua::default()
        })
        .collect::<Vec<_>>();

    for (global_index, mut overlay) in compiled.overlays.drain(..).enumerate() {
        let (layer, _) = overlay_map[global_index];
        overlay.parent_index = overlay.parent_index.and_then(|parent| {
            overlay_map
                .get(parent)
                .filter(|(parent_layer, _)| *parent_layer == layer)
                .map(|(_, local)| *local)
        });
        if let SongLuaOverlayKind::ActorProxy {
            target: crate::SongLuaProxyTarget::Actor { overlay_index },
        } = &mut overlay.kind
        {
            *overlay_index = overlay_map
                .get(*overlay_index)
                .filter(|(target_layer, _)| *target_layer == layer)
                .map_or(usize::MAX, |(_, local)| *local);
        }
        outputs[layer].overlays.push(overlay);
    }
    for mut ease in compiled.overlay_eases.drain(..) {
        let Some(&(layer, local)) = overlay_map.get(ease.overlay_index) else {
            continue;
        };
        ease.overlay_index = local;
        outputs[layer].overlay_eases.push(ease);
    }
    for mut update in compiled.overlay_updates.drain(..) {
        let Some(&(layer, local)) = overlay_map.get(update.overlay_index) else {
            continue;
        };
        update.overlay_index = local;
        outputs[layer].overlay_updates.push(update);
    }

    let primary = &mut outputs[primary_index];
    primary.beat_mods = compiled.beat_mods;
    primary.time_mods = compiled.time_mods;
    primary.eases = compiled.eases;
    primary.player_actors = compiled.player_actors;
    primary.song_foreground = compiled.song_foreground;
    primary.hidden_players = compiled.hidden_players;
    primary.note_hides = compiled.note_hides;
    primary.column_offsets = compiled.column_offsets;
    primary.info = compiled.info;
    for output in &mut outputs {
        sort_compiled_song_lua(output);
    }
    Ok(outputs)
}
